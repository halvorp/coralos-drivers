// SPDX-License-Identifier: GPL-2.0-only
//! Accessory detect: jack insertion, 3-pole/4-pole classification, headset buttons. Ported from `da7219-aad.c` / `da7219-aad.h` /
//! `include/sound/da7219-aad.h` (Linux v7.0-rc6): the pdata conversion (`da7219_aad_handle_pdata`, aad.c:763-890), `da7219_aad_init` /
//! `_exit` (aad.c:990-1050), the button-detection work (`da7219_aad_btn_det_work`, aad.c:51-105) and the interrupt thread
//! (`da7219_aad_irq_thread`, aad.c:350-487) as a coroutine whose every register access is a bounded [`Step`].
//!
//! THE INTERRUPT LINE. Atlas wires the codec's IRQ to `GPP_F10` (`HP_IRQ_GPIO`), LEVEL-LOW (ATLAS-PERIPHERALS-FROM-RECOVERY.md section
//! 3.2; aad.c:1019-1021 requests it `IRQF_TRIGGER_LOW | IRQF_ONESHOT`). A level interrupt stays asserted until the event registers are
//! CLEARED, so [`Irq`] always clears what it read before it finishes - and the `.device` must not re-arm the pad until it has.
//!
//! THE HEADPHONE-IMPEDANCE TEST IS NOT PORTED. For a 3-pole jack Linux runs `da7219_aad_hptest_work` (aad.c:107-335) to tell headphones
//! from a line-out: it drives a test tone through the jack with the DAC and headphone gains forced to 0 dB (aad.c:171-175). That is a
//! deliberate burst into whatever is plugged in, which this port will not play before the owner decides. A 3-pole jack is therefore
//! reported as headphones and the outcome says `hp_test_skipped`; the output enable is set exactly as Linux does at the test's end.
use crate::bus::*;
use crate::aad_fields::*;
use crate::fields::*;
use crate::regs::*;
use crate::runner::{Failure, Input, Output, Runner};

// ---- the pdata (include/sound/da7219-aad.h:11-95) -----------------------------------------------------------------------------
macro_rules! code_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident = $val:expr),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($var = $val),+ }
    };
}

code_enum!(/** `enum da7219_aad_micbias_pulse_lvl` (da7219-aad.h:11-15). */ MicbiasPulse { Off = 0, V2_8 = 6, V2_9 = 7 });
code_enum!(/** `enum da7219_aad_btn_cfg` (da7219-aad.h:17-25). */ BtnCfg { Ms2 = 1, Ms5 = 2, Ms10 = 3, Ms50 = 4, Ms100 = 5, Ms200 = 6, Ms500 = 7 });
code_enum!(/** `enum da7219_aad_mic_det_thr` (da7219-aad.h:27-32). */ MicDetThr { Ohm200 = 0, Ohm500 = 1, Ohm750 = 2, Ohm1000 = 3 });
code_enum!(/** `enum da7219_aad_jack_ins_deb` (da7219-aad.h:34-43). */ JackInsDeb { Ms5 = 0, Ms10 = 1, Ms20 = 2, Ms50 = 3, Ms100 = 4, Ms200 = 5, Ms500 = 6, S1 = 7 });
code_enum!(/** `enum da7219_aad_jack_ins_det_pty` (da7219-aad.h:45-48). */ JackInsDetPty { Low = 0, High = 1 });
code_enum!(/** `enum da7219_aad_jack_det_rate` (da7219-aad.h:50-55). */ JackDetRate { Ms32_64 = 0, Ms64_128 = 1, Ms128_256 = 2, Ms256_512 = 3 });
code_enum!(/** `enum da7219_aad_jack_rem_deb` (da7219-aad.h:57-62). */ JackRemDeb { Ms1 = 0, Ms5 = 1, Ms10 = 2, Ms20 = 3 });
code_enum!(/** `enum da7219_aad_btn_avg` (da7219-aad.h:64-69). */ BtnAvg { N1 = 0, N2 = 1, N4 = 2, N8 = 3 });
code_enum!(/** `enum da7219_aad_adc_1bit_rpt` (da7219-aad.h:71-76). */ Adc1BitRpt { N1 = 0, N2 = 1, N4 = 2, N8 = 3 });

