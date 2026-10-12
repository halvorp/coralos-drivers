// SPDX-License-Identifier: GPL-2.0-only
//! The op sequences of dsp.c / cldma.c / loader.c, pinned register by register against the Linux literals.
use avs_fw_core::bdl::*;
use avs_fw_core::effect::*;
use avs_fw_core::programs::*;
use avs_fw_core::runner::*;

fn dsp32(off: u32) -> Reg {
    Reg { space: Space::Dsp, off, width: Width::D32 }
}
fn dsp8(off: u32) -> Reg {
    Reg { space: Space::Dsp, off, width: Width::B8 }
}
fn poll(reg: Reg, mask: u32, want: u32, interval_us: u32, timeout_us: u32) -> Op {
    Op::Poll(PollSpec { reg, mask, want, interval_us, timeout_us, max_polls: timeout_us / interval_us + 2 })
}
fn ops(p: &Program) -> Vec<Op> {
    p.as_slice().iter().map(|s| s.op).collect()
}
fn fails(p: &Program) -> Vec<OnFail> {
    p.as_slice().iter().map(|s| s.on_fail).collect()
}

#[test]
fn core_power_on_and_off_for_the_main_core() {
    // dsp.c:17-44 with AVS_MAIN_CORE_MASK = 1
    let on = core_power(1, true, OnFail::Abort);
    assert_eq!(ops(&on), vec![Op::Update { reg: dsp32(0x04), mask: 0x1_0000, value: 0x1_0000 }, Op::DelayUs(1000), poll(dsp32(0x04), 0x100_0000, 0x100_0000, 500, 10000)]);
    assert_eq!(fails(&on), vec![OnFail::Abort, OnFail::Tolerate, OnFail::Abort]);
    let off = core_power(1, false, OnFail::Tolerate);
    assert_eq!(ops(&off), vec![Op::Update { reg: dsp32(0x04), mask: 0x1_0000, value: 0 }, Op::DelayUs(1000), poll(dsp32(0x04), 0x100_0000, 0, 500, 10000)]);
}

#[test]
fn core_power_for_two_cores_shifts_both_bits() {
    // APL's core_init_mask is 3 (core.c:829); the shifts must carry both bits
    let on = core_power(3, true, OnFail::Abort);
    assert_eq!(ops(&on)[0], Op::Update { reg: dsp32(0x04), mask: 0x3_0000, value: 0x3_0000 });
    assert_eq!(ops(&on)[2], poll(dsp32(0x04), 0x300_0000, 0x300_0000, 500, 10000));
}

#[test]
fn core_reset_enter_and_exit() {
    // dsp.c:46-68
    assert_eq!(ops(&core_reset(1, false, OnFail::Abort)), vec![Op::Update { reg: dsp32(0x04), mask: 1, value: 0 }, poll(dsp32(0x04), 1, 0, 500, 10000)]);
    assert_eq!(ops(&core_reset(1, true, OnFail::Abort)), vec![Op::Update { reg: dsp32(0x04), mask: 1, value: 1 }, poll(dsp32(0x04), 1, 1, 500, 10000)]);
}

#[test]
fn core_stall_and_unstall() {
    // dsp.c:70-96 - note the 1 ms propagation delay after the poll
    assert_eq!(
        ops(&core_stall(1, false, OnFail::Abort)),
        vec![Op::Update { reg: dsp32(0x04), mask: 0x100, value: 0 }, poll(dsp32(0x04), 0x100, 0, 500, 10000), Op::DelayUs(1000)]
    );
    assert_eq!(
        ops(&core_stall(1, true, OnFail::Abort)),
        vec![Op::Update { reg: dsp32(0x04), mask: 0x100, value: 0x100 }, poll(dsp32(0x04), 0x100, 0x100, 500, 10000), Op::DelayUs(1000)]
    );
}

