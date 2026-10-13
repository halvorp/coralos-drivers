// SPDX-License-Identifier: GPL-2.0-only
//! The Sunrise Point-LP HD-audio controller (BAR0): global registers, the capability chain (SPB, processing pipe, multi-link), and the
//! stream engines. A playback stream, once its RUN bit is set, reads its buffer cyclically at 48 kHz stereo 16-bit (192 bytes per
//! millisecond of virtual time), advances `LPIB`, raises the buffer-completion status at each period, and copies what it read to
//! `played` so a test can compare the samples that reached the jack with the ones the client wrote.
#![allow(dead_code)]
use hda_ext_core::regs::*;

pub const LEN: usize = 0x4000;
pub const GCAP: u16 = (7 << 12) | (7 << 8) | 1; // 7 output + 7 input streams, 64-bit capable
pub const SPB_AT: u32 = 0x700;
pub const PP_AT: u32 = 0x800;
pub const ML_AT: u32 = 0x900;
pub const BYTES_PER_MS: u64 = 192;

pub struct Hda {
    pub mem: Vec<u8>,
    /// Bytes each stream's engine has read since it started running (a test's view of "frames the hardware read").
    pub played: Vec<Vec<u8>>,
    /// When each stream's RUN bit last went up (virtual microseconds), and the bytes already read since.
    start_us: Vec<u64>,
    done: Vec<u64>,
    was_running: Vec<bool>,
    /// Break the engine: it stops raising period interrupts.
    pub mute_irqs: bool,
    /// Make the engine report a descriptor error instead of completing a period.
    pub desc_error: bool,
}

pub fn sd(i: u32) -> u32 {
    0x80 + 0x20 * i
}

