// SPDX-License-Identifier: GPL-2.0-only
//! [`Live`]: the loop. One [`Live::step`] is: compute the next deadline, BLOCK ([`Transport::wait`]), handle exactly the wake that ended
//! the wait, advance the device ([`UapiService::poll`], cheap when idle), and send every reply that is ready.
use super::regions::{PayloadRegions, RegionError};
use super::transport::{Recv, SendOutcome, Transport, Wake};
use crate::exec::dma::{CpuMem, GpuMem};
use crate::exec::Executor;
use crate::log::{emit, Log};
use crate::uapi::{frame, Dispatched, Hw, UapiService};
use alloc::vec::Vec;
use coralos_hal::{Dma, Mmio};
use i915_wire::region::RegionRef;
use i915_wire::{errno, Header, Status, WireError, HEADER_BYTES, MAX_FRAME_BYTES, MAX_PAYLOAD};

/// Milliseconds before an unsent reply (the peer's reply ring was full) is tried again. A timer exists only while a reply is unsent.
pub const REPLY_RETRY_MS: u64 = 2;

/// Counters of what the loop did; the tests read them, the shutdown line prints them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveStats {
    /// Calls to [`Transport::wait`]: every one is a block, so this IS the number of times the loop slept.
    pub waits: u64,
    pub request_wakes: u64,
    pub interrupt_wakes: u64,
    pub deadline_wakes: u64,
    pub peer_dead_wakes: u64,
    /// uAPI frames handed to the service (inline or through a region).
    pub calls: u64,
    /// Calls that arrived through a payload region.
    pub region_calls: u64,
    /// Replies that went out through a payload region.
    pub region_replies: u64,
    /// Frames refused before the service saw them (undecodable header, oversized, region unavailable, reply too large).
    pub refused: u64,
    pub replies_sent: u64,
    /// Replies given up on (denied, or no room to keep them).
    pub replies_dropped: u64,
    pub sessions_freed_by_peer_death: u64,
}

/// What one step did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The wait ended with this wake and it was handled.
    Woke(Wake),
    /// The transport closed; the loop is over.
    Closed,
}

/// Why the loop could not start (before any request was taken): a named refusal, nothing half-made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveError {
    /// The frame buffers could not be allocated.
    NoMemory { what: &'static str },
}

impl LiveError {
    pub const fn name(&self) -> &'static str {
        match self {
            LiveError::NoMemory { .. } => "LIVE-NOMEM",
        }
    }
}

pub struct Live<T: Transport, R: PayloadRegions, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    pub t: T,
    pub regions: R,
    pub svc: UapiService,
    pub ex: Executor<B>,
    pub gm: GpuMem<D, M>,
    pub log: L,
    pub stats: LiveStats,
    /// The largest reply payload that is sent inline to a REGION call; a bigger one goes to the client's reply area. [`MAX_PAYLOAD`]
    /// (the most a frame may carry) unless a test lowers it to exercise the reply-area path with an ordinary reply.
    pub inline_reply_max: usize,
    inbuf: Vec<u8>,
    outbuf: Vec<u8>,
    /// Replies the peer's ring had no room for, oldest first. While it is non-empty no new request is taken (back-pressure), so it holds
    /// at most the replies of one request plus the parked calls that finished meanwhile - bounded by the work the device already took.
    outbox: Vec<Vec<u8>>,
}

fn vec_of(len: usize, what: &'static str) -> Result<Vec<u8>, LiveError> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).map_err(|_| LiveError::NoMemory { what })?;
    v.resize(len, 0);
    Ok(v)
}

impl<T: Transport, R: PayloadRegions, B: Mmio, D: Dma, M: CpuMem, L: Log> Live<T, R, B, D, M, L> {
    /// The receive and reply buffers are the largest inline frame ([`MAX_FRAME_BYTES`]) each; if the heap cannot give them the device
    /// does not start (`LIVE-NOMEM`), instead of starting with a buffer that cannot hold a frame.
    pub fn new(t: T, regions: R, svc: UapiService, ex: Executor<B>, gm: GpuMem<D, M>, log: L) -> Result<Self, LiveError> {
        Ok(Live { t, regions, svc, ex, gm, log, stats: LiveStats::default(), inline_reply_max: MAX_PAYLOAD as usize, inbuf: vec_of(MAX_FRAME_BYTES, "receive buffer")?, outbuf: vec_of(MAX_FRAME_BYTES, "reply buffer")?, outbox: Vec::new() })
    }

    /// Step until the transport closes.
    pub fn run(&mut self) {
        while self.step() != Step::Closed {}
    }

