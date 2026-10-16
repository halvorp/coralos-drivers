// SPDX-License-Identifier: GPL-2.0-only
//! The device's uAPI service: the i915 calls of ABI 1.1 ([`i915_wire::msg`]) answered from the port crates.
//!
//! PLACEMENT (KBL-I915-PORT-PLAN section 2): this GPL device owns ALL device semantics - GEM handles and their incarnations, CPU
//! mapping leases, PRIME grants, syncobj state, GGTT/PPGTT, relocation patching and softpin validation. The closed shim only adapts
//! Mesa's ioctls to the FLATTENED, pointer-free wire (`i915-wire`) and back. Nothing here knows a Mesa pointer, an fd number or a
//! process address.
//!
//! | piece | what it does |
//! |---|---|
//! | [`frame`] | frame-level answers that need no service: malformed frames and the `Unsupported (increment not enabled)` reply |
//! | `service` (feature `p3-exec`) | [`UapiService`]: sessions, dispatch, deferred waits, polling, session death |
//! | `calls_display` | the display AUTHORITY's calls (D18): the one mode, the scanout pair and its flips |
//! | `calls_*`, `exec_call` | one handler per opcode: decode (wire) -> validate (uapi-core) -> plan (gem-core) -> execute (exec/) |
//! | `adapt` | the seams gem-core plans against, over the executor |
//! | `pool` | page recycling, and giving idle grants back to the host where `Dma::free` exists |
//! | `mmap_lease`, `kernel_lease` | GEM CPU mapping through the kernel's DMA lease: when one is minted, revoked, and when its pages may be reused |
//! | `fx` | applying the planner's effects; hardware contexts and address-space pages |
//!
//! The loop that serves these calls on the device's endpoints is [`crate::live`] (same feature).
//!
//! INCREMENT GATING: everything but [`frame`] is compiled only with `p3-exec`. With it OFF, [`frame::answer_without_service`] answers
//! every uAPI request `Unsupported` with the reason `INCREMENT-NOT-ENABLED`, and the default build is unchanged: proof of life and the
//! forcewake writes, nothing else.
pub mod frame;

#[cfg(feature = "p3-exec")]
mod adapt;
#[cfg(feature = "p3-exec")]
mod calls_ctx;
#[cfg(feature = "p3-exec")]
mod calls_display;
#[cfg(feature = "p4-present")]
mod calls_scanout;
#[cfg(feature = "p3-exec")]
mod calls_gem;
#[cfg(feature = "p3-exec")]
mod calls_sync;
#[cfg(feature = "p3-exec")]
mod exec_call;
#[cfg(feature = "p3-exec")]
mod fail;
#[cfg(feature = "p3-exec")]
mod fx;
#[cfg(feature = "p3-exec")]
pub mod kernel_lease;
#[cfg(feature = "p3-exec")]
mod mmap_lease;
#[cfg(feature = "p3-exec")]
pub mod pool;
#[cfg(feature = "p3-exec")]
mod service;
#[cfg(all(feature = "p3-exec", target_arch = "x86_64"))]
pub mod rt_lease;

#[cfg(feature = "p3-exec")]
pub use adapt::ObjTable;
#[cfg(feature = "p3-exec")]
pub use fail::Fail;
#[cfg(feature = "p3-exec")]
pub use service::{bring_up_render, Dispatched, Handled, Hw, RenderConfig, UapiService, COMPLETION_POLL_MS};
#[cfg(feature = "p3-exec")]
pub(crate) use i915_wire::errno;
