// SPDX-License-Identifier: GPL-2.0-only
//! The write plan `intel_dmc_load_program()` (intel_dmc.c:913-943) would execute on a display version 9 part,
//! computed without touching a register. The device applies it in order: program memory words, then the MMIO pairs,
//! then the DC_STATE_DEBUG read-modify-write.
//!
//! Display version < 12 skips `disable_all_event_handlers()` (intel_dmc.c:458),
//! and `disable_dmc_evt()` only rewrites pipe-DMC / TGL / ADL-S pairs, so on Kaby Lake the header pairs are written
//! verbatim.

use crate::format::{DmcId, DmcPackage};
use alloc::vec::Vec;

/// `#define DC_STATE_DEBUG_MASK_CORES (1 << 0)` — intel_display_regs.h:2832.
pub const DC_STATE_DEBUG_MASK_CORES: u32 = 1 << 0;
/// `#define DC_STATE_DEBUG_MASK_MEMORY_UP (1 << 1)` — intel_display_regs.h:2833.
pub const DC_STATE_DEBUG_MASK_MEMORY_UP: u32 = 1 << 1;
/// What `gen9_set_dc_state_debugmask()` ORs into DC_STATE_DEBUG — intel_dmc.c:433-440.
pub const DC_STATE_DEBUG_MASK_SET: u32 = DC_STATE_DEBUG_MASK_CORES | DC_STATE_DEBUG_MASK_MEMORY_UP;
/// `#define DC_STATE_DEBUG _MMIO(0x45520)` — intel_display_regs.h:2831.
pub const DC_STATE_DEBUG: u32 = 0x45520;

/// One 32-bit register write at a BAR0 offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegWrite {
    pub offset: u32,
    pub value: u32,
}

/// The validated plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPlan {
    /// DMC_VERSION(major, minor) from the CSS header.
    pub version: u32,
    /// Program-memory words, in order: `DMC_PROGRAM(start, i)` = start + 4 * i — intel_dmc_regs.h:277.
    pub program: Vec<RegWrite>,
    /// The header's (offset, value) pairs, in header order (`dmc_load_mmio`, intel_dmc.c:647-656).
    pub mmio: Vec<RegWrite>,
    /// After the writes: read-modify-write `DC_STATE_DEBUG |= DC_STATE_DEBUG_MASK_SET`, then a posting read.
    pub dc_state_debug_or: u32,
}

/// Build the plan for the MAIN image (Kaby Lake has no pipe DMCs; any present are ignored here exactly as the
/// platform ignores them — a v1 package yields only DMC_FW_MAIN, intel_dmc.c:994-1013).
pub fn build_plan(pkg: &DmcPackage) -> Option<LoadPlan> {
    let main = pkg.image(DmcId::Main)?;
    let program = main
        .payload
        .iter()
        .enumerate()
        .map(|(i, &w)| RegWrite { offset: main.start_mmioaddr + 4 * i as u32, value: w })
        .collect();
    let mmio = main.mmio.iter().map(|&(offset, value)| RegWrite { offset, value }).collect();
    Some(LoadPlan { version: pkg.version, program, mmio, dc_state_debug_or: DC_STATE_DEBUG_MASK_SET })
}
