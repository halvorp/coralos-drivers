// SPDX-License-Identifier: GPL-2.0-only
//! Master SCL/SDA timing selection.
//!
//! Ported from Linux `drivers/i2c/busses/i2c-designware-master.c:34-186` and using the pure SCL
//! arithmetic from Linux `drivers/i2c/busses/i2c-designware-common.c:527-567`, supplied by
//! `i2c-designware-core`.
//!
//! Copyright (C) 2006 Texas Instruments.
//! Copyright (C) 2007 MontaVista Software Inc.
//! Copyright (C) 2009 Provigent Ltd.
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.

use i2c_designware_core::{regs::bits, timing::{scl_hcnt, scl_lcnt}};

/// `I2C_MAX_STANDARD_MODE_FREQ` (include/linux/i2c.h:44).
pub const STANDARD_FREQ_HZ: u32 = 100_000;
/// `I2C_MAX_FAST_MODE_FREQ` (include/linux/i2c.h:45).
pub const FAST_FREQ_HZ: u32 = 400_000;
/// `I2C_MAX_FAST_MODE_PLUS_FREQ` (include/linux/i2c.h:46).
pub const FAST_PLUS_FREQ_HZ: u32 = 1_000_000;
/// `I2C_MAX_HIGH_SPEED_MODE_FREQ` (include/linux/i2c.h:48).
pub const HIGH_SPEED_FREQ_HZ: u32 = 3_400_000;
/// The four frequency-mode names consumed by this master implementation. The values are from
/// `include/linux/i2c.h:44-48`; their uses are at i2c-designware-master.c:82,139,939-947.
pub const FREQUENCY_MODES: [(&str, u32); 4] = [
    ("standard", 100_000), // include/linux/i2c.h:44
    ("fast", 400_000), // include/linux/i2c.h:45
    ("fast-plus", 1_000_000), // include/linux/i2c.h:46
    ("high-speed", 3_400_000), // include/linux/i2c.h:48
];
/// Default SDA/SCL fall time (i2c-designware-master.c:53-54).
pub const DEFAULT_FALL_NS: u32 = 300;

/// Caller- or firmware-supplied HCNT/LCNT pair. Zero means absent, matching Linux's predicates
/// (i2c-designware-master.c:57,87,113,144).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub hcnt: u32,
    pub lcnt: u32,
}

/// Inputs to `i2c_dw_set_timings_master` that do not require MMIO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingInput {
    pub bus_freq_hz: u32,
    /// Controller input clock in kHz, matching `get_clk_rate_khz` and `i2c_dw_clk_rate`
    /// (i2c-designware-core.h:218,281,353; i2c-designware-common.c:680-690).
    pub ic_clk_khz: u32,
    pub sda_fall_ns: u32,
    pub scl_fall_ns: u32,
    pub ss: Counts,
    pub fs: Counts,
    pub fp: Counts,
    pub hs: Counts,
    pub bus_capacitance_pf: u32,
    pub clk_freq_optimized: bool,
    pub master_cfg: u32,
    pub comp_param_1: u32,
}

/// Fully selected timing state, including Linux's high-speed fallback result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingOutput {
    pub bus_freq_hz: u32,
    pub ss: Counts,
    pub fs: Counts,
    pub hs: Counts,
    pub master_cfg: u32,
}

/// A timing setup refusal that names the value and bound instead of clamping it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingError {
    ClockRateZero,
    CountOutOfRange { name: &'static str, value: i64, maximum: u32 },
}

fn falls(input: TimingInput) -> (u32, u32) {
    (if input.sda_fall_ns == 0 { DEFAULT_FALL_NS } else { input.sda_fall_ns },
     if input.scl_fall_ns == 0 { DEFAULT_FALL_NS } else { input.scl_fall_ns })
}

