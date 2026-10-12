// SPDX-License-Identifier: GPL-2.0-only
//! The live loop's view of the kernel: an endpoint pair, a millisecond clock and ONE blocking wait.

/// What ended a wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    /// A request is queued on the request endpoint.
    Request,
    /// The engine interrupt line fired.
    Interrupt,
    /// The deadline passed.
    Deadline,
    /// The peer is gone.
    PeerDead,
    /// The transport is being torn down (tests; the production binding never returns it).
    Closed,
}

/// What a receive produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recv {
    /// A frame of this many bytes is in the buffer.
    Frame(usize),
    /// Nothing queued (a spurious wake).
    Empty,
    /// A frame longer than the buffer; carries its length. Nothing of it is usable.
    Oversized(u64),
    PeerDead,
    /// The kernel refused the receive; carries its word.
    Error(u64),
}

/// What a reply send produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    /// The reply ring is full: the peer is not draining. The reply is kept and retried.
    RingFull,
    PeerDead,
    /// The send was denied by capability (a defect of the endowment): the reply is dropped, loudly.
    Denied,
}

pub trait Transport {
    /// Monotonic milliseconds.
    fn now_ms(&mut self) -> u64;
    /// Is an engine interrupt armed (so a completion wakes the loop by itself and needs no timer)?
    fn irq_armed(&self) -> bool;
    /// Block until a request is queued (only when `accept_requests`), the engine interrupts, the ABSOLUTE `deadline_ms` has passed
    /// (`None` = no deadline: block until something else happens), or the peer dies. A deadline already past returns at once.
    /// `accept_requests = false` is the back-pressure of an unsent reply: no new request is taken until it is out.
    fn wait(&mut self, deadline_ms: Option<u64>, accept_requests: bool) -> Wake;
    /// Take the next queued request into `buf` without blocking.
    fn recv(&mut self, buf: &mut [u8]) -> Recv;
    /// Send one reply frame.
    fn send(&mut self, frame: &[u8]) -> SendOutcome;
}
