// SPDX-License-Identifier: GPL-2.0-only
//! The device's view of the kernel: a listen endpoint, a (request, event) endpoint pair per connection, a millisecond clock and ONE
//! blocking wait.
use i915_wire::frag::Limits;

/// One accepted `connect()`. Minted by the transport and never reused within a run (a counter, not an endpoint number), so a reply or a
/// held flag can never reach the connection that took a dead one's endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnId(pub u64);

/// Who a connection's client is, as the kernel's connect announce named it: what a lease needs to name its RECIPIENT. Opaque to the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer {
    /// The session's request endpoint (this task holds RECV on it).
    pub req_ep: u16,
    /// The client's node (a packed task reference: slot and generation).
    pub node: u64,
}

/// What ended a wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    /// A connect announce is queued on the listen endpoint (only when the wait was told to accept).
    Accept,
    /// A message (or the death mark) is queued on this connection's request endpoint.
    Request(ConnId),
    /// The engine interrupt line fired.
    Interrupt,
    /// The deadline passed.
    Deadline,
    /// The transport is being torn down (tests; the production binding never returns it).
    Closed,
}

/// What taking one announce produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accepted {
    Conn(ConnId),
    /// Nothing queued (a spurious wake).
    Empty,
    /// The announce was not usable, or the session could not be tracked; carries the name. Nothing was admitted.
    Refused(&'static str),
}

/// What a receive produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recv {
    /// A message of this many bytes is in the buffer.
    Msg(usize),
    /// Nothing queued (a spurious wake).
    Empty,
    /// A message longer than the buffer; carries its length. Nothing of it is usable.
    Oversized(u64),
    /// The client is gone (the kernel's PEER_DEAD mark on its request endpoint).
    PeerDead,
    /// The kernel refused the receive; carries its word.
    Error(u64),
}

/// What sending one message produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    /// The connection's event ring is full: the client is not draining. The message is kept and retried.
    RingFull,
    PeerDead,
    /// The send was denied by capability (a defect of the endowment): the message is dropped, loudly.
    Denied,
}

pub trait Transport {
    /// The message geometry of this transport (the kernel's, unless a test shrinks it).
    fn limits(&self) -> Limits {
        Limits::KERNEL
    }
    /// Monotonic milliseconds.
    fn now_ms(&mut self) -> u64;
    /// Is an engine interrupt armed (so a completion wakes the loop by itself and needs no timer)?
    fn irq_armed(&self) -> bool;
    /// Make room for ONE more connection in the transport's own table (fallible growth). `false` = out of memory: the loop then stops
    /// listening, so the announce stays queued and the kernel refuses the connect when the listen ring fills - a named refusal BEFORE a
    /// session exists, never a session the device cannot track.
    fn make_room(&mut self) -> bool;
    /// Block until a connect announce is queued (only when `accept`), a connection that is not held has a message or a death mark queued,
    /// the engine interrupts, the ABSOLUTE `deadline_ms` has passed (`None` = no deadline: block until something else happens). A
    /// deadline already past returns at once.
    fn wait(&mut self, deadline_ms: Option<u64>, accept: bool) -> Wake;
    /// Take the next connect announce without blocking.
    fn accept(&mut self) -> Accepted;
    /// Stop (`held`) or resume taking requests from `conn`: the back-pressure of a reply the client's ring had no room for. A held
    /// connection's messages wait in its own ring; every other connection is served meanwhile.
    fn hold(&mut self, conn: ConnId, held: bool);
    /// Take the next queued message of `conn` into `buf` without blocking.
    fn recv(&mut self, conn: ConnId, buf: &mut [u8]) -> Recv;
    /// Send one message to `conn`.
    fn send(&mut self, conn: ConnId, msg: &[u8]) -> SendOutcome;
    /// The client of `conn` as the announce named it. `None` for a transport that does not know (the parked loop never needs it).
    fn peer(&self, _conn: ConnId) -> Option<Peer> {
        None
    }
    /// The connection is over (its client died or the device closed it): forget it and give its endpoints back. `false` = the kernel
    /// refused the release (the pair stays held until this task exits); the connection is forgotten either way.
    fn release(&mut self, conn: ConnId) -> bool;
}
