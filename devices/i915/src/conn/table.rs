// SPDX-License-Identifier: GPL-2.0-only
//! The connection table and what each connection owns: the reassembly of its inbound frames and the queue of its outbound ones.
//!
//! GROWTH (doctrine RUNTIME-TABLES-HAVE-NO-CAP; DECISIONS D10): the table is a `Vec` grown by `try_reserve` BEFORE a connect is
//! accepted ([`ConnTable::reserve_one`]); it has no compile-time size. A reassembly buffer is reserved exactly (`try_reserve_exact`) at the
//! first message of a frame, whose declared length is already validated against the wire's own frame bound (`i915_wire::frag`); when the
//! heap refuses it the frame is DISCARDED BY NAME (its remaining messages are skipped, not misread as new frames).
use super::transport::{ConnId, Recv, SendOutcome, Transport};
use alloc::vec::Vec;
use coralos_rt::session::is_tombstone;
use i915_wire::frag::{split, Feed, FragError, Joiner};
use i915_wire::{Header, MAGIC};

/// A reply (or any outbound frame) the client's ring had no room for, and how far through its messages it got.
struct Waiting {
    bytes: Vec<u8>,
    /// Messages already sent.
    next: usize,
}

/// Where a `send_frame` left the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Queued {
    /// Every message is in the client's ring.
    Sent,
    /// The ring was full part-way (or a frame was already waiting): the rest is kept and retried.
    Kept,
    /// The client is gone.
    Dead,
    /// The frame cannot be sent and is dropped, by name: it is not whole (fragmentation refused it), the send was denied, or the heap
    /// had no room to keep it.
    Dropped(&'static str),
}

/// Where a `flush` left the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flush {
    Empty,
    /// The ring is full again; the rest waits.
    Pending,
    Dead,
}

/// What a flush did, for the loop's counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlushCount {
    /// Frames now completely in the client's ring.
    pub sent: u64,
    /// Frames given up on (a denied send).
    pub dropped: u64,
}

/// What pulling one message off a connection produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pulled {
    /// A WHOLE frame of this many bytes is in the `frame` buffer.
    Frame(usize),
    /// A first-generation message (no I915 magic) of this many bytes is in the `frame` buffer; it travels as one message.
    Legacy(usize),
    /// Part of a frame was kept; nothing to execute yet.
    Partial,
    /// Nothing for the loop to do: an empty wake, the kernel's end-of-session tombstone, or a skipped fragment of a discarded frame.
    Idle,
    /// A sequence that is not a frame, refused by name. `head` is the header of the frame it broke, when one had begun (so the refusal
    /// can answer THAT request instead of leaving the client waiting).
    Refused { err: FragError, head: Option<Header> },
    /// The heap could not hold the frame being received; it is discarded. `head` as above.
    NoMemory { need: usize, head: Option<Header> },
    /// A message longer than the transport can deliver (carries its length).
    Oversized(u64),
    /// The client is gone.
    Dead,
    /// The kernel refused the receive; carries its word.
    Error(u64),
}

/// One client.
pub struct Conn {
    pub id: ConnId,
    /// The uAPI session this connection opened (its incarnation), if it has one. One connection, one session.
    pub session: Option<u64>,
    join: Joiner,
    rx: Vec<u8>,
    /// The header of the frame being received.
    head: Option<Header>,
    /// The frame being received did not fit the heap: its messages are skipped until it ends.
    discarding: bool,
    out: Vec<Waiting>,
    /// The transport has been told to stop taking this connection's requests (its replies are backed up).
    pub held: bool,
}

impl Conn {
    fn new(id: ConnId, join: Joiner) -> Conn {
        Conn { id, session: None, join, rx: Vec::new(), head: None, discarding: false, out: Vec::new(), held: false }
    }

    /// Does the connection hold replies the client's ring had no room for?
    pub fn has_pending(&self) -> bool {
        !self.out.is_empty()
    }

