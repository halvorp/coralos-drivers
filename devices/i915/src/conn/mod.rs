// SPDX-License-Identifier: GPL-2.0-only
//! CONNECTIONS: how the device talks to its clients. One client = one `connect()` = one [`ConnId`], with its own pair of endpoints, its
//! own reassembly of fragmented frames, its own queue of replies the client's ring had no room for, and at most one uAPI session.
//!
//! | piece | what it is |
//! |---|---|
//! | [`transport`] | the kernel as the device sees it: a LISTEN endpoint that announces connections, a request/event endpoint pair per connection, a clock and ONE blocking wait (the production binding is `rt_transport::RtTransport`) |
//! | [`table`] | the connection table (growable; a row is reserved BEFORE a connect is accepted) and each connection's inbound reassembly and outbound queue |
//!
//! WHY NOT ONE STATIC PAIR. A static request/reply pair cannot route a reply to the process that asked: every client would read every
//! reply. A client `connect()`s to the LISTEN endpoint and the kernel mints it a private (req, evt) pair from the dynamic pool, so a
//! reply goes to exactly the connection whose request it answers, and a client's death is visible as PEER_DEAD on ITS pair alone.
//!
//! FRAMES. The kernel's message is 4096 bytes (`send` truncates a longer one) and a ring holds 16 (a full ring drops the newest), so a
//! frame travels as consecutive messages that end where its own header says: `i915_wire::frag`, the one definition both ends compile.
//! Nothing executes until a frame is whole, and a short, overlong or interleaved sequence is a NAMED refusal.
pub mod parked;
pub mod table;
pub mod transport;

#[cfg(target_arch = "x86_64")]
mod rt_transport;
#[cfg(target_arch = "x86_64")]
pub use rt_transport::RtTransport;

pub use table::{Conn, ConnTable, Flush, FlushCount, Pulled, Queued};
/// Milliseconds before a reply the client's ring had no room for is tried again. A timer exists only while a reply is unsent.
pub const REPLY_RETRY_MS: u64 = 2;

pub use parked::Parked;
pub use transport::{Accepted, ConnId, Recv, SendOutcome, Transport, Wake};
