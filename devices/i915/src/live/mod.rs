// SPDX-License-Identifier: GPL-2.0-only
//! The device's LIVE LOOP (feature `p3-exec`): the uAPI service ([`crate::uapi`]) answering on the device's real endpoints.
//!
//! | piece | what it is |
//! |---|---|
//! | [`crate::conn`] | the connections: the listen endpoint, one endpoint pair per client, the clock and the wait, behind a trait (the production binding is `conn::RtTransport`), and each connection's frame reassembly |
//! | [`regions`] | where a call too large for one inline frame is read from, and a large reply written to (`i915_wire::region`) |
//! | `serve` | [`Live`]: one `step` = one wake, handled; `run` = steps until the transport closes |
//! | [`assemble`] | bringing the service up from what the proof of life learned and what the host supplies |
//!
//! WHAT THE LOOP BLOCKS ON, and nothing else: a request on the request endpoint, the engine's interrupt (when one is armed), the next
//! deadline the service reports ([`crate::uapi::UapiService::next_deadline_ms`]: a parked timed wait, or - only while the engine has
//! submitted work in flight - its stall check and completion poll), and the peer's death. There is NO fixed-period poll: an idle device
//! passes `None` as its deadline and sleeps until a request arrives (doctrine: an idle wake-up is a bug).
//!
//! CONNECTIONS. The device's endpoint is a LISTEN endpoint: a client `connect()`s and the kernel mints it a private (request, event) pair,
//! so a reply goes to exactly the client that asked and a client's death is PEER_DEAD on ITS pair alone. A connection opens at most one
//! uAPI session and may name only that one (an incarnation is a counter, so another client could guess it; the loop refuses it by name).
//!
//! FRAMES. The kernel's message is 4096 bytes, so a frame travels as consecutive messages that end where its header says
//! ([`i915_wire::frag`], the one definition both ends compile): per connection a bounded reassembly buffer (reserved at the first message,
//! sized by the header, which the wire has already bounded), nothing executes until the frame is whole, and a short, overlong or interleaved
//! sequence is refused by name. A call too large to fragment (more than 15 messages) travels through a payload region (ABI 1.2). The loop
//! answers a refused or malformed frame with a frame, never silence.
//!
//! PEER DEATH. When a connection's client dies - seen on a receive or a reply send - its session is `session_dead`, which frees everything it
//! held (requests still on the engine finish first; their objects are freed when they retire), and the endpoint pair goes back to the kernel.
//! Every other connection is untouched.
pub mod assemble;
pub mod regions;
mod serve;

pub use crate::conn::{Accepted, ConnId, Recv, SendOutcome, Transport, Wake, REPLY_RETRY_MS};
pub use regions::{NoRegions, PayloadRegions, RegionError};
pub use serve::{Live, LiveError, LiveStats, Step};
