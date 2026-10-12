// SPDX-License-Identifier: GPL-2.0-only
//! The eDP panel power sequencer (PPS) of an SPT-LP PCH: delays from the VBT / firmware / eDP spec, the register
//! programming, and VDD / panel power / panel backlight-gate sequences with bounded waits.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_pps.c (v7.0-rc6): `intel_pps_get_registers` (:499-525),
//! `edp_have_panel_power` / `edp_have_panel_vdd` (:547-571), the `IDLE_*` masks (:586-596), `wait_panel_status`
//! (:606-644), `wait_panel_on/off/power_cycle` (:646-695), `wait_backlight_on` / `edp_wait_backlight_off` (:706-720),
//! `ilk_get_pp_control` (:722-736), `intel_pps_vdd_on_unlocked` (:743-806), `intel_pps_vdd_off_sync_unlocked` (:825-868),
//! `edp_panel_vdd_schedule_off` (:896-921), `intel_pps_vdd_off_unlocked` (:923-944), `intel_pps_on_unlocked` (:955-1016),
//! `intel_pps_off_unlocked` (:1027-1070), `intel_pps_backlight_on/off` (:1082-1131), `intel_pps_readout_hw_state` (:1368-1404),
//! `pps_delays_valid` .. `pps_init_delays` (:1438-1575), `pps_init_registers` (:1577-1669), `intel_num_pps` (:355-372),
//! `intel_pps_setup` (:1773-1781). Copyright (c) 2008, 2018 Intel Corporation.
//!
//! SHAPE. State that Linux keeps in `intel_dp->pps` lives in [`PpsState`]. A sequence is a [`PpsRun`] pull machine: it asks
//! the executor to read a register ([`PpsOp::Read`]), apply effects ([`PpsOp::Apply`]) or read the clock
//! ([`PpsOp::Now`]), and decides its next step on the answer — exactly the reads Linux's `intel_de_read()` /
//! `ktime_get_boottime()` calls make. Time is milliseconds on any monotonic clock the executor owns.
//!
//! DEVIATIONS FROM LINUX, each named:
//!  * `PANEL_STATUS_TIMEOUT_IS_FATAL`: `wait_panel_status()` logs "panel status timeout" and CARRIES ON (:630-640). A sequence here
//!    stops with [`PpsError::PanelStatusTimeout`] — a panel that never reached the state must not be assumed powered.
//!  * `WAIT_POLL`: `poll_timeout_us(.., 10 ms sleep, 5000 ms timeout)` is [`PollPlan::with_timeout`] with the same numbers.
//!  * Power-domain references (`intel_display_power_get/put` of the AUX domain around VDD), the PPS mutex, the delayed-work
//!    queue (`edp_panel_vdd_schedule_off` returns the delay for the caller to schedule), the BIOS-state debug dumps, the
//!    state verify (`intel_pps_verify_state`), Ironlake / display 13-14 workarounds, VLV/CHV/BXT/GLK/ICP pipe/index selection
//!    (SPT-LP has exactly one sequencer, `intel_num_pps` = 1) and the DP/PCH port-select bits (only IBX/CPT) are not ported.
use crate::plan::{Effect, Plan, PollPlan, PollTimeout};
use crate::regs::*;
use alloc::vec;
use i915_common_core::reg::{field_get, field_prep};
use i915_common_core::Reg;
use i915_vbt_core::edp::EdpParams;

/// `poll_timeout_us(.., sleep_us = 10 * 1000, timeout_us = 5000 * 1000)` — intel_pps.c:626-627.
pub const PPS_WAIT_INTERVAL_US: u32 = 10 * 1000;
pub const PPS_WAIT_TIMEOUT_US: u32 = 5000 * 1000;

