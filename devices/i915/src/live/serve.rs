// SPDX-License-Identifier: GPL-2.0-only
//! [`Live`]: the loop. One [`Live::step`] is: compute the next deadline, BLOCK ([`Transport::wait`]), handle exactly the wake that ended
//! the wait (a connect, one message of one connection, the engine, the clock), advance the device ([`UapiService::poll`], cheap when
//! idle), and send every reply that is ready - each to the connection that asked.
use super::regions::{PayloadRegions, RegionError};
use crate::conn::{Accepted, ConnId, ConnTable, FlushCount, Pulled, Queued, Transport, Wake, REPLY_RETRY_MS};
use crate::exec::dma::{CpuMem, GpuMem};
use crate::exec::Executor;
use crate::log::{emit, Log};
use crate::uapi::{frame, Dispatched, Hw, UapiService};
use alloc::vec::Vec;
use coralos_hal::{Dma, Mmio};
use i915_wire::frag::{FragError, Joiner};
use i915_wire::region::RegionRef;
use i915_wire::{errno, Header, Opcode, Status, WireError, HEADER_BYTES, MAX_FRAME_BYTES, MAX_PAYLOAD};

/// Counters of what the loop did; the tests read them, the shutdown line prints them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveStats {
    /// Calls to [`Transport::wait`]: every one is a block, so this IS the number of times the loop slept.
    pub waits: u64,
    pub accept_wakes: u64,
    pub request_wakes: u64,
    pub interrupt_wakes: u64,
    pub deadline_wakes: u64,
    /// uAPI frames handed to the service (inline or through a region).
    pub calls: u64,
    /// Calls that arrived through a payload region.
    pub region_calls: u64,
    /// Replies that went out through a payload region.
    pub region_replies: u64,
    /// Frames refused before the service saw them (undecodable header, bad framing, oversized, region unavailable, reply too large,
    /// a session that is not this connection's).
    pub refused: u64,
    /// Frames now completely in a client's ring.
    pub replies_sent: u64,
    /// Replies given up on (denied, no room to keep them, no connection left to take them).
    pub replies_dropped: u64,
    /// Connections accepted / released (the client died, or a send found it dead).
    pub conns_accepted: u64,
    pub conns_reaped: u64,
    /// Connects the transport could not take (a malformed announce, no room to track it).
    pub connects_refused: u64,
    /// Sessions freed because their client died.
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
    /// Every live connection (one client each; each owns at most one uAPI session).
    pub conns: ConnTable,
    /// The largest reply payload that is sent inline to a REGION call; a bigger one goes to the client's reply area. The most one
    /// fragmented frame may carry unless a test lowers it to exercise the reply-area path with an ordinary reply.
    pub inline_reply_max: usize,
    /// One message off a connection (the transport's message size).
    msgbuf: Vec<u8>,
    /// The whole frame being served.
    inbuf: Vec<u8>,
    outbuf: Vec<u8>,
    /// The connection whose request is being served (replies built in `outbuf` go to it).
    cur: ConnId,
    /// Is the loop accepting connects? False while the heap had no room for another row (reported once, by name).
    can_accept: bool,
    paused_reported: bool,
}

fn vec_of(len: usize, what: &'static str) -> Result<Vec<u8>, LiveError> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).map_err(|_| LiveError::NoMemory { what })?;
    v.resize(len, 0);
    Ok(v)
}

impl<T: Transport, R: PayloadRegions, B: Mmio, D: Dma, M: CpuMem, L: Log> Live<T, R, B, D, M, L> {
    /// The message, frame and reply buffers are allocated up front: a message, and the largest inline frame ([`MAX_FRAME_BYTES`]) each. If
    /// the heap cannot give them the device does not start (`LIVE-NOMEM`), instead of starting with a buffer that cannot hold a frame.
    pub fn new(t: T, regions: R, svc: UapiService, ex: Executor<B>, gm: GpuMem<D, M>, log: L) -> Result<Self, LiveError> {
        let limits = t.limits();
        let reply_max = limits.max_frame_bytes().min(MAX_FRAME_BYTES) - HEADER_BYTES;
        Ok(Live {
            t,
            regions,
            svc,
            ex,
            gm,
            log,
            stats: LiveStats::default(),
            conns: ConnTable::new(),
            inline_reply_max: reply_max,
            msgbuf: vec_of(limits.msg_cap, "message buffer")?,
            inbuf: vec_of(MAX_FRAME_BYTES, "receive buffer")?,
            outbuf: vec_of(MAX_FRAME_BYTES, "reply buffer")?,
            cur: ConnId(0),
            can_accept: true,
            paused_reported: false,
        })
    }

    /// Step until the transport closes.
    pub fn run(&mut self) {
        while self.step() != Step::Closed {}
    }

