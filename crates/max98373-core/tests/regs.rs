// SPDX-License-Identifier: GPL-2.0-only
//! The register map, pinned to LINUX LITERALS (max98373.h) typed here by hand, and the control arithmetic against ALSA's.
use max98373_core::ctl::{self, Ctl, CtlError, Kind};
use max98373_core::regs;

#[test]
fn the_registers_are_the_header_s() {
    for (name, got, want) in [
        ("R2000_SW_RESET", regs::R2000_SW_RESET, 0x2000u16),
        ("R2020_PCM_TX_HIZ_EN_1", regs::R2020_PCM_TX_HIZ_EN_1, 0x2020),
        ("R2021_PCM_TX_HIZ_EN_2", regs::R2021_PCM_TX_HIZ_EN_2, 0x2021),
        ("R2022_PCM_TX_SRC_1", regs::R2022_PCM_TX_SRC_1, 0x2022),
        ("R2023_PCM_TX_SRC_2", regs::R2023_PCM_TX_SRC_2, 0x2023),
        ("R2024_PCM_DATA_FMT_CFG", regs::R2024_PCM_DATA_FMT_CFG, 0x2024),
        ("R2026_PCM_CLOCK_RATIO", regs::R2026_PCM_CLOCK_RATIO, 0x2026),
        ("R2027_PCM_SR_SETUP_1", regs::R2027_PCM_SR_SETUP_1, 0x2027),
        ("R2028_PCM_SR_SETUP_2", regs::R2028_PCM_SR_SETUP_2, 0x2028),
        ("R2029_PCM_TO_SPK_MONO_MIX_1", regs::R2029_PCM_TO_SPK_MONO_MIX_1, 0x2029),
        ("R202A_PCM_TO_SPK_MONO_MIX_2", regs::R202A_PCM_TO_SPK_MONO_MIX_2, 0x202A),
        ("R202B_PCM_RX_EN", regs::R202B_PCM_RX_EN, 0x202B),
        ("R202C_PCM_TX_EN", regs::R202C_PCM_TX_EN, 0x202C),
        ("R203D_AMP_DIG_VOL_CTRL", regs::R203D_AMP_DIG_VOL_CTRL, 0x203D),
        ("R203E_AMP_PATH_GAIN", regs::R203E_AMP_PATH_GAIN, 0x203E),
        ("R203F_AMP_DSP_CFG", regs::R203F_AMP_DSP_CFG, 0x203F),
        ("R2043_AMP_EN", regs::R2043_AMP_EN, 0x2043),
        ("R2046_IV_SENSE_ADC_DSP_CFG", regs::R2046_IV_SENSE_ADC_DSP_CFG, 0x2046),
        ("R2090_BDE_LVL_HOLD", regs::R2090_BDE_LVL_HOLD, 0x2090),
        ("R2091_BDE_GAIN_ATK_REL_RATE", regs::R2091_BDE_GAIN_ATK_REL_RATE, 0x2091),
        ("R2097_BDE_L1_THRESH", regs::R2097_BDE_L1_THRESH, 0x2097),
        ("R2098_BDE_L2_THRESH", regs::R2098_BDE_L2_THRESH, 0x2098),
        ("R2099_BDE_L3_THRESH", regs::R2099_BDE_L3_THRESH, 0x2099),
        ("R209A_BDE_L4_THRESH", regs::R209A_BDE_L4_THRESH, 0x209A),
        ("R209B_BDE_THRESH_HYST", regs::R209B_BDE_THRESH_HYST, 0x209B),
        ("R20A8_BDE_L1_CFG_1", regs::R20A8_BDE_L1_CFG_1, 0x20A8),
        ("R20A9_BDE_L1_CFG_2", regs::R20A9_BDE_L1_CFG_2, 0x20A9),
        ("R20AA_BDE_L1_CFG_3", regs::R20AA_BDE_L1_CFG_3, 0x20AA),
        ("R20AB_BDE_L2_CFG_1", regs::R20AB_BDE_L2_CFG_1, 0x20AB),
        ("R20AC_BDE_L2_CFG_2", regs::R20AC_BDE_L2_CFG_2, 0x20AC),
        ("R20AD_BDE_L2_CFG_3", regs::R20AD_BDE_L2_CFG_3, 0x20AD),
        ("R20AE_BDE_L3_CFG_1", regs::R20AE_BDE_L3_CFG_1, 0x20AE),
        ("R20AF_BDE_L3_CFG_2", regs::R20AF_BDE_L3_CFG_2, 0x20AF),
        ("R20B0_BDE_L3_CFG_3", regs::R20B0_BDE_L3_CFG_3, 0x20B0),
        ("R20B1_BDE_L4_CFG_1", regs::R20B1_BDE_L4_CFG_1, 0x20B1),
        ("R20B2_BDE_L4_CFG_2", regs::R20B2_BDE_L4_CFG_2, 0x20B2),
        ("R20B3_BDE_L4_CFG_3", regs::R20B3_BDE_L4_CFG_3, 0x20B3),
        ("R20B5_BDE_EN", regs::R20B5_BDE_EN, 0x20B5),
        ("R20D1_DHT_CFG", regs::R20D1_DHT_CFG, 0x20D1),
        ("R20D2_DHT_ATTACK_CFG", regs::R20D2_DHT_ATTACK_CFG, 0x20D2),
        ("R20D3_DHT_RELEASE_CFG", regs::R20D3_DHT_RELEASE_CFG, 0x20D3),
        ("R20D4_DHT_EN", regs::R20D4_DHT_EN, 0x20D4),
        ("R20FE_DEVICE_AUTO_RESTART_CFG", regs::R20FE_DEVICE_AUTO_RESTART_CFG, 0x20FE),
        ("R20FF_GLOBAL_SHDN", regs::R20FF_GLOBAL_SHDN, 0x20FF),
        ("R21FF_REV_ID", regs::R21FF_REV_ID, 0x21FF),
    ] {
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn the_fields_are_the_header_s() {
    assert_eq!(regs::SOFT_RESET, 0x01);
    assert_eq!(regs::PCM_TX_CH_SRC_A_V_SHIFT, 0);
    assert_eq!(regs::PCM_TX_CH_SRC_A_I_SHIFT, 4);
    assert_eq!(regs::PCM_MODE_CFG_FORMAT_MASK, 0x38);
    assert_eq!(regs::PCM_MODE_CFG_FORMAT_SHIFT, 3);
    assert_eq!(regs::PCM_FORMAT_TDM_MODE0, 0x03);
    assert_eq!(regs::PCM_MODE_CFG_CHANSZ_MASK, 0xC0);
    assert_eq!(regs::PCM_MODE_CFG_CHANSZ_16, 0x40);
    assert_eq!(regs::PCM_MODE_CFG_PCM_BCLKEDGE, 0x10);
    assert_eq!(regs::PCM_CLK_SETUP_BSEL_MASK, 0x0F);
    assert_eq!(regs::PCM_SR_SET1_SR_MASK, 0x0F);
    assert_eq!(regs::PCM_SR_SET1_SR_48000, 0x08);
    assert_eq!(regs::PCM_SR_SET2_SR_MASK, 0xF0);
    assert_eq!(regs::PCM_SR_SET2_SR_SHIFT, 4);
    assert_eq!(regs::PCM_SR_SET2_IVADC_SR_MASK, 0x0F);
    assert_eq!(regs::PCM_TO_SPK_MONOMIX_CFG_MASK, 0xC0);
    assert_eq!(regs::PCM_TO_SPK_MONOMIX_CFG_SHIFT, 6);
    assert_eq!(regs::PCM_TO_SPK_CH0_SRC_MASK, 0x0F);
    assert_eq!(regs::SPK_EN_MASK, 0x01);
    assert_eq!(regs::GLOBAL_EN_MASK, 0x01);
}

fn single(reg: u16, shift: u8, max: u8, invert: bool) -> Ctl {
    Ctl { name: "t", kind: Kind::Single, reg, shift, max, invert }
}

#[test]
fn single_controls_follow_snd_soc_put_volsw() {
    // mask = (1 << fls(max)) - 1, shifted; an inverted control stores max - value
    let c = single(0x20D1, 4, 9, false); // DHT Min Volume: fls(9) = 4 -> 0xF << 4
    assert_eq!(c.mask(), 0xF0);
    assert_eq!(c.encode(9), Ok((0xF0, 0x90)));
    assert_eq!(c.encode(10), Err(CtlError::OutOfRange { value: 10, max: 9 }));
    let c = single(0x203D, 0, 0x7F, true); // Digital Volume
    assert_eq!(c.encode(127), Ok((0x7F, 0x00)));
    assert_eq!(c.encode(0), Ok((0x7F, 0x7F)));
    assert_eq!(c.encode(128), Err(CtlError::OutOfRange { value: 128, max: 127 }));
    let c = single(0x20A9, 0, 0x3C, true); // BDE clip thresh: fls(0x3C) = 6 -> 0x3F
    assert_eq!(c.mask(), 0x3F);
    assert_eq!(c.encode(60), Ok((0x3F, 0x00)));
    assert_eq!(c.encode(58), Ok((0x3F, 0x02)));
    let c = single(0x20D2, 3, 4, false); // step size: fls(4) = 3 -> 0x7 << 3
    assert_eq!(c.mask(), 0x38);
    let c = single(0x20B2, 7, 1, false); // a switch at bit 7
    assert_eq!(c.encode(1), Ok((0x80, 0x80)));
    assert_eq!(c.encode(2), Err(CtlError::OutOfRange { value: 2, max: 1 }));
}

#[test]
fn enum_controls_follow_snd_soc_put_enum_double() {
    // mask = roundup_pow_of_two(items) - 1
    let e = |reg, shift, items| Ctl { name: "e", kind: Kind::Enum, reg, shift, max: items, invert: false };
    assert_eq!(e(0x203E, 0, 10).mask(), 0x0F, "ten items -> 16 -> 0xF");
    assert_eq!(e(0x20D2, 0, 8).mask(), 0x07, "eight items -> 0x7");
    assert_eq!(e(0x2029, 6, 3).mask(), 0xC0, "three items -> 4 -> 0x3 << 6");
    assert_eq!(e(0x203E, 0, 10).encode(9), Ok((0x0F, 0x09)));
    assert_eq!(e(0x203E, 0, 10).encode(10), Err(CtlError::OutOfRange { value: 10, max: 10 }));
    assert_eq!(e(0x2029, 6, 3).encode(2), Ok((0xC0, 0x80)));
    assert_eq!(e(0x2029, 6, 3).encode(3), Err(CtlError::OutOfRange { value: 3, max: 3 }));
}

#[test]
fn the_table_has_the_controls_the_ucm_names_and_only_registers_the_crate_knows() {
    for c in ctl::VERB.iter().chain(ctl::DEVICE.iter()) {
        assert!(ctl::find(c.ctl).is_some(), "{}", c.ctl);
    }
    let pin = ctl::find("Spk Switch").unwrap();
    assert_eq!(pin.kind, Kind::Pin);
    assert_eq!(pin.encode(1), Err(CtlError::NoRegister));
    assert!(ctl::find("DSM data").is_none(), "DSM is not here (D17)");
    assert!(ctl::find("WAVES data").is_none());
    // no two controls claim overlapping bits of one register unless the Linux table does (the shared registers are known)
    for (i, a) in ctl::CTLS.iter().enumerate() {
        for b in ctl::CTLS.iter().skip(i + 1) {
            if a.kind != Kind::Pin && b.kind != Kind::Pin && a.reg == b.reg && a.mask() & b.mask() != 0 {
                // the only overlap in the table: none
                panic!("{} and {} overlap in {:#06x}", a.name, b.name, a.reg);
            }
        }
    }
}
