// SPDX-License-Identifier: GPL-2.0-only
//! The UCM protection set, register by register. The EXPECTED side is written out as literals (the image below was derived once, outside
//! the crate, from `HiFi.conf` and the Linux control table with `soc_mixer_control` / `soc_enum` arithmetic; PORT_CONVENTIONS.md 3: never
//! derive an expected list from the table it tests), and the ACTUAL side travels through `ctl::VERB` / `ctl::DEVICE` / `plan::ucm_plan`.
//! When the recovery image's UCM is on this machine the production table is ALSO compared line by line with the owner's file
//! (`CORAL_AVS_REQUIRE_BLOBS=1` makes its absence a failure).
use max98373_core::ctl::{self, Kind, Side};
use max98373_core::plan::{self, Op};

/// verb: (side, UCM control, UCM value, register, mask, bits) in the file's order; a pin switch has register 0.
pub const VERB_IMAGE: &[(&str, &str, u8, u16, u8, u8)] = &[
    ("Left", "DAI Sel Mux", 0, 0x2029, 0xC0, 0x00),
    ("Right", "DAI Sel Mux", 1, 0x2029, 0xC0, 0x40),
    ("Left", "Output Voltage", 4, 0x203E, 0x0F, 0x04),
    ("Right", "Output Voltage", 4, 0x203E, 0x0F, 0x04),
    ("Left", "Speaker Volume", 0, 0x203E, 0xF0, 0x00),
    ("Right", "Speaker Volume", 0, 0x203E, 0xF0, 0x00),
    ("Left", "Digital Volume", 127, 0x203D, 0x7F, 0x00),
    ("Right", "Digital Volume", 127, 0x203D, 0x7F, 0x00),
    ("Left", "DHT Min Volume", 0, 0x20D1, 0xF0, 0x00),
    ("Right", "DHT Min Volume", 0, 0x20D1, 0xF0, 0x00),
    ("Left", "DHT Rot Pnt Volume", 2, 0x20D1, 0x0F, 0x0D),
    ("Right", "DHT Rot Pnt Volume", 2, 0x20D1, 0x0F, 0x0D),
    ("Left", "DHT Attack Step Volume", 0, 0x20D2, 0x38, 0x00),
    ("Right", "DHT Attack Step Volume", 0, 0x20D2, 0x38, 0x00),
    ("Left", "DHT Attack Rate", 2, 0x20D2, 0x07, 0x02),
    ("Right", "DHT Attack Rate", 2, 0x20D2, 0x07, 0x02),
    ("Left", "DHT Release Step Volume", 0, 0x20D3, 0x38, 0x00),
    ("Right", "DHT Release Step Volume", 0, 0x20D3, 0x38, 0x00),
    ("Left", "DHT Release Rate", 3, 0x20D3, 0x07, 0x03),
    ("Right", "DHT Release Rate", 3, 0x20D3, 0x07, 0x03),
    ("Left", "BDE LVL1 Thresh", 120, 0x2097, 0xFF, 0x78),
    ("Right", "BDE LVL1 Thresh", 120, 0x2097, 0xFF, 0x78),
    ("Left", "BDE LVL1 Clip Reduction Volume", 58, 0x20AA, 0x3F, 0x02),
    ("Right", "BDE LVL1 Clip Reduction Volume", 58, 0x20AA, 0x3F, 0x02),
    ("Left", "BDE LVL1 Clip Thresh Volume", 60, 0x20A9, 0x3F, 0x00),
    ("Right", "BDE LVL1 Clip Thresh Volume", 60, 0x20A9, 0x3F, 0x00),
    ("Left", "BDE LVL1 Limiter Thresh Volume", 14, 0x20A8, 0x0F, 0x01),
    ("Right", "BDE LVL1 Limiter Thresh Volume", 14, 0x20A8, 0x0F, 0x01),
    ("Left", "BDE LVL2 Thresh", 117, 0x2098, 0xFF, 0x75),
    ("Right", "BDE LVL2 Thresh", 117, 0x2098, 0xFF, 0x75),
    ("Left", "BDE LVL2 Clip Reduction Volume", 58, 0x20AD, 0x3F, 0x02),
    ("Right", "BDE LVL2 Clip Reduction Volume", 58, 0x20AD, 0x3F, 0x02),
    ("Left", "BDE LVL2 Clip Thresh Volume", 60, 0x20AC, 0x3F, 0x00),
    ("Right", "BDE LVL2 Clip Thresh Volume", 60, 0x20AC, 0x3F, 0x00),
    ("Left", "BDE LVL2 Limiter Thresh Volume", 13, 0x20AB, 0x0F, 0x02),
    ("Right", "BDE LVL2 Limiter Thresh Volume", 13, 0x20AB, 0x0F, 0x02),
    ("Left", "BDE LVL3 Thresh", 114, 0x2099, 0xFF, 0x72),
    ("Right", "BDE LVL3 Thresh", 114, 0x2099, 0xFF, 0x72),
    ("Left", "BDE LVL3 Clip Reduction Volume", 56, 0x20B0, 0x3F, 0x04),
    ("Right", "BDE LVL3 Clip Reduction Volume", 56, 0x20B0, 0x3F, 0x04),
    ("Left", "BDE LVL3 Clip Thresh Volume", 60, 0x20AF, 0x3F, 0x00),
    ("Right", "BDE LVL3 Clip Thresh Volume", 60, 0x20AF, 0x3F, 0x00),
    ("Left", "BDE LVL3 Limiter Thresh Volume", 12, 0x20AE, 0x0F, 0x03),
    ("Right", "BDE LVL3 Limiter Thresh Volume", 12, 0x20AE, 0x0F, 0x03),
    ("Left", "BDE LVL4 Thresh", 112, 0x209A, 0xFF, 0x70),
    ("Right", "BDE LVL4 Thresh", 112, 0x209A, 0xFF, 0x70),
    ("Left", "BDE LVL4 Mute Switch", 0, 0x20B2, 0x80, 0x00),
    ("Right", "BDE LVL4 Mute Switch", 0, 0x20B2, 0x80, 0x00),
    ("Left", "BDE LVL4 Hold Switch", 0, 0x20B2, 0x40, 0x00),
    ("Right", "BDE LVL4 Hold Switch", 0, 0x20B2, 0x40, 0x00),
    ("Left", "BDE LVL4 Clip Reduction Volume", 55, 0x20B3, 0x3F, 0x05),
    ("Right", "BDE LVL4 Clip Reduction Volume", 55, 0x20B3, 0x3F, 0x05),
    ("Left", "BDE LVL4 Clip Thresh Volume", 60, 0x20B2, 0x3F, 0x00),
    ("Right", "BDE LVL4 Clip Thresh Volume", 60, 0x20B2, 0x3F, 0x00),
    ("Left", "BDE LVL4 Limiter Thresh Volume", 11, 0x20B1, 0x0F, 0x04),
    ("Right", "BDE LVL4 Limiter Thresh Volume", 11, 0x20B1, 0x0F, 0x04),
    ("Left", "BDE Hold Time", 100, 0x2090, 0xFF, 0x64),
    ("Right", "BDE Hold Time", 100, 0x2090, 0xFF, 0x64),
    ("Left", "BDE Thresh Hysteresis", 2, 0x209B, 0xFF, 0x02),
    ("Right", "BDE Thresh Hysteresis", 2, 0x209B, 0xFF, 0x02),
    ("Left", "BDE Attack Rate", 0, 0x2091, 0xF0, 0x00),
    ("Right", "BDE Attack Rate", 0, 0x2091, 0xF0, 0x00),
    ("Left", "BDE Release Rate", 5, 0x2091, 0x0F, 0x05),
    ("Right", "BDE Release Rate", 5, 0x2091, 0x0F, 0x05),
    ("Left", "Digital Vol Sel Switch", 1, 0x203F, 0x80, 0x80),
    ("Right", "Digital Vol Sel Switch", 1, 0x203F, 0x80, 0x80),
    ("Left", "Spk Switch", 0, 0x0000, 0x00, 0x00),
    ("Right", "Spk Switch", 0, 0x0000, 0x00, 0x00),
];
/// dev: (side, UCM control, UCM value, register, mask, bits) in the file's order; a pin switch has register 0.
pub const DEVICE_IMAGE: &[(&str, &str, u8, u16, u8, u8)] = &[
    ("Left", "Output Voltage", 5, 0x203E, 0x0F, 0x05),
    ("Right", "Output Voltage", 5, 0x203E, 0x0F, 0x05),
    ("Left", "DHT Switch", 1, 0x20D4, 0x01, 0x01),
    ("Right", "DHT Switch", 1, 0x20D4, 0x01, 0x01),
    ("Left", "BDE Switch", 1, 0x20B5, 0x01, 0x01),
    ("Right", "BDE Switch", 1, 0x20B5, 0x01, 0x01),
    ("Left", "VI Sense Switch", 1, 0x202C, 0x01, 0x01),
    ("Right", "VI Sense Switch", 1, 0x202C, 0x01, 0x01),
    ("Left", "Spk Switch", 1, 0x0000, 0x00, 0x00),
    ("Right", "Spk Switch", 1, 0x0000, 0x00, 0x00),
];