/// `struct da7219_aad_pdata` (da7219-aad.h:78-95).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AadPdata {
    pub micbias_pulse_lvl: MicbiasPulse,
    pub micbias_pulse_time_ms: u32,
    pub btn_cfg: BtnCfg,
    pub mic_det_thr: MicDetThr,
    pub jack_ins_deb: JackInsDeb,
    pub jack_ins_det_pty: JackInsDetPty,
    pub jack_det_rate: JackDetRate,
    pub jack_rem_deb: JackRemDeb,
    pub a_d_btn_thr: u8,
    pub d_b_btn_thr: u8,
    pub b_c_btn_thr: u8,
    pub c_mic_btn_thr: u8,
    pub btn_avg: BtnAvg,
    pub adc_1bit_rpt: Adc1BitRpt,
}

/// Atlas' ACPI `_DSD` values (ATLAS-PERIPHERALS-FROM-RECOVERY.md section 3.2): `btn_cfg = 50`, `mic_det_thr = 200`, `jack_ins_deb = 20`,
/// `jack_det_rate = 32_64`, `jack_rem_deb = 1`, thresholds a_d 0xa / d_b 0x16 / b_c 0x21 / c_mic 0x3e, `btn_avg = 4`, `adc_1bit_rpt = 1`.
/// The three properties the notes do NOT list take `da7219_aad_fw_to_pdata` defaults (aad.c:661-756; pulse level :683-686, time :690, polarity :710-713): micbias pulse off,
/// pulse time 0 (unset stays 0), insertion polarity low. Those three are recon facts.
pub const ATLAS_AAD_PDATA: AadPdata = AadPdata {
    micbias_pulse_lvl: MicbiasPulse::Off,
    micbias_pulse_time_ms: 0,
    btn_cfg: BtnCfg::Ms50,
    mic_det_thr: MicDetThr::Ohm200,
    jack_ins_deb: JackInsDeb::Ms20,
    jack_ins_det_pty: JackInsDetPty::Low,
    jack_det_rate: JackDetRate::Ms32_64,
    jack_rem_deb: JackRemDeb::Ms1,
    a_d_btn_thr: 0x0a,
    d_b_btn_thr: 0x16,
    b_c_btn_thr: 0x21,
    c_mic_btn_thr: 0x3e,
    btn_avg: BtnAvg::N4,
    adc_1bit_rpt: Adc1BitRpt::N1,
};

/// The hidden registers the insertion-polarity and ground-switch writes use (aad.c:343, :421, :482, :879-889, :942 - not in any header).
pub const REG_PAGE_SELECT: u8 = 0xF0; // da7219-aad.c:881
pub const REG_INSERTION_POLARITY: u8 = 0x75; // da7219-aad.c:882
pub const PAGE_SELECT_UNLOCK: u8 = 0x8B; // da7219-aad.c:881
pub const REG_GROUND_SWITCH: u8 = 0xFB; // da7219-aad.c:343
pub const GROUND_SWITCH_EN: u8 = 0x01; // da7219-aad.c:343

/// `da7219_aad_handle_gnd_switch_time` (aad.c:896-925): the ground-switch delay of a detect rate, in milliseconds.
pub const fn gnd_switch_delay_ms(rate: JackDetRate) -> u16 {
    match rate {
        JackDetRate::Ms32_64 => 32,
        JackDetRate::Ms64_128 => 64,
        JackDetRate::Ms128_256 => 128,
        JackDetRate::Ms256_512 => 256,
    }
}

