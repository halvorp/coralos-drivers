// SPDX-License-Identifier: GPL-2.0-only
//! Power domains and wells: the domain -> well table (written out per domain), refcounts, the PW1-before-PW2 order, the request /
//! state / fuse handshakes (exact plans), and every handshake timing out BY NAME with both bounds enforced.
mod common;
use common::*;
use i915_common_core::Reg;
use i915_display_clock_core::error::{Bound, Handshake};
use i915_display_clock_core::plan::{OnTimeout, PollPlan, Step, DEFAULT_MAX_POLLS};
use i915_display_clock_core::power::*;
use i915_display_clock_core::power_map::*;
use i915_display_clock_core::regs::*;
use i915_display_clock_core::{ClockError, Domain, PowerWells};
use i915_kbl_display_core::power::{decode_dc_state, decode_well, PowerState, Well};

fn rd(o: u32) -> Reg {
    Reg::new(o)
}

#[test]
fn domain_to_well_table_written_out() {
    use Domain::*;
    // skl_pwdoms_pw_2 / SKL_PW_2_POWER_DOMAINS (intel_display_power_map.c:306-330): PW_2 powers exactly these 20 domains.
    let pw2 = [
        PipeB, PipeC, PanelFitterB, PanelFitterC, TranscoderA, TranscoderB, TranscoderC, DdiLanesB, DdiLanesC, DdiLanesD, DdiLanesE, Vga,
        AudioMmio, AudioPlayback, AuxIoB, AuxIoC, AuxIoD, AuxB, AuxC, AuxD,
    ];
    for d in pw2 {
        assert_eq!(d.wells(), &[Well::Pw2], "{d:?}");
    }
    // skl_pwdoms_ddi_io_a_e / _b / _c / _d (:339-354)
    assert_eq!(DdiIoA.wells(), &[Well::DdiAE]);
    assert_eq!(DdiIoE.wells(), &[Well::DdiAE]);
    assert_eq!(DdiIoB.wells(), &[Well::DdiB]);
    assert_eq!(DdiIoC.wells(), &[Well::DdiC]);
    assert_eq!(DdiIoD.wells(), &[Well::DdiD]);
    // POWER_DOMAIN_INIT is in PW_2 and every DDI IO well (:330, :342, :346, :350, :354), in table order
    assert_eq!(Init.wells(), &[Well::Pw2, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD]);
    // In the always-on well only
    for d in [DisplayCore, PipeA, PanelFitterA, TranscoderEdp, DdiLanesA, PortCrt, PortOther, AuxIoA, AuxA, Gmbus, GtIrq, DcOff] {
        assert!(d.wells().is_empty(), "{d:?}");
    }
    // every domain accounted for: 20 + 5 + 1 + 12
    assert_eq!(Domain::COUNT, 38);
    let mut in_pw2 = 0;
    for d in Domain::ALL {
        if d.wells() == [Well::Pw2] {
            in_pw2 += 1;
        }
    }
    assert_eq!(in_pw2, 20);
}

#[test]
fn dc_off_membership_is_skl_pwdoms_dc_off() {
    use Domain::*;
    // skl_pwdoms_dc_off (:332-337): the PW_2 domains + AUX_A, GT_IRQ, DC_OFF, INIT
    for d in Domain::ALL {
        let want = matches!(
            d,
            PipeB | PipeC | PanelFitterB | PanelFitterC | TranscoderA | TranscoderB | TranscoderC | DdiLanesB | DdiLanesC | DdiLanesD
                | DdiLanesE | Vga | AudioMmio | AudioPlayback | AuxIoB | AuxIoC | AuxIoD | AuxB | AuxC | AuxD | AuxA | GtIrq | DcOff | Init
        );
        assert_eq!(d.needs_dc_off(), want, "{d:?}");
    }
}

