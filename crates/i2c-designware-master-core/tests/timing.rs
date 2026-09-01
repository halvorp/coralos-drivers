// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for `i2c-designware-master.c:34-186` timing selection.
//!
//! Copyright (C) 2006 Texas Instruments; Copyright (C) 2007 MontaVista Software Inc.;
//! Copyright (C) 2009 Provigent Ltd.

use i2c_designware_core::regs::bits;
use i2c_designware_master_core::timing::*;

fn input() -> TimingInput {
    TimingInput {
        bus_freq_hz: FAST_FREQ_HZ, ic_clk_khz: 10_000,
        sda_fall_ns: 0, scl_fall_ns: 0,
        ss: Counts::default(), fs: Counts::default(), fp: Counts::default(), hs: Counts::default(),
        bus_capacitance_pf: 100, clk_freq_optimized: false,
        master_cfg: bits::CON_SPEED_FAST, comp_param_1: 0,
    }
}

/// Frozen independently of `FREQUENCY_MODES`: do not generate this list from the production
/// table. Each value is Linux's literal for the named mode.
const LINUX_FREQUENCY_MODES: [(&str, u32); 4] = [
    ("standard", 100000),  // include/linux/i2c.h:44
    ("fast", 400000),      // include/linux/i2c.h:45
    ("fast-plus", 1000000), // include/linux/i2c.h:46
    ("high-speed", 3400000), // include/linux/i2c.h:48
];

/// include/linux/i2c.h:44-48 and i2c-designware-master.c:53-54.
#[test]
fn every_frequency_mode_name_and_value_matches_linux() {
    assert_eq!(FREQUENCY_MODES.len(), 4, "Linux modes consumed by this master");
    for index in 0..4 {
        let (actual_name, actual_value) = FREQUENCY_MODES[index];
        let (linux_name, linux_value) = LINUX_FREQUENCY_MODES[index];
        assert_eq!(actual_name, linux_name, "FREQUENCY_MODES[{index}] name");
        assert_eq!(actual_value, linux_value, "FREQUENCY_MODES[{index}] value");
    }

    assert_eq!(STANDARD_FREQ_HZ, 100000);
    assert_eq!(FAST_FREQ_HZ, 400000);
    assert_eq!(FAST_PLUS_FREQ_HZ, 1000000);
    assert_eq!(HIGH_SPEED_FREQ_HZ, 3400000);
    assert_eq!(DEFAULT_FALL_NS, 300);
}

/// master.c:57-73,113-128. `i2c_dw_clk_rate` returns kHz (common.c:680-690 and core.h:218).
/// Linux's common.c:547/:567 formulas at 10 MHz (10,000 kHz) produce these literals:
/// SS high 10000*(4000+300)/1e6-3 = 40; low ...*(4700+300)-1 = 49;
/// FS high ...*(600+300)-3 = 6; low ...*(1300+300)-1 = 15.
#[test]
fn standard_and_fast_counts_use_linux_parameters_and_default_falls() {
    let got = compute(input()).unwrap();
    assert_eq!(got.ss, Counts { hcnt: 40, lcnt: 49 });
    assert_eq!(got.fs, Counts { hcnt: 6, lcnt: 15 });
}

/// master.c:82-105. Firmware FPCN wins when both counts are present; otherwise 260/500 ns is used.
#[test]
fn fast_plus_prefers_supplied_pair_then_computes_linux_literals() {
    let mut i = input();
    i.bus_freq_hz = 1000000; // include/linux/i2c.h:46
    i.fp = Counts { hcnt: 0x123, lcnt: 0x456 };
    assert_eq!(compute(i).unwrap().fs, Counts { hcnt: 0x123, lcnt: 0x456 });
    i.fp = Counts::default();
    // 10000*(260+300)/1e6-3 = 3; 10000*(500+300)/1e6-1 = 7.
    assert_eq!(compute(i).unwrap().fs, Counts { hcnt: 3, lcnt: 7 });
}

/// master.c:133-178: unsupported high speed falls back to 400 kHz and FAST; supported 100 pF and
/// 400 pF paths use the four literal timing pairs selected at :154-161.
#[test]
fn high_speed_falls_back_or_uses_capacitance_parameters() {
    let mut i = input();
    i.bus_freq_hz = 3400000;
    i.master_cfg = bits::CON_SPEED_HIGH;
    let fallback = compute(i).unwrap();
    assert_eq!(fallback.bus_freq_hz, 400000); // master.c:139
    assert_eq!(fallback.master_cfg & 0x6, 0x4); // core.h:30,32
    assert_eq!(fallback.hs, Counts { hcnt: 0, lcnt: 0 });

    i.comp_param_1 = 0xc; // i2c-designware-core.h:143
    assert_eq!(compute(i).unwrap().hs, Counts { hcnt: 1, lcnt: 4 }); // 60/160 + 300
    i.clk_freq_optimized = true;
    assert_eq!(compute(i).unwrap().hs, Counts { hcnt: 1, lcnt: 3 }); // 60/120 + 300
    i.bus_capacitance_pf = 400;
    assert_eq!(compute(i).unwrap().hs, Counts { hcnt: 2, lcnt: 5 }); // 160/320 + 300
    i.clk_freq_optimized = false;
    assert_eq!(compute(i).unwrap().hs, Counts { hcnt: 1, lcnt: 5 }); // 120/320 + 300
}

/// Named refusals expose both a zero clock and an over-wide value with its 16-bit maximum.
#[test]
fn invalid_computed_counts_are_named_not_clamped() {
    let mut i = input();
    i.ic_clk_khz = 0;
    assert_eq!(compute(i), Err(TimingError::ClockRateZero));
    i.ic_clk_khz = u32::MAX;
    assert_eq!(compute(i), Err(TimingError::CountOutOfRange {
        name: "SS_HCNT", value: 18468356, maximum: 65535,
    }));
}
