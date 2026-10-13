// SPDX-License-Identifier: GPL-2.0-only
//! The DA7219 executor: da7219-core's I2C transaction PLANS performed over the codec's bus, and the sequences the device runs with them
//! (attach, safe state, power up, volume, mute, power down, one pass of the accessory-detect interrupt).
//!
//! Every message is built through `da7219_core::bus::wire`, which refuses any address but the codec's and names the speaker amplifiers
//! when asked for them; the [`Rig`] gate then checks the address AGAIN against the board's allow-list. A plan is judged step by step by
//! the core's [`Runner`], which counts its own polls: this executor only performs the step and reports what the bus did.
use crate::machine::{I2cFault, Machine};
use crate::pump::PumpError;
use crate::rig::Rig;
use alloc::vec::Vec;
use da7219_core::aad::{self, AadState, Irq, IrqOutcome, IrqOutput, ATLAS_AAD_PDATA, SND_JACK_HEADPHONE, SND_JACK_HEADSET};
use da7219_core::bus::{wire, BusError, Op, Plan, Step, Wiring};
use da7219_core::clock::{self, ClkSrc, SysClk};
use da7219_core::dai::{self, ATLAS_DAI_FMT};
use da7219_core::hp::{self, GainPolicy, PowerUp};
use da7219_core::init::{self, ATLAS_PDATA};
use da7219_core::regs::{DA7219_CHIP_REVISION, DA7219_SYSTEM_ACTIVE};
use da7219_core::runner::{Cause, Input, Output, Runner};
use da7219_core::safe;
use avs_wire::Jack;

/// Why a codec sequence stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecFail {
    /// A step of a plan failed: its label and why.
    Step { label: &'static str, cause: Cause, bus: Option<I2cFault> },
    /// A plan could not be built (a rate, a width, a clock the codec cannot make, a plan past its capacity).
    Build { what: &'static str },
    /// A message addressed something other than the codec (a programming error, refused before the wire).
    Refused(BusError),
    /// The bus was addressed outside the board's allow-list.
    AddressRefused { addr: u16 },
    OutOfMemory,
    /// The stream's interrupt service, run between steps of a long sequence, failed.
    Stream(PumpError),
}

/// Run between the steps of a long sequence so the playing stream keeps being serviced (see [`crate::session::Dsp::tick`]).
pub type Tick<'a, M> = &'a mut dyn FnMut(&mut Rig<M>) -> Result<(), PumpError>;

impl CodecFail {
    /// The step name an init failure reports.
    pub const fn step(&self) -> &'static str {
        match self {
            CodecFail::Step { label, .. } => label,
            CodecFail::Build { what } => what,
            CodecFail::Refused(_) => "codec-refused-address",
            CodecFail::AddressRefused { .. } => "codec-address-not-allowed",
            CodecFail::OutOfMemory => "codec-out-of-memory",
            CodecFail::Stream(e) => e.name(),
        }
    }
    /// The bus fault behind a NAK, as the status detail.
    pub fn detail(&self) -> u32 {
        match self {
            CodecFail::Step { bus: Some(I2cFault::Abort { source }), .. } => *source,
            CodecFail::Step { cause: Cause::VerifyMismatch { got, .. }, .. } => *got as u32,
            _ => 0,
        }
    }
}

fn bld(what: &'static str) -> CodecFail {
    CodecFail::Build { what }
}

/// How the output volume is chosen: Aura's 0..=1000 level, mapped to DAC gain codes under the hardware ceiling.
pub const DAC_RANGE_CODES: u32 = 64; // 64 steps of 0.75 dB = 48 dB below the ceiling

/// The DAC gain code for `level` (1..=1000), or `None` for silence (level 0). Never above `policy.dac_max`; monotonic.
pub fn dac_code_for(level: u16, policy: &GainPolicy) -> Option<u8> {
    if level == 0 {
        return None;
    }
    let scale = avs_wire::VOLUME_MAX as u32;
    let level = (level as u32).min(scale);
    let below = (scale - level) * DAC_RANGE_CODES / scale;
    Some((policy.dac_max as u32).saturating_sub(below).min(policy.dac_max as u32) as u8)
}

/// What the volume currently is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Applied {
    pub dac_code: u8,
    pub hp_code: u8,
    pub muted: bool,
}

pub struct Codec {
    addr: u16,
    amps: Vec<u16>,
    pub aad: AadState,
    pub policy: GainPolicy,
    mclk_hz: u32,
    /// The last bus fault, for the detail of a NAK-caused step failure.
    last_bus: Option<I2cFault>,
    pub powered: bool,
    pub applied: Applied,
}

