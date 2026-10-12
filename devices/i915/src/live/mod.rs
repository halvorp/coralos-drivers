// SPDX-License-Identifier: GPL-2.0-only
//! The device's LIVE LOOP (feature `p3-exec`): the uAPI service ([`crate::uapi`]) answering on the device's real endpoints.
//!
//! | piece | what it is |
//! |---|---|
//! | [`transport`] | the endpoint pair, the clock and the wait, behind a trait (the production binding is `service::RtTransport`) |
//! | [`regions`] | where a call too large for one inline frame is read from, and a large reply written to (`i915_wire::region`) |
//! | `serve` | [`Live`]: one `step` = one wake, handled; `run` = steps until the transport closes |
//! | [`assemble`] | bringing the service up from what the proof of life learned and what the host supplies |
//!
//! WHAT THE LOOP BLOCKS ON, and nothing else: a request on the request endpoint, the engine's interrupt (when one is armed), the next
//! deadline the service reports ([`crate::uapi::UapiService::next_deadline_ms`]: a parked timed wait, or - only while the engine has
//! submitted work in flight - its stall check and completion poll), and the peer's death. There is NO fixed-period poll: an idle device
//! passes `None` as its deadline and sleeps until a request arrives (doctrine: an idle wake-up is a bug).
//!
//! FRAMES. The receive buffer is the largest inline frame there is ([`i915_wire::MAX_FRAME_BYTES`], header plus 64 KiB), never the 64
//! bytes of the first skeleton. A frame the receive path reports larger than that is refused by name (`WIRE-PAYLOAD-TOO-LARGE`, status
//! Malformed) and the call must travel through a payload region instead (ABI 1.2). The loop answers a refused or malformed frame with a
//! frame, never silence.
//!
//! PEER DEATH. The endpoint pair is one connection (the CoralOS shim's): when the transport reports the peer dead - on a wait, a receive or a
//! reply send - every session of that connection is `session_dead`, which frees everything it held (requests still on the engine finish
//! first; their objects are freed when they retire).
pub mod assemble;
pub mod regions;
mod serve;
pub mod transport;

pub use regions::{NoRegions, PayloadRegions, RegionError};
pub use serve::{Live, LiveError, LiveStats, Step, REPLY_RETRY_MS};
pub use transport::{Recv, SendOutcome, Transport, Wake};
