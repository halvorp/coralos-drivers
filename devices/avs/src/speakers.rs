// SPDX-License-Identifier: GPL-2.0-only
//! The speaker amplifiers (two MAX98373 on the codec's bus), executed: [`max98373_core`]'s plans performed over I2C, and the amplifiers'
//! hardware reset line. DECISIONS D17 (amending D11 and D14): the speakers are PROTECTED, not refused.
//!
//! WHAT "PROTECTED" MEANS HERE, in the order a speaker session runs it:
//!
//! 1. The amplifiers are in hardware RESET whenever no speaker session is running (the pad is driven low first of all, [`gpio::hold_speakers`],
//!    and again at every way out). Nothing on the bus can reach a part in reset; the I2C fence refuses their addresses unless armed.
//! 2. [`Speakers::begin`]: the fence is armed for exactly the registers the plans write; the reset is released (after at least 50 ms low,
//!    Linux's `max98373_slot_config`); each part is soft-reset, probed and given its DAI format; then ChromeOS's UCM protection set is written,
//!    Left and Right, in the file's order, and READ BACK - DHT, BDE, VI sense and the gains. Any failure ends the session: [`Speakers::safe`].
//!    No audio clock exists yet and neither part is enabled.
//! 3. [`Speakers::power_up`]: only once the DSP pipelines run (the clocks exist) and the readback held: `SPK_EN`, `PCM_RX_EN`, `GLOBAL_EN`.
//! 4. The DSP gain is then raised from silence to the client's level by the peakvol module, capped by
//!    [`max98373_core::gain::CEILING_LINEAR`] twice (the mapping and the module's handle).
//! 5. [`Speakers::safe`], on stop, unplug, fault and the client's death: gain to silence (the session's job), `GLOBAL_EN` off and proven,
//!    then the reset line low and proven by readback, then the fence disarmed. A dying task cannot allocate: [`Speakers::emergency`] writes
//!    `GLOBAL_EN` off and drives the reset low, nothing else.
use crate::codec::Tick;
use crate::gpio;
use crate::log::emit;
use crate::machine::{I2cFault, Machine};
use crate::pump::PumpError;
use crate::rig::Rig;
use alloc::vec::Vec;
use avs_wire::board::Pad;
use avs_wire::SpeakerHold;
use max98373_core::ctl::Side;
use max98373_core::plan::{self, wire, Amp, Op, PlanError, Step};

/// The amplifiers are held in reset at least this long before they are released (`max98373_slot_config`: the reset is asserted for
/// `msleep(50)`, max98373.c:494-496), and given this long after (`msleep(20)`, :497-498).
pub const RESET_LOW_MS: u64 = 50;
pub const RESET_SETTLE_MS: u64 = 20;

/// Why a speaker sequence stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpkFail {
    /// A step of a plan failed on the amplifier at `addr`: its label and why.
    Step { label: &'static str, addr: u16, why: StepWhy },
    /// A plan could not be built (a UCM value that does not fit its control, out of memory).
    Plan(PlanError),
    /// The reset line could not be released and read back high.
    Release,
    /// Enabling was asked for before the protection set was written and read back: refused here, whatever the caller meant.
    NotProtected,
    /// The reset line could not be driven low and read back low.
    Hold,
    /// The stream's interrupt service, run between steps of a long sequence, failed.
    Stream(PumpError),
    OutOfMemory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepWhy {
    /// The bus did not complete the transfer.
    Bus(I2cFault),
    /// A readback did not return what was written: the register's value.
    Mismatch { reg: u16, got: u8 },
}

impl SpkFail {
    /// The step name the status carries.
    pub const fn step(&self) -> &'static str {
        match self {
            SpkFail::Step { label, .. } => label,
            SpkFail::Plan(e) => e.name(),
            SpkFail::Release => "speaker-reset-release",
            SpkFail::NotProtected => "speaker-not-protected",
            SpkFail::Hold => "speaker-hold",
            SpkFail::Stream(e) => e.name(),
            SpkFail::OutOfMemory => "speaker-out-of-memory",
        }
    }
    /// A number that helps: the bus abort source, the register that did not read back.
    pub fn detail(&self) -> u32 {
        match self {
            SpkFail::Step { why: StepWhy::Bus(I2cFault::Abort { source }), .. } => *source,
            SpkFail::Step { why: StepWhy::Mismatch { reg, got }, .. } => (*reg as u32) << 8 | *got as u32,
            _ => 0,
        }
    }
}