fn side_name(s: Side) -> &'static str {
    s.prefix()
}

#[test]
fn the_verb_is_68_csets_and_the_speaker_device_adds_ten() {
    assert_eq!(VERB_IMAGE.len(), 68, "the verb's EnableSequence has 68 csets besides the two cset-tlv blobs");
    assert_eq!(DEVICE_IMAGE.len(), 10);
    assert_eq!(ctl::VERB.len() * 2, 68);
    assert_eq!(ctl::DEVICE.len() * 2, 10);
    assert_eq!(ctl::csets(&ctl::VERB).count(), 68);
    assert_eq!(ctl::csets(&ctl::DEVICE).count(), 10);
}

#[test]
fn every_production_cset_is_the_ucm_line_in_the_ucm_order() {
    for (list, image) in [(&ctl::VERB[..], VERB_IMAGE), (&ctl::DEVICE[..], DEVICE_IMAGE)] {
        let got: Vec<_> = ctl::csets(list).collect();
        assert_eq!(got.len(), image.len());
        for (g, (side, name, value, _, _, _)) in got.iter().zip(image) {
            assert_eq!((side_name(g.side), g.ctl, g.value), (*side, *name, *value));
        }
    }
}

#[test]
fn every_cset_becomes_the_register_bits_linux_would_write() {
    for (list, image) in [(&ctl::VERB[..], VERB_IMAGE), (&ctl::DEVICE[..], DEVICE_IMAGE)] {
        for (c, (side, name, value, reg, mask, bits)) in ctl::csets(list).zip(image) {
            let ctl = ctl::find(c.ctl).unwrap_or_else(|| panic!("{} has no control", c.ctl));
            if ctl.kind == Kind::Pin {
                assert_eq!((*reg, *mask, *bits), (0, 0, 0), "{side} {name}: a pin has no register");
                assert_eq!(ctl.encode(c.value), Err(ctl::CtlError::NoRegister));
                continue;
            }
            assert_eq!(ctl.reg, *reg, "{side} {name} register");
            assert_eq!(ctl.encode(c.value), Ok((*mask, *bits)), "{side} {name} = {value}");
        }
    }
}

