// SPDX-License-Identifier: GPL-2.0-only
//! Controller bring-up plans: link reset (`snd_hdac_bus_reset_link`), interrupt clear/enable/disable, the position buffer, the
//! AVS wrapper `avs_hdac_bus_init_chip` (clock gating around the reset, the stream-to-link map reset, the DUM bit), the processing-pipe
//! enables and `snd_hdac_bus_stop_chip`. Steps are [`avs_fw_core`] steps; a plan that depends on a count the hardware states (streams,
//! links) is a list of [`Program`]s, none longer than [`PROGRAM_CAP`].
use crate::caps::Link;
use crate::consts::*;
use crate::error::HdaError;
use crate::regs::*;
use crate::stream::{sd_reg, Streams};
use alloc::vec::Vec;
use avs_fw_core::effect::{Op, OnFail, PollSpec, Reg, Space, Step, Width};
use avs_fw_core::regs::{AZX_CGCTL_MISCBDCGE_MASK, AZX_PCIREG_CGCTL};
use avs_fw_core::runner::{Program, PROGRAM_CAP};

pub(crate) fn hda(off: u32, width: Width) -> Reg {
    Reg { space: Space::Hda, off, width }
}

/// Programs built a step at a time, a new one started when the last is full.
pub(crate) struct Chunker {
    done: Vec<Program>,
    cur: Program,
}