    /// The deadline this step would block until (absolute ms), for the tests and the shutdown line.
    pub fn next_deadline(&mut self) -> Option<u64> {
        let now = self.t.now_ms();
        let d = self.svc.next_deadline_ms(now, self.t.irq_armed());
        if !self.conns.any_pending() {
            return d;
        }
        let retry = now.saturating_add(REPLY_RETRY_MS);
        Some(d.map_or(retry, |d| d.min(retry)))
    }

    pub fn step(&mut self) -> Step {
        self.make_room();
        let deadline = self.next_deadline();
        self.stats.waits += 1;
        let wake = self.t.wait(deadline, self.can_accept);
        match wake {
            Wake::Closed => return Step::Closed,
            Wake::Accept => self.stats.accept_wakes += 1,
            Wake::Request(_) => self.stats.request_wakes += 1,
            Wake::Interrupt => self.stats.interrupt_wakes += 1,
            Wake::Deadline => self.stats.deadline_wakes += 1,
        }
        match wake {
            Wake::Accept => self.accept_one(),
            Wake::Request(id) => self.take_request(id),
            _ => {}
        }
        self.advance();
        Step::Woke(wake)
    }

    // ---- connections ----------------------------------------------------------------------------------------------------------

    /// Room for ONE more connection, in our table and the transport's, BEFORE any connect is accepted (doctrine RUNTIME-TABLES-HAVE-NO-CAP).
    /// If the heap has none the loop stops listening: the announce stays queued, the kernel's listen ring fills, and the kernel refuses the
    /// next `connect()` itself - a named refusal before a session exists.
    fn make_room(&mut self) {
        let room = self.conns.reserve_one() && self.t.make_room();
        if !room && !self.paused_reported {
            emit(&mut self.log, format_args!("[I915-LIVE accept paused name=CONN-NOMEM conns={}]", self.conns.len()));
        }
        self.paused_reported = !room;
        self.can_accept = room;
    }

    fn accept_one(&mut self) {
        match self.t.accept() {
            Accepted::Conn(id) => {
                if !self.conns.admit(id, Joiner::new(self.t.limits())) {
                    // the row was reserved this step, so this cannot happen; if it does the connection is not left half-served
                    self.stats.connects_refused += 1;
                    self.t.release(id);
                    emit(&mut self.log, format_args!("[I915-LIVE connect refused name=CONN-NOMEM id={}]", id.0));
                    return;
                }
                self.stats.conns_accepted += 1;
                emit(&mut self.log, format_args!("[I915-LIVE conn accepted id={} conns={}]", id.0, self.conns.len()));
            }
            Accepted::Empty => {}
            Accepted::Refused(name) => {
                self.stats.connects_refused += 1;
                emit(&mut self.log, format_args!("[I915-LIVE connect refused name={}]", name));
            }
        }
    }

    /// The connection is gone (its client died, or a send found it so): its session dies with it - everything it held is freed, requests
    /// still on the engine finish first and their objects are freed when they retire - and its endpoints go back to the kernel.
    fn reap(&mut self, id: ConnId, why: &str) {
        let Some(c) = self.conns.remove(id) else { return };
        self.regions.conn_ended(id);
        if let Some(inc) = c.session {
            let now = self.t.now_ms();
            let mut hw = Hw { ex: &mut self.ex, gm: &mut self.gm, log: &mut self.log, now_ms: now };
            self.svc.session_dead(&mut hw, inc);
            self.stats.sessions_freed_by_peer_death += 1;
        }
        let released = self.t.release(id);
        self.stats.conns_reaped += 1;
        emit(&mut self.log, format_args!("[I915-LIVE conn dead id={} why={} session_freed={} released={} conns={}]", id.0, why, c.session.is_some(), released, self.conns.len()));
    }

    // ---- requests -------------------------------------------------------------------------------------------------------------

    fn take_request(&mut self, id: ConnId) {
        let Some(c) = self.conns.get_mut(id) else { return };
        let pulled = c.pull(&mut self.t, &mut self.msgbuf, &mut self.inbuf);
        self.cur = id;
        match pulled {
            Pulled::Frame(n) => self.dispatch(n),
            Pulled::Legacy(n) => {
                // the first generation's frames ('I', version, op) never start with the magic; they keep their four-byte answer
                if let Some(rep) = crate::wire::handle(n as u64, &self.inbuf) {
                    self.send_message(&rep);
                }
            }
            Pulled::Partial | Pulled::Idle => {}
            Pulled::Refused { err, head } => self.refuse_framing(err, head),
            Pulled::NoMemory { need, head } => {
                self.stats.refused += 1;
                emit(&mut self.log, format_args!("[I915-LIVE frame refused name=LIVE-NOMEM frame_bytes={}]", need));
                match head {
                    Some(h) => self.refuse(h, Status::Ok, errno::ENOMEM, "LIVE-NOMEM", need as u64),
                    None => self.reply_malformed(WireError::LimitExceeded { what: "frame.memory", got: need as u64, max: 0 }),
                }
            }
            Pulled::Oversized(got) => {
                self.stats.refused += 1;
                let e = WireError::PayloadTooLarge { got: got.saturating_sub(HEADER_BYTES as u64).min(u32::MAX as u64) as u32, max: MAX_PAYLOAD };
                emit(&mut self.log, format_args!("[I915-LIVE frame refused name={} frame_bytes={} max_message={}]", e.name(), got, self.msgbuf.len()));
                self.reply_malformed(e);
            }
            Pulled::Dead => self.reap(id, "request-endpoint"),
            Pulled::Error(w) => emit(&mut self.log, format_args!("[I915-LIVE recv refused word={:#x}]", w)),
        }
    }

