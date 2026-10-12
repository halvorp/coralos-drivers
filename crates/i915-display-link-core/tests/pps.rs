// SPDX-License-Identifier: GPL-2.0-only
//! Panel power sequencer vs intel_pps.c / intel_pps_regs.h (v7.0-rc6), with the Pixelbook Go's real VBT power sequence.
#![allow(unused_imports)]
use i915_common_core::Reg;
use i915_display_link_core::plan::*;
use i915_display_link_core::pps::*;
use i915_display_link_core::regs::*;
use i915_vbt_core::edp::parse_edp;
use i915_vbt_core::Vbt;

// ---- registers -----------------------------------------------------------------------------------------------------
#[test]
fn the_pps_sits_at_the_pch_base() {
    // intel_pps_setup(): HAS_PCH_SPLIT -> mmio_base = PCH_PPS_BASE (0xC7200); _MMIO_PPS = base - PPS_BASE + reg + idx * 0x100
    assert_eq!(pp_status(0).offset(), 0xC7200);
    assert_eq!(pp_control(0).offset(), 0xC7204);
    assert_eq!(pp_on_delays(0).offset(), 0xC7208);
    assert_eq!(pp_off_delays(0).offset(), 0xC720C);
    assert_eq!(pp_divisor(0).offset(), 0xC7210);
    assert_eq!(pp_status(1).offset(), 0xC7300);
    assert_eq!((PPS_BASE, PCH_PPS_BASE), (0x61200, 0xC7200));
}

#[test]
fn pp_register_fields_are_linuxs() {
    assert_eq!(PP_ON, 1 << 31);
    assert_eq!(PP_READY, 1 << 30);
    assert_eq!(PP_SEQUENCE_MASK, 0x3000_0000);
    assert_eq!(PP_SEQUENCE_NONE, 0);
    assert_eq!(PP_SEQUENCE_POWER_UP, 1 << 28);
    assert_eq!(PP_SEQUENCE_POWER_DOWN, 2 << 28);
    assert_eq!(PP_CYCLE_DELAY_ACTIVE, 1 << 27);
    assert_eq!(PP_SEQUENCE_STATE_MASK, 0xf);
    assert_eq!(PP_SEQUENCE_STATE_OFF_IDLE, 0);
    assert_eq!(PP_SEQUENCE_STATE_ON_IDLE, 8);
    assert_eq!(PANEL_UNLOCK_MASK, 0xffff_0000);
    assert_eq!(PANEL_UNLOCK_REGS, 0xabcd_0000);
    assert_eq!(BXT_POWER_CYCLE_DELAY_MASK, 0x1f0);
    assert_eq!(EDP_FORCE_VDD, 1 << 3);
    assert_eq!(EDP_BLC_ENABLE, 1 << 2);
    assert_eq!(PANEL_POWER_RESET, 1 << 1);
    assert_eq!(PANEL_POWER_ON, 1 << 0);
    assert_eq!(PANEL_POWER_UP_DELAY_MASK, 0x1fff_0000);
    assert_eq!(PANEL_LIGHT_ON_DELAY_MASK, 0x1fff);
    assert_eq!(PANEL_POWER_DOWN_DELAY_MASK, 0x1fff_0000);
    assert_eq!(PANEL_LIGHT_OFF_DELAY_MASK, 0x1fff);
    assert_eq!(PP_REFERENCE_DIVIDER_MASK, 0xffff_ff00);
    assert_eq!(PANEL_POWER_CYCLE_DELAY_MASK, 0x1f);
    assert_eq!(PANEL_PORT_SELECT_MASK, 0xc000_0000);
    assert_eq!(PANEL_PORT_SELECT_DPA, 1 << 30);
}

