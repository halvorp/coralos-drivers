// SPDX-License-Identifier: GPL-2.0-only
//! i915-syncobj-core: `drm_syncobj` semantics as a PURE module - binary and timeline (chain) syncobjs, session-scoped handle tables,
//! wait / signal / reset / query / transfer, sync_file import and export, cross-session export tokens - with NO clock, NO blocking and
//! NO hardware. `devices/i915` owns the clock and the wake-ups: a wait here is an EVALUATION ([`WaitEval::Done`] or
//! [`WaitEval::Pending`]) that the device re-runs when a fence signals or a syncobj changes, and times out on its own clock.
//!
//! Ported from Linux drivers/gpu/drm/drm_syncobj.c and drivers/dma-buf/dma-fence-chain.c (v7.0-rc6).
//!
//! | module | Linux | what it gives a consumer |
//! |---|---|---|
//! | [`fence`] | dma-buf/dma-fence.c, dma-fence-unwrap.c | pending / signalled(error) / merged fences, explicit `get` / `put` |
//! | [`syncobj`] | drm_syncobj.c:333-520, dma-fence-chain.c:90-275 | replace_fence, add_point, find_fence, query |
//! | [`session`] | drm_syncobj.c:554-1738 | per-session handle tables and the ioctl-level operations |
//!
//! WHO SIGNALS: the device creates one pending fence per request ([`Sync::new_fence`]), attaches it to the out-syncobjs of the
//! submission ([`Sync::attach_out`], `signal_fence_array`, gem/i915_gem_execbuffer.c:3018-3045), and signals it from request
//! retirement ([`Sync::signal_fence`]). A hung request is signalled with `-EIO`.
//!
//! TABLES (RUNTIME-TABLES-HAVE-NO-CAP): sessions, handles, syncobjs, fences, chain points, sync_files and export tokens are all
//! growable with FALLIBLE growth and the named refusal `SYNC-NOMEM`; [`Sync::set_budget`] is the per-device quota and the failure
//! injector. Nothing here is sized by a constant. `alloc` is unavoidable and justified by that ruling (PORT_CONVENTIONS.md section 7).

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod error;
pub mod fence;
pub mod session;
pub mod slab;
pub mod syncobj;

pub use error::SyncError;
pub use fence::{FenceId, FenceStatus, Fences};
pub use session::{Sync, WaitEval};
pub use slab::Budget;
pub use syncobj::{
    SyncobjId, CREATE_SIGNALED, FD_TO_HANDLE_IMPORT_SYNC_FILE, FD_TO_HANDLE_TIMELINE, HANDLE_TO_FD_EXPORT_SYNC_FILE, HANDLE_TO_FD_TIMELINE, QUERY_LAST_SUBMITTED, WAIT_ALL,
    WAIT_AVAILABLE, WAIT_DEADLINE, WAIT_FOR_SUBMIT,
};