#[test]
fn cldma_stop_and_reset() {
    // cldma.c:151-196. SD_CTL is a BYTE register at BAR4+0x80
    let stop = cldma_stop(OnFail::Abort);
    assert_eq!(
        ops(&stop),
        vec![
            Op::Update { reg: dsp32(0x08), mask: 0x2, value: 0 },
            Op::Update { reg: dsp8(0x80), mask: 0x1E, value: 0 },
            poll(dsp8(0x80), 0x02, 0, 3, 300)
        ]
    );
    let reset = cldma_reset(OnFail::Abort);
    let mut want = ops(&stop);
    want.extend([
        Op::Update { reg: dsp8(0x80), mask: 1, value: 1 },
        poll(dsp8(0x80), 1, 1, 3, 300),
        Op::Update { reg: dsp8(0x80), mask: 1, value: 0 },
        poll(dsp8(0x80), 1, 0, 3, 300),
    ]);
    assert_eq!(ops(&reset), want);
}

#[test]
fn cldma_setup_writes_the_bdl_geometry() {
    // cldma.c:234-249. BDL at 0x1_ABCD_E0F0 -> low dword masked to GENMASK(31,7) = ...E080, high dword 1
    let l = ClDmaLayout::plan(&[Extent { addr: 0x2_0000_0000, len: 128 * 1024 }], 0x1_ABCD_E0F0, 128 * 1024, 1).unwrap();
    let p = cldma_setup(&l);
    assert_eq!(
        ops(&p),
        vec![
            Op::Write { reg: dsp32(0x98), value: 0xABCD_E080 },
            Op::Write { reg: dsp32(0x9c), value: 1 },
            Op::Write { reg: dsp32(0x88), value: 131072 },
            Op::Write { reg: dsp8(0x8c), value: 1 },
            Op::Update { reg: dsp32(0x80), mask: 0x00F0_0000, value: 0x0010_0000 },
            Op::Write { reg: dsp32(0xa4), value: 1 },
        ]
    );
    // the tag lands in bits 23:20, another tag proves it is the tag and not a constant
    let l7 = ClDmaLayout::plan(&[Extent { addr: 0, len: 128 * 1024 }], 0, 128 * 1024, 7).unwrap();
    assert_eq!(ops(&cldma_setup(&l7))[4], Op::Update { reg: dsp32(0x80), mask: 0x00F0_0000, value: 0x0070_0000 });
}

#[test]
fn int_control_enables_the_ipc_interrupt_and_the_hipc_done_busy_bits() {
    // ipc.c:541-558
    assert_eq!(
        ops(&int_control(true, OnFail::Abort)),
        vec![Op::Update { reg: dsp32(0x08), mask: 1, value: 1 }, Op::Update { reg: dsp32(0x50), mask: 3, value: 3 }]
    );
    assert_eq!(
        ops(&int_control(false, OnFail::Abort)),
        vec![Op::Update { reg: dsp32(0x08), mask: 1, value: 0 }, Op::Update { reg: dsp32(0x50), mask: 3, value: 0 }]
    );
}

#[test]
fn gating_disabled_sets_pgctl_lsrmd_and_clears_cgctl_and_l1sen() {
    // core.c:54-88: value = enable ? 0 : pgctl_mask; value = enable ? cgctl_mask : 0; L1SEN cleared when disabling
    let pci = |off| Reg { space: Space::PciConfig, off, width: Width::D32 };
    let hda = Reg { space: Space::Hda, off: 0x1030, width: Width::D32 };
    assert_eq!(
        ops(&gating(false)),
        vec![
            Op::Update { reg: pci(0x44), mask: 0x10, value: 0x10 },
            Op::Update { reg: pci(0x48), mask: 0x40, value: 0 },
            Op::Update { reg: hda, mask: 0x2000, value: 0 }
        ]
    );
    assert_eq!(
        ops(&gating(true)),
        vec![
            Op::Update { reg: pci(0x44), mask: 0x10, value: 0 },
            Op::Update { reg: pci(0x48), mask: 0x40, value: 0x40 },
            Op::Update { reg: hda, mask: 0x2000, value: 0x2000 }
        ]
    );
}