impl Chunker {
    pub(crate) fn new() -> Self {
        Self { done: Vec::new(), cur: Program::new() }
    }
    pub(crate) fn push(&mut self, s: Step) -> Result<(), HdaError> {
        if self.cur.len() == PROGRAM_CAP {
            self.flush()?;
        }
        self.cur.push(s);
        Ok(())
    }
    pub(crate) fn extend(&mut self, p: &Program) -> Result<(), HdaError> {
        for s in p.as_slice() {
            self.push(*s)?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), HdaError> {
        self.done.try_reserve(1).map_err(|_| HdaError::OutOfMemory { what: "controller programs" })?;
        self.done.push(core::mem::take(&mut self.cur));
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<Vec<Program>, HdaError> {
        if !self.cur.is_empty() {
            self.flush()?;
        }
        Ok(self.done)
    }
}

/// `avs_hdac_clock_gating_enable` (avs/core.c:62-69): PCI `CGCTL` `MISCBDCGE`, cleared to disable gating and set to enable it.
pub fn clock_gating(enable: bool) -> Program {
    let mut p = Program::new();
    let reg = Reg { space: Space::PciConfig, off: AZX_PCIREG_CGCTL, width: Width::D32 };
    let label = if enable { "clock-gating-on" } else { "clock-gating-off" };
    p.push(Step::new(label, Op::Update { reg, mask: AZX_CGCTL_MISCBDCGE_MASK, value: if enable { AZX_CGCTL_MISCBDCGE_MASK } else { 0 } }, OnFail::Tolerate));
    p
}

/// `snd_hdac_bus_reset_link` (controller.c:523-561) with its two silent waits made named failures.
///
/// `full_reset` cycles the controller through reset: enter (a 32-bit update, controller.c:495), wait for `CRST` to read 0 (100 ms in
/// 500 us steps, :497-500), settle 500 us (:538), exit (a BYTE update, :514), wait for `CRST` to read 1 (:516-518), settle 1 ms (:544).
/// Linux waits for the byte to be NON-ZERO; `CRST` is its only bit after reset, so the wait is for `CRST` itself. A controller that
/// never reports out of reset is the named failure of `gctl-reset-exited` (Linux's final `-EBUSY`, controller.c:548-551).
/// Without `full_reset` only the readiness check runs. `STATESTS` is cleared UNCONDITIONALLY (Linux does it only when the controller is
/// out of reset, :528-531; a write to a controller in reset is ignored) and read last: that read is the codec mask (:553-557).
pub fn link_reset(full_reset: bool) -> Program {
    let gctl8 = hda(AZX_REG_GCTL, Width::B8);
    let gctl32 = hda(AZX_REG_GCTL, Width::D32);
    let statests = hda(AZX_REG_STATESTS, Width::W16);
    let mut p = Program::new();
    let a = OnFail::Abort;
    if full_reset {
        p.push(Step::new("statests-clear", Op::Write { reg: statests, value: STATESTS_INT_MASK }, OnFail::Tolerate));
        p.push(Step::new("gctl-enter-reset", Op::Update { reg: gctl32, mask: AZX_GCTL_RESET, value: 0 }, a));
        p.push(Step::new("gctl-reset-entered", Op::Poll(PollSpec::new(gctl8, AZX_GCTL_RESET, 0, LINK_RESET_POLL_INTERVAL_US, LINK_RESET_TIMEOUT_US)), a));
        p.push(Step::new("link-reset-settle", Op::DelayUs(LINK_RESET_ENTER_SETTLE_US), OnFail::Tolerate));
        p.push(Step::new("gctl-exit-reset", Op::Update { reg: gctl8, mask: AZX_GCTL_RESET, value: AZX_GCTL_RESET }, a));
    }
    p.push(Step::new("gctl-reset-exited", Op::Poll(PollSpec::new(gctl8, AZX_GCTL_RESET, AZX_GCTL_RESET, LINK_RESET_POLL_INTERVAL_US, LINK_RESET_TIMEOUT_US)), a));
    if full_reset {
        p.push(Step::new("codec-settle", Op::DelayUs(LINK_RESET_EXIT_SETTLE_US), OnFail::Tolerate));
    }
    p.push(Step::new("codec-mask", Op::Read { reg: statests }, a));
    p
}

/// `azx_int_clear` (controller.c:586-603): every stream's status, `STATESTS`, `RIRBSTS`, then `INTSTS`.
pub fn int_clear(streams: &Streams) -> Result<Vec<Program>, HdaError> {
    let mut c = Chunker::new();
    for i in 0..streams.total() {
        c.push(Step::new("sd-sts-clear", Op::Write { reg: sd_reg(i, AZX_REG_SD_STS, Width::B8), value: SD_INT_MASK }, OnFail::Abort))?; // controller.c:592
    }
    c.push(Step::new("statests-clear", Op::Write { reg: hda(AZX_REG_STATESTS, Width::W16), value: STATESTS_INT_MASK }, OnFail::Abort))?; // :595
    c.push(Step::new("rirbsts-clear", Op::Write { reg: hda(AZX_REG_RIRBSTS, Width::B8), value: RIRB_INT_MASK }, OnFail::Abort))?; // :598
    c.push(Step::new("intsts-clear", Op::Write { reg: hda(AZX_REG_INTSTS, Width::D32), value: AZX_INT_CTRL_EN | AZX_INT_ALL_STREAM }, OnFail::Abort))?; // :601
    c.finish()
}

/// `azx_int_enable` (controller.c:564-571): the controller and global interrupt enables.
pub fn int_enable() -> Program {
    let mut p = Program::new();
    let m = AZX_INT_CTRL_EN | AZX_INT_GLOBAL_EN;
    p.push(Step::new("intctl-enable", Op::Update { reg: hda(AZX_REG_INTCTL, Width::D32), mask: m, value: m }, OnFail::Abort));
    p
}

/// `azx_int_disable` (controller.c:573-584): each stream's interrupt enables, then all of `INTCTL`.
pub fn int_disable(streams: &Streams) -> Result<Vec<Program>, HdaError> {
    let mut c = Chunker::new();
    for i in 0..streams.total() {
        c.push(Step::new("sd-int-disable", Op::Update { reg: sd_reg(i, AZX_REG_SD_CTL, Width::B8), mask: SD_INT_MASK, value: 0 }, OnFail::Abort))?; // :579
    }
    c.push(Step::new("intctl-disable", Op::Write { reg: hda(AZX_REG_INTCTL, Width::D32), value: 0 }, OnFail::Abort))?; // :582
    c.finish()
}

/// The position buffer base (controller.c:627-630 programs it, :656-658 clears it). `addr` must be 128-byte aligned: `DPLBASE` bit 0
/// is the enable, which the stream setup sets (stream.c:297-301).
pub fn posbuf(addr: Option<u64>) -> Result<Program, HdaError> {
    let a = addr.unwrap_or(0);
    if a & 0x7f != 0 {
        return Err(HdaError::UnalignedDmaAddress { addr: a, align: 128 });
    }
    let mut p = Program::new();
    p.push(Step::new(if addr.is_some() { "dplbase" } else { "dplbase-clear" }, Op::Write { reg: hda(AZX_REG_DPLBASE, Width::D32), value: a as u32 }, OnFail::Abort));
    p.push(Step::new(if addr.is_some() { "dpubase" } else { "dpubase-clear" }, Op::Write { reg: hda(AZX_REG_DPUBASE, Width::D32), value: (a >> 32) as u32 }, OnFail::Abort));
    Ok(p)
}

/// `avs_hdac_bus_init_chip(bus, full_reset = true)` (avs/core.c:106-128) around `snd_hdac_bus_init_chip` (controller.c:609-636):
/// clock gating off, link reset, interrupt clear, interrupt enable, position buffer, the stream-to-link map reset of every link
/// (avs/core.c:116-117), clock gating on, then the DUM bit (avs/core.c:121-125). The CORB/RIRB rings (`snd_hdac_bus_init_cmd_io`,
/// controller.c:621) are NOT planned.
pub fn init_chip(streams: &Streams, links: &[Link], posbuf_addr: Option<u64>) -> Result<Vec<Program>, HdaError> {
    let mut c = Chunker::new();
    c.extend(&clock_gating(false))?;
    c.extend(&link_reset(true))?;
    for p in int_clear(streams)? {
        c.extend(&p)?;
    }
    c.extend(&int_enable())?;
    if posbuf_addr.is_some() {
        c.extend(&posbuf(posbuf_addr)?)?;
    }
    for l in links {
        c.push(Step::new("link-losidv-reset", Op::Write { reg: hda(l.base + AZX_REG_ML_LOSIDV, Width::D32), value: 0 }, OnFail::Abort))?;
    }
    c.extend(&clock_gating(true))?;
    c.push(Step::new("vs-em2-dum", Op::Update { reg: hda(AZX_REG_VS_EM2, Width::D32), mask: AZX_VS_EM2_DUM, value: AZX_VS_EM2_DUM }, OnFail::Abort))?;
    c.finish()
}

/// `snd_hdac_bus_stop_chip` (controller.c:642-665): interrupts off and cleared, position buffer off. (CORB/RIRB stop is not planned.)
pub fn stop_chip(streams: &Streams, had_posbuf: bool) -> Result<Vec<Program>, HdaError> {
    let mut c = Chunker::new();
    for p in int_disable(streams)? {
        c.extend(&p)?;
    }
    for p in int_clear(streams)? {
        c.extend(&p)?;
    }
    if had_posbuf {
        c.extend(&posbuf(None)?)?;
    }
    c.finish()
}

/// `snd_hdac_ext_bus_ppcap_enable` (ext/controller.c:25-43): the global processing-pipe enable. `pp` is [`crate::caps::Caps::pp`].
pub fn ppcap_enable(pp: u32, enable: bool) -> Program {
    let mut p = Program::new();
    p.push(Step::new(if enable { "ppctl-gproc-on" } else { "ppctl-gproc-off" }, Op::Update { reg: hda(pp + AZX_REG_PP_PPCTL, Width::D32), mask: AZX_PPCTL_GPROCEN, value: if enable { AZX_PPCTL_GPROCEN } else { 0 } }, OnFail::Abort));
    p
}

/// `snd_hdac_ext_bus_ppcap_int_enable` (ext/controller.c:46-63): the processing-pipe interrupt enable.
pub fn ppcap_int_enable(pp: u32, enable: bool) -> Program {
    let mut p = Program::new();
    p.push(Step::new(if enable { "ppctl-pie-on" } else { "ppctl-pie-off" }, Op::Update { reg: hda(pp + AZX_REG_PP_PPCTL, Width::D32), mask: AZX_PPCTL_PIE, value: if enable { AZX_PPCTL_PIE } else { 0 } }, OnFail::Abort));
    p
}