/// What a safe-state pass managed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpkSafe {
    /// Both parts answered `GLOBAL_EN` off and `PCM_RX_EN` off (not attempted when they could not be reached).
    pub amps_off: bool,
    /// The reset line was driven low and read back low: the strongest safe state.
    pub held: SpeakerHold,
}

impl SpkSafe {
    /// Safe means the part cannot play: it is in hardware reset, proven by the pad's readback.
    pub fn verified(&self) -> bool {
        self.held == SpeakerHold::Held
    }
}

/// What the bus answered during one sequence on one part (for the ring: silence must be diagnosable from the log alone).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BusTally {
    /// Register writes the part acknowledged (a plain write, or the write half of an update).
    pub writes_acked: u32,
    /// Readbacks that matched.
    pub verified: u32,
    /// The last value read from `REV_ID` (0x21FF), when the sequence read it.
    pub rev_id: Option<u8>,
}

pub struct Speakers {
    amps: [Amp; 2],
    pad: Pad,
    /// The registers the plans write (the fence's list), computed once.
    regs: Vec<u16>,
    /// When the reset line was last proven low (ms), for the 50 ms rule.
    low_since: Option<u64>,
    /// The parts are out of reset and the fence is armed.
    pub active: bool,
    /// The protection set was written and read back.
    pub protected: bool,
    /// `GLOBAL_EN` is on.
    pub enabled: bool,
}

impl Speakers {
    /// The speakers of a board whose amplifier list is the Pixelbook Go's, or `None` (a board this port does not know has no speaker path).
    pub fn for_board(board_amps: &[u16], pad: Pad) -> Result<Option<Self>, SpkFail> {
        let Some(amps) = plan::atlas_amps(board_amps) else { return Ok(None) };
        let regs = plan::written_registers(&amps).map_err(SpkFail::Plan)?;
        Ok(Some(Self { amps, pad, regs, low_since: None, active: false, protected: false, enabled: false }))
    }

    /// The reset pad.
    pub fn pad(&self) -> Pad {
        self.pad
    }

    /// The registers the I2C fence is armed with.
    pub fn written_registers(&self) -> &[u16] {
        &self.regs
    }

    pub fn amps(&self) -> &[Amp; 2] {
        &self.amps
    }

    fn amp(&self, side: Side) -> &Amp {
        self.amps.iter().find(|a| a.side == side).unwrap_or(&self.amps[0])
    }

    /// Perform `steps` on the part at `addr`, counting what the bus acknowledged into `tally`. A failing step is named on the ring with
    /// the part, the register and what the bus said, before the error is returned.
    fn exec<M: Machine>(rig: &mut Rig<M>, addr: u16, steps: &[Step], tally: &mut BusTally, tick: Tick<'_, M>) -> Result<(), SpkFail> {
        for s in steps {
            if let Err(e) = Self::exec_one(rig, addr, s, tally) {
                let (reg, why, got) = match e {
                    SpkFail::Step { why: StepWhy::Mismatch { reg, got }, .. } => (reg, "readback-mismatch", got as u32),
                    SpkFail::Step { why: StepWhy::Bus(f), .. } => (step_reg(s), f.name(), match f { I2cFault::Abort { source } => source, _ => 0 }),
                    _ => (step_reg(s), "other", 0),
                };
                emit(&mut rig.m, format_args!("[AVS-SPK FAIL addr={:#04x} step={} reg={:#06x} why={} detail={:#x} acked-before={}]", addr, s.label, reg, why, got, tally.writes_acked));
                return Err(e);
            }
            tick(rig).map_err(SpkFail::Stream)?;
        }
        Ok(())
    }

