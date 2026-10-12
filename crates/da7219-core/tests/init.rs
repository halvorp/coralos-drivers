// SPDX-License-Identifier: GPL-2.0-only
//! Codec initialisation: probe order, identity check, the state it leaves, and the helpers around it.
mod common;
use common::*;
use da7219_core::init::*;
use da7219_core::runner::*;
use da7219_core::safe::first_unsafe;

#[test]
fn the_pdata_values_are_the_atlas_acpi_ones() {
    // micbias_lvl 2600 mV = enum index 5 (include/sound/da7219.h:15-20), mic_amp_in_sel diff = 0 (:25)
    assert_eq!(ATLAS_PDATA.micbias as u8, 5);
    assert_eq!(ATLAS_PDATA.mic_amp_in as u8, 0);
    assert_eq!(ATLAS_PDATA.io_voltage_level, 0x01, "1.2-2.8 V range (da7219.h:574)");
    assert_eq!((CHIP_ID1, CHIP_ID2), (0x23, 0x93)); // da7219.c:2368-2369
}

#[test]
fn init_runs_in_the_probe_order_and_leaves_a_verified_safe_codec() {
    let mut f = Fake::reset();
    f.run(init_plan(ATLAS_PDATA).unwrap()).unwrap();
    let labels: Vec<_> = f.steps.iter().copied().collect();
    let order = ["accdet-off", "soft-reset", "system-active-off", "system-active-on", "register-shadow-reinit", "io-voltage-level", "chip-id-1", "chip-id-2", "micbias-level", "mic-amp-input", "pc-count-freerun", "ramp-hp-r", "min-gain-hp-l", "tone-gen-cycles", "master-bias"];
    let mut at = 0;
    for want in order {
        let i = labels[at..].iter().position(|l| *l == want).unwrap_or_else(|| panic!("{want} out of order in {labels:?}"));
        at += i + 1;
    }
    assert_eq!(f.reg(0x91), 0x01, "IO_CTRL");
    assert_eq!(f.reg(0x62), 5, "MICBIAS_CTRL level 2.6 V");
    assert_eq!(f.reg(0x10), 0, "MIC_1_SELECT diff");
    assert_eq!(f.reg(0x32) & 0x08, 0x08, "master bias");
    assert_eq!(f.reg(0x94) & 1, 1, "PC counter free-running");
    assert_eq!(f.reg(0xb6), 0x07, "tone generator cycles: infinite");
    for (r, bit) in [(0x65u8, 0x20u8), (0x67, 0x20), (0x69, 0x20), (0x6a, 0x20), (0x6b, 0x20), (0x6c, 0x20), (0x6b, 0x04), (0x6c, 0x04)] {
        assert_eq!(f.reg(r) & bit, bit, "reg {r:#x} bit {bit:#x}");
    }
    assert_eq!(first_unsafe(|r| f.reg(r)), None);
    assert_only_codec(&f);
}

#[test]
fn a_part_with_the_wrong_identity_is_refused_by_name_before_it_is_configured() {
    for (reg, step, got) in [(0x81usize, "chip-id-1", 0x24u8), (0x82, "chip-id-2", 0x00)] {
        let mut f = Fake::reset();
        f.stuck.push((reg as u8, 0xff)); // the fake's ID registers cannot be corrected by a write
        f.regs[reg] = got;
        let e = f.run(init_plan(ATLAS_PDATA).unwrap()).unwrap_err();
        assert_eq!(e.step, step);
        assert!(matches!(e.cause, Cause::VerifyMismatch { got: g, .. } if g == got));
        assert!(!f.steps.contains(&"micbias-level"), "nothing past the identity check ran");
        assert!(!f.steps.contains(&"master-bias"));
    }
}

#[test]
fn quiesce_stops_a_running_codec_with_a_bounded_wait_and_tolerates_a_status_that_never_clears() {
    let mut f = Fake::reset();
    f.regs[0xe0] = 0x01; // SYSTEM_STATUS never clears
    let t = f.run(quiesce_plan().unwrap()).unwrap();
    assert_eq!(t, Some(Failure { step: "system-status-clear", cause: Cause::PollBound { reads: 6, last: 1 } }));
    assert_eq!(f.poll_reads, 6, "DA7219_SYS_STAT_CHECK_RETRIES");
    assert_eq!((f.reg(0x92), f.reg(0x50), f.reg(0x51)), (1, 0, 1), "ramp nominal, inputs off, outputs mode 1");
    assert_eq!(quiesce_plan().unwrap().max_wait_ms(), 6 * 50);
    let mut g = Fake::reset();
    assert!(g.run(quiesce_plan().unwrap()).unwrap().is_none());
    assert_eq!(g.poll_reads, 1);
}

#[test]
fn the_system_active_probe_is_a_single_read() {
    let p = read_system_active_plan();
    assert_eq!(p.len(), 1);
    let mut f = Fake::reset();
    f.regs[0xfd] = 1;
    f.run(p).unwrap();
}

#[test]
fn only_a_chip_revision_with_minor_zero_needs_the_aa_patch() {
    assert!(needs_rev_aa_patch(0x00));
    assert!(needs_rev_aa_patch(0x10), "major does not matter (da7219.h:566)");
    assert!(!needs_rev_aa_patch(0x01));
    assert!(!needs_rev_aa_patch(0x0f));
    let mut f = Fake::reset();
    f.run(rev_aa_patch_plan()).unwrap();
    assert_eq!(f.reg(0x32), 0x08, "REFERENCES = 0x08 (da7219.c:2450-2452)");
}

#[test]
fn bias_off_clears_only_the_master_bias_bit() {
    let mut f = Fake::reset();
    f.regs[0x32] = 0x08 | 0x01;
    f.run(bias_off_plan()).unwrap();
    assert_eq!(f.reg(0x32), 0x01);
}

#[test]
fn init_fits_its_cap_with_room_to_spare() {
    let p = init_plan(ATLAS_PDATA).unwrap();
    assert!(!p.overflowed() && p.len() <= 48, "{}", p.len());
}
