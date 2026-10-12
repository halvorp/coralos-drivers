// SPDX-License-Identifier: GPL-2.0-only
//! i915-uncore-core: the Gen9 uncore - forcewake.
//!
//! Ported from Linux drivers/gpu/drm/i915/intel_uncore.c and intel_uncore.h (v7.0-rc6), the part a Gen9 (Skylake / Kaby Lake)
//! part uses: three forcewake domains ([`domain`]), the `__gen9_fw_ranges` / `gen8_shadowed_regs` tables and the
//! `NEEDS_FORCE_WAKE` rule ([`ranges`]), and the get / put / ack-wait protocol ([`forcewake`]) over `coralos_hal::Mmio`.
//! Copyright (c) Intel Corporation and the i915 authors.
//!
//! WHAT THIS CRATE WRITES: only the three forcewake SET registers ([`domain::WRITE_ALLOWLIST`]) - `fw_set` / `fw_clear` of
//! intel_uncore.c:135-136. A caller that wraps its `Mmio` in an `i915_common_core::WriteGuard` over that list gets a
//! mechanical proof (the guard's refusal count) that nothing else was touched.
//!
//! DEVIATIONS FROM LINUX, each named:
//!  * `DELAYED_PUT`: Linux's `intel_uncore_forcewake_put` releases immediately but the auto-forcewake read path arms a 1 ms hrtimer;
//!    this crate has no timers, so [`forcewake::Uncore::read32`] REQUIRES the domain to be held and refuses otherwise.
//!  * `UDELAY_BY_READS`: Linux `udelay(10 * pass)` in the ack-fallback loop (intel_uncore.c:242) has no clock here; the delay is
//!    made of that many ack-register reads.
//!  * `ALL_ONES_IS_NOT_AN_ACK`: Linux treats an ack register reading `~0` as "set" when waiting for set (intel_uncore.c:279);
//!    here a `0xffff_ffff` read is `MmioUnreliable`, because a dead BAR reads all-ones.
//!  * the wait is bounded by BOTH `Mmio::deadline_expired()` (50 ms, `FORCEWAKE_ACK_TIMEOUT_MS`, intel_uncore.c:42) AND an
//!    iteration cap, so a host with no clock still terminates.

#![no_std]
#![forbid(unsafe_code)]

pub mod domain;
pub mod forcewake;
pub mod ranges;

pub use domain::{Domain, DomainId, FORCEWAKE_ALL, FORCEWAKE_GT, FORCEWAKE_MEDIA, FORCEWAKE_RENDER, GEN9_DOMAINS, VLV_DOMAINS, WRITE_ALLOWLIST};
pub use forcewake::{Uncore, MAX_POLLS, FORCEWAKE_ACK_TIMEOUT_MS};
pub use ranges::{find_fw_domain, is_shadowed, needs_force_wake, read_fw_domains, write_fw_domains, FwRange, GEN9_FW_RANGES};
