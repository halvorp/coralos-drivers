// SPDX-License-Identifier: GPL-2.0-only
//! The PARKED service: every request is answered, none is served (the shipped default build, and what the uAPI loop falls back to being
//! when its increment is off). It speaks to clients exactly as the live loop does - `connect()` sessions on the LISTEN endpoint, frames
//! reassembled from messages by `i915_wire::frag`, replies split the same way - so the CoralOS shim sees one device whichever build runs.
//! A frame is answered only once it is whole; a bad sequence is refused by name; a dead client's connection is released.
use super::table::{ConnTable, FlushCount, Pulled, Queued};
use super::transport::{Accepted, ConnId, Transport, Wake};
use super::REPLY_RETRY_MS;
use crate::log::{emit, Log};
use crate::uapi::frame;
use alloc::vec::Vec;
use i915_wire::frag::Joiner;
use i915_wire::{WireError, HEADER_BYTES, MAX_FRAME_BYTES};

/// What the parked service did, for the tests and the log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParkedStats {
    pub accepted: u64,
    pub reaped: u64,
    /// Frames (and first-generation messages) answered.
    pub answered: u64,
    /// Sequences refused by name.
    pub refused: u64,
    pub replies_dropped: u64,
}

pub struct Parked<T: Transport, L: Log> {
    pub t: T,
    pub log: L,
    pub conns: ConnTable,
    pub stats: ParkedStats,
    msgbuf: Vec<u8>,
    inbuf: Vec<u8>,
    outbuf: Vec<u8>,
}

fn buf(len: usize) -> Option<Vec<u8>> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).ok()?;
    v.resize(len, 0);
    Some(v)
}

impl<T: Transport, L: Log> Parked<T, L> {
    /// `None` = the heap could not give the buffers (the device does not start half-able to read a frame).
    pub fn new(t: T, log: L) -> Option<Self> {
        let cap = t.limits().msg_cap;
        Some(Parked { t, log, conns: ConnTable::new(), stats: ParkedStats::default(), msgbuf: buf(cap)?, inbuf: buf(MAX_FRAME_BYTES)?, outbuf: buf(MAX_FRAME_BYTES)? })
    }

    /// Serve until the transport closes.
    pub fn run(&mut self) {
        while self.step() != Wake::Closed {}
    }

    /// One wake, handled. Blocks on a connect, a message, or - only while a reply is unsent - the retry timer; never on a fixed poll.
    pub fn step(&mut self) -> Wake {
        let room = self.conns.reserve_one() && self.t.make_room();
        let deadline = self.conns.any_pending().then(|| self.t.now_ms().saturating_add(REPLY_RETRY_MS));
        let wake = self.t.wait(deadline, room);
        match wake {
            Wake::Accept => self.accept_one(),
            Wake::Request(id) => self.serve(id),
            _ => {}
        }
        self.flush();
        wake
    }

    fn accept_one(&mut self) {
        match self.t.accept() {
            Accepted::Conn(id) => {
                if self.conns.admit(id, Joiner::new(self.t.limits())) {
                    self.stats.accepted += 1;
                } else {
                    self.t.release(id);
                }
            }
            Accepted::Empty => {}
            Accepted::Refused(name) => emit(&mut self.log, format_args!("[I915-PARKED connect refused name={}]", name)),
        }
    }

    fn reap(&mut self, id: ConnId) {
        if self.conns.remove(id).is_some() {
            self.t.release(id);
            self.stats.reaped += 1;
        }
    }

    fn serve(&mut self, id: ConnId) {
        let Some(c) = self.conns.get_mut(id) else { return };
        let answer = match c.pull(&mut self.t, &mut self.msgbuf, &mut self.inbuf) {
            Pulled::Frame(n) => Some((true, frame::answer_without_service(&self.inbuf[..n], &mut self.outbuf))),
            Pulled::Legacy(n) => crate::wire::handle(n as u64, &self.inbuf).map(|r| {
                self.outbuf[..r.len()].copy_from_slice(&r);
                (false, Ok(r.len()))
            }),
            Pulled::Refused { err, head } => {
                self.stats.refused += 1;
                emit(&mut self.log, format_args!("[I915-PARKED frame refused name={}]", err.name()));
                let w = err.wire();
                Some((true, match head {
                    Some(h) => frame::refusal_reply(h, w.status(), w.errno(), &mut self.outbuf),
                    None => frame::malformed_reply(w, &mut self.outbuf),
                }))
            }
            Pulled::NoMemory { need, .. } => {
                self.stats.refused += 1;
                Some((true, frame::malformed_reply(WireError::LimitExceeded { what: "frame.memory", got: need as u64, max: 0 }, &mut self.outbuf)))
            }
            Pulled::Oversized(got) => {
                self.stats.refused += 1;
                let e = WireError::PayloadTooLarge { got: got.saturating_sub(HEADER_BYTES as u64).min(u32::MAX as u64) as u32, max: i915_wire::MAX_PAYLOAD };
                Some((true, frame::malformed_reply(e, &mut self.outbuf)))
            }
            Pulled::Dead => {
                self.reap(id);
                return;
            }
            Pulled::Partial | Pulled::Idle | Pulled::Error(_) => None,
        };
        let Some((is_frame, Ok(n))) = answer else { return };
        self.stats.answered += 1;
        let Some(c) = self.conns.get_mut(id) else { return };
        let q = if is_frame { c.send_frame(&mut self.t, &self.outbuf[..n]) } else { c.send_message(&mut self.t, &self.outbuf[..n]) };
        match q {
            Queued::Sent => {}
            Queued::Kept => self.conns.hold_kept(&mut self.t, id),
            Queued::Dead => self.reap(id),
            Queued::Dropped(name) => {
                self.stats.replies_dropped += 1;
                emit(&mut self.log, format_args!("[I915-PARKED reply dropped name={}]", name));
            }
        }
    }

    fn flush(&mut self) {
        let mut count = FlushCount::default();
        while let Some(dead) = self.conns.flush_pending(&mut self.t, &mut count) {
            self.reap(dead);
        }
        self.stats.replies_dropped += count.dropped;
    }
}