    /// A sequence of messages that is not a frame. If the frame it broke had begun, the refusal answers THAT request.
    fn refuse_framing(&mut self, err: FragError, head: Option<Header>) {
        self.stats.refused += 1;
        let w = err.wire();
        emit(&mut self.log, format_args!("[I915-LIVE frame refused name={} {:?}]", err.name(), err));
        match head {
            Some(h) => {
                if let Ok(n) = frame::refusal_reply(h, w.status(), w.errno(), &mut self.outbuf) {
                    self.send_out(n);
                }
            }
            None => self.reply_malformed(w),
        }
    }

    fn dispatch(&mut self, n: usize) {
        let h = match Header::decode_header(&self.inbuf[..n]) {
            Ok(h) => h,
            Err(e) => {
                self.stats.refused += 1;
                emit(&mut self.log, format_args!("[I915-LIVE frame refused name={} got_bytes={}]", e.name(), n));
                return self.reply_malformed(e);
            }
        };
        if !self.owns(&h) {
            return;
        }
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
        self.bind(&h, &done);
        self.finish_inline(h, done);
    }

    /// ONE CONNECTION, ONE SESSION, AND NOBODY ELSE'S: a connection opens at most one uAPI session at a time, and may name only that
    /// one. Incarnations are a counter, so another client could GUESS one; a frame naming a session that is not this connection's is
    /// answered as an unknown session (it neither confirms the other session exists nor touches it). Returns false when refused.
    fn owns(&mut self, h: &Header) -> bool {
        let mine = self.conns.get_mut(self.cur).and_then(|c| c.session);
        if h.opcode == Opcode::SessionOpen && mine.is_some() {
            self.stats.refused += 1;
            self.refuse(*h, Status::Ok, errno::EBUSY, "CONN-SESSION-EXISTS", mine.unwrap_or(0));
            return false;
        }
        if h.opcode != Opcode::SessionOpen && h.session_incarnation != 0 && mine != Some(h.session_incarnation) {
            self.stats.refused += 1;
            self.refuse(*h, Status::StaleSession, errno::ENODEV, "SESSION-NOT-ON-THIS-CONNECTION", h.session_incarnation);
            return false;
        }
        true
    }