fn checked_counts(name: &'static str, clock_khz: u32, high: u32, low: u32,
                  sda_fall: u32, scl_fall: u32) -> Result<Counts, TimingError> {
    let hcnt = scl_hcnt(clock_khz, high, sda_fall, 0);
    let lcnt = scl_lcnt(clock_khz, low, scl_fall, 0);
    for (suffix, value) in [("HCNT", hcnt), ("LCNT", lcnt)] {
        if value <= 0 || value > u16::MAX as i64 {
            return Err(TimingError::CountOutOfRange {
                name: if suffix == "HCNT" { name } else {
                    match name {
                        "SS_HCNT" => "SS_LCNT",
                        "FS_HCNT" => "FS_LCNT",
                        "FP_HCNT" => "FP_LCNT",
                        _ => "HS_LCNT",
                    }
                },
                value,
                maximum: u16::MAX as u32,
            });
        }
    }
    Ok(Counts { hcnt: hcnt as u32, lcnt: lcnt as u32 })
}

/// Select all master timing counts exactly as `i2c_dw_set_timings_master` does
/// (i2c-designware-master.c:52-182), without reading or writing hardware.
pub fn compute(input: TimingInput) -> Result<TimingOutput, TimingError> {
    let needs_clock = input.ss.hcnt == 0 || input.ss.lcnt == 0
        || (input.bus_freq_hz == FAST_PLUS_FREQ_HZ
            && (input.fp.hcnt == 0 || input.fp.lcnt == 0))
        || (input.bus_freq_hz != FAST_PLUS_FREQ_HZ
            && (input.fs.hcnt == 0 || input.fs.lcnt == 0))
        || (input.master_cfg & bits::CON_SPEED_MASK == bits::CON_SPEED_HIGH
            && input.comp_param_1 & bits::COMP_PARAM_1_SPEED_MODE_MASK
                == bits::COMP_PARAM_1_SPEED_MODE_HIGH
            && (input.hs.hcnt == 0 || input.hs.lcnt == 0));
    if input.ic_clk_khz == 0 && needs_clock {
        return Err(TimingError::ClockRateZero);
    }
    let (sda_fall, scl_fall) = falls(input);
    let ss = if input.ss.hcnt == 0 || input.ss.lcnt == 0 {
        checked_counts("SS_HCNT", input.ic_clk_khz, 4000, 4700, sda_fall, scl_fall)? // master.c:63,70
    } else { input.ss };

    let mut fs = input.fs;
    if input.bus_freq_hz == FAST_PLUS_FREQ_HZ {
        fs = if input.fp.hcnt != 0 && input.fp.lcnt != 0 {
            input.fp // master.c:87-89
        } else {
            checked_counts("FP_HCNT", input.ic_clk_khz, 260, 500, sda_fall, scl_fall)? // master.c:96,103
        };
    }
    if fs.hcnt == 0 || fs.lcnt == 0 {
        fs = checked_counts("FS_HCNT", input.ic_clk_khz, 600, 1300, sda_fall, scl_fall)?; // master.c:119,126
    }

    let mut bus_freq_hz = input.bus_freq_hz;
    let mut master_cfg = input.master_cfg;
    let mut hs = input.hs;
    if master_cfg & bits::CON_SPEED_MASK == bits::CON_SPEED_HIGH {
        if input.comp_param_1 & bits::COMP_PARAM_1_SPEED_MODE_MASK
            != bits::COMP_PARAM_1_SPEED_MODE_HIGH {
            bus_freq_hz = FAST_FREQ_HZ; // master.c:139
            master_cfg = (master_cfg & !bits::CON_SPEED_MASK) | bits::CON_SPEED_FAST; // master.c:140-141
            hs = Counts::default(); // master.c:142-143
        } else if hs.hcnt == 0 || hs.lcnt == 0 {
            let (high, low) = if input.bus_capacitance_pf >= 400 {
                (if input.clk_freq_optimized { 160 } else { 120 }, 320) // master.c:154-157
            } else {
                (60, if input.clk_freq_optimized { 120 } else { 160 }) // master.c:158-161
            };
            hs = checked_counts("HS_HCNT", input.ic_clk_khz, high, low, sda_fall, scl_fall)?;
        }
    }
    Ok(TimingOutput { bus_freq_hz, ss, fs, hs, master_cfg })
}