impl Codec {
    pub fn new(addr: u16, amps: &[u16], mclk_hz: u32) -> Result<Self, CodecFail> {
        let mut a = Vec::new();
        a.try_reserve_exact(amps.len()).map_err(|_| CodecFail::OutOfMemory)?;
        a.extend_from_slice(amps);
        let policy = GainPolicy::ATLAS_HEADPHONES;
        Ok(Self {
            addr,
            amps: a,
            aad: AadState::new(ATLAS_AAD_PDATA.jack_det_rate),
            policy,
            mclk_hz,
            last_bus: None,
            powered: false,
            applied: Applied { dac_code: policy.dac_initial, hp_code: policy.hp_initial, muted: true },
        })
    }

    fn step<M: Machine>(&mut self, rig: &mut Rig<M>, w: &Wiring<'_>, s: &Step) -> Result<Input, CodecFail> {
        let a = self.addr;
        let rd = |rig: &mut Rig<M>, reg: u8, n: usize, out: &mut [u8; 2]| -> Result<(), StepErr> {
            let t = wire::read(w, reg, n).map_err(StepErr::Refused)?;
            rig.i2c_write_read(a, &t.out[..1], &mut out[..n]).map_err(StepErr::Bus)
        };
        let wr = |rig: &mut Rig<M>, reg: u8, first: u8, second: Option<u8>| -> Result<(), StepErr> {
            let t = match second {
                None => wire::write(w, reg, first),
                Some(s2) => wire::write_pair(w, reg, first, s2),
            }
            .map_err(StepErr::Refused)?;
            let n = t.messages()[0].len;
            rig.i2c_write(a, &t.out[..n]).map_err(StepErr::Bus)
        };
        let mut b = [0u8; 2];
        let r: Result<Input, StepErr> = (|| {
            Ok(match s.op {
                Op::Write { reg, value } => {
                    wr(rig, reg, value, None)?;
                    Input::Done
                }
                Op::WritePair { reg, first, second } => {
                    wr(rig, reg, first, Some(second))?;
                    Input::Done
                }
                Op::Update { reg, mask, value } => {
                    rd(rig, reg, 1, &mut b)?;
                    wr(rig, reg, (b[0] & !mask) | (value & mask), None)?;
                    Input::Done
                }
                Op::Read { reg } | Op::Verify { reg, .. } => {
                    rd(rig, reg, 1, &mut b)?;
                    Input::Value(b[0])
                }
                Op::Poll { reg, mask, want, interval_ms, .. } => {
                    rd(rig, reg, 1, &mut b)?;
                    if b[0] & mask != want {
                        rig.sleep_ms(interval_ms as u64);
                    }
                    Input::Value(b[0])
                }
                Op::ReadPair { reg } => {
                    rd(rig, reg, 2, &mut b)?;
                    Input::Pair(b)
                }
                Op::DelayMs(ms) => {
                    rig.sleep_ms(ms as u64);
                    Input::Done
                }
            })
        })();
        match r {
            Ok(i) => Ok(i),
            Err(StepErr::Refused(e)) => Err(CodecFail::Refused(e)),
            Err(StepErr::Bus(I2cFault::AddressRefused { addr })) => Err(CodecFail::AddressRefused { addr }),
            Err(StepErr::Bus(f)) => {
                self.last_bus = Some(f);
                Ok(Input::Nak)
            }
        }
    }

    /// Run a plan to its end.
    pub fn run<M: Machine>(&mut self, rig: &mut Rig<M>, plan: Plan) -> Result<(), CodecFail> {
        self.run_tick(rig, plan, &mut |_| Ok(()))
    }