#[test]
fn well_order_and_requirements() {
    // skl_power_wells[] order (:407-411): PW_1, MISC_IO, (DC_off), PW_2, DDI IO A_E, B, C, D
    assert_eq!(WELL_ORDER, [Well::Pw1, Well::MiscIo, Well::Pw2, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD]);
    assert_eq!(requires(Well::Pw1), None);
    assert_eq!(requires(Well::MiscIo), None);
    for w in [Well::Pw2, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD] {
        assert_eq!(requires(w), Some(Well::Pw1), "{w:?}");
    }
    // pw_idx_to_pg (intel_display_power_well.c:45-50): PW1 -> SKL_PG1, PW2 -> SKL_PG2; the others have no fuses
    assert_eq!(power_gate(Well::Pw1), Some(SKL_PG1));
    assert_eq!(power_gate(Well::Pw2), Some(SKL_PG2));
    for w in [Well::MiscIo, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD] {
        assert_eq!(power_gate(w), None);
    }
}

fn fuse(pg: u32) -> Step {
    // gen9_wait_for_power_well_fuses: SKL_FUSE_PG_DIST_STATUS(pg) = 1 << (27 - pg) in SKL_FUSE_STATUS, 1 ms
    Step::Poll(PollPlan::wait_set(Handshake::FuseDistribution(pg), rd(0x42000), 1 << (27 - pg), 1, OnTimeout::Fail))
}

#[test]
fn pw1_enable_is_exactly_linuxs_sequence() {
    // hsw_power_well_enable (intel_display_power_well.c:382-423) for PW1 (idx 14): fuse PG0, request, state, fuse PG1.
    let mut w = PowerWells::new();
    let t = w.core_init().unwrap();
    let want = [
        fuse(0),
        Step::Rmw { reg: rd(0x45404), clear: 0, set: 0x2000_0000 }, // HSW_PWR_WELL_CTL_REQ(14) = 0x2 << 28
        Step::Poll(PollPlan::wait_set(Handshake::PowerWellEnable(Well::Pw1), rd(0x45404), 0x1000_0000, 1, OnTimeout::Fail)),
        fuse(1),
        // MISC_IO (idx 0): no fuses
        Step::Rmw { reg: rd(0x45404), clear: 0, set: 0x2 },
        Step::Poll(PollPlan::wait_set(Handshake::PowerWellEnable(Well::MiscIo), rd(0x45404), 0x1, 1, OnTimeout::Fail)),
    ];
    assert_eq!(t.forward.len(), want.len());
    for (i, s) in want.iter().enumerate() {
        assert_eq!(t.forward.get(i), Some(s), "step {i}");
    }
    assert!(w.is_enabled(Well::Pw1) && w.is_enabled(Well::MiscIo));
    // the rollback withdraws MISC_IO's request first, then PW1's
    assert_eq!(t.rollback.get(0), Some(&Step::Rmw { reg: rd(0x45404), clear: 0x2, set: 0 }));
    assert_eq!(t.rollback.get(1), Some(&Step::Rmw { reg: rd(0x45404), clear: 0x2000_0000, set: 0 }));
}

#[test]
fn request_and_state_bits_of_every_well() {
    // (well, request, state): HSW_PWR_WELL_CTL_REQ(idx) = 0x2 << (idx * 2), STATE = 0x1 << (idx * 2)
    // (intel_display_regs.h:2129-2130) with idx from SKL_PW_CTL_IDX_* (:2136-2146): PW2 15, PW1 14, DDI_D 4, C 3, B 2, A_E 1, MISC_IO 0.
    let rows = [
        (Well::Pw1, 0x2000_0000u32, 0x1000_0000u32),
        (Well::Pw2, 0x8000_0000, 0x4000_0000),
        (Well::MiscIo, 0x2, 0x1),
        (Well::DdiAE, 0x8, 0x4),
        (Well::DdiB, 0x20, 0x10),
        (Well::DdiC, 0x80, 0x40),
        (Well::DdiD, 0x200, 0x100),
    ];
    for (well, req, state) in rows {
        let mut w = PowerWells::new();
        w.core_init().unwrap();
        // enable exactly this well through its domain (PW1 / MISC_IO through core_init)
        let t = match well {
            Well::Pw1 | Well::MiscIo => {
                let mut x = PowerWells::new();
                x.core_init().unwrap()
            }
            Well::Pw2 => w.get(Domain::PipeB).unwrap(),
            Well::DdiAE => w.get(Domain::DdiIoA).unwrap(),
            Well::DdiB => w.get(Domain::DdiIoB).unwrap(),
            Well::DdiC => w.get(Domain::DdiIoC).unwrap(),
            Well::DdiD => w.get(Domain::DdiIoD).unwrap(),
        };
        let rmw = t.forward.iter().any(|s| *s == Step::Rmw { reg: rd(0x45404), clear: 0, set: req });
        let poll = t.forward.iter().any(|s| matches!(s, Step::Poll(p) if p.what == Handshake::PowerWellEnable(well) && p.mask == state && p.want == state && p.reg == rd(0x45404)));
        assert!(rmw, "{well:?} request {req:#x}");
        assert!(poll, "{well:?} state {state:#x}");
    }
}

