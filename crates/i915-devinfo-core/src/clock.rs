// SPDX-License-Identifier: GPL-2.0-only
//! The command-stream timestamp frequency: `gen9_read_clock_frequency` and `read_reference_ts_freq`
//! (gt/intel_gt_clock_utils.c:14-29, :87-106), over register VALUES; and Gen6-Gen8's fixed `gen6_read_clock_frequency` (:108-118),
//! which `read_clock_frequency` picks for GRAPHICS_VER 6..=8 (:160-161) - Cherryview included.
use i915_common_core::reg::field_get;
use i915_gt_regs_core::{
    CTC_SHIFT_PARAMETER_MASK, CTC_SOURCE_DIVIDE_LOGIC, CTC_SOURCE_PARAMETER_MASK, GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DENOMINATOR_MASK,
    GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DENOMINATOR_SHIFT, GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DIVIDER_MASK,
};

/// The crystal Gen9 BC parts count from: 24 MHz (`IS_GEN9_LP ? 19200000 : 24000000`, intel_gt_clock_utils.c:95).
pub const GEN9_BC_CRYSTAL_HZ: u32 = 24_000_000; // intel_gt_clock_utils.c:95

/// `gen6_read_clock_frequency` (intel_gt_clock_utils.c:108-118): "The PCU TSC counts 10ns increments; this timestamp reflects bits
/// 38:3 of the TSC (i.e. 80ns granularity)" - 12.5 MHz, no register read.
pub const GEN6_TS_HZ: u32 = 12_500_000; // intel_gt_clock_utils.c:117

/// What `CTC_MODE` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gen9Clock {
    /// The timestamp counts the crystal shifted down: this many Hz.
    Hz(u32),
    /// `CTC_SOURCE_DIVIDE_LOGIC`: the frequency comes from `GEN9_TIMESTAMP_OVERRIDE` - read it and call
    /// [`reference_ts_freq`] (intel_gt_clock_utils.c:92-93).
    NeedsTimestampOverride,
}

/// `gen9_read_clock_frequency` up to the register read it cannot make (intel_gt_clock_utils.c:87-106), non-LP.
pub fn gen9_clock_from_ctc(ctc_mode: u32) -> Gen9Clock {
    if ctc_mode & CTC_SOURCE_PARAMETER_MASK == CTC_SOURCE_DIVIDE_LOGIC {
        Gen9Clock::NeedsTimestampOverride // :92-93
    } else {
        // freq >>= 3 - REG_FIELD_GET(CTC_SHIFT_PARAMETER_MASK, ctc_reg) (:102)
        Gen9Clock::Hz(GEN9_BC_CRYSTAL_HZ >> (3 - field_get(CTC_SHIFT_PARAMETER_MASK, ctc_mode))) // :102
    }
}

/// `read_reference_ts_freq` (intel_gt_clock_utils.c:14-29) over the `GEN9_TIMESTAMP_OVERRIDE` value.
pub fn reference_ts_freq(ts_override: u32) -> u32 {
    let base = ((ts_override & GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DIVIDER_MASK) + 1) * 1_000_000; // :19-21
    let denom = (ts_override & GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DENOMINATOR_MASK) >> GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DENOMINATOR_SHIFT;
    base + 1_000_000 / (denom + 1) // :23-28 (frac_freq, return base + frac)
}
