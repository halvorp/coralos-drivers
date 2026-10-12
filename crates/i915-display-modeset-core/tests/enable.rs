// SPDX-License-Identifier: GPL-2.0-only
//! The eDP pipe-enable transaction: step ORDER against `hsw_crtc_enable()` (and the commit-tail / encoder hooks it calls), plane and
//! watermark values, rollback restores, readbacks, named refusals, and that every wait is bounded.
mod common;
use common::*;
use i915_display_clock_core::{Step, WritePlan};
use i915_display_link_core::plan::Effect;
use i915_display_modeset_core::enable::*;
use i915_display_modeset_core::plan::{Body, Interactive, StageId};

fn build() -> (Fixture, EnableTransaction) {
    let f = fixture();
    let pre = f.pre.clone();
    let snap = move |r: u32| pre.get(&r).copied().unwrap_or(0);
    let tx = build_enable(&inputs(&f, &snap)).unwrap();
    (f, tx)
}

#[test]
fn stage_order_is_hsw_crtc_enable() {
    let (_, tx) = build();
    let got: Vec<&str> = tx.order().iter().map(|s| s.name()).collect();
    // Each line cites the Linux call that PRECEDES the next one (drivers/gpu/drm/i915/display/, v7.0-rc6).
    let want = [
        "POWER-DOMAINS",     // intel_atomic_commit_tail: intel_modeset_get_crtc_power_domains (before cdclk)
        "CDCLK-SET",         // intel_set_cdclk_pre_plane_update
        "DBUF-SLICE",        // intel_dbuf_pre_plane_update
        "AUX-POWER",         // hsw_crtc_enable -> intel_encoders_pre_pll_enable -> intel_ddi_pre_pll_enable
        "DPLL-ENABLE",       // intel_dpll_enable (after pre_pll_enable, before pre_enable)
        "PPS-PANEL-ON",      // intel_encoders_pre_enable -> hsw_ddi_pre_enable_dp: intel_ddi_init_dp_buf_reg, intel_pps_on
        "DDI-CLOCK-ENABLE",  //   intel_ddi_enable_clock
        "DDI-IO-POWER",      //   intel_display_power_get(ddi_io_power_domain)
        "DDI-BUF-TRANS",     //   hsw_prepare_dp_ddi_buffers
        "SIGNAL-LEVELS",     //   encoder->set_signal_levels
        "SINK-POWER-D0",     //   intel_dp_set_power(D0)
        "LINK-TRAIN",        //   intel_dp_start_link_train
        "STOP-LINK-TRAIN",   //   intel_dp_stop_link_train
        "TRANSCODER-CLOCK",  //   intel_ddi_enable_transcoder_clock (end of hsw_ddi_pre_enable_dp)
        "PIPE-SRC",          // hsw_crtc_enable: intel_set_pipe_src_size
        "PIPE-MISC",         //   bdw_set_pipe_misc
        "LINK-MN",           // hsw_configure_cpu_transcoder: intel_cpu_transcoder_set_m1_n1
        "TRANSCODER-TIMINGS",//   intel_set_transcoder_timings
        "FRAME-START-DELAY", //   hsw_set_frame_start_delay
        "TRANSCONF",         //   hsw_set_transconf
        "PANEL-FIT",         // skl_pfit_enable
        "COLOR-MODESET",     // intel_color_modeset
        "LINETIME-WM",       // hsw_set_linetime_wm  (intel_initial_watermarks: no hook on ver 9)
        "DDI-FUNC-CTL",      // intel_encoders_enable -> intel_ddi_enable: intel_ddi_enable_transcoder_func
        "TRANSCODER-ENABLE", //   intel_enable_transcoder
        "BACKLIGHT-PWM",     //   intel_ddi_enable_dp -> intel_edp_backlight_on: intel_backlight_enable
        "PPS-BACKLIGHT-ON",  //   intel_pps_backlight_on
        "PLANE-NOARM",       // intel_update_crtc: skl_plane_update_noarm (stride, pos, size, watermarks)
        "PLANE-ARM",         //   skl_plane_update_arm (PLANE_SURF last)
        "CURSOR-WM",         //   the cursor's update_arm: skl_write_cursor_wm
    ];
    assert_eq!(got, want);
}