fn up() -> PowerWells {
    let mut w = PowerWells::new();
    w.core_init().unwrap();
    w
}

#[test]
fn pw2_enable_waits_state_then_pg2_fuse() {
    let mut w = up();
    let t = w.get(Domain::PipeB).unwrap();
    let want = [
        Step::Rmw { reg: rd(0x45404), clear: 0, set: 0x8000_0000 },
        Step::Poll(PollPlan::wait_set(Handshake::PowerWellEnable(Well::Pw2), rd(0x45404), 0x4000_0000, 1, OnTimeout::Fail)),
        fuse(2), // intel_display_power_well.c:412-418 (PG2 only; PG0 is waited for before PW1 alone)
    ];
    assert_eq!(t.forward.len(), 3);
    for (i, s) in want.iter().enumerate() {
        assert_eq!(t.forward.get(i), Some(s), "step {i}");
    }
}

#[test]
fn pw2_before_pw1_is_refused_and_leaves_state_untouched() {
    let mut w = PowerWells::new(); // nothing on, PW1 not enabled
    let before = w;
    let e = w.get(Domain::PipeB).unwrap_err();
    assert_eq!(e, ClockError::WellOrder { well: Well::Pw2, needs: Well::Pw1 });
    assert_eq!(e.name(), "POWER-WELL-ORDER");
    assert_eq!(w, before);
    // the DDI IO wells sit behind PW1 as well
    for d in [Domain::DdiIoA, Domain::DdiIoB, Domain::DdiIoC, Domain::DdiIoD, Domain::Init] {
        let e = w.get(d).unwrap_err();
        assert!(matches!(e, ClockError::WellOrder { needs: Well::Pw1, .. }), "{d:?}");
        assert_eq!(w, before, "{d:?}");
    }
    // and PW1 may not come up AFTER being refused: a clean retry works once PW1 is on
    w.core_init().unwrap();
    assert!(w.get(Domain::PipeB).is_ok());
}

#[test]
fn pw1_cannot_go_down_under_pw2() {
    let mut w = up();
    w.get(Domain::PipeB).unwrap();
    let before = w;
    let e = w.core_uninit().unwrap_err();
    assert_eq!(e, ClockError::WellStillNeeded { well: Well::Pw1, held_by: Well::Pw2 });
    assert_eq!(e.name(), "POWER-WELL-STILL-NEEDED");
    assert_eq!(w, before);
    w.put(Domain::PipeB).unwrap();
    let t = w.core_uninit().unwrap(); // skl_display_core_uninit: only PW1's request is withdrawn, MISC_IO stays (:1484-1512)
    assert_eq!(t.forward.get(0), Some(&Step::Rmw { reg: rd(0x45404), clear: 0x2000_0000, set: 0 }));
    assert!(!w.is_enabled(Well::Pw1) && w.is_enabled(Well::MiscIo));
}

#[test]
fn refcounts_enable_on_first_get_and_disable_on_last_put() {
    let mut w = up();
    let t1 = w.get(Domain::PipeB).unwrap(); // 0 -> 1: enable
    assert!(!t1.forward.is_empty());
    let t2 = w.get(Domain::TranscoderA).unwrap(); // another PW2 domain: well 1 -> 2, no hardware work
    assert!(t2.forward.is_empty());
    assert_eq!(w.well_count(Well::Pw2), 2);
    assert_eq!((w.domain_count(Domain::PipeB), w.domain_count(Domain::TranscoderA)), (1, 1));
    let t3 = w.put(Domain::PipeB).unwrap(); // 2 -> 1: still needed
    assert!(t3.forward.is_empty());
    assert!(w.is_enabled(Well::Pw2));
    let t4 = w.put(Domain::TranscoderA).unwrap(); // 1 -> 0: disable
    assert_eq!(t4.forward.get(0), Some(&Step::Rmw { reg: rd(0x45404), clear: 0x8000_0000, set: 0 }));
    assert!(!w.is_enabled(Well::Pw2));
    // the same domain twice
    w.get(Domain::PipeB).unwrap();
    w.get(Domain::PipeB).unwrap();
    assert_eq!(w.domain_count(Domain::PipeB), 2);
    assert!(w.put(Domain::PipeB).unwrap().forward.is_empty());
    assert!(!w.put(Domain::PipeB).unwrap().forward.is_empty());
}

