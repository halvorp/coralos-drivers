// SPDX-License-Identifier: GPL-2.0-only
//! Gen8 GT interrupt decode and set-up (gt/intel_gt_irq.c:405-489): `gen8_gt_irq_handler` as a pure decoder over the master
//! control word and the GT IIR banks, `gen8_gt_irq_reset` / `gen8_gt_irq_postinstall` as write plans, and the per-engine
//! interrupt bits (`execlists_irq_handler`, gt/intel_execlists_submission.c:2486-2520).
use crate::effect::{Effect, WritePlan};
use crate::error::GtError;
use crate::fixed::FixedVec;
use i915_gt_regs_core as r;

/// What one engine's 16-bit IIR chunk means (`execlists_irq_handler`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EngineIrq {
    /// The raw 16-bit chunk (`iir >> shift`).
    pub iir: u16,
    /// `GT_RENDER_USER_INTERRUPT`: a breadcrumb signalled.
    pub user_interrupt: bool,
    /// `GT_CONTEXT_SWITCH_INTERRUPT`: the CSB has new events (tasklet).
    pub context_switch: bool,
    /// `GT_CS_MASTER_ERROR_INTERRUPT`: the command streamer raised an error (read `RING_EIR`, mask `RING_EMR`, reset).
    pub cs_error: bool,
    /// `GT_WAIT_SEMAPHORE_INTERRUPT`: a semaphore wait yielded the engine.
    pub wait_semaphore: bool,
}

impl EngineIrq {
    pub const fn decode(iir: u16) -> EngineIrq {
        let v = iir as u32;
        EngineIrq {
            iir,
            user_interrupt: v & r::GT_RENDER_USER_INTERRUPT != 0,
            context_switch: v & r::GT_CONTEXT_SWITCH_INTERRUPT != 0,
            cs_error: v & r::GT_CS_MASTER_ERROR_INTERRUPT != 0,
            wait_semaphore: v & r::GT_WAIT_SEMAPHORE_INTERRUPT != 0,
        }
    }
    /// `intel_engine_cs_irq`: the handler runs only for a non-zero chunk (gt/intel_gt_irq.h:44-48).
    pub const fn is_set(&self) -> bool {
        self.iir != 0
    }
}

/// The IIR banks `gen8_gt_irq_handler` reads, as the caller read them (only those the master control word selected are consulted).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gen8GtIir {
    pub bank0: u32,
    pub bank1: u32,
    pub bank2: u32,
    pub bank3: u32,
}

/// The decode of one interrupt: which engines fired and which IIR banks must be acknowledged (written back with the same value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gen8GtIrq {
    pub rcs0: EngineIrq,
    pub bcs0: EngineIrq,
    pub vcs0: EngineIrq,
    pub vcs1: EngineIrq,
    pub vecs0: EngineIrq,
    /// Bank 2 (`GEN8_GT_IIR(2)`): PM / RPS events in the low half, GuC in the high half. Passed through, not interpreted here.
    pub pm_iir: u32,
    pub guc_iir: u16,
    /// Acknowledge writes (`raw_reg_write(GEN8_GT_IIR(n), iir)`), only for banks whose IIR was non-zero (`likely(iir)`).
    pub acks: FixedVec<WritePlan, 4>,
}

/// `gen8_gt_irq_handler` (intel_gt_irq.c:405-449). `master_ctl` is `GEN8_MASTER_IRQ` with the control bit stripped as the caller
/// did; `iir` holds the banks the caller read for the bits `master_ctl` set (a bank not selected by `master_ctl` is ignored,
/// exactly as Linux does not read it).
pub fn decode_gen8_gt_irq(master_ctl: u32, iir: Gen8GtIir) -> Result<Gen8GtIrq, GtError> {
    let mut out = Gen8GtIrq {
        rcs0: EngineIrq::default(),
        bcs0: EngineIrq::default(),
        vcs0: EngineIrq::default(),
        vcs1: EngineIrq::default(),
        vecs0: EngineIrq::default(),
        pm_iir: 0,
        guc_iir: 0,
        acks: FixedVec::new("gt irq acks", WritePlan { reg: r::gen8_gt_iir(0), value: 0 }),
    };
    // bank 0: RCS | BCS (intel_gt_irq.c:410-419)
    if master_ctl & (r::GEN8_GT_RCS_IRQ | r::GEN8_GT_BCS_IRQ) != 0 && iir.bank0 != 0 {
        out.rcs0 = EngineIrq::decode((iir.bank0 >> r::GEN8_RCS_IRQ_SHIFT) as u16);
        out.bcs0 = EngineIrq::decode((iir.bank0 >> r::GEN8_BCS_IRQ_SHIFT) as u16);
        out.acks.push(WritePlan { reg: r::gen8_gt_iir(0), value: iir.bank0 })?;
    }
    // bank 1: VCS0 | VCS1 (:421-430)
    if master_ctl & (r::GEN8_GT_VCS0_IRQ | r::GEN8_GT_VCS1_IRQ) != 0 && iir.bank1 != 0 {
        out.vcs0 = EngineIrq::decode((iir.bank1 >> r::GEN8_VCS0_IRQ_SHIFT) as u16);
        out.vcs1 = EngineIrq::decode((iir.bank1 >> r::GEN8_VCS1_IRQ_SHIFT) as u16);
        out.acks.push(WritePlan { reg: r::gen8_gt_iir(1), value: iir.bank1 })?;
    }
    // bank 3: VECS (:432-439)
    if master_ctl & r::GEN8_GT_VECS_IRQ != 0 && iir.bank3 != 0 {
        out.vecs0 = EngineIrq::decode((iir.bank3 >> r::GEN8_VECS_IRQ_SHIFT) as u16);
        out.acks.push(WritePlan { reg: r::gen8_gt_iir(3), value: iir.bank3 })?;
    }
    // bank 2: PM | GuC (:441-448)
    if master_ctl & (r::GEN8_GT_PM_IRQ | r::GEN8_GT_GUC_IRQ) != 0 && iir.bank2 != 0 {
        out.pm_iir = iir.bank2;
        out.guc_iir = (iir.bank2 >> 16) as u16;
        out.acks.push(WritePlan { reg: r::gen8_gt_iir(2), value: iir.bank2 })?;
    }
    Ok(out)
}