#[test]
fn the_idle_masks_are_the_or_of_their_named_parts() {
    // intel_pps.c:586-593 written out as raw numbers
    assert_eq!(IDLE_ON_MASK, PP_ON | PP_SEQUENCE_MASK | PP_SEQUENCE_STATE_MASK);
    assert_eq!(IDLE_ON_MASK, 0xB000_000F);
    assert_eq!(IDLE_ON_VALUE, 0x8000_0008);
    assert_eq!(IDLE_OFF_MASK, 0xB000_0000);
    assert_eq!(IDLE_OFF_VALUE, 0);
    assert_eq!(IDLE_CYCLE_MASK, PP_ON | PP_SEQUENCE_MASK | PP_CYCLE_DELAY_ACTIVE | PP_SEQUENCE_STATE_MASK);
    assert_eq!(IDLE_CYCLE_MASK, 0xB800_000F);
    assert_eq!(IDLE_CYCLE_VALUE, 0);
    // the wait: poll_timeout_us(.., 10 ms sleep, 5000 ms timeout)
    assert_eq!((PPS_WAIT_INTERVAL_US, PPS_WAIT_TIMEOUT_US), (10_000, 5_000_000));
}

// ---- delays --------------------------------------------------------------------------------------------------------
fn atlas_vbt() -> Option<Vec<u8>> {
    let p = std::env::var("I915_ATLAS_VBT").unwrap_or_else(|_| "/home/peter/Projects/coral_handoffs/atlas/sources/atlas-vbt.bin".into());
    std::fs::read(p).ok()
}

#[test]
fn atlas_vbt_power_sequence_becomes_the_pps_delays() {
    // The i915-vbt-core fixture (real Pixelbook Go VBT) — the same file its tests/real_vbt.rs parses. Expected values are the
    // independent decoder's "eDP power seq (100us units) t1_t3,t8,t9,t10,t11_t12 = (2000, 10, 2000, 500, 5000)".
    let Some(buf) = atlas_vbt() else {
        eprintln!("SKIP: no Atlas VBT on this host");
        return;
    };
    let vbt = Vbt::parse(&buf).unwrap();
    let edp = parse_edp(&vbt, 2).unwrap().unwrap();
    let d = PpsDelays::from_vbt(&edp);
    assert_eq!(d, PpsDelays { power_up: 2000, backlight_on: 10, backlight_off: 2000, power_down: 500, power_cycle: 5000 });
}

const ATLAS: PpsDelays = PpsDelays { power_up: 2000, backlight_on: 10, backlight_off: 2000, power_down: 500, power_cycle: 5000 };

#[test]
fn atlas_timing_when_the_firmware_left_nothing() {
    let t = PpsTiming::compute(PpsDelays::default(), ATLAS);
    // ms = DIV_ROUND_UP(units, 10): T1+T3 200, T8 1, T9 200, T10 50, T11+T12 500
    assert_eq!(
        (t.panel_power_up_delay_ms, t.backlight_on_delay_ms, t.backlight_off_delay_ms, t.panel_power_down_delay_ms, t.panel_power_cycle_delay_ms),
        (200, 1, 200, 50, 500)
    );
    // register values: backlight delays forced to 1 (we wait manually), power cycle rounded UP to 100 ms units (1000)
    assert_eq!(t.hw, PpsDelays { power_up: 2000, backlight_on: 1, backlight_off: 1, power_down: 500, power_cycle: 5000 });
}

#[test]
fn the_larger_of_firmware_and_vbt_wins_per_field() {
    let bios = PpsDelays { power_up: 3000, backlight_on: 5, backlight_off: 0, power_down: 100, power_cycle: 7777 };
    let t = PpsTiming::compute(bios, ATLAS);
    assert_eq!(t.panel_power_up_delay_ms, 300); // bios 3000 > vbt 2000
    assert_eq!(t.backlight_on_delay_ms, 1); // vbt 10 > bios 5 -> 10 units = 1 ms
    assert_eq!(t.backlight_off_delay_ms, 200); // vbt (bios 0)
    assert_eq!(t.panel_power_down_delay_ms, 50); // vbt 500 > bios 100
    assert_eq!(t.panel_power_cycle_delay_ms, 778); // DIV_ROUND_UP(7777, 10)
    assert_eq!(t.hw.power_cycle, 8000); // 7777 rounded up to 1000
}