#[test]
fn always_on_domains_touch_no_hardware() {
    let mut w = up();
    for d in [Domain::DisplayCore, Domain::PipeA, Domain::TranscoderEdp, Domain::AuxA, Domain::Gmbus] {
        assert!(w.get(d).unwrap().forward.is_empty(), "{d:?}");
        assert!(w.put(d).unwrap().forward.is_empty(), "{d:?}");
    }
}

#[test]
fn unbalanced_put_is_refused() {
    let mut w = up();
    let before = w;
    let e = w.put(Domain::PipeB).unwrap_err();
    assert_eq!(e, ClockError::UnbalancedPut { what: "domain" });
    assert_eq!(e.name(), "UNBALANCED-PUT");
    assert_eq!(w, before);
    // an always-on domain with no reference too
    assert_eq!(w.put(Domain::PipeA).unwrap_err(), ClockError::UnbalancedPut { what: "domain" });
}

#[test]
fn init_domain_enables_forward_and_disables_in_reverse() {
    let mut w = up();
    let t = w.get(Domain::Init).unwrap();
    let reqs: Vec<u32> = t.forward.iter().filter_map(|s| match s { Step::Rmw { set, .. } => Some(*set), _ => None }).collect();
    assert_eq!(reqs, [0x8000_0000, 0x8, 0x20, 0x80, 0x200]); // PW2, A_E, B, C, D
    // the rollback withdraws them last-first
    let rb: Vec<u32> = t.rollback.iter().filter_map(|s| match s { Step::Rmw { clear, .. } => Some(*clear), _ => None }).collect();
    assert_eq!(rb, [0x200, 0x80, 0x20, 0x8, 0x8000_0000]);
    let t = w.put(Domain::Init).unwrap();
    let clears: Vec<u32> = t.forward.iter().filter_map(|s| match s { Step::Rmw { clear, .. } => Some(*clear), _ => None }).collect();
    assert_eq!(clears, [0x200, 0x80, 0x20, 0x8, 0x8000_0000]); // reverse of the enable order (intel_display_power.c:625)
}

#[test]
fn simulated_get_put_cycle_reaches_and_leaves_the_state() {
    let mut s = Sim::new();
    let mut w = PowerWells::new();
    run_tx(&mut s, &w.core_init().unwrap()).0.unwrap();
    assert_eq!(s.get(HSW_PWR_WELL_CTL2) & 0x3000_0003, 0x3000_0003); // PW1 + MISC_IO req and state
    run_tx(&mut s, &w.get(Domain::Init).unwrap()).0.unwrap();
    assert_eq!(s.get(HSW_PWR_WELL_CTL2) & 0xc000_0000, 0xc000_0000);
    run_tx(&mut s, &w.put(Domain::Init).unwrap()).0.unwrap();
    assert_eq!(s.get(HSW_PWR_WELL_CTL2), 0x3000_0003); // only PW1 + MISC_IO left
    assert!(s.tolerated.is_empty());
}