#[test]
fn ordering_constraints_that_matter() {
    let (_, tx) = build();
    let pos = |id| tx.order().iter().position(|s| *s == id).unwrap();
    // power before clocks before the link; the link trains before the transcoder is configured; the pipe runs before planes arm;
    // the panel backlight comes after the pipe (PWM enable THEN the PPS backlight bit, intel_dp.c intel_edp_backlight_on).
    assert!(pos(StageId::PowerDomains) < pos(StageId::CdclkSet));
    assert!(pos(StageId::CdclkSet) < pos(StageId::DpllEnable));
    assert!(pos(StageId::DpllEnable) < pos(StageId::DdiClockEnable));
    assert!(pos(StageId::PpsPanelOn) < pos(StageId::LinkTrain));
    assert!(pos(StageId::DdiBufTrans) < pos(StageId::SignalLevels));
    assert!(pos(StageId::SinkPowerD0) < pos(StageId::LinkTrain));
    assert!(pos(StageId::LinkTrain) < pos(StageId::StopLinkTrain));
    assert!(pos(StageId::StopLinkTrain) < pos(StageId::PipeSrc));
    assert!(pos(StageId::LinkMn) < pos(StageId::TranscoderTimings));
    assert!(pos(StageId::Transconf) < pos(StageId::DdiFuncCtl));
    assert!(pos(StageId::DdiFuncCtl) < pos(StageId::TranscoderEnable));
    assert!(pos(StageId::TranscoderEnable) < pos(StageId::BacklightPwm));
    assert!(pos(StageId::BacklightPwm) < pos(StageId::PpsBacklightOn));
    assert!(pos(StageId::PpsBacklightOn) < pos(StageId::PlaneNoarm));
    assert!(pos(StageId::PlaneNoarm) < pos(StageId::PlaneArm));
    assert!(pos(StageId::PlaneArm) < pos(StageId::CursorWm));
}

#[test]
fn every_stage_names_its_linux_function_and_has_a_rollback_or_a_reason() {
    let (_, tx) = build();
    for s in &tx.stages {
        assert!(!s.linux.is_empty(), "{:?}", s.id);
        match (&s.forward, &s.rollback) {
            (Body::Empty(why), Body::Empty(why2)) => assert!(!why.is_empty() && !why2.is_empty(), "{}", s.id.name()),
            (Body::Empty(_), _) | (_, Body::Empty(_)) => panic!("{}: one side empty", s.id.name()),
            _ => {}
        }
    }
    // The stages whose forward body is a non-empty LOCAL plan all have a non-empty rollback.
    for id in [StageId::PipeSrc, StageId::PipeMisc, StageId::LinkMn, StageId::TranscoderTimings, StageId::FrameStartDelay, StageId::Transconf, StageId::LinetimeWm, StageId::DdiFuncCtl, StageId::TranscoderEnable, StageId::PlaneNoarm, StageId::PlaneArm, StageId::CursorWm] {
        let s = tx.stage(id).unwrap();
        match &s.rollback {
            Body::Link(p) => assert!(!p.is_empty(), "{}", id.name()),
            other => panic!("{}: rollback {other:?}", id.name()),
        }
    }
}

#[test]
fn interactive_stages_pair_with_their_inverses() {
    let (_, tx) = build();
    let pairs = [
        (StageId::PpsPanelOn, Interactive::PpsPanelOn, Interactive::PpsPanelOff),
        (StageId::SinkPowerD0, Interactive::SinkPowerD0, Interactive::SinkPowerD3),
        (StageId::PpsBacklightOn, Interactive::PpsBacklightOn, Interactive::PpsBacklightOff),
    ];
    for (id, f, r) in pairs {
        let s = tx.stage(id).unwrap();
        assert_eq!(s.forward, Body::Interactive(f));
        assert_eq!(s.rollback, Body::Interactive(r));
    }
    let lt = tx.stage(StageId::LinkTrain).unwrap();
    assert_eq!(lt.forward, Body::Interactive(Interactive::LinkTraining));
    assert!(matches!(&lt.rollback, Body::Link(p) if !p.is_empty()), "training is undone by DdiLink::buf_disable");
}