    /// Take the next message of this connection and advance its reassembly. `msg` is the per-message buffer (the transport's message
    /// size); `frame` receives a whole frame (at least the wire's largest fragmentable frame).
    pub fn pull<T: Transport>(&mut self, t: &mut T, msg: &mut [u8], frame: &mut [u8]) -> Pulled {
        let n = match t.recv(self.id, msg) {
            Recv::Msg(n) => n,
            Recv::Empty => return Pulled::Idle,
            Recv::Oversized(got) => return Pulled::Oversized(got),
            Recv::PeerDead => return Pulled::Dead,
            Recv::Error(w) => return Pulled::Error(w),
        };
        let m = &msg[..n];
        if !self.join.in_progress() {
            if is_tombstone(m) {
                return Pulled::Idle; // the kernel's end-of-session mark; the PEER_DEAD that follows reaps the connection
            }
            if n < 4 || m[..4] != MAGIC.to_le_bytes() {
                if n > frame.len() {
                    return Pulled::Oversized(n as u64);
                }
                frame[..n].copy_from_slice(m);
                return Pulled::Legacy(n);
            }
        }
        match self.join.feed(m) {
            Err(err) => {
                let head = self.head.take();
                self.rx = Vec::new();
                self.discarding = false;
                Pulled::Refused { err, head }
            }
            Ok(Feed::Whole) => {
                if n > frame.len() {
                    return Pulled::Oversized(n as u64);
                }
                frame[..n].copy_from_slice(m);
                Pulled::Frame(n)
            }
            Ok(Feed::Begin { total }) => {
                self.head = Header::decode_header(m).ok();
                let mut rx = Vec::new();
                if total > frame.len() || rx.try_reserve_exact(total).is_err() {
                    self.discarding = true;
                    return Pulled::NoMemory { need: total, head: self.head };
                }
                rx.extend_from_slice(m);
                self.rx = rx;
                Pulled::Partial
            }
            Ok(Feed::Middle) => {
                if self.discarding {
                    return Pulled::Idle;
                }
                self.rx.extend_from_slice(m);
                Pulled::Partial
            }
            Ok(Feed::End) => {
                self.head = None;
                if self.discarding {
                    self.discarding = false;
                    return Pulled::Idle;
                }
                self.rx.extend_from_slice(m);
                let total = self.rx.len();
                frame[..total].copy_from_slice(&self.rx);
                self.rx = Vec::new();
                Pulled::Frame(total)
            }
        }
    }

    /// Send a whole frame to the client as the messages `i915_wire::frag::split` cuts it into. If the client's ring fills part-way, or a
    /// frame is already waiting (order is kept), the rest is kept for [`Conn::flush`].
    pub fn send_frame<T: Transport>(&mut self, t: &mut T, frame: &[u8]) -> Queued {
        let limits = t.limits();
        let chunks = match split(frame, limits) {
            Ok(c) => c,
            Err(e) => return Queued::Dropped(e.name()),
        };
        if !self.out.is_empty() {
            return self.keep(frame, 0);
        }
        for (i, m) in chunks.enumerate() {
            match t.send(self.id, m) {
                SendOutcome::Sent => {}
                SendOutcome::RingFull => return self.keep(frame, i),
                SendOutcome::PeerDead => return Queued::Dead,
                SendOutcome::Denied => return Queued::Dropped("SEND-DENIED"),
            }
        }
        Queued::Sent
    }

    /// Send ONE message that is not an i915-wire frame (the first generation's four-byte answer), in order with the frames.
    pub fn send_message<T: Transport>(&mut self, t: &mut T, msg: &[u8]) -> Queued {
        if msg.len() > t.limits().msg_cap {
            return Queued::Dropped("MESSAGE-TOO-LONG");
        }
        if !self.out.is_empty() {
            return self.keep(msg, 0);
        }
        match t.send(self.id, msg) {
            SendOutcome::Sent => Queued::Sent,
            SendOutcome::RingFull => self.keep(msg, 0),
            SendOutcome::PeerDead => Queued::Dead,
            SendOutcome::Denied => Queued::Dropped("SEND-DENIED"),
        }
    }