#[test]
fn every_enable_handshake_times_out_by_name_with_the_time_bound() {
    // (domain that enables it, idx of the well, expected handshake name)
    let rows = [
        (Domain::PipeB, 15, Handshake::PowerWellEnable(Well::Pw2), "PW2-ENABLE-ACK"),
        (Domain::DdiIoA, 1, Handshake::PowerWellEnable(Well::DdiAE), "DDI-A-E-ENABLE-ACK"),
        (Domain::DdiIoB, 2, Handshake::PowerWellEnable(Well::DdiB), "DDI-B-ENABLE-ACK"),
        (Domain::DdiIoC, 3, Handshake::PowerWellEnable(Well::DdiC), "DDI-C-ENABLE-ACK"),
        (Domain::DdiIoD, 4, Handshake::PowerWellEnable(Well::DdiD), "DDI-D-ENABLE-ACK"),
    ];
    for (d, idx, hs, name) in rows {
        let mut s = Sim::new();
        let mut w = up();
        s.stuck_pw_idx = 1 << idx;
        let before = w;
        let t = w.get(d).unwrap();
        let (r, rb) = run_tx(&mut s, &t);
        match r.unwrap_err() {
            ClockError::HandshakeTimeout { what, bound, reg, mask, want, got, polls, elapsed_us } => {
                assert_eq!(what, hs);
                assert_eq!(what.name(), name);
                assert_eq!(bound, Bound::Time, "{name}");
                assert_eq!((reg.offset(), mask, want, got), (HSW_PWR_WELL_CTL2, 0x1 << (idx * 2), 0x1 << (idx * 2), 0), "{name}");
                assert!(elapsed_us >= 1000 && polls >= 1000 && polls < 1100, "{name}: {polls} polls, {elapsed_us} us"); // 1 ms at 1 us/sample
            }
            e => panic!("{name}: {}", e.name()),
        }
        // the rollback withdrew the request
        rb.unwrap().unwrap();
        assert_eq!(s.get(HSW_PWR_WELL_CTL2) & (0x2 << (idx * 2)), 0, "{name}: request withdrawn");
        // and the caller restores the software state it kept
        w.rollback_applied(before);
        assert_eq!(w, before);
    }
}

#[test]
fn pw1_and_misc_io_enable_handshakes_time_out_by_name() {
    for (idx, name) in [(14, "PW1-ENABLE-ACK"), (0, "MISC-IO-ENABLE-ACK")] {
        let mut s = Sim::new();
        s.stuck_pw_idx = 1 << idx;
        let mut w = PowerWells::new();
        let t = w.core_init().unwrap();
        let e = run_plan(&mut s, &t.forward).unwrap_err();
        match e {
            ClockError::HandshakeTimeout { what, .. } => assert_eq!(what.name(), name),
            e => panic!("{}", e.name()),
        }
    }
}

#[test]
fn every_fuse_distribution_wait_times_out_by_name() {
    // PG0 and PG1 are waited for around PW1's enable, PG2 after PW2's
    for (pg, name) in [(0u32, "FUSE-PG0-DISTRIBUTION"), (1, "FUSE-PG1-DISTRIBUTION"), (2, "FUSE-PG2-DISTRIBUTION")] {
        let mut s = Sim::new();
        s.set(SKL_FUSE_STATUS, (SKL_FUSE_DOWNLOAD_STATUS | (1 << 27) | (1 << 26) | (1 << 25)) & !(1 << (27 - pg)));
        let t = if pg < 2 { PowerWells::new().core_init().unwrap() } else { up().get(Domain::PipeB).unwrap() };
        let e = run_plan(&mut s, &t.forward).unwrap_err();
        match e {
            ClockError::HandshakeTimeout { what, reg, mask, want, got, .. } => {
                assert_eq!(what, Handshake::FuseDistribution(pg));
                assert_eq!(what.name(), name);
                assert_eq!((reg.offset(), mask, want, got), (SKL_FUSE_STATUS, 1 << (27 - pg), 1 << (27 - pg), 0));
            }
            e => panic!("{name}: {}", e.name()),
        }
    }
}

#[test]
fn the_sample_bound_is_enforced_when_the_clock_does_not_advance() {
    // us_per_sample = 0: elapsed time never grows, so ONLY max_polls can stop the wait. Removing the check would spin to
    // HARD_LIMIT and the interpreter's panic fails this test.
    let mut s = Sim::new();
    s.us_per_sample = 0;
    s.stuck_pw_idx = 1 << 15;
    let mut w = up();
    let t = w.get(Domain::PipeB).unwrap();
    match run_plan(&mut s, &t.forward).unwrap_err() {
        ClockError::HandshakeTimeout { what, bound, polls, elapsed_us, .. } => {
            assert_eq!(what, Handshake::PowerWellEnable(Well::Pw2));
            assert_eq!(bound, Bound::Polls);
            assert_eq!(polls, DEFAULT_MAX_POLLS);
            assert_eq!(elapsed_us, 0);
        }
        e => panic!("{}", e.name()),
    }
    // a plan can carry a tighter bound and it is the one enforced
    let tight = PollPlan::wait_set(Handshake::PowerWellEnable(Well::Pw2), rd(0x45404), 0x4000_0000, 1, OnTimeout::Fail).with_max_polls(7);
    let mut p = i915_display_clock_core::plan::WritePlan::new();
    p.poll(tight).unwrap();
    match run_plan(&mut s, &p).unwrap_err() {
        ClockError::HandshakeTimeout { polls, bound, .. } => assert_eq!((polls, bound), (7, Bound::Polls)),
        e => panic!("{}", e.name()),
    }
}