#[test]
fn with_no_information_at_all_the_edp_spec_limits_apply() {
    let t = PpsTiming::compute(PpsDelays::default(), PpsDelays::default());
    // intel_pps.c:1498-1513: T1+T3 10+200 ms, 50, 50, T10 500, T11+T12 10+500
    assert_eq!(SPEC_DELAYS, PpsDelays { power_up: 2100, backlight_on: 500, backlight_off: 500, power_down: 5000, power_cycle: 5100 });
    assert_eq!(t.panel_power_up_delay_ms, 210);
    assert_eq!(t.backlight_on_delay_ms, 50);
    assert_eq!(t.backlight_off_delay_ms, 50);
    assert_eq!(t.panel_power_down_delay_ms, 500);
    assert_eq!(t.panel_power_cycle_delay_ms, 510);
    assert_eq!(t.hw, PpsDelays { power_up: 2100, backlight_on: 1, backlight_off: 1, power_down: 5000, power_cycle: 6000 });
}

#[test]
fn unit_conversions_round_up() {
    assert_eq!(msecs_to_pps_units(7), 70);
    assert_eq!(pps_units_to_msecs(0), 0);
    assert_eq!(pps_units_to_msecs(1), 1);
    assert_eq!(pps_units_to_msecs(10), 1);
    assert_eq!(pps_units_to_msecs(11), 2);
    assert!(!PpsDelays::default().is_valid());
    assert!(PpsDelays { power_cycle: 1, ..Default::default() }.is_valid());
    assert!(PpsDelays { backlight_on: 1, ..Default::default() }.is_valid());
}

#[test]
fn readout_decodes_what_the_firmware_programmed() {
    // pp_on = power_up 2000 << 16 | backlight_on 1; pp_off = power_down 500 << 16 | backlight_off 1; divisor cycle field 6
    let d = PpsDelays::from_regs((2000 << 16) | 1, (500 << 16) | 1, (1199 << 8) | 6);
    assert_eq!(d, PpsDelays { power_up: 2000, backlight_on: 1, backlight_off: 1, power_down: 500, power_cycle: 5000 }); // "(field - 1) * 1000"
    assert_eq!(PpsDelays::from_regs(0, 0, 0).power_cycle, 0); // field 0 means unset, not -1
    assert_eq!(PpsDelays::from_regs(0, 0, 1).power_cycle, 0);
}

#[test]
fn the_register_programming_for_atlas_matches_the_bspec_formula() {
    let t = PpsTiming::compute(PpsDelays::default(), ATLAS);
    // rawclk 24 MHz: div = 24; reference divider (100 * 24) / 2 - 1 = 1199; cycle field DIV_ROUND_UP(5000, 1000) + 1 = 6
    let p = t.registers_plan(24_000, false).unwrap();
    assert_eq!(
        p,
        vec![
            Effect::Write { reg: Reg::new(0xC7208), value: 0x07D0_0001 },
            Effect::Write { reg: Reg::new(0xC720C), value: 0x01F4_0001 },
            Effect::Write { reg: Reg::new(0xC7210), value: (1199 << 8) | 6 },
        ]
    );
    assert_eq!((1199u32 << 8) | 6, 0x0004_AF06);
    // "force_disable_vdd": the leftover VDD bit is cleared first
    let p = t.registers_plan(24_000, true).unwrap();
    assert_eq!(p[0], Effect::Rmw { reg: Reg::new(0xC7204), clear: 8, set: 0 });
    assert_eq!(p.len(), 4);
    // 19.2 MHz class rawclk (19200 kHz): div = 19 -> (100 * 19) / 2 - 1 = 949
    let p = t.registers_plan(19_200, false).unwrap();
    assert_eq!(p[2], Effect::Write { reg: Reg::new(0xC7210), value: (949 << 8) | 6 });
    // a PCH_RAWCLK_FREQ of 0 cannot make a divisor: named refusal, no writes
    let e = t.registers_plan(0, false).unwrap_err();
    assert_eq!(e, PpsError::RawclkUnknown { rawclk_khz: 0 });
    assert_eq!(e.name(), "PPS-RAWCLK-UNKNOWN");
    assert_eq!(pch_rawclk_khz(24), 24_000); // (PCH_RAWCLK_FREQ & 0x3ff) * 1000
    assert_eq!(pch_rawclk_khz(0xfc00_0000 | 24), 24_000);
}

#[test]
fn a_power_cycle_delay_that_is_already_a_multiple_stays_put() {
    let t = PpsTiming::compute(PpsDelays::default(), PpsDelays { power_cycle: 3000, power_up: 1, ..Default::default() });
    assert_eq!(t.hw.power_cycle, 3000);
    let t = PpsTiming::compute(PpsDelays::default(), PpsDelays { power_cycle: 3001, power_up: 1, ..Default::default() });
    assert_eq!(t.hw.power_cycle, 4000);
}