    fn exec_one<M: Machine>(rig: &mut Rig<M>, addr: u16, s: &Step, tally: &mut BusTally) -> Result<(), SpkFail> {
        let bus = |e: I2cFault| SpkFail::Step { label: s.label, addr, why: StepWhy::Bus(e) };
        match s.op {
            Op::Write { reg, value } => {
                rig.i2c_write(addr, &wire::write(reg, value)).map_err(bus)?;
                tally.writes_acked += 1;
            }
            Op::Update { reg, mask, value } => {
                let cur = Self::read(rig, addr, reg).map_err(bus)?;
                rig.i2c_write(addr, &wire::write(reg, (cur & !mask) | (value & mask))).map_err(bus)?;
                tally.writes_acked += 1;
            }
            Op::Verify { reg, mask, want } => {
                let got = Self::read(rig, addr, reg).map_err(bus)?;
                if got & mask != want {
                    return Err(SpkFail::Step { label: s.label, addr, why: StepWhy::Mismatch { reg, got } });
                }
                tally.verified += 1;
            }
            Op::Read { reg } => {
                let v = Self::read(rig, addr, reg).map_err(bus)?;
                if reg == max98373_core::regs::R21FF_REV_ID {
                    tally.rev_id = Some(v);
                }
            }
            Op::DelayMs(ms) => rig.sleep_ms(ms as u64),
        }
        Ok(())
    }

    fn read<M: Machine>(rig: &mut Rig<M>, addr: u16, reg: u16) -> Result<u8, I2cFault> {
        let mut b = [0u8; 1];
        rig.i2c_write_read(addr, &wire::read_addr(reg), &mut b)?;
        Ok(b[0])
    }

    /// The reset line low and proven low. Records the moment for the 50 ms rule.
    pub fn hold<M: Machine>(&mut self, rig: &mut Rig<M>) -> SpeakerHold {
        let h = gpio::hold_speakers(rig, self.pad);
        if h == SpeakerHold::Held {
            self.low_since = Some(rig.now());
        }
        h
    }

    /// Step 2 of the module comment: arm, release, configure, protect, read back. Nothing is enabled.
    pub fn begin<M: Machine>(&mut self, rig: &mut Rig<M>, tick: Tick<'_, M>) -> Result<(), SpkFail> {
        // 1. the reset has been low long enough (a session that stopped a moment ago left it low a moment ago)
        match self.low_since {
            Some(t) => {
                // both instants are whole milliseconds, each rounded down: one more than the rule guarantees the rule
                let need = RESET_LOW_MS + 1;
                let waited = rig.now().saturating_sub(t);
                if waited < need {
                    rig.sleep_ms(need - waited);
                }
            }
            None => {
                if self.hold(rig) != SpeakerHold::Held {
                    return Err(SpkFail::Hold);
                }
                rig.sleep_ms(RESET_LOW_MS);
            }
        }
        // 2. the fence, then the part
        gpio::release_speakers(rig, self.pad).map_err(|_| SpkFail::Release)?;
        // the part is out of reset from here: it can be addressed, so the fence opens (for its registers only) and `safe` knows to switch it off
        self.active = true;
        rig.i2c.arm_amps(&self.regs).map_err(|_| SpkFail::OutOfMemory)?;
        rig.sleep_ms(RESET_SETTLE_MS);
        tick(rig).map_err(SpkFail::Stream)?;
        // 3. reset, probe defaults, DAI format: per part
        for amp in self.amps {
            let p = plan::bring_up(&amp).map_err(SpkFail::Plan)?;
            let mut t = BusTally::default();
            Self::exec(rig, amp.addr, p.steps(), &mut t, &mut *tick)?;
            emit(&mut rig.m, format_args!("[AVS-SPK BRINGUP addr={:#04x} side={} rev={} writes-acked={} of {} ok]", amp.addr, amp.side.prefix(), Hex(t.rev_id), t.writes_acked, count_writes(p.steps())));
        }
        // 4. the UCM protection set in the file's order: for each control, Left then Right
        let mut ucm = [BusTally::default(); 2];
        for (side, step) in plan::ucm_plan().map_err(SpkFail::Plan)? {
            let addr = self.amp(side).addr;
            Self::exec(rig, addr, core::slice::from_ref(&step), &mut ucm[side as usize], &mut *tick)?;
        }
        // 5. ... and proven
        for side in [Side::Left, Side::Right] {
            let v = plan::ucm_verify(side).map_err(SpkFail::Plan)?;
            Self::exec(rig, self.amp(side).addr, &v, &mut ucm[side as usize], &mut *tick)?;
        }
        for side in [Side::Left, Side::Right] {
            let t = ucm[side as usize];
            emit(&mut rig.m, format_args!("[AVS-SPK PROTECT addr={:#04x} side={} ucm-writes-acked={} readbacks-ok={} ok]", self.amp(side).addr, side.prefix(), t.writes_acked, t.verified));
        }
        self.protected = true;
        Ok(())
    }