#[test]
fn a_disable_that_does_not_stick_is_tolerated_like_linux() {
    // hsw_wait_for_power_well_disable (intel_display_power_well.c:338-371) only logs "forced on"; the sequence continues.
    let mut s = Sim::new();
    let mut w = PowerWells::new();
    run_tx(&mut s, &w.core_init().unwrap()).0.unwrap();
    run_tx(&mut s, &w.get(Domain::PipeB).unwrap()).0.unwrap();
    s.forced_on_idx = 1 << 15; // a KVMR / DMC request holds PW2
    let t = w.put(Domain::PipeB).unwrap();
    run_tx(&mut s, &t).0.unwrap();
    assert_eq!(s.tolerated.len(), 1);
    match s.tolerated[0] {
        ClockError::HandshakeTimeout { what, .. } => assert_eq!(what.name(), "PW2-DISABLE-ACK"),
        e => panic!("{}", e.name()),
    }
}

#[test]
fn disable_wait_is_zero_ms_when_another_agent_requests_the_well() {
    // hsw_wait_for_power_well_disable: `reqs ? 0 : 1` ms (intel_display_power_well.c:355-359)
    let mut w = up();
    w.get(Domain::PipeB).unwrap();
    w.note_requesters(Well::Pw2, 1); // BIOS request bit set
    let t = w.put(Domain::PipeB).unwrap();
    let to = t.forward.iter().find_map(|s| match s { Step::Poll(p) => Some(p.timeout_us), _ => None }).unwrap();
    assert_eq!(to, 0);
    let mut w = up();
    w.get(Domain::PipeB).unwrap();
    let t = w.put(Domain::PipeB).unwrap();
    let to = t.forward.iter().find_map(|s| match s { Step::Poll(p) => Some(p.timeout_us), _ => None }).unwrap();
    assert_eq!(to, 1000);
    assert_eq!(WELL_ENABLE_TIMEOUT_MS, 1);
    assert_eq!(FUSE_WAIT_TIMEOUT_MS, 1);
    assert_eq!(WELL_DISABLE_TIMEOUT_MS, 1);
}

#[test]
fn state_adopts_the_readout() {
    // PW1 + MISC_IO on in the BIOS register only (the DMC-bug case, intel_display_power_well.c:634-636), PW2 on in the driver's.
    let bios = 0x3000_0003u32; // REQ+STATE of PW1 (idx 14) and MISC_IO (idx 0)
    let drv = 0xc000_0000u32; // REQ+STATE of PW2 (idx 15)
    let wells = [Well::Pw1, Well::Pw2, Well::MiscIo, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD].map(|w| decode_well(w, bios, drv));
    let st = PowerState { wells, fuse_status: 0, dc_state: decode_dc_state(0) };
    let w = PowerWells::from_readout(&st);
    assert!(w.is_enabled(Well::Pw1) && w.is_enabled(Well::MiscIo) && w.is_enabled(Well::Pw2));
    assert!(!w.is_enabled(Well::DdiAE) && !w.is_enabled(Well::DdiD));
    // a PW2 the firmware left on, with no software reference, can be referenced without any new hardware work: its request is
    // in place; the first get still plans the request (Linux's count 0 -> 1 calls enable, which is idempotent).
    let mut w = w;
    assert!(w.get(Domain::PipeB).is_ok());
}

#[test]
fn plan_cap_covers_the_biggest_domain() {
    // INIT: PW2 (3 steps) + 4 DDI wells (2 steps each) = 11 steps, inside PLAN_CAP
    let mut w = up();
    assert_eq!(w.get(Domain::Init).unwrap().forward.len(), 11);
}