// ---- sequences on a scripted panel ---------------------------------------------------------------------------------
/// A panel sequencer that completes instantly: PP_STATUS follows PP_CONTROL.
struct Panel {
    ctrl: u32,
    log: Vec<String>,
    now: u64,
    /// Replace the status the panel reports (to model a sequencer that never reaches idle).
    stuck_status: Option<u32>,
    poll_reads: u32,
    delays_us: Vec<u32>,
    ops: usize,
}

impl Panel {
    fn new(ctrl: u32, now: u64) -> Panel {
        Panel { ctrl, log: vec![], now, stuck_status: None, poll_reads: 0, delays_us: vec![], ops: 0 }
    }
    fn status_for(ctrl: u32, stuck: Option<u32>) -> u32 {
        if let Some(s) = stuck {
            return s;
        }
        if ctrl & PANEL_POWER_ON != 0 {
            PP_ON | PP_SEQUENCE_STATE_ON_IDLE
        } else {
            0
        }
    }
    fn status(&self) -> u32 {
        Panel::status_for(self.ctrl, self.stuck_status)
    }
    fn read(&self, r: Reg) -> u32 {
        if r == pp_status(0) {
            self.status()
        } else if r == pp_control(0) {
            self.ctrl
        } else {
            panic!("unexpected PPS read {:#x}", r.offset())
        }
    }
    fn apply(&mut self, plan: &Plan) -> PpsFeed {
        for e in plan {
            match *e {
                Effect::Write { reg, value } => {
                    assert_eq!(reg, pp_control(0));
                    self.ctrl = value;
                    self.log.push(format!("W ctrl {value:#x}"));
                }
                Effect::PostingRead { reg } => self.log.push(format!("PR {:#x}", reg.offset())),
                Effect::DelayUs(us) => {
                    self.delays_us.push(us);
                    self.now += (us / 1000) as u64;
                    self.log.push(format!("D {us}"));
                }
                Effect::Poll(p) => {
                    let mut reads = 0;
                    let mut slept = vec![];
                    let (ctrl, stuck) = (self.ctrl, self.stuck_status);
                    let r = p.run(
                        || {
                            reads += 1;
                            Panel::status_for(ctrl, stuck)
                        },
                        |us| slept.push(us),
                    );
                    self.poll_reads += reads;
                    self.delays_us.extend(slept);
                    self.log.push(format!("P {:#x}/{:#x}", p.mask, p.want));
                    if let Err(t) = r {
                        return PpsFeed::PollTimedOut(t);
                    }
                }
                other => panic!("unexpected effect {other:?}"),
            }
        }
        PpsFeed::Ack
    }
    fn run(&mut self, st: &mut PpsState, kind: Kind) -> Result<PpsDone, PpsError> {
        let mut run = st.begin(kind);
        let mut feed = PpsFeed::Start;
        loop {
            self.ops += 1;
            assert!(self.ops < 1000, "PPS sequence did not terminate");
            match run.step(st, feed) {
                PpsStep::Op(PpsOp::Read(r)) => {
                    self.log.push(format!("R {:#x}", r.offset()));
                    feed = PpsFeed::Value(self.read(r));
                }
                PpsStep::Op(PpsOp::Apply(p)) => feed = self.apply(&p),
                PpsStep::Op(PpsOp::Now) => {
                    self.log.push("NOW".into());
                    feed = PpsFeed::Time(self.now);
                }
                PpsStep::Done(r) => return r,
            }
        }
    }
}

fn state(now: u64) -> PpsState {
    PpsState::new(PpsTiming::compute(PpsDelays::default(), ATLAS), now)
}