/// `IDLE_ON_MASK` / `IDLE_ON_VALUE` — intel_pps.c:586-587.
pub const IDLE_ON_MASK: u32 = PP_ON | PP_SEQUENCE_MASK | PP_SEQUENCE_STATE_MASK;
pub const IDLE_ON_VALUE: u32 = PP_ON | PP_SEQUENCE_NONE | PP_SEQUENCE_STATE_ON_IDLE;
/// `IDLE_OFF_MASK` / `IDLE_OFF_VALUE` — intel_pps.c:589-590.
pub const IDLE_OFF_MASK: u32 = PP_ON | PP_SEQUENCE_MASK;
pub const IDLE_OFF_VALUE: u32 = PP_SEQUENCE_NONE;
/// `IDLE_CYCLE_MASK` / `IDLE_CYCLE_VALUE` — intel_pps.c:592-593.
pub const IDLE_CYCLE_MASK: u32 = PP_ON | PP_SEQUENCE_MASK | PP_CYCLE_DELAY_ACTIVE | PP_SEQUENCE_STATE_MASK;
pub const IDLE_CYCLE_VALUE: u32 = PP_SEQUENCE_NONE | PP_SEQUENCE_STATE_OFF_IDLE;

/// `struct intel_pps_delays` — intel_display_types.h:314-320. All in 100 us units (eDP: T1+T3, T8, T9, T10, T11+T12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PpsDelays {
    pub power_up: u16,
    pub backlight_on: u16,
    pub backlight_off: u16,
    pub power_down: u16,
    pub power_cycle: u16,
}

impl PpsDelays {
    /// `pps_delays_valid()` — intel_pps.c:1438-1442: any field set.
    pub const fn is_valid(&self) -> bool {
        self.power_up != 0 || self.backlight_on != 0 || self.backlight_off != 0 || self.power_down != 0 || self.power_cycle != 0
    }

    /// The VBT's eDP power sequence as `connector->panel.vbt.edp.pps` (`vbt_edp_to_pps_delays`, intel_bios.c:1406): T1+T3 ->
    /// power_up, T8 -> backlight_on, T9 -> backlight_off, T10 -> power_down, T11+T12 -> power_cycle.
    pub fn from_vbt(edp: &EdpParams) -> PpsDelays {
        let s = edp.power_seq;
        PpsDelays { power_up: s.t1_t3, backlight_on: s.t8, backlight_off: s.t9, power_down: s.t10, power_cycle: s.t11_t12 }
    }

    /// `intel_pps_readout_hw_state()` — intel_pps.c:1368-1404, for the PP_DIVISOR layout (SPT: before CNP the cycle delay is in
    /// PP_DIVISOR): the registers the firmware left. "hardware wants <delay>+1 in 100ms units".
    pub fn from_regs(pp_on: u32, pp_off: u32, pp_div: u32) -> PpsDelays {
        let cycle = field_get(PANEL_POWER_CYCLE_DELAY_MASK, pp_div);
        PpsDelays {
            power_up: field_get(PANEL_POWER_UP_DELAY_MASK, pp_on) as u16,
            backlight_on: field_get(PANEL_LIGHT_ON_DELAY_MASK, pp_on) as u16,
            backlight_off: field_get(PANEL_LIGHT_OFF_DELAY_MASK, pp_off) as u16,
            power_down: field_get(PANEL_POWER_DOWN_DELAY_MASK, pp_off) as u16,
            power_cycle: if cycle != 0 { ((cycle - 1) * 1000) as u16 } else { 0 },
        }
    }
}

/// `pps_init_delays_spec()` — intel_pps.c:1498-1513: "Upper limits from eDP 1.3 spec".
pub const SPEC_DELAYS: PpsDelays = PpsDelays {
    power_up: msecs_to_pps_units(10 + 200),      // T1+T3, :1506
    backlight_on: msecs_to_pps_units(50),        // :1507
    backlight_off: msecs_to_pps_units(50),       // :1508
    power_down: msecs_to_pps_units(500),         // T10, :1509
    power_cycle: msecs_to_pps_units(10 + 500),   // T11+T12, :1510
};

/// `msecs_to_pps_units()` — intel_pps.c:1444-1448 ("PPS uses 100us units").
pub const fn msecs_to_pps_units(msecs: u32) -> u16 {
    (msecs * 10) as u16
}

/// `pps_units_to_msecs()` — intel_pps.c:1450-1454: `DIV_ROUND_UP(val, 10)`.
pub const fn pps_units_to_msecs(val: u32) -> u32 {
    (val + 9) / 10
}