/// `da7219_aad_handle_pdata` + `da7219_aad_init`'s register writes (aad.c:763-890, :990-1033): configure detection, leave button
/// detection OFF, and unmask the interrupts last. Detection itself stays disabled until [`detection_plan`] turns it on.
pub fn init_plan(p: AadPdata) -> Result<Plan, crate::PlanError> {
    let mut plan = Plan::new();
    plan.push(Step::abort("aad-mic-det-thresh", Op::Update { reg: DA7219_ACCDET_CONFIG_1, mask: DA7219_MIC_DET_THRESH_MASK, value: (p.mic_det_thr as u8) << DA7219_MIC_DET_THRESH_SHIFT }));
    let cfg2 = ((p.jack_ins_deb as u8) << DA7219_JACKDET_DEBOUNCE_SHIFT) | ((p.jack_det_rate as u8) << DA7219_JACK_DETECT_RATE_SHIFT) | ((p.jack_rem_deb as u8) << DA7219_JACKDET_REM_DEB_SHIFT);
    plan.push(Step::abort("aad-debounce-rates", Op::Update { reg: DA7219_ACCDET_CONFIG_2, mask: DA7219_JACKDET_DEBOUNCE_MASK | DA7219_JACK_DETECT_RATE_MASK | DA7219_JACKDET_REM_DEB_MASK, value: cfg2 }));
    plan.push(Step::abort("aad-btn-thr-a-d", Op::Write { reg: DA7219_ACCDET_CONFIG_3, value: p.a_d_btn_thr }));
    plan.push(Step::abort("aad-btn-thr-d-b", Op::Write { reg: DA7219_ACCDET_CONFIG_4, value: p.d_b_btn_thr }));
    plan.push(Step::abort("aad-btn-thr-b-c", Op::Write { reg: DA7219_ACCDET_CONFIG_5, value: p.b_c_btn_thr }));
    plan.push(Step::abort("aad-btn-thr-c-mic", Op::Write { reg: DA7219_ACCDET_CONFIG_6, value: p.c_mic_btn_thr }));
    let cfg7 = ((p.btn_avg as u8) << DA7219_BUTTON_AVERAGE_SHIFT) | ((p.adc_1bit_rpt as u8) << DA7219_ADC_1_BIT_REPEAT_SHIFT);
    plan.push(Step::abort("aad-btn-avg-rpt", Op::Update { reg: DA7219_ACCDET_CONFIG_7, mask: DA7219_BUTTON_AVERAGE_MASK | DA7219_ADC_1_BIT_REPEAT_MASK, value: cfg7 }));
    // the insertion polarity lives behind a page switch (aad.c:879-889)
    let polarity = match p.jack_ins_det_pty {
        JackInsDetPty::Low => 0x80,
        JackInsDetPty::High => 0x00,
    };
    plan.push(Step::abort("aad-page-unlock", Op::Write { reg: REG_PAGE_SELECT, value: PAGE_SELECT_UNLOCK }));
    plan.push(Step::abort("aad-insertion-polarity", Op::Write { reg: REG_INSERTION_POLARITY, value: polarity }));
    plan.push(Step::abort("aad-page-lock", Op::Write { reg: REG_PAGE_SELECT, value: 0x00 }));
    // da7219_aad_init: "Disable button detection" (aad.c:1002-1004)
    plan.push(Step::abort("aad-button-detect-off", Op::Update { reg: DA7219_ACCDET_CONFIG_1, mask: DA7219_BUTTON_CONFIG_MASK, value: 0 }));
    // "Unmask AAD IRQs" (aad.c:1027-1031), last
    plan.push(Step::abort("aad-irq-unmask", Op::WritePair { reg: DA7219_ACCDET_IRQ_MASK_A, first: 0, second: 0 }));
    plan.checked()
}

/// `da7219_aad_exit`: mask every AAD interrupt (aad.c:1041-1046).
pub fn exit_plan() -> Plan {
    let mut plan = Plan::new();
    plan.push(Step::abort("aad-irq-mask", Op::WritePair { reg: DA7219_ACCDET_IRQ_MASK_A, first: 0xFF, second: 0xFF }));
    plan
}