#[test]
fn vdd_on_from_cold_waits_the_cycle_forces_vdd_then_the_power_up_delay() {
    let mut st = state(10_000);
    let mut p = Panel::new(0, 10_000);
    let done = p.run(&mut st, Kind::VddOn).unwrap();
    assert!(done.need_to_disable); // VDD was not wanted before: the caller must pair this with a VDD off
    assert!(st.want_panel_vdd);
    // R ctrl (no vdd), R status (no power) -> cycle wait [NOW, idle-cycle poll], R ctrl, write |FORCE_VDD, posting read,
    // R status (still no power) -> T1+T3 = 200 ms
    assert_eq!(
        p.log,
        vec!["R 0xc7204", "R 0xc7200", "NOW", "P 0xb800000f/0x0", "R 0xc7204", "W ctrl 0x8", "PR 0xc7204", "R 0xc7200", "D 200000"]
    );
    assert_eq!(p.ctrl, EDP_FORCE_VDD);
}

#[test]
fn vdd_on_sleeps_out_what_remains_of_the_power_cycle() {
    let mut st = state(10_000);
    st.panel_power_off_time_ms = 9_900; // panel went off 100 ms ago; T11+T12 = 500 ms
    let mut p = Panel::new(0, 10_000);
    p.run(&mut st, Kind::VddOn).unwrap();
    assert_eq!(p.delays_us[0], 400_000); // 500 - 100
    // an off-time long ago needs no sleep (the first delay is the 200 ms power-up)
    let mut st = state(10_000);
    st.panel_power_off_time_ms = 1;
    let mut p = Panel::new(0, 10_000);
    p.run(&mut st, Kind::VddOn).unwrap();
    assert_eq!(p.delays_us, vec![200_000]);
}

#[test]
fn vdd_on_with_the_panel_already_powered_skips_both_waits() {
    let mut st = state(10_000);
    let mut p = Panel::new(PANEL_POWER_ON | PANEL_POWER_RESET, 10_000);
    p.run(&mut st, Kind::VddOn).unwrap();
    assert_eq!(p.log, vec!["R 0xc7204", "R 0xc7200", "R 0xc7204", "W ctrl 0xb", "PR 0xc7204", "R 0xc7200"]);
    assert!(p.delays_us.is_empty());
}

#[test]
fn vdd_on_when_vdd_is_already_forced_returns_at_once() {
    let mut st = state(10_000);
    let mut p = Panel::new(EDP_FORCE_VDD, 10_000);
    let done = p.run(&mut st, Kind::VddOn).unwrap();
    assert_eq!(p.log, vec!["R 0xc7204"]);
    assert!(done.need_to_disable);
    // a second request while one is outstanding does not need its own off
    let done = p.run(&mut st, Kind::VddOn).unwrap();
    assert!(!done.need_to_disable);
}

#[test]
fn vdd_off_clears_the_bit_and_stamps_the_off_time_only_when_the_panel_is_off() {
    let mut st = state(10_000);
    st.want_panel_vdd = true;
    let mut p = Panel::new(EDP_FORCE_VDD, 12_345);
    p.run(&mut st, Kind::VddOffSync).unwrap();
    assert!(!st.want_panel_vdd);
    assert_eq!(p.ctrl, 0);
    assert_eq!(st.panel_power_off_time_ms, 12_345);
    assert_eq!(p.log, vec!["R 0xc7204", "R 0xc7204", "W ctrl 0x0", "PR 0xc7204", "NOW"]);
    // panel still powered: VDD drops but no off-time is recorded (:858-863)
    let mut st = state(10_000);
    let mut p = Panel::new(EDP_FORCE_VDD | PANEL_POWER_ON | PANEL_POWER_RESET, 99_999);
    p.run(&mut st, Kind::VddOffSync).unwrap();
    assert_eq!(st.panel_power_off_time_ms, 0);
    assert_eq!(p.ctrl, PANEL_POWER_ON | PANEL_POWER_RESET);
    // VDD not on: nothing to do
    let mut p = Panel::new(0, 5);
    p.run(&mut st, Kind::VddOffSync).unwrap();
    assert_eq!(p.log, vec!["R 0xc7204"]);
}