/// The result of `pps_init_delays()`: the millisecond delays Linux waits by and the values the registers get.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpsTiming {
    pub panel_power_up_delay_ms: u32,
    pub backlight_on_delay_ms: u32,
    pub backlight_off_delay_ms: u32,
    pub panel_power_down_delay_ms: u32,
    pub panel_power_cycle_delay_ms: u32,
    /// `intel_dp->pps.pps_delays` AFTER the overrides at :1556-1573 (backlight delays forced to 1, power cycle rounded up to
    /// the hardware's 100 ms granularity): what `pps_init_registers()` writes.
    pub hw: PpsDelays,
}

impl PpsTiming {
    /// `pps_init_delays()` — intel_pps.c:1515-1575. `bios` = the delays read back from the registers the firmware left,
    /// `vbt` = the VBT's. "Use the max of the register settings and vbt. If both are unset, fall back to the spec limits."
    pub fn compute(bios: PpsDelays, vbt: PpsDelays) -> PpsTiming {
        let pick = |a: u16, b: u16, spec: u16| -> u16 {
            let m = if a > b { a } else { b };
            if m == 0 {
                spec
            } else {
                m
            }
        };
        let mut f = PpsDelays {
            power_up: pick(bios.power_up, vbt.power_up, SPEC_DELAYS.power_up),
            backlight_on: pick(bios.backlight_on, vbt.backlight_on, SPEC_DELAYS.backlight_on),
            backlight_off: pick(bios.backlight_off, vbt.backlight_off, SPEC_DELAYS.backlight_off),
            power_down: pick(bios.power_down, vbt.power_down, SPEC_DELAYS.power_down),
            power_cycle: pick(bios.power_cycle, vbt.power_cycle, SPEC_DELAYS.power_cycle),
        };
        let t = PpsTiming {
            panel_power_up_delay_ms: pps_units_to_msecs(f.power_up as u32),
            backlight_on_delay_ms: pps_units_to_msecs(f.backlight_on as u32),
            backlight_off_delay_ms: pps_units_to_msecs(f.backlight_off as u32),
            panel_power_down_delay_ms: pps_units_to_msecs(f.power_down as u32),
            panel_power_cycle_delay_ms: pps_units_to_msecs(f.power_cycle as u32),
            hw: f,
        };
        // "We override the HW backlight delays to 1 because we do manual waits on them." — :1556-1568
        f.backlight_on = 1;
        f.backlight_off = 1;
        // "HW has only a 100msec granularity for power_cycle so round it up accordingly." — :1570-1573
        let unit = msecs_to_pps_units(100) as u32;
        f.power_cycle = (((f.power_cycle as u32) + unit - 1) / unit * unit) as u16;
        PpsTiming { hw: f, ..t }
    }

    /// `pps_init_registers()` — intel_pps.c:1577-1669, SPT-LP (one PPS at the PCH base, PP_DIVISOR present, no port-select
    /// bits). `rawclk_khz` is `DISPLAY_RUNTIME_INFO->rawclk_freq` (SPT: [`pch_rawclk_khz`]). With `force_disable_vdd` the
    /// leftover `EDP_FORCE_VDD` is cleared first (:1603-1614).
    pub fn registers_plan(&self, rawclk_khz: u32, force_disable_vdd: bool) -> Result<Plan, PpsError> {
        let div = rawclk_khz / 1000;
        if div == 0 {
            return Err(PpsError::RawclkUnknown { rawclk_khz });
        }
        let mut p = Plan::new();
        if force_disable_vdd {
            p.push(Effect::Rmw { reg: pp_control(0), clear: EDP_FORCE_VDD, set: 0 });
        }
        let pp_on = field_prep(PANEL_POWER_UP_DELAY_MASK, self.hw.power_up as u32) | field_prep(PANEL_LIGHT_ON_DELAY_MASK, self.hw.backlight_on as u32);
        let pp_off = field_prep(PANEL_LIGHT_OFF_DELAY_MASK, self.hw.backlight_off as u32) | field_prep(PANEL_POWER_DOWN_DELAY_MASK, self.hw.power_down as u32);
        p.push(Effect::Write { reg: pp_on_delays(0), value: pp_on });
        p.push(Effect::Write { reg: pp_off_delays(0), value: pp_off });
        // "Compute the divisor for the pp clock, simply match the Bspec formula." — :1648-1660
        let cycle = ((self.hw.power_cycle as u32) + 999) / 1000 + 1;
        p.push(Effect::Write {
            reg: pp_divisor(0),
            value: field_prep(PP_REFERENCE_DIVIDER_MASK, (100 * div) / 2 - 1) | field_prep(PANEL_POWER_CYCLE_DELAY_MASK, cycle),
        });
        Ok(p)
    }
}

