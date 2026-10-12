// SPDX-License-Identifier: GPL-2.0-only
//! i915-uapi-core: the i915 userspace ABI for a Gen9 device, over the FLATTENED `i915.device` wire form.
//!
//! Ported from Linux include/uapi/drm/i915_drm.h, drivers/gpu/drm/i915/{i915_getparam.c, i915_query.c, i915_user_extensions.c,
//! gem/i915_gem_execbuffer.c} and the engine / SSEU facts they read (gt/intel_engine_user.c, gt/intel_sseu.c), at v7.0-rc6.
//! Original copyright: Intel Corporation and the i915 authors.
//!
//! WHAT THIS CRATE IS. The closed CoralOS shim flattens a Mesa ioctl into one pointer-free payload (KBL-I915-PORT-PLAN section 2);
//! the public `i915.device` decodes it here, and every decision Linux would make before it touches hardware is made here, with the
//! Linux errno it would return, as a NAMED refusal:
//! - [`consts`] / [`types`] / [`gem_types`]: the uAPI constants and data types (consts are generated from the header, with lines);
//! - [`getparam`]: `DRM_IOCTL_I915_GETPARAM` for Gen9, every value derived from [`UapiDevice`] (the `PlatformDescriptor` plus
//!   measured facts), never typed in;
//! - [`query`]: `DRM_IOCTL_I915_QUERY` (topology, engine info, memory regions; the rest answer what a Gen9 execlists device answers);
//! - [`execbuf_wire`] + [`execbuf`]: the bounded, transactional decode and the validation of `DRM_IOCTL_I915_GEM_EXECBUFFER2`.
//!
//! WHAT IT IS NOT. No MMIO, no allocation, no execution: relocation patching, pinning, request building and fence plumbing belong
//! to `i915-gem-core` / `i915-submit-core`, which consume the [`ExecPlan`] this crate returns. GEM object, context and VM
//! *semantics*, syncobj and PRIME state, userptr (refused until revocable pins exist) and i915 perf are out of scope here.

#![no_std]
#![forbid(unsafe_code)]

pub mod codec;
pub mod consts;
pub mod device;
pub mod errno;
pub mod error;
pub mod execbuf;
pub mod execbuf_wire;
pub mod gem_types;
pub mod getparam;
pub mod query;
pub mod types;

pub use device::{UabiEngine, UabiEngines, UapiDevice};
pub use errno::Errno;
pub use error::UapiError;
pub use execbuf::{validate_execbuf2, ContextInfo, EngineChoice, ExecEnv, ExecPlan, InFence, ObjectInfo, SyncobjInfo};
pub use execbuf_wire::{decode_execbuf2, ExecLimits, ExecbufRequest};
pub use getparam::{getparam, handle_getparam};
pub use query::{handle_query, query_item};