#[test]
fn deferred_vdd_off_asks_for_five_power_cycles() {
    let mut st = state(10_000);
    st.want_panel_vdd = true;
    let mut p = Panel::new(EDP_FORCE_VDD, 1);
    let done = p.run(&mut st, Kind::VddOffDeferred).unwrap();
    assert_eq!(done.schedule_vdd_off_after_ms, Some(500 * 5)); // panel_power_cycle_delay * 5, intel_pps.c:917
    assert!(!st.want_panel_vdd);
    assert!(p.log.is_empty()); // nothing touched yet
    // the work item does nothing if VDD was wanted again in the meantime
    st.want_panel_vdd = true;
    p.run(&mut st, Kind::VddOffWork).unwrap();
    assert_eq!(p.ctrl, EDP_FORCE_VDD);
    st.want_panel_vdd = false;
    p.run(&mut st, Kind::VddOffWork).unwrap();
    assert_eq!(p.ctrl, 0);
}

#[test]
fn panel_on_waits_idle_sets_power_and_reset_then_the_idle_on_state() {
    let mut st = state(10_000);
    let mut p = Panel::new(EDP_FORCE_VDD, 20_000);
    p.run(&mut st, Kind::PanelOn).unwrap();
    assert_eq!(p.ctrl, EDP_FORCE_VDD | PANEL_POWER_ON | PANEL_POWER_RESET);
    assert_eq!(p.log, vec!["R 0xc7200", "NOW", "P 0xb800000f/0x0", "R 0xc7204", "W ctrl 0xb", "PR 0xc7204", "P 0xb000000f/0x80000008", "NOW"]);
    assert_eq!(st.last_power_on_ms, 20_000);
    // already on: warn-and-return (a single status read)
    let mut p = Panel::new(PANEL_POWER_ON | PANEL_POWER_RESET, 21_000);
    p.run(&mut st, Kind::PanelOn).unwrap();
    assert_eq!(p.log, vec!["R 0xc7200"]);
}

#[test]
fn panel_off_clears_power_reset_vdd_and_blc_then_waits_idle_off() {
    let mut st = state(10_000);
    st.want_panel_vdd = true;
    let mut p = Panel::new(PANEL_POWER_ON | PANEL_POWER_RESET | EDP_FORCE_VDD | EDP_BLC_ENABLE, 30_000);
    p.run(&mut st, Kind::PanelOff).unwrap();
    assert_eq!(p.ctrl, 0);
    assert!(!st.want_panel_vdd);
    assert_eq!(st.panel_power_off_time_ms, 30_000);
    assert_eq!(p.log, vec!["R 0xc7204", "W ctrl 0x0", "PR 0xc7204", "P 0xb0000000/0x0", "NOW"]);
    // unrelated control bits (the BXT cycle-delay field, the unlock key) survive
    let mut p = Panel::new(PANEL_POWER_ON | PANEL_UNLOCK_REGS | (3 << 4), 1);
    p.run(&mut st, Kind::PanelOff).unwrap();
    assert_eq!(p.ctrl, PANEL_UNLOCK_REGS | (3 << 4));
}

#[test]
fn the_backlight_gate_honours_t8_and_t9() {
    let mut st = state(10_000);
    st.timing = PpsTiming::compute(PpsDelays::default(), PpsDelays { power_up: 1, backlight_on: 1000, backlight_off: 800, ..Default::default() });
    // T8 = 100 ms; the panel powered up 30 ms ago -> wait 70 ms, then set EDP_BLC_ENABLE
    st.last_power_on_ms = 20_000;
    let mut p = Panel::new(PANEL_POWER_ON | PANEL_POWER_RESET, 20_030);
    p.run(&mut st, Kind::BacklightOn).unwrap();
    assert_eq!(p.delays_us, vec![70_000]);
    assert_eq!(p.ctrl, PANEL_POWER_ON | PANEL_POWER_RESET | EDP_BLC_ENABLE);
    // long after power-up: no wait
    let mut p = Panel::new(PANEL_POWER_ON | PANEL_POWER_RESET, 99_000);
    p.run(&mut st, Kind::BacklightOn).unwrap();
    assert!(p.delays_us.is_empty());
    // off: clear the bit, then the whole T9 = 80 ms
    let mut p = Panel::new(PANEL_POWER_ON | PANEL_POWER_RESET | EDP_BLC_ENABLE, 99_000);
    p.run(&mut st, Kind::BacklightOff).unwrap();
    assert_eq!(p.ctrl, PANEL_POWER_ON | PANEL_POWER_RESET);
    assert_eq!(p.delays_us, vec![80_000]);
}