/// `pch_rawclk()` — intel_cdclk.c:3791-3794: `(PCH_RAWCLK_FREQ & RAWCLK_FREQ_MASK) * 1000` kHz (SPT, `HAS_PCH_SPLIT`).
pub const fn pch_rawclk_khz(pch_rawclk_freq: u32) -> u32 {
    (pch_rawclk_freq & RAWCLK_FREQ_MASK) * 1000
}

/// PPS refusals, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PpsError {
    /// The raw clock reads 0: the divisor `(100 * div) / 2 - 1` would underflow.
    RawclkUnknown { rawclk_khz: u32 },
    /// A bounded PP_STATUS wait ran out (`wait_panel_status()`'s "panel status timeout").
    PanelStatusTimeout(PollTimeout),
    /// The executor answered a step with the wrong kind of reply.
    OutOfSequence,
}

impl PpsError {
    pub const fn name(&self) -> &'static str {
        match self {
            PpsError::RawclkUnknown { .. } => "PPS-RAWCLK-UNKNOWN",
            PpsError::PanelStatusTimeout(_) => "PPS-PANEL-STATUS-TIMEOUT",
            PpsError::OutOfSequence => "PPS-OUT-OF-SEQUENCE",
        }
    }
}

/// What persists between sequences (`intel_dp->pps`).
#[derive(Clone, Copy, Debug)]
pub struct PpsState {
    pub timing: PpsTiming,
    /// `want_panel_vdd`.
    pub want_panel_vdd: bool,
    /// `panel_power_off_time` (ms; 0 = "assume it was toggled long ago", intel_pps.c:1355-1360).
    pub panel_power_off_time_ms: u64,
    /// `last_power_on` (ms).
    pub last_power_on_ms: u64,
}

impl PpsState {
    /// `pps_init_timestamps()` — intel_pps.c:1355-1366, with `now_ms` for `jiffies`.
    pub fn new(timing: PpsTiming, now_ms: u64) -> PpsState {
        PpsState { timing, want_panel_vdd: false, panel_power_off_time_ms: 0, last_power_on_ms: now_ms }
    }

    /// Start a sequence.
    pub fn begin(&mut self, kind: Kind) -> PpsRun {
        PpsRun { kind, st: Sq::Start, pp: 0, need_to_disable: false, scheduled: None }
    }
}

/// The sequences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `intel_pps_vdd_on_unlocked()`.
    VddOn,
    /// `intel_pps_vdd_off_unlocked(sync = true)` = clear `want_panel_vdd` then `intel_pps_vdd_off_sync_unlocked()`.
    VddOffSync,
    /// `intel_pps_vdd_off_unlocked(sync = false)`: clear `want_panel_vdd`, ask the caller to run [`Kind::VddOffWork`] later.
    VddOffDeferred,
    /// `edp_panel_vdd_work()`: if VDD is still not wanted, `intel_pps_vdd_off_sync_unlocked()`.
    VddOffWork,
    /// `intel_pps_on_unlocked()`.
    PanelOn,
    /// `intel_pps_off_unlocked()`.
    PanelOff,
    /// `intel_pps_backlight_on()`.
    BacklightOn,
    /// `intel_pps_backlight_off()`.
    BacklightOff,
}

/// What the executor does for a [`PpsRun`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PpsOp {
    /// Read the register; answer [`PpsFeed::Value`].
    Read(Reg),
    /// Apply the effects in order; answer [`PpsFeed::Ack`], or [`PpsFeed::PollTimedOut`] if a `Poll` in them ran out.
    Apply(Plan),
    /// Read the monotonic clock in ms; answer [`PpsFeed::Time`].
    Now,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PpsFeed {
    Start,
    Ack,
    Value(u32),
    Time(u64),
    PollTimedOut(PollTimeout),
}