/// `da7219_aad_jack_det` (aad.c:31-41): enable or disable jack detection.
pub fn detection_plan(enable: bool) -> Plan {
    let mut plan = Plan::new();
    let label = if enable { "aad-detection-on" } else { "aad-detection-off" };
    plan.push(Step::abort(label, Op::Update { reg: DA7219_ACCDET_CONFIG_1, mask: DA7219_ACCDET_EN_MASK, value: if enable { DA7219_ACCDET_EN_MASK } else { 0 } }));
    plan
}

/// The delayed `da7219_aad_jack_det_work` (aad.c:336-344): "Enable ground switch". And the disable that detect-complete and removal run.
pub fn ground_switch_plan(enable: bool) -> Plan {
    let mut plan = Plan::new();
    let label = if enable { "ground-switch-on" } else { "ground-switch-off" };
    plan.push(Step::abort(label, Op::Update { reg: REG_GROUND_SWITCH, mask: GROUND_SWITCH_EN, value: if enable { GROUND_SWITCH_EN } else { 0 } }));
    plan
}

/// "Drive headphones/lineout" and "Un-drive": the output-enable bit of both amplifiers (aad.c:62-68, :465-469). Right first on removal,
/// as Linux does; left first on insertion.
pub fn output_drive_plan(drive: bool) -> Plan {
    let mut plan = Plan::new();
    let on = |bit: u8| if drive { bit } else { 0 };
    if drive {
        plan.push(Step::abort("hp-l-oe-on", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_OE_MASK, value: on(DA7219_HP_L_AMP_OE_MASK) }));
        plan.push(Step::abort("hp-r-oe-on", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_OE_MASK, value: on(DA7219_HP_R_AMP_OE_MASK) }));
    } else {
        plan.push(Step::abort("hp-r-oe-off", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_OE_MASK, value: 0 }));
        plan.push(Step::abort("hp-l-oe-off", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_OE_MASK, value: 0 }));
    }
    plan
}

/// `da7219_aad_btn_det_work` (aad.c:51-105): drive the output, make sure mic bias is up, optionally pulse it, then enable button detection.
/// The mic-bias DAPM pin is `MICBIAS_CTRL` bit 3 (da7219.c:957-960).
pub fn button_detect_plan(p: AadPdata) -> Result<Plan, crate::PlanError> {
    let mut plan = output_drive_plan(true);
    plan.push(Step::abort("mic-bias-enable", Op::Update { reg: DA7219_MICBIAS_CTRL, mask: DA7219_MICBIAS1_EN_MASK, value: DA7219_MICBIAS1_EN_MASK }));
    // aad.c:70-83: five looks, 10 ms apart; Linux only warns ("Mic bias status check timed out")
    plan.push(Step::new(
        "mic-bias-up",
        Op::Poll { reg: DA7219_ACCDET_STATUS_A, mask: DA7219_MICBIAS_UP_STS_MASK, want: DA7219_MICBIAS_UP_STS_MASK, interval_ms: DA7219_AAD_MICBIAS_CHK_DELAY as u16, max_polls: DA7219_AAD_MICBIAS_CHK_RETRIES },
        OnFail::Tolerate,
    ));
    if p.micbias_pulse_lvl != MicbiasPulse::Off && p.micbias_pulse_time_ms != 0 {
        // aad.c:91-100. The pulse restores the previous level; the previous level is the pdata's own (it was written at init).
        plan.push(Step::abort("mic-bias-pulse-up", Op::Update { reg: DA7219_MICBIAS_CTRL, mask: DA7219_MICBIAS1_LEVEL_MASK, value: (p.micbias_pulse_lvl as u8) << DA7219_MICBIAS1_LEVEL_SHIFT }));
        plan.push(Step::abort("mic-bias-pulse-hold", Op::DelayMs(p.micbias_pulse_time_ms.min(u16::MAX as u32) as u16)));
    }
    plan.push(Step::abort("aad-button-detect-on", Op::Update { reg: DA7219_ACCDET_CONFIG_1, mask: DA7219_BUTTON_CONFIG_MASK, value: (p.btn_cfg as u8) << DA7219_BUTTON_CONFIG_SHIFT }));
    plan.checked()
}