    /// Record what the call did to the connection's session: a SESSION-OPEN that succeeded binds the new incarnation to it; a
    /// SESSION-CLOSE that ended it unbinds.
    fn bind(&mut self, h: &Header, done: &Result<Dispatched, WireError>) {
        let peer = self.t.peer(self.cur);
        let Some(c) = self.conns.get_mut(self.cur) else { return };
        match (h.opcode, done) {
            (Opcode::SessionOpen, Ok(Dispatched::Reply(rh, _))) if rh.session_incarnation != 0 => {
                c.session = Some(rh.session_incarnation);
                // who the session's client is: the recipient of its mapping leases
                if let Some(p) = peer {
                    self.svc.set_session_peer(rh.session_incarnation, p);
                }
            }
            (Opcode::SessionClose, _) => {
                if let Some(inc) = c.session {
                    if self.svc.session_idx(inc).is_none() {
                        c.session = None;
                    }
                }
            }
            _ => {}
        }
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
        if let Err(e) = self.regions.read(self.cur, rr.region, rr.offset, &mut req) {
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
        self.bind(&h, &done);
        match done {
            Ok(Dispatched::Reply(rh, len)) if len > self.inline_reply_max => {
                if len > rr.reply_cap as usize {
                    self.stats.refused += 1;
                    return self.refuse(h, Status::Ok, errno::EFAULT, "REPLY-TOO-LARGE", len as u64);
                }
                let area: &[u8] = if big.is_empty() { &self.outbuf[HEADER_BYTES..HEADER_BYTES + len] } else { &big[HEADER_BYTES..HEADER_BYTES + len] };
                if let Err(e) = self.regions.write(self.cur, rr.reply_region, rr.reply_offset, area) {
                    return self.refuse_region(h, e);
                }
                let placed = RegionRef { region: rr.reply_region, offset: rr.reply_offset, len: len as u32, reply_cap: rr.reply_cap, reply_offset: rr.reply_offset, reply_region: rr.reply_region };
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
        let cap = self.t.limits().max_frame_bytes() - HEADER_BYTES;
        match done {
            Ok(Dispatched::Reply(rh, len)) if len <= cap => match frame::finish(rh, len, &mut self.outbuf) {
                Ok(total) => self.send_out(total),
                Err(e) => self.reply_too_large(req, e, len),
            },
            // a reply that cannot travel as messages is refused by name, never truncated (an oversized reply to a REGION call took the reply-area path)
            Ok(Dispatched::Reply(_, len)) => self.reply_too_large(req, WireError::PayloadTooLarge { got: len.min(u32::MAX as usize) as u32, max: cap as u32 }, len),
            Ok(Dispatched::Parked) => {}
            Err(e) => {
                self.stats.refused += 1;
                emit(&mut self.log, format_args!("[I915-LIVE call refused name={}]", e.name()));
                self.reply_malformed(e);
            }
        }
    }

    fn reply_too_large(&mut self, req: Header, e: WireError, len: usize) {
        self.stats.refused += 1;
        emit(&mut self.log, format_args!("[I915-LIVE reply refused name=REPLY-TOO-LARGE wire={} payload_bytes={} max_inline={}]", e.name(), len, self.t.limits().max_frame_bytes() - HEADER_BYTES));
        self.refuse(req, Status::Ok, errno::EFAULT, "REPLY-TOO-LARGE", len as u64);
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
            RegionError::Revoked { region } => self.refuse(h, Status::Ok, errno::ENODEV, e.name(), region as u64),
            RegionError::NotYours { region } | RegionError::ReadOnly { region } => self.refuse(h, Status::Ok, errno::EACCES, e.name(), region as u64),
            RegionError::NoMemory { .. } => self.refuse(h, Status::Ok, errno::ENOMEM, e.name(), 0),
            RegionError::Refused { word, .. } => self.refuse(h, Status::Ok, errno::EIO, e.name(), word),
        }
    }

    // ---- replies ----------------------------------------------------------------------------------------------------------------

    /// Send the frame in `outbuf[..n]` to the connection being served.
    fn send_out(&mut self, n: usize) {
        let id = self.cur;
        let Some(c) = self.conns.get_mut(id) else { return };
        let q = c.send_frame(&mut self.t, &self.outbuf[..n]);
        self.settle(id, q);
    }

    /// Send one message that is not a frame (the first generation's answer) to the connection being served.
    fn send_message(&mut self, m: &[u8]) {
        let id = self.cur;
        let Some(c) = self.conns.get_mut(id) else { return };
        let q = c.send_message(&mut self.t, m);
        self.settle(id, q);
    }

    /// What became of a send: count it, hold the connection's requests when its ring was full (and keep the reply for a retry), reap it
    /// when it is dead.
    fn settle(&mut self, id: ConnId, q: Queued) {
        match q {
            Queued::Sent => self.stats.replies_sent += 1,
            Queued::Kept => self.conns.hold_kept(&mut self.t, id),
            Queued::Dead => self.reap(id, "reply-send"),
            Queued::Dropped(name) => {
                self.stats.replies_dropped += 1;
                emit(&mut self.log, format_args!("[I915-LIVE reply dropped name={}]", name));
            }
        }
    }

    /// Advance the device, then send everything that is ready: first the replies that did not fit last time, then parked calls that
    /// finished - each to the connection that owns the session it belongs to.
    fn advance(&mut self) {
        let now = self.t.now_ms();
        {
            let mut hw = Hw { ex: &mut self.ex, gm: &mut self.gm, log: &mut self.log, now_ms: now };
            self.svc.poll(&mut hw);
        }
        self.flush_all();
        while let Some(f) = self.svc.take_ready() {
            let owner = Header::decode_header(&f).ok().and_then(|h| self.conns.owner_of(h.session_incarnation));
            let Some(id) = owner else {
                self.stats.replies_dropped += 1;
                emit(&mut self.log, format_args!("[I915-LIVE reply dropped name=NO-CONNECTION]"));
                continue;
            };
            let Some(c) = self.conns.get_mut(id) else { continue };
            let q = c.send_frame(&mut self.t, &f);
            self.settle(id, q);
        }
    }

    fn flush_all(&mut self) {
        let mut count = FlushCount::default();
        while let Some(dead) = self.conns.flush_pending(&mut self.t, &mut count) {
            self.reap(dead, "reply-flush");
        }
        self.stats.replies_sent += count.sent;
        self.stats.replies_dropped += count.dropped;
    }
}
