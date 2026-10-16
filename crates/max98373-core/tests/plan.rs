// SPDX-License-Identifier: GPL-2.0-only
//! The bring-up, the DAPM power sequences and the safe state, against LINUX LITERALS written out by hand (the expected side never travels
//! through the crate's constants: PORT_CONVENTIONS.md 3, 3a.4). Every line cites the Linux statement it reproduces.
use max98373_core::ctl::Side;
use max98373_core::plan::{self, wire, Amp, Op};

fn ops(p: &plan::Plan) -> Vec<Op> {
    p.steps().iter().map(|s| s.op).collect()
}

fn w(reg: u16, value: u8) -> Op {
    Op::Write { reg, value }
}
fn u(reg: u16, mask: u8, value: u8) -> Op {
    Op::Update { reg, mask, value }
}

#[test]
fn the_pixelbook_go_amplifiers_are_the_coreboot_devicetrees() {
    // devicetree.cb:402-417: MAXR uid 0 at 0x31 (vmon 4, imon 5); MAXL uid 1 at 0x32 (vmon 6, imon 7); boards/max98373.c:23-28 prefixes
    assert_eq!(plan::ATLAS_AMPS, [Amp { side: Side::Right, addr: 0x31, vmon_slot: 4, imon_slot: 5 }, Amp { side: Side::Left, addr: 0x32, vmon_slot: 6, imon_slot: 7 }]);
    // boards/max98373.c:76 and :83: the tdm transmit masks
    assert_eq!(plan::ATLAS_AMPS[0].tx_mask(), 0x30);
    assert_eq!(plan::ATLAS_AMPS[1].tx_mask(), 0xC0);
    // a board that lists anything else has no speaker path
    assert!(plan::atlas_amps(&[0x31, 0x32]).is_some());
    assert!(plan::atlas_amps(&[0x32, 0x31]).is_some());
    assert!(plan::atlas_amps(&[0x31]).is_none());
    assert!(plan::atlas_amps(&[0x31, 0x33]).is_none());
    assert!(plan::atlas_amps(&[]).is_none());
    assert!(plan::atlas_amps(&[0x31, 0x32, 0x1a]).is_none());
}

#[test]
fn the_right_amplifier_is_brought_up_exactly_as_linux_probes_and_formats_it() {
    let p = plan::bring_up(&plan::ATLAS_AMPS[0]).unwrap();
    assert_eq!(
        ops(&p),
        vec![
            w(0x2000, 0x01),       // max98373.c:325-340 software reset
            Op::DelayMs(10),       // :339
            Op::Read { reg: 0x21FF }, // :342
            w(0x2020, 0xFF),       // :362
            w(0x2021, 0xFF),       // :365
            w(0x2029, 0x80),       // :369
            w(0x202A, 0x01),       // :372
            w(0x203F, 0x03),       // :376
            w(0x2046, 0x07),       // :380
            w(0x2022, 0x54),       // :384: imon 5 << 4 | vmon 4
            u(0x2020, 0x10, 0x00), // :388-390 vmon slot 4 out of Hi-Z
            u(0x2020, 0x20, 0x00), // :397-399 imon slot 5
            w(0x20FE, 0x0F),       // :407
            w(0x2023, 0x02),       // :412 spkfb slot default 2 (:504)
            // set_fmt, max98373-i2c.c:121-171: NB_NF, DSP_B
            u(0x2026, 0x10, 0x00),
            u(0x2024, 0x38, 0x18),
            // set_tdm_slot, :317-401: 8 x 16 = 128 BCLK per LRCLK -> table index 4 + 2
            u(0x2026, 0x0F, 0x06),
            u(0x2024, 0xC0, 0x40),
            u(0x2029, 0x0F, 0x00), // rx slot 0
            w(0x202A, 0x01),       // rx slot 1
            w(0x2020, 0xCF),       // ~0x30 & 0xFF
            w(0x2021, 0xFF),       // (~0x30 & 0xFF00) >> 8
            // hw_params, :213-315: S16, 48 kHz
            u(0x2024, 0xC0, 0x40),
            u(0x2027, 0x0F, 0x08),
            u(0x2028, 0xF0, 0x80),
            u(0x2028, 0x0F, 0x08),
        ]
    );
}

#[test]
fn the_left_amplifier_differs_only_in_its_slots() {
    let p = plan::bring_up(&plan::ATLAS_AMPS[1]).unwrap();
    let got = ops(&p);
    let right = ops(&plan::bring_up(&plan::ATLAS_AMPS[0]).unwrap());
    assert_eq!(got.len(), right.len());
    let diff: Vec<usize> = (0..got.len()).filter(|i| got[*i] != right[*i]).collect();
    assert_eq!(diff, vec![9, 10, 11, 20], "TX_SRC_1, the two Hi-Z updates, the final Hi-Z write");
    assert_eq!(got[9], w(0x2022, 0x76)); // imon 7 << 4 | vmon 6
    assert_eq!(got[10], u(0x2020, 0x40, 0x00));
    assert_eq!(got[11], u(0x2020, 0x80, 0x00));
    assert_eq!(got[20], w(0x2020, 0x3F)); // ~0xC0 & 0xFF
}