/// A finished sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpsDone {
    /// `VddOn`: Linux's return value, "the caller must pair this with a VDD off" (`need_to_disable`).
    pub need_to_disable: bool,
    /// `VddOffDeferred`: schedule [`Kind::VddOffWork`] this many ms from now (`panel_power_cycle_delay * 5`).
    pub schedule_vdd_off_after_ms: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PpsStep {
    Op(PpsOp),
    Done(Result<PpsDone, PpsError>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sq {
    Start,
    // VddOn
    VOnHaveVdd,
    VOnHavePower1,
    VOnCycleNow,
    VOnCycleDone,
    VOnCtrl,
    VOnWritten,
    VOnHavePower2,
    VOnUpDelay,
    // VddOffSync
    VOffHaveVdd,
    VOffCtrl,
    VOffWritten,
    VOffStamp,
    // PanelOn
    OnStatus,
    OnCycleNow,
    OnCycleDone,
    OnCtrl,
    OnWritten,
    OnWaited,
    OnStamp,
    // PanelOff
    OffCtrl,
    OffWritten,
    OffStamp,
    // backlight
    BlOnNow,
    BlOnWaited,
    BlOnCtrl,
    BlOnWritten,
    BlOffCtrl,
    BlOffWritten,
    BlOffWaited,
    Finished,
}

/// One sequence in progress.
#[derive(Clone, Copy, Debug)]
pub struct PpsRun {
    kind: Kind,
    st: Sq,
    /// The `pp_control` value being edited.
    pp: u32,
    need_to_disable: bool,
    scheduled: Option<u32>,
}

fn status_poll(mask: u32, value: u32, what: &'static str) -> Effect {
    Effect::Poll(PollPlan::with_timeout(pp_status(0), mask, value, PPS_WAIT_INTERVAL_US, PPS_WAIT_TIMEOUT_US, what))
}

fn ctrl_write_plan(value: u32) -> Plan {
    vec![Effect::Write { reg: pp_control(0), value }, Effect::PostingRead { reg: pp_control(0) }]
}

impl PpsRun {
    fn done(&mut self) -> PpsStep {
        self.st = Sq::Finished;
        PpsStep::Done(Ok(PpsDone { need_to_disable: self.need_to_disable, schedule_vdd_off_after_ms: self.scheduled }))
    }

    fn err(&mut self, e: PpsError) -> PpsStep {
        self.st = Sq::Finished;
        PpsStep::Done(Err(e))
    }

    fn read_status(&mut self, next: Sq) -> PpsStep {
        self.st = next;
        PpsStep::Op(PpsOp::Read(pp_status(0)))
    }

    fn read_ctrl(&mut self, next: Sq) -> PpsStep {
        self.st = next;
        PpsStep::Op(PpsOp::Read(pp_control(0)))
    }

    fn apply(&mut self, plan: Plan, next: Sq) -> PpsStep {
        self.st = next;
        PpsStep::Op(PpsOp::Apply(plan))
    }

    fn now(&mut self, next: Sq) -> PpsStep {
        self.st = next;
        PpsStep::Op(PpsOp::Now)
    }

    /// `wait_panel_power_cycle()` second half: sleep out what remains of T11+T12, then wait for the sequencer to be idle
    /// (`IDLE_CYCLE`), :670-695.
    fn cycle_wait_plan(&self, st: &PpsState, now_ms: u64) -> Plan {
        let elapsed = now_ms.saturating_sub(st.panel_power_off_time_ms);
        let remaining = (st.timing.panel_power_cycle_delay_ms as u64).saturating_sub(elapsed);
        let mut p = Plan::new();
        if remaining != 0 {
            p.push(Effect::DelayUs((remaining * 1000) as u32));
        }
        p.push(status_poll(IDLE_CYCLE_MASK, IDLE_CYCLE_VALUE, "panel power cycle"));
        p
    }

    /// Advance. `PpsFeed::Start` first; `st` is the persistent [`PpsState`] this run belongs to.
    pub fn step(&mut self, st: &mut PpsState, feed: PpsFeed) -> PpsStep {
        // A timed-out wait ends the sequence with the named error, wherever it happened.
        if let PpsFeed::PollTimedOut(t) = feed {
            return self.err(PpsError::PanelStatusTimeout(t));
        }
        match (self.st, feed) {
            // ---------------------------------------------------------------- VDD on (:743-806)
            (Sq::Start, PpsFeed::Start) => match self.kind {
                Kind::VddOn => {
                    self.need_to_disable = !st.want_panel_vdd; // :749
                    st.want_panel_vdd = true; // :757
                    self.read_ctrl(Sq::VOnHaveVdd)
                }
                Kind::VddOffSync => {
                    st.want_panel_vdd = false; // intel_pps_vdd_off_unlocked, :937
                    self.read_ctrl(Sq::VOffHaveVdd)
                }
                Kind::VddOffDeferred => {
                    st.want_panel_vdd = false;
                    self.scheduled = Some(st.timing.panel_power_cycle_delay_ms * 5); // :913
                    self.done()
                }
                Kind::VddOffWork => {
                    if st.want_panel_vdd {
                        self.done() // edp_panel_vdd_work(): "if (!want_panel_vdd)"
                    } else {
                        self.read_ctrl(Sq::VOffHaveVdd)
                    }
                }
                Kind::PanelOn => self.read_status(Sq::OnStatus),
                Kind::PanelOff => {
                    // "need VDD to turn off panel" is a WARN only (:1043-1045); want_panel_vdd is cleared (:1056).
                    self.read_ctrl(Sq::OffCtrl)
                }
                Kind::BacklightOn => self.now(Sq::BlOnNow),
                Kind::BacklightOff => self.read_ctrl(Sq::BlOffCtrl),
            },
            (Sq::VOnHaveVdd, PpsFeed::Value(v)) => {
                if v & EDP_FORCE_VDD != 0 {
                    return self.done(); // already on: :759-760
                }
                self.read_status(Sq::VOnHavePower1)
            }
            (Sq::VOnHavePower1, PpsFeed::Value(v)) => {
                if v & PP_ON == 0 {
                    self.now(Sq::VOnCycleNow) // `if (!edp_have_panel_power) wait_panel_power_cycle()`, :773-774
                } else {
                    self.read_ctrl(Sq::VOnCtrl)
                }
            }
            (Sq::VOnCycleNow, PpsFeed::Time(t)) => {
                let plan = self.cycle_wait_plan(st, t);
                self.apply(plan, Sq::VOnCycleDone)
            }
            (Sq::VOnCycleDone, PpsFeed::Ack) => self.read_ctrl(Sq::VOnCtrl),
            (Sq::VOnCtrl, PpsFeed::Value(v)) => {
                self.pp = v | EDP_FORCE_VDD; // :776-777
                let plan = ctrl_write_plan(self.pp);
                self.apply(plan, Sq::VOnWritten)
            }
            (Sq::VOnWritten, PpsFeed::Ack) => self.read_status(Sq::VOnHavePower2),
            (Sq::VOnHavePower2, PpsFeed::Value(v)) => {
                if v & PP_ON == 0 {
                    // "If the panel wasn't on, delay before accessing aux channel" — :788-796
                    let us = st.timing.panel_power_up_delay_ms * 1000;
                    self.apply(vec![Effect::DelayUs(us)], Sq::VOnUpDelay)
                } else {
                    self.done()
                }
            }
            (Sq::VOnUpDelay, PpsFeed::Ack) => self.done(),

            // ---------------------------------------------------------------- VDD off (:825-868)
            (Sq::VOffHaveVdd, PpsFeed::Value(v)) => {
                if v & EDP_FORCE_VDD == 0 {
                    return self.done(); // :836-837
                }
                self.read_ctrl(Sq::VOffCtrl)
            }
            (Sq::VOffCtrl, PpsFeed::Value(v)) => {
                self.pp = v & !EDP_FORCE_VDD; // :843-844
                let plan = ctrl_write_plan(self.pp);
                self.apply(plan, Sq::VOffWritten)
            }
            (Sq::VOffWritten, PpsFeed::Ack) => {
                if self.pp & PANEL_POWER_ON == 0 {
                    self.now(Sq::VOffStamp) // panel_power_off_time = ktime_get_boottime(), :861
                } else {
                    self.done()
                }
            }
            (Sq::VOffStamp, PpsFeed::Time(t)) => {
                st.panel_power_off_time_ms = t;
                self.done()
            }

            // ---------------------------------------------------------------- panel on (:955-1016)
            (Sq::OnStatus, PpsFeed::Value(v)) => {
                if v & PP_ON != 0 {
                    return self.done(); // "panel power already on": WARN and return, :971-976
                }
                self.now(Sq::OnCycleNow) // wait_panel_power_cycle(), :978
            }
            (Sq::OnCycleNow, PpsFeed::Time(t)) => {
                let plan = self.cycle_wait_plan(st, t);
                self.apply(plan, Sq::OnCycleDone)
            }
            (Sq::OnCycleDone, PpsFeed::Ack) => self.read_ctrl(Sq::OnCtrl),
            (Sq::OnCtrl, PpsFeed::Value(v)) => {
                // `pp |= PANEL_POWER_ON; if (!ironlake) pp |= PANEL_POWER_RESET;` — :997-1003
                self.pp = v | PANEL_POWER_ON | PANEL_POWER_RESET;
                let plan = ctrl_write_plan(self.pp);
                self.apply(plan, Sq::OnWritten)
            }
            (Sq::OnWritten, PpsFeed::Ack) => self.apply(vec![status_poll(IDLE_ON_MASK, IDLE_ON_VALUE, "panel power on")], Sq::OnWaited),
            (Sq::OnWaited, PpsFeed::Ack) => self.now(Sq::OnStamp),
            (Sq::OnStamp, PpsFeed::Time(t)) => {
                st.last_power_on_ms = t; // :1005
                self.done()
            }

            // ---------------------------------------------------------------- panel off (:1027-1070)
            (Sq::OffCtrl, PpsFeed::Value(v)) => {
                // "We need to switch off panel power _and_ force vdd, for otherwise some panels get very unhappy" — :1051-1052
                self.pp = v & !(PANEL_POWER_ON | PANEL_POWER_RESET | EDP_FORCE_VDD | EDP_BLC_ENABLE);
                st.want_panel_vdd = false;
                let mut plan = ctrl_write_plan(self.pp);
                plan.push(status_poll(IDLE_OFF_MASK, IDLE_OFF_VALUE, "panel power off"));
                self.apply(plan, Sq::OffWritten)
            }
            (Sq::OffWritten, PpsFeed::Ack) => self.now(Sq::OffStamp),
            (Sq::OffStamp, PpsFeed::Time(t)) => {
                st.panel_power_off_time_ms = t; // :1062
                self.done()
            }

            // ---------------------------------------------------------------- backlight gate (:1082-1131)
            (Sq::BlOnNow, PpsFeed::Time(t)) => {
                // wait_backlight_on(): sleep out what remains of T8 since the last power-on (:706-710, :1092)
                let elapsed = t.saturating_sub(st.last_power_on_ms);
                let remaining = (st.timing.backlight_on_delay_ms as u64).saturating_sub(elapsed);
                let plan = if remaining != 0 { vec![Effect::DelayUs((remaining * 1000) as u32)] } else { Plan::new() };
                if plan.is_empty() {
                    self.read_ctrl(Sq::BlOnCtrl)
                } else {
                    self.apply(plan, Sq::BlOnWaited)
                }
            }
            (Sq::BlOnWaited, PpsFeed::Ack) => self.read_ctrl(Sq::BlOnCtrl),
            (Sq::BlOnCtrl, PpsFeed::Value(v)) => {
                self.pp = v | EDP_BLC_ENABLE;
                let plan = ctrl_write_plan(self.pp);
                self.apply(plan, Sq::BlOnWritten)
            }
            (Sq::BlOnWritten, PpsFeed::Ack) => self.done(),
            (Sq::BlOffCtrl, PpsFeed::Value(v)) => {
                self.pp = v & !EDP_BLC_ENABLE;
                let plan = ctrl_write_plan(self.pp);
                self.apply(plan, Sq::BlOffWritten)
            }
            (Sq::BlOffWritten, PpsFeed::Ack) => {
                // last_backlight_off = jiffies; edp_wait_backlight_off(): the whole T9 remains (:1125-1126, :712-716)
                let us = st.timing.backlight_off_delay_ms * 1000;
                self.apply(vec![Effect::DelayUs(us)], Sq::BlOffWaited)
            }
            (Sq::BlOffWaited, PpsFeed::Ack) => self.done(),

            _ => self.err(PpsError::OutOfSequence),
        }
    }
}