#[test]
fn the_plan_applies_exactly_those_updates_in_that_order() {
    let plan = plan::ucm_plan().unwrap();
    let want: Vec<_> = VERB_IMAGE.iter().chain(DEVICE_IMAGE).filter(|r| r.3 != 0).collect();
    assert_eq!(plan.len(), want.len(), "a pin switch is the only cset with no step");
    for ((side, step), (wside, name, _, reg, mask, bits)) in plan.iter().zip(want) {
        assert_eq!(side_name(*side), *wside);
        assert_eq!(step.label, *name);
        assert_eq!(step.op, Op::Update { reg: *reg, mask: *mask, value: *bits }, "{wside} {name}");
    }
    // the protections are all in before the device section (DHT, BDE, VI sense switch on) and the enable-time output voltage
    let first_dev = plan.iter().position(|(_, s)| s.label == "Output Voltage" && matches!(s.op, Op::Update { value: 5, .. })).unwrap();
    assert_eq!(first_dev, 66, "66 register updates (68 minus the two pin switches) precede the speaker device section");
}

#[test]
fn the_readback_proves_the_final_value_of_each_shared_register() {
    // 0x203E holds Output Voltage (low nibble: 4 then 5) and Speaker Volume (high nibble: 0): the last write left 5 / 0
    let v = plan::ucm_verify(Side::Left).unwrap();
    let r = v.iter().find(|s| matches!(s.op, Op::Verify { reg: 0x203E, .. })).unwrap();
    assert_eq!(r.op, Op::Verify { reg: 0x203E, mask: 0xFF, want: 0x05 });
    // 0x2029: DAI Sel Mux (Left = 0 -> bits 7:6 = 00; Right = 1 -> 01)
    assert!(v.iter().any(|s| s.op == Op::Verify { reg: 0x2029, mask: 0xC0, want: 0x00 }));
    let vr = plan::ucm_verify(Side::Right).unwrap();
    assert!(vr.iter().any(|s| s.op == Op::Verify { reg: 0x2029, mask: 0xC0, want: 0x40 }));
    // protections on: DHT_EN, BDE_EN, PCM_TX_EN (VI sense)
    for reg in [0x20D4u16, 0x20B5, 0x202C] {
        assert!(v.iter().any(|s| s.op == Op::Verify { reg, mask: 0x01, want: 0x01 }), "{reg:#06x} must be read back ON");
    }
    // the digital volume is 0 dB (register 0 after the inversion), the speaker gain 0
    assert!(v.iter().any(|s| s.op == Op::Verify { reg: 0x203D, mask: 0x7F, want: 0x00 }));
}

