// SPDX-License-Identifier: GPL-2.0-only
//! MCLK and PLL: Linux-literal dividers for the Atlas clock tree and every rejected input.
mod common;
use common::*;
use da7219_core::clock::*;

#[test]
fn atlas_clock_values_are_the_kbl_ones_of_the_avs_machine_driver() {
    assert_eq!(ATLAS_MCLK_HZ, 24_576_000); // avs/boards/da7219.c:106
    assert_eq!(ATLAS_PLL_OUT_HZ, 98_304_000); // da7219.h:763
    assert_eq!(PLL_OUT_FREQS, [90_316_800, 98_304_000]); // da7219.h:762-763
    assert_eq!((MCLK_MIN_HZ, MCLK_MAX_HZ), (2_000_000, 54_000_000)); // da7219.c:1177
}

#[test]
fn input_divider_buckets_match_linux_at_every_edge() {
    // (mclk, INDIV bits, divider value): da7219.c:1229-1256, da7219.h:196-200, :766-770
    let table = [
        (2_000_000, 0x00u8, 1u32),
        (4_500_000, 0x00, 1),
        (4_500_001, 0x04, 2),
        (9_000_000, 0x04, 2),
        (9_000_001, 0x08, 4),
        (18_000_000, 0x08, 4),
        (18_000_001, 0x0c, 8),
        (24_576_000, 0x0c, 8),
        (36_000_000, 0x0c, 8),
        (36_000_001, 0x10, 16),
        (54_000_000, 0x10, 16),
    ];
    for (hz, bits, div) in table {
        assert_eq!(indiv_for(hz), Ok((bits, div)), "{hz}");
    }
    assert_eq!(indiv_for(1_999_999), Err(ClockError::MclkOutOfRange { hz: 1_999_999 }));
    assert_eq!(indiv_for(54_000_001), Err(ClockError::MclkOutOfRange { hz: 54_000_001 }));
}

#[test]
fn the_atlas_srm_pll_is_integer_32_with_no_fraction() {
    // 24.576 MHz / 8 = 3.072 MHz reference; 98.304 MHz / 3.072 MHz = 32 exactly (da7219.c:1277)
    let p = pll_dividers(24_576_000, SysClk::PllSrm, 98_304_000).unwrap();
    assert_eq!((p.indiv, p.integer, p.frac_top, p.frac_bot), (8, 32, 0, 0));
    assert_eq!(p.ctrl, 0x0c | 0x80, "INDIV 18-36 MHz | MODE_SRM");
    let n = pll_dividers(24_576_000, SysClk::Pll, 98_304_000).unwrap();
    assert_eq!(n.ctrl, 0x0c | 0x40, "MODE_NORMAL");
}

#[test]
fn the_44k1_family_pll_has_a_fraction_worked_by_hand() {
    // 90_316_800 = 29 * 3_072_000 + 1_228_800; 1_228_800 * 8192 / 3_072_000 = 3276 (3276.8 truncated) = 0x0CCC
    let p = pll_dividers(24_576_000, SysClk::Pll, 90_316_800).unwrap();
    assert_eq!((p.integer, p.frac_top, p.frac_bot), (29, 0x0c, 0xcc));
}

#[test]
fn the_pll_reproduces_fout_within_one_fraction_step_for_every_bucket() {
    // the integer divider fits its 7-bit field for every legal MCLK and both outputs (type-pinned in clock.rs)
    for mclk in (2_000_000u32..=54_000_000).step_by(50_000).chain([54_000_000]) {
        for fout in PLL_OUT_FREQS {
            let p = pll_dividers(mclk, SysClk::Pll, fout).unwrap();
            assert!(p.integer < 0x80 && p.integer > 0, "{mclk} {fout}");
            let r = (mclk / p.indiv) as u64;
            let frac = ((p.frac_top as u64) << 8) | p.frac_bot as u64;
            let got = r * p.integer as u64 + r * frac / 8192;
            assert!(fout as u64 - got <= r / 8192 + 1, "{mclk} {fout} {got}");
            assert!(p.frac_top < 0x20, "13-bit fraction: top byte is 5 bits");
        }
    }
}

#[test]
fn bypass_has_no_dividers_and_an_unsupported_output_is_refused_by_name() {
    let b = pll_dividers(24_576_000, SysClk::Mclk, 0).unwrap();
    assert_eq!((b.ctrl, b.integer, b.frac_top, b.frac_bot), (0x0c, 0, 0, 0));
    assert_eq!(pll_dividers(24_576_000, SysClk::PllSrm, 100_000_000), Err(ClockError::UnsupportedPllOut { hz: 100_000_000 }));
    assert_eq!(pll_dividers(1_000_000, SysClk::PllSrm, 98_304_000), Err(ClockError::MclkOutOfRange { hz: 1_000_000 }));
}

#[test]
fn starting_the_pll_writes_dividers_before_the_control_register_and_stopping_writes_only_the_control_register() {
    let mut f = Fake::reset();
    f.run(atlas_clock_up().unwrap()).unwrap();
    let order: Vec<_> = f.writes.iter().map(|w| (w.1, w.2)).collect();
    // MCLK source (SQR off), then FRAC_TOP, FRAC_BOT, INTEGER, then PLL_CTRL = 0x8c (INDIV 18-36, SRM)
    assert_eq!(order, vec![(0x20, 0x10 & !0x20), (0x22, 0), (0x23, 0), (0x24, 32), (0x20, 0x8c)]);
    assert_only_codec(&f);
    let mut g = Fake::reset();
    g.regs[0x20] = 0x8c;
    g.run(atlas_clock_down().unwrap()).unwrap();
    assert_eq!(g.writes.iter().map(|w| (w.1, w.2)).collect::<Vec<_>>(), vec![(0x20, 0x0c)]);
}

#[test]
fn the_mclk_source_is_the_square_wave_enable_bit_and_the_rate_is_validated() {
    let mut f = Fake::reset();
    f.run(sysclk_plan(ClkSrc::MclkSqr, 24_576_000).unwrap()).unwrap();
    assert_eq!(f.reg(0x20) & 0x20, 0x20);
    f.run(sysclk_plan(ClkSrc::Mclk, 24_576_000).unwrap()).unwrap();
    assert_eq!(f.reg(0x20) & 0x20, 0);
    assert_eq!(sysclk_plan(ClkSrc::Mclk, 1_000_000), Err(ClockError::MclkOutOfRange { hz: 1_000_000 }));
}