/// The jack-removal clean-up (aad.c:454-483): un-drive the outputs, disable button detection, disable mic bias, ground switch off.
pub fn removal_plan() -> Result<Plan, crate::PlanError> {
    let mut plan = output_drive_plan(false);
    plan.push(Step::abort("aad-button-detect-off", Op::Update { reg: DA7219_ACCDET_CONFIG_1, mask: DA7219_BUTTON_CONFIG_MASK, value: 0 }));
    plan.push(Step::abort("mic-bias-disable", Op::Update { reg: DA7219_MICBIAS_CTRL, mask: DA7219_MICBIAS1_EN_MASK, value: 0 }));
    plan.extend(&ground_switch_plan(false));
    plan.checked()
}

// ---- jack reports (include/sound/jack.h:39-53) ---------------------------------------------------------------------------------
pub const SND_JACK_HEADPHONE: u16 = 0x0001; // jack.h:39
pub const SND_JACK_MICROPHONE: u16 = 0x0002; // jack.h:40
pub const SND_JACK_HEADSET: u16 = SND_JACK_HEADPHONE | SND_JACK_MICROPHONE; // jack.h:41
pub const SND_JACK_LINEOUT: u16 = 0x0004; // jack.h:42
pub const SND_JACK_MECHANICAL: u16 = 0x0008; // jack.h:43
pub const SND_JACK_BTN_0: u16 = 0x4000; // jack.h:50
pub const SND_JACK_BTN_1: u16 = 0x2000; // jack.h:51
pub const SND_JACK_BTN_2: u16 = 0x1000; // jack.h:52
pub const SND_JACK_BTN_3: u16 = 0x0800; // jack.h:53
/// `DA7219_AAD_REPORT_ALL_MASK` (da7219-aad.h:168-171).
pub const REPORT_ALL_MASK: u16 = SND_JACK_MECHANICAL | SND_JACK_HEADSET | SND_JACK_LINEOUT | SND_JACK_BTN_0 | SND_JACK_BTN_1 | SND_JACK_BTN_2 | SND_JACK_BTN_3; // da7219-aad.h:168-171

/// `snd_soc_jack_report(jack, report, mask)`: within `mask`, the jack status becomes `report`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JackReport {
    pub report: u16,
    pub mask: u16,
}

impl JackReport {
    pub const fn mechanical_inserted(&self) -> bool {
        self.mask & SND_JACK_MECHANICAL != 0 && self.report & SND_JACK_MECHANICAL != 0
    }
    /// Buttons currently down within the report, as indices 0..=3 (button A = 0).
    pub const fn buttons_down(&self) -> [bool; 4] {
        [self.report & SND_JACK_BTN_0 != 0, self.report & SND_JACK_BTN_1 != 0, self.report & SND_JACK_BTN_2 != 0, self.report & SND_JACK_BTN_3 != 0]
    }
}

/// What persists between interrupts (`da7219_aad_priv`: `gnd_switch_delay`, `jack_inserted`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AadState {
    pub jack_inserted: bool,
    pub gnd_switch_delay_ms: u16,
}

impl AadState {
    pub const fn new(rate: JackDetRate) -> Self {
        Self { jack_inserted: false, gnd_switch_delay_ms: gnd_switch_delay_ms(rate) }
    }
}

/// How the jack was classified when detection completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pole {
    /// `JACK_TYPE_STS` set: a headset with a microphone.
    Four,
    /// Clear: headphones or line-out; the test that would tell them apart is not ported.
    Three,
}