fn ucm_path() -> std::path::PathBuf {
    let d = std::env::var("CORAL_AVS_UCM_DIR").unwrap_or_else(|_| "/home/peter/Projects/coral_handoffs/atlas/recovery/configs/usr/share/alsa/ucm/avs_max98373".to_string());
    std::path::Path::new(&d).join("HiFi.conf")
}

type Row = (String, String, String);

/// The owner's file, section by section: `(SectionVerb csets, SectionDevice."Speaker" EnableSequence csets)`.
fn read_ucm() -> Option<(Vec<Row>, Vec<Row>)> {
    let p = ucm_path();
    if !p.exists() {
        assert!(std::env::var("CORAL_AVS_REQUIRE_BLOBS").is_err(), "HiFi.conf is required (CORAL_AVS_REQUIRE_BLOBS) and is not at {}", p.display());
        eprintln!("HiFi.conf: SKIPPED (set CORAL_AVS_UCM_DIR)");
        return None;
    }
    let text = std::fs::read_to_string(p).unwrap();
    let (mut verb, mut dev) = (vec![], vec![]);
    let mut sect = 0; // 1 verb, 2 speaker enable
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with("SectionVerb") {
            sect = 1;
        } else if t.starts_with("SectionDevice.\"Speaker\"") {
            sect = 2;
        } else if t.starts_with("SectionDevice.") || t.starts_with("DisableSequence") {
            sect = 0;
        }
        if let Some(rest) = t.strip_prefix("cset \"name='") {
            let (name, tail) = rest.split_once("' ").unwrap();
            let value = tail.trim_end_matches('"').to_string();
            let (side, ctl) = name.split_once(' ').unwrap();
            let row = (side.to_string(), ctl.to_string(), value);
            match sect {
                1 => verb.push(row),
                2 => dev.push(row),
                _ => {}
            }
        }
    }
    Some((verb, dev))
}

#[test]
fn the_production_tables_are_the_owners_file_line_for_line() {
    let Some((verb, dev)) = read_ucm() else { return };
    let text_of = |ctl: &str, v: u8| -> String {
        match (ctl, v) {
            ("DAI Sel Mux", 0) => "Left".into(),
            ("DAI Sel Mux", 1) => "Right".into(),
            ("Spk Switch" | "DHT Switch" | "BDE Switch" | "VI Sense Switch", 0) => "off".into(),
            ("Spk Switch" | "DHT Switch" | "BDE Switch" | "VI Sense Switch", 1) => "on".into(),
            _ => v.to_string(),
        }
    };
    for (list, file) in [(&ctl::VERB[..], verb), (&ctl::DEVICE[..], dev)] {
        let got: Vec<Row> = ctl::csets(list).map(|c| (side_name(c.side).to_string(), c.ctl.to_string(), text_of(c.ctl, c.value))).collect();
        assert_eq!(got, file);
    }
}