#[test]
fn basefw_pre_is_the_front_half_of_avs_cldma_load_basefw_in_order() {
    let l = ClDmaLayout::plan(&[Extent { addr: 0x1000_0000, len: 128 * 1024 }], 0x2000, 128 * 1024, 1).unwrap();
    let p = basefw_pre(&l);
    let labels: Vec<&str> = p.as_slice().iter().map(|s| s.label).collect();
    assert_eq!(
        labels,
        vec![
            "core-power-on", "core-power-on-settle", "core-power-on-cpa", // loader.c:144
            "core-reset-exit", "core-reset-exit-ack",                      // :148
            "cldma-irq-off", "cldma-run-off", "cldma-stopped", "cldma-srst-set", "cldma-srst-ack", "cldma-srst-clear", "cldma-srst-clear-ack", // :152
            "cldma-bdlpl", "cldma-bdlpu", "cldma-cbl", "cldma-lvi", "cldma-stream-tag", "cldma-spib-enable", // :157
            "core-unstall", "core-unstall-ack", "core-unstall-settle", // :159
            "int-ipc", "int-hipcctl",                                 // :163-164
            "rom-init",                                               // :167
        ]
    );
    let last = *p.as_slice().last().unwrap();
    // loader.c:167-169: STATUS & 1 == 1, 5 us polling, 1 s timeout; failure disables the core (:173)
    assert_eq!(last.op, poll(dsp32(0x8000), 1, 1, 5, 1_000_000));
    assert_eq!(last.on_fail, OnFail::AbortDisable);
    // every earlier step that can fail returns the error with nothing undone (:144-162)
    for s in &p.as_slice()[..p.len() - 1] {
        assert!(matches!(s.on_fail, OnFail::Abort | OnFail::Tolerate), "{}", s.label);
    }
    assert_eq!(p.as_slice()[0].on_fail, OnFail::Abort);
}

#[test]
fn basefw_post_polls_for_entered_then_stops_the_loader_then_waits_for_fw_ready() {
    let p = basefw_post();
    let labels: Vec<&str> = p.as_slice().iter().map(|s| s.label).collect();
    assert_eq!(labels, vec!["basefw-entered", "cldma-irq-off", "cldma-run-off", "cldma-stopped", "fw-ready"]);
    // loader.c:180-182: (STATUS & 0xFF) == 0xF, 500 us polling, 3 s; failure: stop + disable (:183-188)
    assert_eq!(p.as_slice()[0].op, poll(dsp32(0x8000), 0xFF, 0xF, 500, 3_000_000));
    assert_eq!(p.as_slice()[0].on_fail, OnFail::AbortStopDisable);
    // :183 the stop's own result is ignored
    assert!(p.as_slice()[1..4].iter().all(|s| s.on_fail == OnFail::Tolerate));
    // :589-596
    assert_eq!(p.as_slice()[4].op, Op::Wait { event: Event::FwReady, timeout_ms: 3000 });
    assert_eq!(p.as_slice()[4].on_fail, OnFail::AbortDisable);
}

#[test]
fn every_program_fits_its_storage() {
    let l = ClDmaLayout::plan(&[Extent { addr: 0, len: 128 * 1024 }], 0, 128 * 1024, 1).unwrap();
    for (name, p) in [
        ("basefw_pre", basefw_pre(&l)),
        ("basefw_post", basefw_post()),
        ("core_disable", core_disable(1)),
        ("core_disable_cleanup", core_disable_cleanup()),
        ("stop_then_disable_cleanup", stop_then_disable_cleanup()),
        ("cldma_reset", cldma_reset(OnFail::Abort)),
        ("cldma_setup", cldma_setup(&l)),
        ("gating", gating(true)),
        ("int_control", int_control(true, OnFail::Abort)),
    ] {
        assert!(!p.overflowed(), "{name} overflowed");
        assert!(p.len() < PROGRAM_CAP, "{name} uses {} of {PROGRAM_CAP}", p.len());
    }
}