#[test]
fn the_bclk_table_is_linux_s() {
    // max98373-i2c.c:173-175, :177-186 (index + 2)
    for (bclk, sel) in [(32, 2), (48, 3), (64, 4), (96, 5), (128, 6), (192, 7), (256, 8), (384, 9), (512, 10), (320, 11)] {
        assert_eq!(plan::bclk_sel(bclk), Some(sel), "{bclk}");
    }
    for bad in [0, 16, 31, 33, 100, 1024] {
        assert_eq!(plan::bclk_sel(bad), None);
    }
}

#[test]
fn power_up_is_spk_enable_then_the_amp_enable_widget_then_global_enable_then_a_readback() {
    assert_eq!(
        ops(&plan::power_up().unwrap()),
        vec![
            u(0x2043, 0x01, 0x01),                              // max98373.c:424 (probe), deferred to here
            u(0x202B, 0x01, 0x01),                              // :64-66 Amp Enable's register
            u(0x20FF, 0x01, 0x01),                              // :26-30 POST_PMU
            Op::DelayMs(30),                                    // :31
            Op::Verify { reg: 0x20FF, mask: 0x01, want: 0x01 }, // ours: the part says it is on
        ]
    );
}

#[test]
fn power_down_is_global_disable_then_the_widget_register_then_a_proof() {
    assert_eq!(
        ops(&plan::power_down().unwrap()),
        vec![
            u(0x20FF, 0x01, 0x00), // :33-35 PRE_PMD
            Op::DelayMs(30),       // :37
            u(0x202B, 0x01, 0x00), // :64-66
            Op::Verify { reg: 0x20FF, mask: 0x01, want: 0x00 },
            Op::Verify { reg: 0x202B, mask: 0x01, want: 0x00 },
        ]
    );
    // a protection is never switched OFF on the way down (the UCM DisableSequence would): no write to DHT_EN, BDE_EN or PCM_TX_EN
    for s in plan::power_down().unwrap().steps() {
        if let Op::Write { reg, .. } | Op::Update { reg, .. } = s.op {
            assert!(![0x20D4u16, 0x20B5, 0x202C].contains(&reg), "{:#06x}", reg);
        }
    }
}

#[test]
fn the_emergency_write_and_the_safe_checks() {
    assert_eq!(plan::EMERGENCY_WRITE, (0x20FF, 0x00));
    assert_eq!(plan::SAFE_CHECKS.len(), 2);
    assert_eq!((plan::SAFE_CHECKS[0].reg, plan::SAFE_CHECKS[0].mask, plan::SAFE_CHECKS[0].want), (0x20FF, 0x01, 0x00));
    assert_eq!((plan::SAFE_CHECKS[1].reg, plan::SAFE_CHECKS[1].mask, plan::SAFE_CHECKS[1].want), (0x202B, 0x01, 0x00));
}

#[test]
fn the_i2c_message_forms_are_a_sixteen_bit_big_endian_register() {
    // max98373-i2c.c:503-504 reg_bits = 16, val_bits = 8
    assert_eq!(wire::write(0x20FF, 0x01), [0x20, 0xFF, 0x01]);
    assert_eq!(wire::write(0x2000, 0xAA), [0x20, 0x00, 0xAA]);
    assert_eq!(wire::read_addr(0x21FF), [0x21, 0xFF]);
    assert_eq!(wire::decode_write(&[0x20, 0xFF, 0x01]), Some((0x20FF, 0x01)));
    assert_eq!(wire::decode_write(&[0x20, 0xFF]), None);
    assert_eq!(wire::decode_write(&[0x20, 0xFF, 1, 2]), None);
    assert_eq!(wire::decode_read_addr(&[0x21, 0xFF]), Some(0x21FF));
    assert_eq!(wire::decode_read_addr(&[0x21]), None);
}

#[test]
fn the_ordering_rule_protection_before_enable() {
    // The whole session as the device runs it: bring-up (both amps), the UCM set, its readback, then and only then power-up.
    let up = plan::power_up().unwrap();
    let enable_reg = |op: &Op| matches!(op, Op::Update { reg: 0x20FF, .. } | Op::Update { reg: 0x2043, .. } | Op::Update { reg: 0x202B, .. });
    for a in plan::ATLAS_AMPS {
        assert!(!plan::bring_up(&a).unwrap().steps().iter().any(|s| enable_reg(&s.op) || matches!(s.op, Op::Write { reg: 0x20FF, .. })), "bring-up never enables the part");
    }
    assert!(!plan::ucm_plan().unwrap().iter().any(|(_, s)| enable_reg(&s.op)), "the UCM set never enables the part");
    assert!(up.steps().iter().any(|s| enable_reg(&s.op)));
    // the readback covers every register the set wrote (so nothing is enabled on an unproven protection)
    let written: std::collections::BTreeSet<u16> = plan::ucm_plan().unwrap().iter().filter_map(|(_, s)| if let Op::Update { reg, .. } = s.op { Some(reg) } else { None }).collect();
    let verified: std::collections::BTreeSet<u16> = [Side::Left, Side::Right].into_iter().flat_map(|sd| plan::ucm_verify(sd).unwrap()).filter_map(|s| if let Op::Verify { reg, .. } = s.op { Some(reg) } else { None }).collect();
    assert_eq!(written, verified);
}