/// `GEN8_GT_IRQS` (gt/intel_gt_irq.h:15-21): every GT bank bit of the master control word.
pub const GEN8_GT_IRQS: u32 = r::GEN8_GT_RCS_IRQ
    | r::GEN8_GT_BCS_IRQ
    | r::GEN8_GT_VCS0_IRQ
    | r::GEN8_GT_VCS1_IRQ
    | r::GEN8_GT_VECS_IRQ
    | r::GEN8_GT_PM_IRQ
    | r::GEN8_GT_GUC_IRQ; // intel_gt_irq.h:15

/// `gen2_irq_reset` for one bank (i915_irq.c:81-93): IMR all-ones + posting read, IER zero, IIR cleared twice with posting reads.
fn gen2_irq_reset(bank: u32, out: &mut FixedVec<Effect, 32>) -> Result<(), GtError> {
    out.push(Effect::Write(WritePlan { reg: r::gen8_gt_imr(bank), value: 0xffff_ffff }))?;
    out.push(Effect::PostingRead(r::gen8_gt_imr(bank)))?;
    out.push(Effect::Write(WritePlan { reg: r::gen8_gt_ier(bank), value: 0 }))?;
    out.push(Effect::Write(WritePlan { reg: r::gen8_gt_iir(bank), value: 0xffff_ffff }))?;
    out.push(Effect::PostingRead(r::gen8_gt_iir(bank)))?;
    out.push(Effect::Write(WritePlan { reg: r::gen8_gt_iir(bank), value: 0xffff_ffff }))?;
    out.push(Effect::PostingRead(r::gen8_gt_iir(bank)))
}

/// `gen8_gt_irq_reset` (intel_gt_irq.c:451-459): `gen2_irq_reset` on banks 0..=3.
pub fn plan_gen8_gt_irq_reset() -> Result<FixedVec<Effect, 32>, GtError> {
    let mut v = FixedVec::new("gt irq reset plan", Effect::FILL);
    for bank in 0..4 {
        gen2_irq_reset(bank, &mut v)?;
    }
    Ok(v)
}

/// The set-up half of `gen2_irq_init` (i915_irq.c:114-122): the caller must have READ `IIR` and found it zero
/// (`gen2_assert_iir_is_zero`, :98-112 - a non-zero IIR is Linux's WARN and a pending interrupt); then IER, IMR, posting read.
fn gen2_irq_init(bank: u32, imr: u32, ier: u32, out: &mut FixedVec<Effect, 32>) -> Result<(), GtError> {
    out.push(Effect::Write(WritePlan { reg: r::gen8_gt_ier(bank), value: ier }))?;
    out.push(Effect::Write(WritePlan { reg: r::gen8_gt_imr(bank), value: imr }))?;
    out.push(Effect::PostingRead(r::gen8_gt_imr(bank)))
}

/// `gen8_gt_irq_postinstall` (intel_gt_irq.c:461-489): per-bank (IER, IMR). Bank 2 (PM / GuC) starts masked with nothing
/// enabled (`pm_ier = 0`, `pm_imr = ~pm_ier`); the rest enable the four engine interrupts for their half-words.
pub fn plan_gen8_gt_irq_postinstall() -> Result<FixedVec<Effect, 32>, GtError> {
    // intel_gt_irq.c:463-467: the interrupts toggled with the ring mask register
    let irqs = r::GT_CS_MASTER_ERROR_INTERRUPT | r::GT_RENDER_USER_INTERRUPT | r::GT_CONTEXT_SWITCH_INTERRUPT | r::GT_WAIT_SEMAPHORE_INTERRUPT;
    let gt_interrupts = [
        (irqs << r::GEN8_RCS_IRQ_SHIFT) | (irqs << r::GEN8_BCS_IRQ_SHIFT),   // :470
        (irqs << r::GEN8_VCS0_IRQ_SHIFT) | (irqs << r::GEN8_VCS1_IRQ_SHIFT), // :471
        0,                                                                   // :472
        irqs << r::GEN8_VECS_IRQ_SHIFT,                                      // :473
    ];
    let mut v = FixedVec::new("gt irq postinstall plan", Effect::FILL);
    gen2_irq_init(0, !gt_interrupts[0], gt_interrupts[0], &mut v)?;
    gen2_irq_init(1, !gt_interrupts[1], gt_interrupts[1], &mut v)?;
    gen2_irq_init(2, !gt_interrupts[2], gt_interrupts[2], &mut v)?; // pm_imr = ~pm_ier, pm_ier = 0
    gen2_irq_init(3, !gt_interrupts[3], gt_interrupts[3], &mut v)?;
    Ok(v)
}

/// The IIR of `bank` must read zero before `gen2_irq_init` (i915_irq.c:98-112). Returns the bank's IIR register, so a caller
/// reads it; a non-zero value is `PendingInterrupt`-class: Linux WARNs and writes `0xffffffff` to IIR twice before continuing.
pub fn irq_init_iir_reg(bank: u32) -> Result<i915_common_core::Reg, GtError> {
    if bank > 3 {
        return Err(GtError::BadIrqBank { bank });
    }
    Ok(r::gen8_gt_iir(bank))
}