/// The result of one interrupt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrqOutcome {
    /// Neither event register had a bit set: `IRQ_NONE` (aad.c:365-369). Nothing was cleared.
    pub spurious: bool,
    /// The report after the button PRESSES were applied, sent before the releases are (aad.c:442, a 4-pole jack only).
    pub press_report: Option<JackReport>,
    /// The final report (`snd_soc_jack_report` at aad.c:486).
    pub report: Option<JackReport>,
    pub pole: Option<Pole>,
    /// The insertion queued the delayed ground-switch work: run [`ground_switch_plan`]`(true)` after this many milliseconds (aad.c:374-384).
    pub ground_switch_after_ms: Option<u16>,
    /// Detection completed on a 3-pole jack: Linux's impedance test would run here and was skipped.
    pub hp_test_skipped: bool,
    pub state_jack_inserted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqOutput {
    Do(Step),
    Done(IrqOutcome),
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Events,
    Status,
    Srm,
    Clear,
    Follow,
    Finished,
}

/// One pass of `da7219_aad_irq_thread`.
#[derive(Clone, Copy, Debug)]
pub struct Irq {
    pdata: AadPdata,
    state: AadState,
    phase: Phase,
    run: Runner,
    events: [u8; 2],
    status_a: u8,
    srm: u8,
    outcome: IrqOutcome,
}

fn one(label: &'static str, op: Op) -> Runner {
    Runner::single(Step::abort(label, op))
}

impl Irq {
    pub fn new(pdata: AadPdata, state: AadState) -> Self {
        Self {
            pdata,
            state,
            phase: Phase::Events,
            run: one("aad-irq-events", Op::ReadPair { reg: DA7219_ACCDET_IRQ_EVENT_A }),
            events: [0; 2],
            status_a: 0,
            srm: 0,
            outcome: IrqOutcome { state_jack_inserted: state.jack_inserted, ..IrqOutcome::default() },
        }
    }

    /// The state to carry into the next interrupt.
    pub const fn state(&self) -> AadState {
        AadState { jack_inserted: self.outcome.state_jack_inserted, gnd_switch_delay_ms: self.state.gnd_switch_delay_ms }
    }

    fn finish(&mut self) -> IrqOutput {
        self.phase = Phase::Finished;
        IrqOutput::Done(self.outcome)
    }

    fn lift(o: Output) -> Option<IrqOutput> {
        match o {
            Output::Do(s) => Some(IrqOutput::Do(s)),
            Output::Failed(f) => Some(IrqOutput::Failed(f)),
            Output::Finished => None,
        }
    }

    pub fn next(&mut self, input: Input) -> IrqOutput {
        if self.phase == Phase::Finished {
            return IrqOutput::Done(self.outcome);
        }
        let out = self.run.next(input);
        if let Some(o) = Self::lift(out) {
            return o;
        }
        // the current step finished
        match self.phase {
            Phase::Events => {
                self.events = self.run.last_pair().unwrap_or([0; 2]);
                if self.events == [0, 0] {
                    self.outcome.spurious = true; // aad.c:365-369: IRQ_NONE
                    return self.finish();
                }
                self.phase = Phase::Status;
                self.run = one("aad-irq-status", Op::Read { reg: DA7219_ACCDET_STATUS_A });
                self.next(Input::Begin)
            }
            Phase::Status => {
                self.status_a = self.run.last_read().unwrap_or(0);
                if self.events[0] & DA7219_E_JACK_INSERTED_MASK != 0 {
                    self.phase = Phase::Srm;
                    self.run = one("aad-irq-srm", Op::Read { reg: DA7219_PLL_SRM_STS });
                } else {
                    self.phase = Phase::Clear;
                    self.run = one("aad-irq-clear", Op::WritePair { reg: DA7219_ACCDET_IRQ_EVENT_A, first: self.events[0], second: self.events[1] });
                }
                self.next(Input::Begin)
            }
            Phase::Srm => {
                self.srm = self.run.last_read().unwrap_or(0) & DA7219_PLL_SRM_STS_MCLK;
                // aad.c:380: gnd_switch_delay * (srm_st == 0 ? 2 : 1) - 2
                let mul = if self.srm == 0 { 2 } else { 1 };
                self.outcome.ground_switch_after_ms = Some((self.state.gnd_switch_delay_ms * mul).saturating_sub(2));
                self.phase = Phase::Clear;
                self.run = one("aad-irq-clear", Op::WritePair { reg: DA7219_ACCDET_IRQ_EVENT_A, first: self.events[0], second: self.events[1] });
                self.next(Input::Begin)
            }
            Phase::Clear => {
                let (plan, outcome) = self.decode();
                self.outcome = outcome;
                self.phase = Phase::Follow;
                match Runner::new(plan) {
                    Ok(r) => {
                        self.run = r;
                        self.next(Input::Begin)
                    }
                    Err(_) => IrqOutput::Failed(Failure { step: "aad-irq-plan", cause: crate::runner::Cause::Protocol }),
                }
            }
            Phase::Follow => self.finish(),
            Phase::Finished => IrqOutput::Done(self.outcome),
        }
    }

