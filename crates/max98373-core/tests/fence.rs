// SPDX-License-Identifier: GPL-2.0-only
//! The write fence: the registers any plan of this crate can WRITE, as a hand-written literal. The `.device` builds its I2C allow-list from
//! `plan::written_registers`; this test is what makes a plan that grows a new write fail HERE, on the host, before it can fail on silicon.
use max98373_core::plan;

const WRITTEN: [u16; 44] = [
    0x2000, 0x2020, 0x2021, 0x2022, 0x2023, 0x2024, 0x2026, 0x2027, 0x2028, 0x2029, 0x202A, 0x202B, 0x202C, 0x203D, 0x203E, 0x203F, 0x2043, 0x2046, 0x2090, 0x2091, 0x2097, 0x2098,
    0x2099, 0x209A, 0x209B, 0x20A8, 0x20A9, 0x20AA, 0x20AB, 0x20AC, 0x20AD, 0x20AE, 0x20AF, 0x20B0, 0x20B1, 0x20B2, 0x20B3, 0x20B5, 0x20D1, 0x20D2, 0x20D3, 0x20D4, 0x20FE, 0x20FF,
];

#[test]
fn the_plans_write_exactly_these_registers() {
    assert_eq!(plan::written_registers(&plan::ATLAS_AMPS).unwrap(), WRITTEN.to_vec());
}

#[test]
fn the_registers_that_must_never_be_written_are_not_in_it() {
    for (reg, why) in [
        (0x21FF, "the revision id is read only"),
        (0x2040, "TONE_GEN_CFG: a test tone generator in the part"),
        (0x2041, "AMP_CFG"),
        (0x2042, "AMP_EDGE_RATE_CFG"),
        (0x2014, "THERM_WARN_THRESH"),
        (0x2015, "THERM_SHDN_THRESH: the part's own thermal limit"),
        (0x2016, "THERM_HYSTERESIS"),
        (0x2017, "THERM_FOLDBACK_SET"),
        (0x2018, "THERM_FOLDBACK_EN"),
        (0x2010, "IRQ_CTRL"),
        (0x20E0, "LIMITER_THRESH_CFG"),
        (0x20E2, "LIMITER_EN"),
        (0x2025, "AUDIO_IF_MODE"),
        (0x2047, "IV_SENSE_ADC_EN"),
        (0x2056, "MEAS_ADC_PVDD_CH_EN"),
    ] {
        assert!(!WRITTEN.contains(&reg), "{reg:#06x} {why}");
    }
}

#[test]
fn an_unreachable_board_has_no_amps_and_so_no_writes_to_plan() {
    // the twelve registers only the per-amplifier bring-up writes: reset, the Hi-Z pair, the tx sources, the data format, the clock ratio,
    // the two sample-rate registers, mono-mix 2, the IV DC blocker and the auto-restart config. The other 32 are the UCM set, the power
    // sequences and the emergency write, which do not depend on the board.
    let none = plan::written_registers(&[]).unwrap();
    assert_eq!(none.len(), 32);
    for reg in [0x2000u16, 0x2020, 0x2021, 0x2022, 0x2023, 0x2024, 0x2026, 0x2027, 0x2028, 0x202A, 0x2046, 0x20FE] {
        assert!(!none.contains(&reg), "{reg:#06x}");
    }
}
