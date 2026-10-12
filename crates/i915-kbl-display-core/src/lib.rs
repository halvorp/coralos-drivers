// SPDX-License-Identifier: GPL-2.0-only
//! i915-kbl-display-core: what a Kaby Lake (Gen9.5, display version 9) GPU's display engine is doing when the
//! firmware hands over, decoded from register values the caller supplies. READ ONLY: this crate contains no write and
//! no MMIO; it asks a closure for a register and decodes the answer exactly as Linux's i915 does at driver load.
//!
//! Ported from Linux drivers/gpu/drm/i915 (display/intel_display.c, intel_ddi.c, intel_dpll_mgr.c, intel_cdclk.c,
//! skl_universal_plane.c, skl_scaler.c, intel_display_power_well.c, intel_display_device.c, i915_pci.c) at v7.0-rc6;
//! every register offset, field and decode carries the FILE:LINE it came from. See `regs` for the register list,
//! `platform` for the PCI id table, `readout` for the entry point.
//!
//! The Linux tree this was ported from is the pinned reference `references/linux-ref` of the CoralOS tree (v7.0-rc6).
//! Copyright (c) Intel Corporation and the i915 authors.

//! # Validating the readout on silicon (the Linux reconnaissance boot)
//!
//! Host tests prove the decode against Linux's own encode rules; they cannot prove the Pixelbook Go's registers hold
//! what Linux says they hold. One Linux boot on the machine settles it, in this order, capturing for every display
//! scenario (panel alone, hub attached, external output active, detach / reconnect, both Type-C orientations):
//!
//! 1. BEFORE i915 touches anything (`modprobe.blacklist=i915` or `nomodeset`): dump the registers this crate reads,
//!    read-only, from BAR0 (`intel_reg read` of the offsets in `regs`, or a plain `/sys/bus/pci/devices/…/resource0`
//!    mmap): power wells 0x45400 / 0x45404 / 0x42000 / 0x45504, LCPLL1/2 0x46010 / 0x46014, DPLL_CTRL1/2/STATUS
//!    0x6c058 / 0x6c05c / 0x6c060, CFGCR1/2 0x6c040-0x6c054, WRPLL_CTL 0x46040 / 0x46060, CDCLK_CTL 0x46000, DDI_BUF_CTL
//!    A..E 0x64000 + 0x100 * n, TRANS_DDI_FUNC_CTL 0x60400 / 0x61400 / 0x62400 / 0x6f400, TRANSCONF 0x70008 / 0x71008 /
//!    0x72008 / 0x7f008, every transcoder's timing, PIPESRC and M/N block, PLANE_CTL/STRIDE/POS/SIZE/SURF/OFFSET of the
//!    three planes of each pipe, and the PS_CTRL/WIN_POS/WIN_SZ scaler registers. This is the takeover state.
//! 2. Then load i915 (`i915.fastboot=1`) and capture its own view of the same instant: debugfs `i915_display_info`
//!    (per pipe: mode, transcoder, encoder, lane count, port clock, DP M/N, bpp, plane format / stride / modifier,
//!    scaler), `i915_shared_dplls_info` (DPLL0-3 `ctrl1` / `cfgcr1` / `cfgcr2` and which is on), `i915_power_domain_info`
//!    (wells), `i915_dmc_info` (DC state), the CDCLK line of the boot log (`drm.debug=0x2`: "Current CDCLK …").
//! 3. Diff: each `[I915-KBL-READOUT …]` line must agree with the step-1 registers decoded by hand AND with step 2.
//!    A disagreement is a bug in this crate or in a reading of Linux, found before any write exists.
//!
//! What only silicon can answer, and the readout is expected to REPORT rather than assume: which DDI the Type-C ports
//! use (B / C?) and through which DPLL; whether the eDP panel is 1080p or 4K and its link rate / lane count; the CDCLK
//! the firmware chose (450 or 540 MHz); the GOP framebuffer's tiling and GGTT address; whether the DMC left a DC state
//! armed; and that the PCI class and revision the config page shows match the table (`platform`).

#![no_std]
#![forbid(unsafe_code)]

pub mod cdclk;
pub mod ddi;
pub mod dpll;
pub mod error;
pub mod plane;
pub mod platform;
pub mod power;
pub mod readout;
pub mod regs;
pub mod timing;
pub mod transcoder;
pub mod types;

pub use error::ReadoutError;
pub use platform::{identify, DisplayCaps, KblDevice, PlatformError, KBL_DISPLAY};
pub use readout::{read_display_state, ActivePipe, PipeReadout, PipeState, ReadoutState};
pub use types::{Ddi, Pipe, Transcoder};