#[test]
fn composed_stages_carry_the_other_crates_plans_unchanged() {
    let (f, tx) = build();
    let cd = f.sub.cdclk.as_ref().unwrap();
    assert_eq!(tx.stage(StageId::CdclkSet).unwrap().forward, Body::Clock(cd.forward));
    assert_eq!(tx.stage(StageId::CdclkSet).unwrap().rollback, Body::Clock(cd.rollback));
    for (id, t) in [
        (StageId::PowerDomains, &f.sub.power_crtc),
        (StageId::AuxPower, &f.sub.power_aux),
        (StageId::DpllEnable, &f.sub.dpll),
        (StageId::DdiClockEnable, &f.sub.ddi_clock),
        (StageId::DdiIoPower, &f.sub.power_ddi_io),
    ] {
        let s = tx.stage(id).unwrap();
        assert_eq!(s.forward, Body::Clock(t.forward), "{}", id.name());
        assert_eq!(s.rollback, Body::Clock(t.rollback), "{}", id.name());
    }
    assert_eq!(tx.stage(StageId::BacklightPwm).unwrap().forward, Body::Link(f.sub.backlight_enable.clone()));
    assert_eq!(tx.stage(StageId::BacklightPwm).unwrap().rollback, Body::Link(f.sub.backlight_disable.clone()));
    assert_eq!(tx.stage(StageId::LinkTrain).unwrap().rollback, Body::Link(f.sub.ddi_buf_disable.clone()));
}