    /// The parts' interrupt/fault banks (`INT_RAW1..3`, `INT_STATE1..3`, `INT_FLAG1..3`) and their enable state (`AMP_EN`, `PCM_RX_EN`,
    /// `GLOBAL_SHDN`), READ ONLY, one ring line per part. `tag` says when (e.g. `t=1000ms`). Only while the parts are out of reset.
    pub fn report_status<M: Machine>(&self, rig: &mut Rig<M>, tag: &str) {
        if !self.active {
            emit(&mut rig.m, format_args!("[AVS-SPK STATUS {} in-reset]", tag));
            return;
        }
        use max98373_core::regs::{R202B_PCM_RX_EN, R2043_AMP_EN, R20FF_GLOBAL_SHDN, STATUS_READBACK};
        for amp in self.amps {
            let mut b = [0u8; 9];
            let mut failed: Option<(u16, I2cFault)> = None;
            for (i, r) in STATUS_READBACK.iter().enumerate() {
                match Self::read(rig, amp.addr, *r) {
                    Ok(v) => b[i] = v,
                    Err(e) => {
                        failed = Some((*r, e));
                        break;
                    }
                }
            }
            let mut en = [0u8; 3];
            if failed.is_none() {
                for (i, r) in [R2043_AMP_EN, R202B_PCM_RX_EN, R20FF_GLOBAL_SHDN].iter().enumerate() {
                    match Self::read(rig, amp.addr, *r) {
                        Ok(v) => en[i] = v,
                        Err(e) => {
                            failed = Some((*r, e));
                            break;
                        }
                    }
                }
            }
            match failed {
                Some((r, e)) => emit(&mut rig.m, format_args!("[AVS-SPK STATUS {} addr={:#04x} read-failed reg={:#06x} why={}]", tag, amp.addr, r, e.name())),
                None => emit(
                    &mut rig.m,
                    format_args!(
                        "[AVS-SPK STATUS {} addr={:#04x} raw={:02x},{:02x},{:02x} state={:02x},{:02x},{:02x} flag={:02x},{:02x},{:02x} amp_en={:02x} rx_en={:02x} shdn={:02x}]",
                        tag, amp.addr, b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], en[0], en[1], en[2]
                    ),
                ),
            }
        }
    }

    /// Both parts answer they are not enabled and not receiving: the question the boot orchestrator asks before it starts any I2S clock.
    pub fn is_idle<M: Machine>(&mut self, rig: &mut Rig<M>) -> bool {
        if !self.active {
            // in reset: the pad is the proof, read without writing
            return gpio::speakers_held(rig, self.pad);
        }
        for amp in self.amps {
            for c in plan::SAFE_CHECKS {
                match Self::read(rig, amp.addr, c.reg) {
                    Ok(v) if v & c.mask == c.want => {}
                    _ => return false,
                }
            }
        }
        true
    }

    /// Step 3: enable the parts. Only after [`Speakers::begin`] succeeded and the clocks run.
    pub fn power_up<M: Machine>(&mut self, rig: &mut Rig<M>, tick: Tick<'_, M>) -> Result<(), SpkFail> {
        if !self.protected {
            // unreachable from the device (it orders the calls); refused here too, so no caller can enable an unprotected part
            return Err(SpkFail::NotProtected);
        }
        for amp in self.amps {
            let p = plan::power_up().map_err(SpkFail::Plan)?;
            let mut t = BusTally::default();
            Self::exec(rig, amp.addr, p.steps(), &mut t, &mut *tick)?;
            emit(&mut rig.m, format_args!("[AVS-SPK ENABLE addr={:#04x} writes-acked={} global-en-readback={} ok]", amp.addr, t.writes_acked, t.verified));
        }
        self.enabled = true;
        Ok(())
    }

    /// Both parts off, proven; continues past a failure on one part so the other is still switched off.
    fn power_down<M: Machine>(&mut self, rig: &mut Rig<M>) -> bool {
        let mut all = true;
        for amp in self.amps {
            match plan::power_down() {
                Ok(p) => {
                    if Self::exec(rig, amp.addr, p.steps(), &mut BusTally::default(), &mut |_| Ok(())).is_err() {
                        all = false;
                    }
                }
                Err(_) => all = false,
            }
        }
        self.enabled = false;
        all
    }

    /// Step 5: every way out. The parts off while the bus can reach them (`graceful`), then the hardware reset, proven; then the fence.
    pub fn safe<M: Machine>(&mut self, rig: &mut Rig<M>, graceful: bool) -> SpkSafe {
        let amps_off = if self.active && graceful && rig.i2c.amps_armed() { self.power_down(rig) } else { false };
        let held = self.hold(rig);
        rig.i2c.disarm_amps();
        self.active = false;
        self.protected = false;
        self.enabled = false;
        SpkSafe { amps_off, held }
    }

    /// What a dying task does: no allocation, no message - `GLOBAL_EN` off on each part if the fence is armed, then the reset low.
    pub fn emergency<M: Machine>(&mut self, rig: &mut Rig<M>) -> SpkSafe {
        let mut off = false;
        if rig.i2c.amps_armed() {
            off = true;
            for amp in self.amps {
                let (reg, value) = plan::EMERGENCY_WRITE;
                if rig.i2c_write(amp.addr, &wire::write(reg, value)).is_err() {
                    off = false;
                }
            }
        }
        let held = self.hold(rig);
        rig.i2c.disarm_amps();
        self.active = false;
        self.protected = false;
        self.enabled = false;
        SpkSafe { amps_off: off, held }
    }
}

/// The register a step touches (for the failure line); 0 for a delay.
fn step_reg(s: &Step) -> u16 {
    match s.op {
        Op::Write { reg, .. } | Op::Update { reg, .. } | Op::Verify { reg, .. } | Op::Read { reg } => reg,
        Op::DelayMs(_) => 0,
    }
}

/// How many register writes a sequence makes (a write, or an update's write half).
fn count_writes(steps: &[Step]) -> u32 {
    steps.iter().filter(|s| matches!(s.op, Op::Write { .. } | Op::Update { .. })).count() as u32
}

/// `0x43`, or `none` when the value was never read.
struct Hex(Option<u8>);

impl core::fmt::Display for Hex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(v) => write!(f, "{:#04x}", v),
            None => f.write_str("none"),
        }
    }
}