    /// The deadline this step would block until (absolute ms), for the tests and the shutdown line.
    pub fn next_deadline(&mut self) -> Option<u64> {
        let now = self.t.now_ms();
        let d = self.svc.next_deadline_ms(now, self.t.irq_armed());
        if self.outbox.is_empty() {
            return d;
        }
        let retry = now.saturating_add(REPLY_RETRY_MS);
        Some(d.map_or(retry, |d| d.min(retry)))
    }

    pub fn step(&mut self) -> Step {
        let deadline = self.next_deadline();
        let accept = self.outbox.is_empty();
        self.stats.waits += 1;
        let wake = self.t.wait(deadline, accept);
        match wake {
            Wake::Closed => return Step::Closed,
            Wake::Request => self.stats.request_wakes += 1,
            Wake::Interrupt => self.stats.interrupt_wakes += 1,
            Wake::Deadline => self.stats.deadline_wakes += 1,
            Wake::PeerDead => self.stats.peer_dead_wakes += 1,
        }
        match wake {
            Wake::Request => self.take_request(),
            Wake::PeerDead => self.peer_gone(),
            _ => {}
        }
        self.advance();
        Step::Woke(wake)
    }

    // ---- requests -------------------------------------------------------------------------------------------------------------

    fn take_request(&mut self) {
        match self.t.recv(&mut self.inbuf) {
            Recv::Frame(n) => self.dispatch(n),
            Recv::Empty => {}
            Recv::Oversized(got) => {
                self.stats.refused += 1;
                let e = WireError::PayloadTooLarge { got: got.saturating_sub(HEADER_BYTES as u64).min(u32::MAX as u64) as u32, max: MAX_PAYLOAD };
                emit(&mut self.log, format_args!("[I915-LIVE frame refused name={} frame_bytes={} max_inline={}]", e.name(), got, MAX_FRAME_BYTES));
                self.reply_malformed(e);
            }
            Recv::PeerDead => self.peer_gone(),
            Recv::Error(w) => emit(&mut self.log, format_args!("[I915-LIVE recv refused word={:#x}]", w)),
        }
    }

    fn dispatch(&mut self, n: usize) {
        // the first generation's frames ('I', version, op) never start with the magic; they keep their four-byte answer
        if n < 4 || self.inbuf[..4] != *b"I915" {
            if let Some(rep) = crate::wire::handle(n as u64, &self.inbuf) {
                self.send_frame(&rep);
            }
            return;
        }
        let h = match Header::decode_header(&self.inbuf[..n]) {
            Ok(h) => h,
            Err(e) => {
                self.stats.refused += 1;
                emit(&mut self.log, format_args!("[I915-LIVE frame refused name={} got_bytes={}]", e.name(), n));
                return self.reply_malformed(e);
            }
        };
        if h.is_region() {
            return self.dispatch_region(h, n);
        }
        let payload = match Header::decode(&self.inbuf[..n]) {
            Ok((_, p)) => p,
            Err(e) => {
                self.stats.refused += 1;
                emit(&mut self.log, format_args!("[I915-LIVE frame refused name={} got_bytes={}]", e.name(), n));
                return self.reply_malformed(e);
            }
        };
        self.stats.calls += 1;
        let now = self.t.now_ms();
        let mut hw = Hw { ex: &mut self.ex, gm: &mut self.gm, log: &mut self.log, now_ms: now };
        let done = self.svc.handle_payload(&mut hw, &h, payload, &mut self.outbuf);
        self.finish_inline(h, done);
    }

