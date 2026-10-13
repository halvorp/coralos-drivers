// SPDX-License-Identifier: GPL-2.0-only
//! The HD-audio controller half of the bring-up: what to read to know the hardware (`GCAP`, the capability chain, the multi-link blocks),
//! and the chip start and stop around a DSP session. All plans are hda-ext-core's; this module only chooses which to run and in what order.
use crate::allow::{Allow, AllowError, HdaLayout};
use crate::fwexec::{perform, run_program, FwCtx, RunFail};
use crate::machine::{Machine, RegFault};
use crate::pump::Pump;
use crate::rig::Rig;
use alloc::vec::Vec;
use avs_fw_core::effect::{Reg, Space, Width};
use avs_fw_core::programs;
use hda_ext_core::caps::{CapWalk, Caps, Link, MlOut, MlProbe, WalkIn, WalkOut};
use hda_ext_core::controller::{init_chip, ppcap_enable, ppcap_int_enable, stop_chip};
use hda_ext_core::error::HdaError;
use hda_ext_core::regs::AZX_REG_GCAP;
use hda_ext_core::stream::{HostStream, Streams};

#[derive(Clone, Debug)]
pub struct Controller {
    pub streams: Streams,
    pub caps: Caps,
    pub links: Vec<Link>,
    pub playback: HostStream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoverFail {
    Reg(RegFault),
    /// `GCAP` reads as nothing (all-ones or zero): the window is not a live controller.
    Dead,
    Hda(HdaError),
    /// The controller has no processing-pipe capability, which every DSP path needs.
    NoProcessingPipe,
    /// The board names a playback stream the controller does not have.
    NoSuchStream { wanted: u32, have: u32 },
    OutOfMemory,
}

impl DiscoverFail {
    pub const fn name(&self) -> &'static str {
        match self {
            DiscoverFail::Reg(e) => e.name(),
            DiscoverFail::Dead => "controller-dead",
            DiscoverFail::Hda(e) => e.name(),
            DiscoverFail::NoProcessingPipe => "no-processing-pipe",
            DiscoverFail::NoSuchStream { .. } => "no-such-stream",
            DiscoverFail::OutOfMemory => "out-of-memory",
        }
    }
}

impl From<RegFault> for DiscoverFail {
    fn from(e: RegFault) -> Self {
        DiscoverFail::Reg(e)
    }
}
impl From<HdaError> for DiscoverFail {
    fn from(e: HdaError) -> Self {
        DiscoverFail::Hda(e)
    }
}
impl From<AllowError> for DiscoverFail {
    fn from(_: AllowError) -> Self {
        DiscoverFail::OutOfMemory
    }
}

/// Read the controller's geometry. READ ONLY: nothing is written until the write list built from this exists.
pub fn discover<M: Machine>(rig: &mut Rig<M>, playback_stream: u32) -> Result<Controller, DiscoverFail> {
    let gcap = rig.read(Reg { space: Space::Hda, off: AZX_REG_GCAP, width: Width::W16 })?;
    if gcap == 0 || gcap == 0xffff {
        return Err(DiscoverFail::Dead);
    }
    let streams = Streams::from_gcap(gcap as u16);
    if playback_stream >= streams.playback {
        return Err(DiscoverFail::NoSuchStream { wanted: playback_stream, have: streams.playback });
    }
    let playback = streams.playback_stream(playback_stream)?;
    let mut pump = Pump::new(avs_ipc_core::txn::Correlation::Strict);
    let mut ctx = FwCtx::bare();
    let mut walk = CapWalk::new();
    let mut input = WalkIn::Begin;
    let caps = loop {
        match walk.next(input) {
            WalkOut::Read(step) => {
                input = match perform(rig, &mut pump, &mut ctx, &step).map_err(|_| DiscoverFail::Dead)? {
                    avs_fw_core::effect::Input::Value(v) => WalkIn::Value(v),
                    _ => return Err(DiscoverFail::Dead),
                };
            }
            WalkOut::Done(c) => break c,
            WalkOut::Failed(e) => return Err(e.into()),
        }
    };
    if caps.pp.is_none() {
        return Err(DiscoverFail::NoProcessingPipe);
    }
    let mut links = Vec::new();
    if let Some(ml) = caps.ml {
        let bar_len = rig.m.window_len(Space::Hda);
        let mut probe = MlProbe::new(ml, bar_len);
        let mut input = WalkIn::Begin;
        links = loop {
            match probe.next(input) {
                MlOut::Read(step) => {
                    input = match perform(rig, &mut pump, &mut ctx, &step).map_err(|_| DiscoverFail::Dead)? {
                        avs_fw_core::effect::Input::Value(v) => WalkIn::Value(v),
                        _ => return Err(DiscoverFail::Dead),
                    };
                }
                MlOut::Done(l) => break l,
                MlOut::Failed(e) => return Err(e.into()),
            }
        };
    }
    Ok(Controller { streams, caps, links, playback })
}

impl Controller {
    /// The write list this controller needs.
    pub fn allow(&self) -> Result<Allow, AllowError> {
        let mut bases = Vec::new();
        bases.try_reserve_exact(self.links.len()).map_err(|_| AllowError::OutOfMemory)?;
        bases.extend(self.links.iter().map(|l| l.base));
        Allow::new(&HdaLayout { streams: self.streams, playback: self.playback, pp: self.caps.pp, spb: self.caps.spb, link_bases: bases })
    }

    /// `avs_hdac_bus_init_chip` and the processing-pipe enables: the controller out of reset with interrupts on.
    pub fn start_chip<M: Machine>(&self, rig: &mut Rig<M>) -> Result<(), RunFail> {
        let programs = init_chip(&self.streams, &self.links, None).map_err(plan_err)?;
        for p in &programs {
            run_program(rig, p)?;
        }
        let pp = self.caps.pp.ok_or(RunFail::Exec(crate::fwexec::ExecFail::PlanBuild("no-processing-pipe")))?;
        run_program(rig, &ppcap_enable(pp, true))?;
        run_program(rig, &ppcap_int_enable(pp, true))
    }

    /// `snd_hdac_bus_stop_chip`, best effort in full: every program is attempted, the first failure is returned.
    pub fn stop_chip<M: Machine>(&self, rig: &mut Rig<M>) -> Result<(), RunFail> {
        let mut first = Ok(());
        if let Some(pp) = self.caps.pp {
            for p in [ppcap_int_enable(pp, false), ppcap_enable(pp, false)] {
                if let Err(e) = run_program(rig, &p) {
                    first = first.and(Err(e));
                }
            }
        }
        match stop_chip(&self.streams, false) {
            Ok(list) => {
                for p in &list {
                    if let Err(e) = run_program(rig, p) {
                        first = first.and(Err(e));
                    }
                }
            }
            Err(e) => first = first.and(Err(plan_err(e))),
        }
        first
    }
}

fn plan_err(e: HdaError) -> RunFail {
    RunFail::Exec(crate::fwexec::ExecFail::PlanBuild(e.name()))
}

/// Power gating back on and the main DSP core off: the end of every session, and the answer to a boot that left the core running.
pub fn dsp_off<M: Machine>(rig: &mut Rig<M>) -> Result<(), RunFail> {
    let a = run_program(rig, &avs_boot_core::boot::Boot::shutdown_program());
    let b = run_program(rig, &programs::gating(true));
    a.and(b)
}
