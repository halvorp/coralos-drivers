// SPDX-License-Identifier: GPL-2.0-only
//! The playback stream's two buffers and the accounting between them.
//!
//! * [`Fifo`] - the device-side queue the client's PCM writes land in. Sized once, when the stream opens, from the negotiated geometry
//!   (a whole number of periods, [`FIFO_PERIODS`] times the DMA ring); a full queue is a SHORT WRITE, never a block and never a drop.
//! * [`RingAcct`] - the host DMA ring (the buffer the HD-audio stream engine reads cyclically). The hardware's progress is the stream's
//!   `LPIB` (link position in buffer); `hw_total` is its running total, wrap-corrected. Everything the client is told about progress comes
//!   from `hw_total` - frames the HARDWARE READ - not from what was submitted and not from an interrupt count.
//!
//! The refill rule: after each period interrupt the device moves queued PCM into the free part of the ring, whole frames only. If the
//! hardware would be left with less than one period to play and the queue is empty, the shortfall is filled with SILENCE and counted as an
//! underrun; real data never waits behind more silence than the margin requires.
use alloc::vec::Vec;

/// The queue holds this many rings of PCM, so a client that writes one period per period interrupt never runs dry.
pub const FIFO_PERIODS: u32 = 2;

#[derive(Debug)]
pub struct Fifo {
    buf: Vec<u8>,
    head: usize,
    len: usize,
}

impl Fifo {
    /// `cap` bytes, grown fallibly: `None` when the heap cannot give them.
    pub fn new(cap: usize) -> Option<Self> {
        let mut buf = Vec::new();
        buf.try_reserve_exact(cap).ok()?;
        buf.resize(cap, 0);
        Some(Self { buf, head: 0, len: 0 })
    }
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }
    pub fn queued(&self) -> usize {
        self.len
    }
    pub fn free(&self) -> usize {
        self.buf.len() - self.len
    }
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
    /// Append as much of `data` as fits, in whole `unit` bytes (a frame); returns the bytes taken.
    pub fn push(&mut self, data: &[u8], unit: usize) -> usize {
        let unit = unit.max(1);
        let n = data.len().min(self.free()) / unit * unit;
        let cap = self.buf.len();
        for (i, b) in data[..n].iter().enumerate() {
            self.buf[(self.head + self.len + i) % cap] = *b;
        }
        self.len += n;
        n
    }
    /// Move up to `out.len()` bytes (a whole number of `unit`s) into `out`; returns the bytes moved.
    pub fn pop_into(&mut self, out: &mut [u8], unit: usize) -> usize {
        let unit = unit.max(1);
        let n = out.len().min(self.len) / unit * unit;
        let cap = self.buf.len();
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = self.buf[(self.head + i) % cap];
        }
        self.head = (self.head + n) % cap;
        self.len -= n;
        n
    }
}

/// One refill decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fill {
    /// Bytes of queued PCM to move into the ring.
    pub pcm: usize,
    /// Bytes of silence to append after them.
    pub silence: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingAcct {
    pub ring_bytes: u32,
    pub period_bytes: u32,
    pub frame_bytes: u32,
    /// Bytes ever written into the ring (PCM and silence).
    pub fed: u64,
    /// Bytes the hardware has read, wrap-corrected from `LPIB`.
    pub hw_total: u64,
    last_lpib: u32,
    /// Bytes of CLIENT pcm moved into the ring (excludes silence).
    pub pcm_fed: u64,
    pub underruns: u32,
}

impl RingAcct {
    pub const fn new(ring_bytes: u32, period_bytes: u32, frame_bytes: u32) -> Self {
        Self { ring_bytes, period_bytes, frame_bytes, fed: 0, hw_total: 0, last_lpib: 0, pcm_fed: 0, underruns: 0 }
    }

    /// Where in the ring the next byte goes.
    pub fn write_pos(&self) -> usize {
        (self.fed % self.ring_bytes as u64) as usize
    }

    /// Bytes written and not yet read by the hardware.
    pub fn in_flight(&self) -> u64 {
        self.fed - self.hw_total
    }

    /// Fold a new `LPIB` reading into the hardware total. A position beyond the ring is not a position (a dead register reads all-ones):
    /// `false`, nothing changes. A position that did not move is zero bytes.
    pub fn advance_hw(&mut self, lpib: u32) -> bool {
        if lpib >= self.ring_bytes {
            return false;
        }
        let delta = if lpib >= self.last_lpib { lpib - self.last_lpib } else { self.ring_bytes - self.last_lpib + lpib };
        self.last_lpib = lpib;
        self.hw_total += delta as u64;
        if self.hw_total > self.fed {
            // the hardware ran past everything written: it replayed stale ring contents. That is an underrun, and the write position is
            // moved up to the hardware's so the next data lands directly in front of it instead of a ring behind
            self.underruns = self.underruns.saturating_add(1);
            // `LPIB` is a byte position and need not sit on a frame boundary; the next sample goes on the next whole frame after it
            let fb = self.frame_bytes as u64;
            self.fed = self.hw_total.div_ceil(fb) * fb;
        }
        true
    }

    /// Frames the hardware has read.
    pub fn played_frames(&self) -> u64 {
        self.hw_total / self.frame_bytes as u64
    }

    /// Decide the next refill given how many bytes are queued. The hardware must never be left with less than `margin_bytes` unread: if the
    /// queue cannot make that up, silence does.
    pub fn plan_fill(&self, queued: usize, margin_bytes: usize) -> Fill {
        let fb = self.frame_bytes as usize;
        let free = (self.ring_bytes as u64 - self.in_flight()) as usize;
        let pcm = queued.min(free) / fb * fb;
        let after = self.in_flight() as usize + pcm;
        let silence = if after < margin_bytes { (margin_bytes - after).min(free - pcm) / fb * fb } else { 0 };
        Fill { pcm, silence }
    }

    /// Record that `fill` was written (the caller wrote exactly those bytes at [`RingAcct::write_pos`] onward, wrapping). Silence that
    /// was not planned (`count_underrun`) is an underrun.
    pub fn commit(&mut self, fill: Fill, count_underrun: bool) {
        self.fed += (fill.pcm + fill.silence) as u64;
        self.pcm_fed += fill.pcm as u64;
        if fill.silence > 0 && count_underrun {
            self.underruns = self.underruns.saturating_add(1);
        }
    }
}