    /// Build the board's wiring over the amplifier list without allocating (the list is taken out of `self` for the call and put back):
    /// the emergency path of a dying task must not touch the allocator.
    fn wired<R>(&mut self, f: impl FnOnce(&mut Self, &Wiring<'_>) -> R) -> Result<R, CodecFail> {
        let amps = core::mem::take(&mut self.amps);
        let r = match Wiring::new(self.addr, &amps) {
            Ok(w) => Ok(f(self, &w)),
            Err(e) => Err(CodecFail::Refused(e)),
        };
        self.amps = amps;
        r
    }

    /// Run a plan to its end, calling `tick` after every step.
    pub fn run_tick<M: Machine>(&mut self, rig: &mut Rig<M>, plan: Plan, tick: Tick<'_, M>) -> Result<(), CodecFail> {
        self.wired(|c, w| {
            let mut runner = Runner::new(plan).map_err(|_| CodecFail::Build { what: "plan-overflow" })?;
            let mut input = Input::Begin;
            // The runner bounds every poll; this backstop only covers a runner that never ends.
            for _ in 0..4096 {
                match runner.next(input) {
                    Output::Finished => return Ok(()),
                    Output::Failed(f) => return Err(CodecFail::Step { label: f.step, cause: f.cause, bus: c.last_bus.take() }),
                    Output::Do(s) => {
                        input = c.step(rig, w, &s)?;
                        tick(rig).map_err(CodecFail::Stream)?;
                    }
                }
            }
            Err(CodecFail::Build { what: "plan-never-ended" })
        })?
    }

    fn run_one<M: Machine>(&mut self, rig: &mut Rig<M>, plan: Result<Plan, da7219_core::PlanError>) -> Result<(), CodecFail> {
        self.run(rig, plan.map_err(|_| CodecFail::Build { what: "plan-overflow" })?)
    }

    /// One register read.
    fn read_reg<M: Machine>(&mut self, rig: &mut Rig<M>, reg: u8) -> Result<u8, CodecFail> {
        self.wired(|c, w| {
            let s = Step::abort("codec-read", Op::Read { reg });
            match c.step(rig, w, &s)? {
                Input::Value(v) => Ok(v),
                _ => Err(CodecFail::Step { label: "codec-read", cause: Cause::BusNak, bus: c.last_bus.take() }),
            }
        })?
    }

    // ---- the sequences ----

    /// Bring a codec of unknown state to the initialised, SAFE state with the accessory detect armed: stop what a previous start left
    /// running, soft reset, check the identity, configure, and prove every output is muted and powered down by readback.
    pub fn attach<M: Machine>(&mut self, rig: &mut Rig<M>) -> Result<(), CodecFail> {
        let active = self.read_reg(rig, DA7219_SYSTEM_ACTIVE)?;
        if active & da7219_core::fields::DA7219_SYSTEM_ACTIVE_MASK != 0 {
            self.run_one(rig, init::quiesce_plan())?;
        }
        self.run_one(rig, init::init_plan(ATLAS_PDATA))?;
        let rev = self.read_reg(rig, DA7219_CHIP_REVISION)?;
        if init::needs_rev_aa_patch(rev) {
            self.run(rig, init::rev_aa_patch_plan())?;
        }
        self.run_one(rig, aad::init_plan(ATLAS_AAD_PDATA))?;
        self.run(rig, aad::detection_plan(true))?;
        self.safe_state(rig)?;
        self.powered = false;
        self.aad = AadState::new(ATLAS_AAD_PDATA.jack_det_rate);
        Ok(())
    }

    /// The SAFE STATE: outputs driven off, amplifier then DAC muted, stages powered down - proven by reading every control register back.
    pub fn safe_state<M: Machine>(&mut self, rig: &mut Rig<M>) -> Result<(), CodecFail> {
        self.run_one(rig, safe::safe_state_plan())?;
        self.powered = false;
        self.applied.muted = true;
        Ok(())
    }

    /// Is the codec in its safe state right now? Reads every check register and compares; changes nothing.
    pub fn is_safe<M: Machine>(&mut self, rig: &mut Rig<M>) -> bool {
        for c in safe::SAFE_CHECKS {
            match self.read_reg(rig, c.reg) {
                Ok(v) if v & c.mask == c.want => {}
                _ => return false,
            }
        }
        true
    }

    /// Clock, DAI, route and the headphone path, powered with every output still muted. `bits` is the container width.
    pub fn power_up<M: Machine>(&mut self, rig: &mut Rig<M>, rate: u32, bits: u32, channels: u32, tick: Tick<'_, M>) -> Result<(), CodecFail> {
        let plans = [
            clock::sysclk_plan(ClkSrc::Mclk, self.mclk_hz).map_err(|_| bld("sysclk-plan"))?,
            clock::pll_plan(self.mclk_hz, SysClk::PllSrm, clock::ATLAS_PLL_OUT_HZ).map_err(|_| bld("pll-plan"))?,
            dai::fmt_plan(ATLAS_DAI_FMT).map_err(|_| bld("dai-fmt-plan"))?,
            dai::hw_params_plan(rate, bits, channels).map_err(|_| bld("dai-hw-params-plan"))?,
            hp::route_plan().map_err(|_| bld("route-plan"))?,
            hp::power_up_plan(PowerUp::default()).map_err(|_| bld("power-up-plan"))?,
            // the accessory detect's own step, repeated here because the safe state un-drove it: the jack's output stage on, still muted
            aad::output_drive_plan(true),
        ];
        for p in plans {
            self.run_tick(rig, p, &mut *tick)?;
        }
        self.powered = true;
        Ok(())
    }

    /// Unmute at the volume the client chose (or the conservative initial gain when it has not chosen one).
    pub fn unmute<M: Machine>(&mut self, rig: &mut Rig<M>, level: Option<u16>, tick: Tick<'_, M>) -> Result<Applied, CodecFail> {
        let (dac, muted) = match level {
            None => (self.policy.dac_initial, false),
            Some(l) => match dac_code_for(l, &self.policy) {
                Some(c) => (c, false),
                None => (self.policy.dac_initial, true),
            },
        };
        let hp_code = self.policy.hp_initial;
        let p = hp::gain_plan(dac, hp_code, self.policy).map_err(|_| CodecFail::Build { what: "gain-above-ceiling" })?;
        self.run_tick(rig, p, &mut *tick)?;
        if !muted {
            let u = hp::unmute_plan().map_err(|_| CodecFail::Build { what: "unmute-plan" })?;
            self.run_tick(rig, u, &mut *tick)?;
        }
        self.applied = Applied { dac_code: dac, hp_code, muted };
        Ok(self.applied)
    }

    /// Change the volume while the path is up (and powered). Level 0 or `mute` mutes; any other level sets the gain under the ceiling and,
    /// if the path was muted, unmutes it.
    pub fn set_level<M: Machine>(&mut self, rig: &mut Rig<M>, level: u16, mute: bool) -> Result<Applied, CodecFail> {
        if mute || level == 0 {
            self.run_one(rig, hp::mute_plan())?;
            self.applied.muted = true;
            return Ok(self.applied);
        }
        let was_muted = self.applied.muted;
        let dac = dac_code_for(level, &self.policy).unwrap_or(self.policy.dac_initial);
        let p = hp::gain_plan(dac, self.policy.hp_initial, self.policy).map_err(|_| CodecFail::Build { what: "gain-above-ceiling" })?;
        self.run(rig, p)?;
        if was_muted {
            self.run_one(rig, hp::unmute_plan())?;
        }
        self.applied = Applied { dac_code: dac, hp_code: self.policy.hp_initial, muted: false };
        Ok(self.applied)
    }

    /// Mute, power the path down in DAPM order, stop the PLL, and prove the safe state.
    pub fn power_down<M: Machine>(&mut self, rig: &mut Rig<M>) -> Result<(), CodecFail> {
        let r = (|| {
            self.run_one(rig, hp::power_down_plan(PowerUp::default()))?;
            let down = clock::pll_plan(self.mclk_hz, SysClk::Mclk, 0).map_err(|_| CodecFail::Build { what: "pll-plan" })?;
            self.run(rig, down)
        })();
        // whatever happened, the safe state is attempted and is what the caller is told about
        let safe = self.safe_state(rig);
        r.and(safe)
    }

    /// One pass of the accessory-detect interrupt: read and clear the events, decode, run the follow-up. The caller delivers the outcome.
    pub fn jack_irq<M: Machine>(&mut self, rig: &mut Rig<M>) -> Result<IrqOutcome, CodecFail> {
        self.wired(|c, w| {
            let mut irq = Irq::new(ATLAS_AAD_PDATA, c.aad);
            let mut input = Input::Begin;
            for _ in 0..4096 {
                match irq.next(input) {
                    IrqOutput::Done(o) => {
                        c.aad = irq.state();
                        return Ok(o);
                    }
                    IrqOutput::Failed(f) => return Err(CodecFail::Step { label: f.step, cause: f.cause, bus: c.last_bus.take() }),
                    IrqOutput::Do(s) => input = c.step(rig, w, &s)?,
                }
            }
            Err(CodecFail::Build { what: "aad-irq-never-ended" })
        })?
    }

    /// The delayed ground-switch work (`da7219_aad_jack_det_work`).
    pub fn ground_switch<M: Machine>(&mut self, rig: &mut Rig<M>, on: bool) -> Result<(), CodecFail> {
        self.run(rig, aad::ground_switch_plan(on))
    }
}

enum StepErr {
    Refused(BusError),
    Bus(I2cFault),
}

/// What a jack report says the jack IS now. A report whose mask covers the headset bits restates the jack (da7219-aad.c:423-427 for a
/// four-pole headset, :309-312 for headphones, :454-483 for removal: report 0 under the full mask); a report about the contact alone
/// (detection still running) or about buttons leaves the classification as it was - not yet a playable output.
pub fn jack_after(report: &da7219_core::aad::JackReport, was: Jack) -> Jack {
    if report.mask & SND_JACK_HEADSET != SND_JACK_HEADSET {
        return was;
    }
    if report.report & SND_JACK_HEADSET == SND_JACK_HEADSET {
        Jack::Headset
    } else if report.report & SND_JACK_HEADPHONE != 0 {
        Jack::Headphones
    } else {
        Jack::Absent
    }
}
