// SPDX-License-Identifier: GPL-2.0-only
//! i915-vbt-core: parse the Video BIOS Table (VBT) and its BIOS Data Block (BDB) the way Linux i915 does at
//! display init. Pure bytes in, typed values out: no MMIO, no I/O, no panics on hostile input.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_bios.c and intel_vbt_defs.h (the pinned reference
//! `references/linux-ref`, v7.0-rc6); every constant and decode carries FILE:LINE.
//! Copyright (c) 2006-2016 Intel Corporation; Authors: Eric Anholt and the i915 authors.
//!
//! Scope = what a Kaby Lake (display version 9, SPT/KBP PCH) panel/port bring-up needs: header + block walk,
//! general features, general definitions + child device configs (DDI port, device type, AUX channel, DDC pin,
//! HDMI/DP flags), eDP block (power sequencing, link params), LFP options + data-pointer table + panel DTD,
//! PSR table, LFP backlight. NOT ported (named, not silent): MIPI/DSI blocks, DSC compression, the
//! `generate_lfp_data_ptrs()` synthesis for VBTs that lack block 41 (we refuse with `LfpDataPtrsMissing`),
//! block 44 (LFP power / PSR enable for BDB >= 228), block 58 generic DTD, display-version >= 11 port/AUX/DDC maps.
#![no_std]
extern crate alloc;

pub mod backlight;
pub mod bdb;
pub mod child;
pub mod edp;
pub mod error;
pub mod general;
pub mod panel;
pub mod psr;

pub use bdb::{Block, BlockId, Vbt};
pub use error::VbtError;