fn writes(b: &Body) -> Vec<(u32, u32)> {
    match b {
        Body::Link(p) => p
            .iter()
            .filter_map(|e| match e {
                Effect::Write { reg, value } => Some((reg.offset(), *value)),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

#[test]
fn transcoder_stage_values_match_linux() {
    let (_, tx) = build();
    let w = |id| writes(&tx.stage(id).unwrap().forward);
    assert_eq!(w(StageId::PipeSrc), [(0x6001c, 0x077f_0437)]); // PIPESRC(pipe A): transcoder-A offset, not the eDP block
    assert_eq!(w(StageId::PipeMisc), [(0x70030, 0)]); // PIPE_MISC_BPC_8
    assert_eq!(
        w(StageId::LinkMn),
        [(0x6f030, 0x7e34_cccc), (0x6f034, 8388608), (0x6f040, 288358), (0x6f044, 524288)] // gcc build of intel_link_compute_m_n: 4 lanes, 148.5 MHz, HBR
    );
    assert_eq!(
        w(StageId::TranscoderTimings),
        [(0x6f028, 0), (0x6f000, 0x0897_077f), (0x6f004, 0x0897_077f), (0x6f008, 0x0803_07d7), (0x6f00c, 0x0464_0437), (0x6f010, 0x0464_0437), (0x6f014, 0x0440_043b)]
    );
    assert_eq!(w(StageId::Transconf), [(0x7f008, 0)]); // not enabled yet
    assert_eq!(w(StageId::LinetimeWm), [(0x45270, 119)]);
    assert_eq!(w(StageId::DdiFuncCtl), [(0x6f400, 0x8203_0006)]);
    // frame start delay is an RMW on CHICKEN_TRANS(EDP)
    match &tx.stage(StageId::FrameStartDelay).unwrap().forward {
        Body::Link(p) => assert_eq!(p[0], Effect::Rmw { reg: i915_common_core::Reg::new(0x420cc), clear: 0x1800_0000, set: 0 }),
        _ => panic!(),
    }
    // the transcoder is enabled by an RMW setting only TRANSCONF_ENABLE
    match &tx.stage(StageId::TranscoderEnable).unwrap().forward {
        Body::Link(p) => assert_eq!(p[0], Effect::Rmw { reg: i915_common_core::Reg::new(0x7f008), clear: 0, set: 0x8000_0000 }),
        _ => panic!(),
    }
}

#[test]
fn plane_and_watermark_writes_match_linux() {
    let (_, tx) = build();
    let noarm = writes(&tx.stage(StageId::PlaneNoarm).unwrap().forward);
    // skl_plane_update_noarm: stride (7680/64), pos, size; then skl_write_plane_wm: PLANE_WM 0..7, TRANS, BUF_CFG, NV12_BUF_CFG
    assert_eq!(noarm[0], (0x70188, 0x78));
    assert_eq!(noarm[1], (0x7018c, 0));
    assert_eq!(noarm[2], (0x70190, 0x0437_077f));
    // gcc over Linux's skl_watermark.c (tests/skl_wm_linux.rs case s1): level 0 blocks 4 lines 0 -> EN|4 ; level 1 blocks 17 lines 1 ...
    let wm: Vec<u32> = noarm[3..11].iter().map(|w| w.1).collect();
    assert_eq!(wm, [0x8000_0004, 0x8000_4011, 0x8000_4011, 0x8000_8017, 0x8000_c025, 0x8000_c031, 0x8001_003d, 0x8001_4048]);
    assert_eq!(noarm[3..11].iter().map(|w| w.0).collect::<Vec<_>>(), [0x70240, 0x70244, 0x70248, 0x7024c, 0x70250, 0x70254, 0x70258, 0x7025c]);
    assert_eq!(noarm[11], (0x70268, 0), "PLANE_WM_TRANS is zero on display ver 9 (WaDisableTWM)");
    assert_eq!(noarm[12], (0x7027c, 0x035b_0000), "PLANE_BUF_CFG: DDB 0..860 (cursor reserve 32 at the top)");
    assert_eq!(noarm[13], (0x70278, 0), "PLANE_NV12_BUF_CFG unused for XRGB");
    assert_eq!(noarm.len(), 14);
    // skl_plane_update_arm: SURF is the last write of the stage and PLANE_CTL the one before it
    let arm = writes(&tx.stage(StageId::PlaneArm).unwrap().forward);
    assert_eq!(arm.last().copied(), Some((0x7019c, 0x0040_0000)));
    assert_eq!(arm[arm.len() - 2], (0x70180, 0x8400_2000));
    // cursor: 8 zero levels, zero trans, DDB reserve 860..892, CURPOS, CURBASE
    let cur = writes(&tx.stage(StageId::CursorWm).unwrap().forward);
    assert_eq!(cur.len(), 12);
    assert!(cur[..9].iter().all(|w| w.1 == 0));
    assert_eq!(cur[9], (0x7017c, 0x037b_035c));
    assert_eq!((cur[10], cur[11]), ((0x70088, 0), (0x70084, 0)));
    // the latch wait is on PLANE_SURFLIVE and shows the programmed address
    let latch = tx.stage(StageId::PlaneArm).unwrap().settle.unwrap();
    assert_eq!((latch.reg.offset(), latch.mask, latch.want), (0x701ac, 0xffff_f000, 0x0040_0000));
}

#[test]
fn readbacks_are_consistent_with_the_writes() {
    let (_, tx) = build();
    for s in &tx.stages {
        for rb in &s.readback {
            assert_eq!(rb.want & !rb.mask, 0, "{}: want has bits outside the mask", s.id.name());
            assert_ne!(rb.mask, 0, "{}: a readback with an empty mask checks nothing", s.id.name());
        }
    }
    let t = tx.stage(StageId::TranscoderTimings).unwrap();
    assert_eq!(t.readback.len(), 7, "all seven timing words are fully-defined 32-bit values and read back whole");
    assert!(t.readback.iter().all(|r| r.mask == u32::MAX));
    // after the modeset-time TRANSCONF write the pipe must still be OFF; after the enable it must be ON
    let off = &tx.stage(StageId::Transconf).unwrap().readback[0];
    assert_eq!((off.reg.offset(), off.want), (0x7f008, 0));
    let on = &tx.stage(StageId::TranscoderEnable).unwrap().readback[0];
    assert_eq!((on.reg.offset(), on.mask, on.want), (0x7f008, 0x8000_0000, 0x8000_0000));
    assert!(tx.stage(StageId::TranscoderEnable).unwrap().settle.is_some());
}

/// Run stages `0..=k` forward, then the rollbacks of `rollback_from(k)`, in a register file starting at the snapshot.
fn run_and_roll_back(tx: &EnableTransaction, pre: &std::collections::BTreeMap<u32, u32>, k: usize) -> Sim {
    let mut sim = Sim { regs: pre.clone(), writes: vec![] };
    for s in &tx.stages[..=k] {
        sim.body(&s.forward);
    }
    for b in tx.rollback_from(k).unwrap() {
        sim.body(b);
    }
    sim
}

const LOCAL: [StageId; 16] = [
    StageId::DbufSlice,
    StageId::DdiBufTrans,
    StageId::SignalLevels,
    StageId::StopLinkTrain,
    StageId::PipeSrc,
    StageId::PipeMisc,
    StageId::LinkMn,
    StageId::TranscoderTimings,
    StageId::FrameStartDelay,
    StageId::Transconf,
    StageId::LinetimeWm,
    StageId::DdiFuncCtl,
    StageId::TranscoderEnable,
    StageId::PlaneNoarm,
    StageId::PlaneArm,
    StageId::CursorWm,
];

#[test]
fn rollback_restores_every_register_the_local_stages_wrote() {
    let (f, tx) = build();
    // A non-trivial snapshot, so "restored" cannot mean "zeroed": every register the local stages touch starts non-zero.
    let mut pre = f.pre.clone();
    let mut local_regs = vec![];
    for s in &tx.stages {
        if LOCAL.contains(&s.id) {
            local_regs.extend(s.forward.written_regs());
        }
    }
    // registers also written by composed / interactive stages are restored by THEIR crates' rollbacks
    let mut composed_regs = vec![];
    for s in &tx.stages {
        if !LOCAL.contains(&s.id) {
            composed_regs.extend(s.forward.written_regs());
            composed_regs.extend(s.rollback.written_regs());
        }
    }
    local_regs.sort_by_key(|r| r.offset());
    local_regs.dedup();
    local_regs.retain(|r| !composed_regs.contains(r));
    for (i, r) in local_regs.iter().enumerate() {
        pre.insert(r.offset(), 0xa5a5_0000 | (i as u32));
    }
    let pre2 = pre.clone();
    let snap = move |r: u32| pre2.get(&r).copied().unwrap_or(0);
    let tx = build_enable(&inputs(&f, &snap)).unwrap();
    assert!(!local_regs.is_empty());
    for k in 0..tx.stages.len() {
        let sim = run_and_roll_back(&tx, &pre, k);
        for r in &local_regs {
            assert_eq!(sim.get(r.offset()), pre[&r.offset()], "after failing at stage {k} ({}), {:#x} not restored", tx.stages[k].id.name(), r.offset());
        }
    }
}

#[test]
fn rollback_order_is_newest_first_and_includes_the_failed_stage() {
    let (_, tx) = build();
    let k = tx.order().iter().position(|s| *s == StageId::TranscoderTimings).unwrap();
    let bodies = tx.rollback_from(k).unwrap();
    assert_eq!(bodies.len(), k + 1);
    for (i, b) in bodies.iter().enumerate() {
        assert_eq!(**b, tx.stages[k - i].rollback);
    }
    assert_eq!(tx.rollback_from(tx.stages.len()).unwrap_err().name(), "MODESET-NO-SUCH-STAGE");
}

#[test]
fn every_wait_is_bounded_and_the_bound_is_what_was_declared() {
    let (_, tx) = build();
    let polls = tx.all_polls();
    assert!(polls.len() >= 6, "{polls:?}");
    for (max, what) in &polls {
        assert!(*max > 0 && *max <= 100_000, "{what}: max_polls {max}");
    }
    // The settle waits this crate declares: interval 100 us, timeout / interval + 1 samples (poll_timeout_us semantics).
    let pipe = tx.stage(StageId::TranscoderEnable).unwrap().settle.unwrap();
    assert_eq!(pipe.max_polls, PIPE_ENABLE_TIMEOUT_US / SETTLE_INTERVAL_US + 1);
    // PLANE_SURFLIVE: 100 ms at 500 us.
    let latch = tx.stage(StageId::PlaneArm).unwrap().settle.unwrap();
    assert_eq!(latch.max_polls, 100_000 / 500 + 1);
    // A wait that never satisfies stops at exactly its bound.
    for p in [pipe, latch] {
        let mut reads = 0u32;
        let r = p.run(
            || {
                reads += 1;
                assert!(reads <= p.max_polls + 1, "unbounded");
                0
            },
            |_| {},
        );
        assert!(r.is_err());
        assert_eq!(reads, p.max_polls);
    }
}

#[test]
fn dbuf_slice_is_powered_only_when_the_snapshot_says_it_is_down() {
    let f = fixture();
    let down = |_r: u32| 0u32;
    let tx = build_enable(&inputs(&f, &down)).unwrap();
    let s = tx.stage(StageId::DbufSlice).unwrap();
    assert_eq!(s.linux, "gen9_dbuf_slice_set");
    match &s.forward {
        Body::Link(p) => assert_eq!(p[0], Effect::Rmw { reg: i915_common_core::Reg::new(0x45008), clear: 0, set: 0x8000_0000 }),
        _ => panic!(),
    }
    let p = s.settle.unwrap();
    assert_eq!((p.reg.offset(), p.mask, p.want), (0x45008, 0x4000_0000, 0x4000_0000));
    assert_eq!(p.max_polls, DBUF_POWER_TIMEOUT_US / SETTLE_INTERVAL_US + 1);
    let up = |r: u32| if r == 0x45008 { 0x4000_0000 } else { 0 };
    let tx = build_enable(&inputs(&f, &up)).unwrap();
    assert!(matches!(tx.stage(StageId::DbufSlice).unwrap().forward, Body::Empty(_)));
}

#[test]
fn refusals_are_named_and_happen_before_any_stage_exists() {
    let f = fixture();
    let snap = |_r: u32| 0u32;
    let mut i = inputs(&f, &snap);
    i.panel_native = (3840, 2160); // pipe source (1920x1080) is not the native mode: a panel fit would be needed
    assert_eq!(build_enable(&i).unwrap_err().name(), "PFIT-SCALER-NOT-PORTED");
    let mut i = inputs(&f, &snap);
    i.plane.rotation = i915_kbl_display_core::skl_plane::WriteRotation::R90;
    assert_eq!(build_enable(&i).unwrap_err().name(), "PLANE-ROTATION-NOT-PORTED");
    let mut i = inputs(&f, &snap);
    i.mode.interlaced = true;
    assert_eq!(build_enable(&i).unwrap_err().name(), "MODE-INTERLACED-NOT-PORTED");
    let mut i = inputs(&f, &snap);
    i.lanes = 3;
    assert_eq!(build_enable(&i).unwrap_err().name(), "MODE-BAD-LANE-COUNT");
    let mut i = inputs(&f, &snap);
    i.pipe_bpp = 36;
    assert_eq!(build_enable(&i).unwrap_err().name(), "MODE-UNSUPPORTED-BPP");
    let mut i = inputs(&f, &snap);
    i.plane.pipe = i915_kbl_display_core::types::Pipe::B;
    assert_eq!(build_enable(&i).unwrap_err().name(), "MODE-NOT-EDP-ON-DDI-A");
    // a vertical blank too short to prefill even level 0
    let mut i = inputs(&f, &snap);
    i.mode.vblank_start = i.mode.vtotal - 1;
    assert_eq!(build_enable(&i).unwrap_err().name(), "WM-VBLANK-TOO-SHORT");
    // the other planes are not present: a Step::Pcode in a stage would be a bug, none is planned here
    let tx = build_enable(&inputs(&f, &snap)).unwrap();
    for s in &tx.stages {
        if let Body::Clock(p) = &s.forward {
            let _: &WritePlan = p;
            assert!(p.iter().all(|st| !matches!(st, Step::Pcode(_)) || s.id == StageId::CdclkSet), "{}", s.id.name());
        }
    }
}

#[test]
fn nothing_outside_the_enable_set_is_written_by_local_stages() {
    // The local stages touch only display registers in the pipe-A / transcoder-EDP / DDI-A / DBUF / WM_LINETIME ranges.
    let (_, tx) = build();
    for s in &tx.stages {
        if !LOCAL.contains(&s.id) {
            continue;
        }
        for r in s.forward.written_regs().into_iter().chain(s.rollback.written_regs()) {
            let o = r.offset();
            let ok = (0x60000..0x60800).contains(&o)      // pipe A transcoder block
                || (0x6f000..0x6f800).contains(&o)         // transcoder EDP block
                || (0x70000..0x71000).contains(&o)         // pipe A planes / cursor / misc / TRANSCONF(A)
                || (0x7f000..0x7f100).contains(&o)         // TRANSCONF(EDP)
                || (0x64000..0x65000).contains(&o)         // DDI A: buf ctl, DP_TP, buf trans
                || o == 0x45008 || o == 0x45270 || o == 0x420cc
                || o == 0x6c00c;                           // DISPIO_CR_TX_BMU_CR0 (set_signal_levels: balance leg)
            assert!(ok, "{}: unexpected write to {o:#x}", s.id.name());
        }
    }
}

#[test]
fn the_single_bit_stages_roll_back_to_the_snapshots_bit_not_to_zero() {
    let f = fixture();
    for (reg, snapshot_has_bit) in [(0x7f008u32, true), (0x7f008, false)] {
        let snap = move |r: u32| if r == reg && snapshot_has_bit { 0x8000_0000 } else { 0 };
        let tx = build_enable(&inputs(&f, &snap)).unwrap();
        // run ONLY this stage's rollback after its forward: Transconf's whole-word restore must not be what saves it
        let mut sim = Sim::default();
        if snapshot_has_bit {
            sim.put(reg, 0x8000_0000);
        }
        let s = tx.stage(StageId::TranscoderEnable).unwrap();
        sim.body(&s.forward);
        assert_eq!(sim.get(reg) & 0x8000_0000, 0x8000_0000);
        sim.body(&s.rollback);
        assert_eq!(sim.get(reg) & 0x8000_0000, if snapshot_has_bit { 0x8000_0000 } else { 0 });
    }
    // DBUF: a snapshot with the request bit set but the state bit clear (the slice is requested, not yet up)
    let snap = |r: u32| if r == 0x45008 { 0x8000_0000 } else { 0 };
    let tx = build_enable(&inputs(&f, &snap)).unwrap();
    let s = tx.stage(StageId::DbufSlice).unwrap();
    let mut sim = Sim::default();
    sim.put(0x45008, 0x8000_0000);
    sim.body(&s.forward);
    sim.body(&s.rollback);
    assert_eq!(sim.get(0x45008), 0x8000_0000, "the firmware's request is left as the firmware had it");
    let snap0 = |_r: u32| 0u32;
    let tx = build_enable(&inputs(&f, &snap0)).unwrap();
    let s = tx.stage(StageId::DbufSlice).unwrap();
    let mut sim = Sim::default();
    sim.body(&s.forward);
    assert_eq!(sim.get(0x45008), 0x8000_0000);
    sim.body(&s.rollback);
    assert_eq!(sim.get(0x45008), 0);
}