    /// The decode of `da7219_aad_irq_thread` (aad.c:386-486) after the events were read and cleared: the report and the follow-up plan.
    fn decode(&self) -> (Plan, IrqOutcome) {
        let mut o = self.outcome;
        let mut plan = Plan::new();
        let (ea, eb) = (self.events[0], self.events[1]);
        let mut report = 0u16;
        let mut mask = 0u16;
        if self.status_a & DA7219_JACK_INSERTION_STS_MASK != 0 {
            if ea & DA7219_E_JACK_INSERTED_MASK != 0 {
                report |= SND_JACK_MECHANICAL; // aad.c:396-401
                mask |= SND_JACK_MECHANICAL;
                o.state_jack_inserted = true;
            }
            let four_pole = self.status_a & DA7219_JACK_TYPE_STS_MASK != 0;
            if ea & DA7219_E_JACK_DETECT_COMPLETE_MASK != 0 {
                plan.extend(&ground_switch_plan(false)); // aad.c:419-421 (the delayed enable is cancelled by the device)
                if four_pole {
                    report |= SND_JACK_HEADSET; // aad.c:423-427
                    mask |= SND_JACK_HEADSET | SND_JACK_LINEOUT;
                    o.pole = Some(Pole::Four);
                    if let Ok(b) = button_detect_plan(self.pdata) {
                        plan.extend(&b);
                    }
                } else {
                    // HP test NOT PORTED: report headphones, drive the output as the test's tail does (aad.c:309-312)
                    report |= SND_JACK_HEADPHONE;
                    mask |= SND_JACK_HEADSET | SND_JACK_LINEOUT;
                    o.pole = Some(Pole::Three);
                    o.hp_test_skipped = true;
                    plan.extend(&output_drive_plan(true));
                }
            }
            if four_pole {
                // aad.c:435-443: presses (A is BTN_0, shifting right)
                for i in 0..DA7219_AAD_MAX_BUTTONS as u16 {
                    if eb & (DA7219_E_BUTTON_A_PRESSED_MASK << i) != 0 {
                        report |= SND_JACK_BTN_0 >> i;
                        mask |= SND_JACK_BTN_0 >> i;
                    }
                }
                o.press_report = Some(JackReport { report, mask });
                // aad.c:445-451: releases (A_RELEASED is bit 7, shifting right)
                for i in 0..DA7219_AAD_MAX_BUTTONS as u16 {
                    if eb & (DA7219_E_BUTTON_A_RELEASED_MASK >> i) != 0 {
                        report &= !(SND_JACK_BTN_0 >> i);
                        mask |= SND_JACK_BTN_0 >> i;
                    }
                }
            }
        } else if ea & DA7219_E_JACK_REMOVED_MASK != 0 {
            report = 0; // aad.c:454-483
            mask = REPORT_ALL_MASK;
            o.state_jack_inserted = false;
            o.pole = None;
            o.ground_switch_after_ms = None;
            if let Ok(r) = removal_plan() {
                plan.extend(&r);
            }
        }
        // aad.c:486: the report is sent even when nothing changed
        o.report = Some(JackReport { report, mask });
        (plan, o)
    }
}