    /// A call whose bytes are in a client-granted region: read them out ONCE into the device's own memory, then serve them like an
    /// inline call. A reply too large for a frame goes back through the client's reply area.
    fn dispatch_region(&mut self, h: Header, n: usize) {
        let rr = match Header::decode(&self.inbuf[..n]).and_then(|(_, p)| RegionRef::decode(p)) {
            Ok(r) => r,
            Err(e) => {
                self.stats.refused += 1;
                emit(&mut self.log, format_args!("[I915-LIVE region ref refused name={}]", e.name()));
                return self.reply_malformed(e);
            }
        };
        let mut req = Vec::new();
        if req.try_reserve_exact(rr.len as usize).is_err() {
            return self.refuse(h, Status::Ok, errno::ENOMEM, "LIVE-NOMEM", rr.len as u64);
        }
        req.resize(rr.len as usize, 0);
        if let Err(e) = self.regions.read(rr.region, rr.offset, &mut req) {
            return self.refuse_region(h, e);
        }
        self.stats.calls += 1;
        self.stats.region_calls += 1;
        // a reply area bigger than an inline frame may be needed: a scratch buffer for it, if the heap has one (else the call fails by
        // name if the reply does not fit the ordinary buffer)
        let mut big = Vec::new();
        if rr.reply_cap as usize > MAX_PAYLOAD as usize && big.try_reserve_exact(HEADER_BYTES + rr.reply_cap as usize).is_ok() {
            big.resize(HEADER_BYTES + rr.reply_cap as usize, 0);
        }
        let now = self.t.now_ms();
        let mut hw = Hw { ex: &mut self.ex, gm: &mut self.gm, log: &mut self.log, now_ms: now };
        let out: &mut [u8] = if big.is_empty() { &mut self.outbuf } else { &mut big };
        let done = self.svc.handle_payload(&mut hw, &h, &req, out);
        match done {
            Ok(Dispatched::Reply(rh, len)) if len > self.inline_reply_max => {
                if len > rr.reply_cap as usize {
                    self.stats.refused += 1;
                    return self.refuse(h, Status::Ok, errno::EFAULT, "REPLY-TOO-LARGE", len as u64);
                }
                let area: &[u8] = if big.is_empty() { &self.outbuf[HEADER_BYTES..HEADER_BYTES + len] } else { &big[HEADER_BYTES..HEADER_BYTES + len] };
                if let Err(e) = self.regions.write(rr.region, rr.reply_offset, area) {
                    return self.refuse_region(h, e);
                }
                let placed = RegionRef { region: rr.region, offset: rr.reply_offset, len: len as u32, reply_cap: rr.reply_cap, reply_offset: rr.reply_offset };
                let head = rh.reply_in_region();
                let ok = placed.encode(&mut self.outbuf[HEADER_BYTES..]).and_then(|_| head.encode(&mut self.outbuf));
                match ok {
                    Ok(()) => {
                        self.stats.region_replies += 1;
                        self.send_out(HEADER_BYTES + i915_wire::region::REGION_REF_BYTES);
                    }
                    Err(e) => self.reply_malformed(e),
                }
            }
            other => {
                if big.is_empty() {
                    self.finish_inline(h, other);
                } else {
                    // the reply fits an inline frame but was built in the scratch buffer: move it to the ordinary one
                    match other {
                        Ok(Dispatched::Reply(rh, len)) => {
                            self.outbuf[HEADER_BYTES..HEADER_BYTES + len].copy_from_slice(&big[HEADER_BYTES..HEADER_BYTES + len]);
                            self.finish_inline(h, Ok(Dispatched::Reply(rh, len)));
                        }
                        o => self.finish_inline(h, o),
                    }
                }
            }
        }
    }

    /// Frame and send a reply built at `outbuf[HEADER_BYTES..]`.
    fn finish_inline(&mut self, req: Header, done: Result<Dispatched, WireError>) {
        match done {
            Ok(Dispatched::Reply(rh, len)) => match frame::finish(rh, len, &mut self.outbuf) {
                Ok(total) => self.send_out(total),
                Err(e) => {
                    self.stats.refused += 1;
                    emit(&mut self.log, format_args!("[I915-LIVE reply refused name=REPLY-TOO-LARGE wire={} payload_bytes={} max_inline={}]", e.name(), len, MAX_PAYLOAD));
                    self.refuse(req, Status::Ok, errno::EFAULT, "REPLY-TOO-LARGE", len as u64);
                }
            },
            Ok(Dispatched::Parked) => {}
            Err(e) => {
                self.stats.refused += 1;
                emit(&mut self.log, format_args!("[I915-LIVE call refused name={}]", e.name()));
                self.reply_malformed(e);
            }
        }
    }

    // ---- refusals ---------------------------------------------------------------------------------------------------------------

    /// A frame whose header did not decode (nothing in it can be echoed).
    fn reply_malformed(&mut self, e: WireError) {
        if let Ok(n) = frame::malformed_reply(e, &mut self.outbuf) {
            self.send_out(n);
        }
    }

    /// A refusal of a call whose header decoded: echo it, say `errno`, and name the reason in the log.
    fn refuse(&mut self, h: Header, status: Status, err: i32, name: &str, got: u64) {
        emit(&mut self.log, format_args!("[I915-LIVE op={} refused name={} errno={} got={}]", h.opcode.name(), name, err, got));
        if let Ok(n) = frame::refusal_reply(h, status, err, &mut self.outbuf) {
            self.send_out(n);
        }
    }