// ---- the bounded wait ----------------------------------------------------------------------------------------------
#[test]
fn a_panel_that_never_reaches_idle_on_times_out_by_name_after_the_capped_number_of_reads() {
    let mut st = state(10_000);
    let mut p = Panel::new(0, 20_000);
    p.stuck_status = Some(0); // never PP_ON | ON_IDLE
    // PanelOn: R status (0 -> off), cycle poll satisfied (status 0 == IDLE_CYCLE_VALUE), write power, then IDLE_ON never arrives
    let e = p.run(&mut st, Kind::PanelOn).unwrap_err();
    match e {
        PpsError::PanelStatusTimeout(t) => {
            assert_eq!(t.what, "panel power on");
            assert_eq!((t.reg, t.mask, t.want), (pp_status(0), IDLE_ON_MASK, IDLE_ON_VALUE));
            assert_eq!(t.polls, 501); // 5000 ms / 10 ms + 1
            assert_eq!(t.got, 0);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(e.name(), "PPS-PANEL-STATUS-TIMEOUT");
    // the cycle poll counted 1 read, the idle-on poll 501; nothing beyond
    assert_eq!(p.poll_reads, 1 + 501);
    // the failed run left last_power_on untouched
    assert_eq!(st.last_power_on_ms, 10_000);
}

#[test]
fn a_stuck_idle_off_wait_is_bounded_too() {
    let mut st = state(10_000);
    st.want_panel_vdd = true;
    let mut p = Panel::new(PANEL_POWER_ON | PANEL_POWER_RESET | EDP_FORCE_VDD, 1);
    p.stuck_status = Some(PP_ON); // never goes off
    let e = p.run(&mut st, Kind::PanelOff).unwrap_err();
    assert!(matches!(e, PpsError::PanelStatusTimeout(t) if t.what == "panel power off" && t.polls == 501));
    assert_eq!(p.poll_reads, 501);
    assert_eq!(st.panel_power_off_time_ms, 0); // the failed power-off is not stamped
}

#[test]
fn a_wrong_answer_to_a_step_is_a_named_error() {
    let mut st = state(10_000);
    let mut run = st.begin(Kind::PanelOn);
    let _ = run.step(&mut st, PpsFeed::Start);
    match run.step(&mut st, PpsFeed::Ack) {
        PpsStep::Done(Err(PpsError::OutOfSequence)) => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(PpsError::OutOfSequence.name(), "PPS-OUT-OF-SEQUENCE");
}

#[test]
fn pps_plans_write_only_pps_registers() {
    let t = PpsTiming::compute(PpsDelays::default(), ATLAS);
    for r in written_regs(&t.registers_plan(24_000, true).unwrap()) {
        assert!((0xC7200..=0xC7210).contains(&r.offset()));
    }
}

// ---- mutation-sweep additions ---------------------------------------------------------------------------------------
#[test]
fn any_single_delay_makes_the_set_valid() {
    for (i, d) in [
        PpsDelays { power_up: 1, ..Default::default() },
        PpsDelays { backlight_on: 1, ..Default::default() },
        PpsDelays { backlight_off: 1, ..Default::default() },
        PpsDelays { power_down: 1, ..Default::default() },
        PpsDelays { power_cycle: 1, ..Default::default() },
    ]
    .iter()
    .enumerate()
    {
        assert!(d.is_valid(), "field {i}");
    }
}

#[test]
fn posting_reads_target_the_control_register() {
    let mut st = state(10_000);
    st.want_panel_vdd = true;
    let mut p = Panel::new(EDP_FORCE_VDD, 5);
    p.run(&mut st, Kind::VddOffSync).unwrap();
    // the harness logs the register of every posting read
    assert!(p.log.contains(&"PR 0xc7204".to_string()), "{:?}", p.log);
}

#[test]
fn a_run_starts_clean() {
    let mut st = state(10_000);
    let run = st.begin(Kind::BacklightOff);
    let _ = run; // a fresh run holds no control value and no pending schedule
    let mut p = Panel::new(PANEL_POWER_ON | EDP_BLC_ENABLE, 1);
    let done = p.run(&mut st, Kind::BacklightOff).unwrap();
    assert_eq!(done, PpsDone { need_to_disable: false, schedule_vdd_off_after_ms: None });
}