impl Hda {
    pub fn new() -> Self {
        let mut h = Self { mem: vec![0; LEN], played: vec![vec![]; 14], start_us: vec![0; 14], done: vec![0; 14], was_running: vec![false; 14], mute_irqs: false, desc_error: false };
        h.w16(AZX_REG_GCAP, GCAP);
        h.w16(AZX_REG_LLCH, SPB_AT as u16);
        // capability chain: SPB -> PP -> ML (headers: id << 16 | next)
        h.w32(SPB_AT, (AZX_SPB_CAP_ID << AZX_CAP_HDR_ID_OFF) | PP_AT);
        h.w32(PP_AT, (AZX_PP_CAP_ID << AZX_CAP_HDR_ID_OFF) | ML_AT);
        h.w32(ML_AT, AZX_ML_CAP_ID << AZX_CAP_HDR_ID_OFF);
        // one link: the SSP link; LCAP without ALT
        h.w32(ML_AT + AZX_REG_ML_MLCD, 0);
        h.w32(ML_AT + AZX_ML_BASE + AZX_REG_ML_LCAP, 0);
        h.w32(AZX_REG_GCTL, 1);
        h
    }
    pub fn r8(&self, off: u32) -> u8 {
        self.mem[off as usize]
    }
    pub fn r16(&self, off: u32) -> u16 {
        u16::from_le_bytes([self.mem[off as usize], self.mem[off as usize + 1]])
    }
    pub fn r32(&self, off: u32) -> u32 {
        u32::from_le_bytes(self.mem[off as usize..off as usize + 4].try_into().unwrap())
    }
    pub fn w16(&mut self, off: u32, v: u16) {
        self.mem[off as usize..off as usize + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub fn w32(&mut self, off: u32, v: u32) {
        self.mem[off as usize..off as usize + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn read(&mut self, off: u32, width: u32) -> u32 {
        if off == AZX_REG_INTSTS && width == 4 {
            return self.intsts();
        }
        let o = off as usize;
        match width {
            1 => self.mem[o] as u32,
            2 => self.r16(off) as u32,
            _ => self.r32(off),
        }
    }

    /// The write, with the registers whose bits clear on a 1.
    pub fn write(&mut self, off: u32, width: u32, v: u32) {
        // SD_STS (byte 3 of each descriptor's first dword) clears on a 1; a dword write to SD_CTL carries it in its top byte
        for i in 0..14u32 {
            let base = sd(i);
            if width == 1 && off == base + 3 {
                self.mem[off as usize] &= !(v as u8);
                return;
            }
            if width == 4 && off == base {
                let old_sts = self.mem[(base + 3) as usize];
                let new_sts = (v >> 24) as u8;
                let ctl = (v & 0x00ff_ffff) | ((old_sts & !new_sts) as u32) << 24;
                self.w32(off, ctl);
                return;
            }
        }
        match (off, width) {
            (AZX_REG_INTSTS, 4) | (AZX_REG_STATESTS, 2) | (AZX_REG_RIRBSTS, 1) => {} // write-1-clear registers the model keeps at zero
            (_, 1) => self.mem[off as usize] = v as u8,
            (_, 2) => self.w16(off, v as u16),
            _ => self.w32(off, v),
        }
    }

    pub fn running(&self, i: u32) -> bool {
        let ctl = self.r8(sd(i));
        ctl & 0x02 != 0 && ctl & 0x01 == 0
    }

    pub fn intsts(&self) -> u32 {
        let mut s = 0;
        for i in 0..14u32 {
            if self.r8(sd(i) + 3) & 0x1c != 0 {
                s |= 1 << i;
            }
        }
        if s != 0 {
            s |= AZX_INT_CTRL_EN;
        }
        s
    }

    /// Is the controller's interrupt asserted? A stream's status bit, enabled in INTCTL, with the global enable on.
    pub fn irq(&self) -> bool {
        let ctl = self.r32(AZX_REG_INTCTL);
        ctl & AZX_INT_GLOBAL_EN != 0 && (self.intsts() & ctl & AZX_INT_ALL_STREAM) != 0
    }

    /// The BDL of stream `i`: (address, length, ioc) entries read from memory by `read_mem`.
    fn bdl(&self, i: u32, read_mem: &dyn Fn(u64, usize) -> Vec<u8>) -> Vec<(u64, u32, bool)> {
        let lvi = (self.r16(sd(i) + AZX_REG_SD_LVI) & 0xff) as usize;
        let base = self.r32(sd(i) + AZX_REG_SD_BDLPL) as u64 | (self.r32(sd(i) + AZX_REG_SD_BDLPU) as u64) << 32;
        let raw = read_mem(base, (lvi + 1) * 16);
        raw.chunks(16)
            .map(|c| (u32::from_le_bytes(c[0..4].try_into().unwrap()) as u64 | (u32::from_le_bytes(c[4..8].try_into().unwrap()) as u64) << 32, u32::from_le_bytes(c[8..12].try_into().unwrap()), u32::from_le_bytes(c[12..16].try_into().unwrap()) & 1 != 0))
            .collect()
    }

    /// Note RUN transitions after a register write.
    pub fn after_write(&mut self, now_us: u64) {
        for i in 0..14u32 {
            if self.r8(sd(i)) & 0x01 != 0 {
                // a stream in reset has its position zeroed
                self.w32(sd(i) + AZX_REG_SD_LPIB, 0);
            }
            let r = self.running(i);
            if r && !self.was_running[i as usize] {
                self.start_us[i as usize] = now_us;
                self.done[i as usize] = 0;
            }
            self.was_running[i as usize] = r;
        }
    }

    /// The virtual time (us) of the next period boundary of any running playback stream, if any.
    pub fn next_boundary_us(&self) -> Option<u64> {
        let mut best: Option<u64> = None;
        for i in 0..14u32 {
            if !self.running(i) || self.mute_irqs {
                continue;
            }
            let period = self.period_bytes(i);
            if period == 0 {
                continue;
            }
            let boundary = (self.done[i as usize] / period + 1) * period;
            // microseconds for `boundary` bytes at 192 bytes/ms, rounded up
            let at = self.start_us[i as usize] + (boundary * 1000).div_ceil(BYTES_PER_MS);
            best = Some(best.map_or(at, |b| b.min(at)));
        }
        best
    }

    fn period_bytes(&self, i: u32) -> u64 {
        let cbl = self.r32(sd(i) + AZX_REG_SD_CBL) as u64;
        let n = (self.r16(sd(i) + AZX_REG_SD_LVI) & 0xff) as u64 + 1;
        cbl.checked_div(n).unwrap_or(0)
    }

    /// Run the engines up to `now_us`. `read_mem(addr, len)` is DMA memory.
    pub fn advance(&mut self, now_us: u64, read_mem: &dyn Fn(u64, usize) -> Vec<u8>) {
        for i in 0..14u32 {
            if !self.running(i) {
                continue;
            }
            let target = (now_us - self.start_us[i as usize]) * BYTES_PER_MS / 1000;
            let cbl = self.r32(sd(i) + AZX_REG_SD_CBL) as u64;
            let period = self.period_bytes(i);
            if cbl == 0 || period == 0 || target <= self.done[i as usize] {
                continue;
            }
            let bdl = self.bdl(i, read_mem);
            let ring_base = bdl.first().map_or(0, |e| e.0);
            let mut lpib = self.r32(sd(i) + AZX_REG_SD_LPIB) as u64;
            let mut left = target - self.done[i as usize];
            self.done[i as usize] = target;
            while left > 0 {
                let to_end = period - lpib % period;
                let n = left.min(to_end);
                let data = read_mem(ring_base + lpib, n as usize);
                self.played[i as usize].extend_from_slice(&data);
                lpib = (lpib + n) % cbl;
                left -= n;
                if lpib.is_multiple_of(period) && !self.mute_irqs {
                    let sts = if self.desc_error { 0x10 } else { 0x04 };
                    self.mem[(sd(i) + 3) as usize] |= sts;
                }
            }
            self.w32(sd(i) + AZX_REG_SD_LPIB, lpib as u32);
        }
    }
}