    fn refuse_region(&mut self, h: Header, e: RegionError) {
        self.stats.refused += 1;
        match e {
            RegionError::Unavailable => self.refuse(h, Status::Unsupported, errno::EOPNOTSUPP, e.name(), 0),
            RegionError::Unknown { region } => self.refuse(h, Status::Ok, errno::EINVAL, e.name(), region as u64),
            RegionError::OutOfRange { offset, .. } => self.refuse(h, Status::Ok, errno::EFAULT, e.name(), offset),
        }
    }

    // ---- replies ----------------------------------------------------------------------------------------------------------------

    /// Send the frame in `outbuf[..n]`.
    fn send_out(&mut self, n: usize) {
        let outcome = self.t.send(&self.outbuf[..n]);
        self.settle(outcome, n, true);
    }

    fn send_frame(&mut self, f: &[u8]) {
        let outcome = self.t.send(f);
        match outcome {
            SendOutcome::Sent => self.stats.replies_sent += 1,
            SendOutcome::PeerDead => self.peer_gone(),
            _ => self.stats.replies_dropped += 1,
        }
    }

    /// What became of a send of `outbuf[..n]`: count it, keep it for a retry when the ring was full, free the peer's sessions when it is dead.
    fn settle(&mut self, outcome: SendOutcome, n: usize, from_outbuf: bool) {
        match outcome {
            SendOutcome::Sent => self.stats.replies_sent += 1,
            SendOutcome::PeerDead => self.peer_gone(),
            SendOutcome::Denied => {
                self.stats.replies_dropped += 1;
                emit(&mut self.log, format_args!("[I915-LIVE reply dropped name=SEND-DENIED]"));
            }
            SendOutcome::RingFull => {
                let kept = from_outbuf && self.keep(n);
                if !kept {
                    self.stats.replies_dropped += 1;
                    emit(&mut self.log, format_args!("[I915-LIVE reply dropped name=REPLY-RING-FULL]"));
                }
            }
        }
    }

    /// Keep `outbuf[..n]` for a retry (fallibly: a heap with no room drops the reply, by name).
    fn keep(&mut self, n: usize) -> bool {
        let mut v = Vec::new();
        if v.try_reserve_exact(n).is_err() || self.outbox.try_reserve(1).is_err() {
            return false;
        }
        v.extend_from_slice(&self.outbuf[..n]);
        self.outbox.push(v);
        true
    }

    /// Advance the device, then send everything that is ready: first the replies that did not fit last time, then parked calls that
    /// finished.
    fn advance(&mut self) {
        let now = self.t.now_ms();
        {
            let mut hw = Hw { ex: &mut self.ex, gm: &mut self.gm, log: &mut self.log, now_ms: now };
            self.svc.poll(&mut hw);
        }
        self.flush_outbox();
        while self.outbox.is_empty() {
            let Some(f) = self.svc.take_ready() else { break };
            let outcome = self.t.send(&f);
            match outcome {
                SendOutcome::RingFull => {
                    if self.outbox.try_reserve(1).is_ok() {
                        self.outbox.push(f);
                    } else {
                        self.stats.replies_dropped += 1;
                    }
                }
                other => {
                    self.settle(other, 0, false);
                }
            }
        }
    }

    fn flush_outbox(&mut self) {
        while !self.outbox.is_empty() {
            match self.t.send(&self.outbox[0]) {
                SendOutcome::RingFull => return,
                SendOutcome::Sent => {
                    self.stats.replies_sent += 1;
                    self.outbox.remove(0);
                }
                SendOutcome::PeerDead => {
                    self.outbox.clear();
                    return self.peer_gone();
                }
                SendOutcome::Denied => {
                    self.stats.replies_dropped += 1;
                    self.outbox.remove(0);
                }
            }
        }
    }

    // ---- peer death -------------------------------------------------------------------------------------------------------------

    /// The peer is gone: every session of this connection dies (`session_dead` frees what each held; requests still on the engine finish
    /// first and their objects are freed when they retire). Replies nobody can read are dropped.
    fn peer_gone(&mut self) {
        self.outbox.clear();
        let now = self.t.now_ms();
        let mut freed = 0u64;
        while let Some(inc) = self.svc.any_session() {
            let mut hw = Hw { ex: &mut self.ex, gm: &mut self.gm, log: &mut self.log, now_ms: now };
            self.svc.session_dead(&mut hw, inc);
            freed += 1;
        }
        self.stats.sessions_freed_by_peer_death += freed;
        emit(&mut self.log, format_args!("[I915-LIVE peer dead sessions_freed={}]", freed));
    }
}