    fn keep(&mut self, frame: &[u8], next: usize) -> Queued {
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(frame.len()).is_err() || self.out.try_reserve(1).is_err() {
            return Queued::Dropped("REPLY-NOMEM");
        }
        bytes.extend_from_slice(frame);
        self.out.push(Waiting { bytes, next });
        Queued::Kept
    }

    /// Send what the ring had no room for, oldest first, and say where the queue stands.
    pub fn flush<T: Transport>(&mut self, t: &mut T, count: &mut FlushCount) -> Flush {
        let cap = t.limits().msg_cap.max(1);
        while let Some(q) = self.out.first_mut() {
            let mut denied = false;
            for m in q.bytes.chunks(cap).skip(q.next) {
                match t.send(self.id, m) {
                    SendOutcome::Sent => q.next += 1,
                    SendOutcome::RingFull => return Flush::Pending,
                    SendOutcome::PeerDead => return Flush::Dead,
                    SendOutcome::Denied => {
                        denied = true;
                        break;
                    }
                }
            }
            if denied {
                count.dropped += 1;
            } else {
                count.sent += 1;
            }
            self.out.remove(0);
        }
        Flush::Empty
    }
}

/// Every live connection.
#[derive(Default)]
pub struct ConnTable {
    conns: Vec<Conn>,
}

impl ConnTable {
    pub fn new() -> ConnTable {
        ConnTable { conns: Vec::new() }
    }

    /// Room for one more row. `false` = the heap refused (the loop then stops accepting, by name, before any session exists).
    pub fn reserve_one(&mut self) -> bool {
        self.conns.try_reserve(1).is_ok()
    }

    /// Enter a connection. The row was reserved by [`ConnTable::reserve_one`]; if it was not, this refuses rather than growing blindly.
    pub fn admit(&mut self, id: ConnId, join: Joiner) -> bool {
        if self.conns.len() == self.conns.capacity() {
            return false;
        }
        self.conns.push(Conn::new(id, join));
        true
    }

    pub fn len(&self) -> usize {
        self.conns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.conns.is_empty()
    }

    pub fn at_mut(&mut self, i: usize) -> Option<&mut Conn> {
        self.conns.get_mut(i)
    }

    pub fn get_mut(&mut self, id: ConnId) -> Option<&mut Conn> {
        self.conns.iter_mut().find(|c| c.id == id)
    }

    pub fn remove(&mut self, id: ConnId) -> Option<Conn> {
        let i = self.conns.iter().position(|c| c.id == id)?;
        Some(self.conns.remove(i))
    }

    /// The connection that owns uAPI session `inc`.
    pub fn owner_of(&self, inc: u64) -> Option<ConnId> {
        self.conns.iter().find(|c| c.session == Some(inc)).map(|c| c.id)
    }

    pub fn ids(&self) -> impl Iterator<Item = ConnId> + '_ {
        self.conns.iter().map(|c| c.id)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Conn> {
        self.conns.iter_mut()
    }

    /// A send left a frame waiting on `id`: stop taking its requests until the ring drains (every other connection is served meanwhile).
    pub fn hold_kept<T: Transport>(&mut self, t: &mut T, id: ConnId) {
        if let Some(c) = self.get_mut(id) {
            if !c.held {
                c.held = true;
                t.hold(id, true);
            }
        }
    }

    /// Flush every connection's waiting replies and resume the requests of those that drained. Returns the first connection whose client
    /// turned out to be dead (the caller reaps it and calls again); `None` = every queue is flushed as far as the rings allow.
    pub fn flush_pending<T: Transport>(&mut self, t: &mut T, count: &mut FlushCount) -> Option<ConnId> {
        for c in self.conns.iter_mut().filter(|c| c.has_pending()) {
            match c.flush(t, count) {
                Flush::Pending => {}
                Flush::Dead => return Some(c.id),
                Flush::Empty => {
                    if c.held {
                        c.held = false;
                        t.hold(c.id, false);
                    }
                }
            }
        }
        None
    }

    /// Does any connection hold unsent replies?
    pub fn any_pending(&self) -> bool {
        self.conns.iter().any(|c| c.has_pending())
    }
}
