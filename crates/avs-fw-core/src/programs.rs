// SPDX-License-Identifier: GPL-2.0-only
//! The fixed op sequences of `dsp.c`, `cldma.c` and `loader.c`, as [`Program`]s. Every sequence says which Linux function it
//! is and what a failure of each step does.
use crate::bdl::ClDmaLayout;
use crate::effect::*;
use crate::regs::*;
use crate::runner::Program;

const ADSPCS: Reg = Reg::dsp32(AVS_ADSP_REG_ADSPCS);
const ADSPIC: Reg = Reg::dsp32(AVS_ADSP_REG_ADSPIC);
const SD_CTL8: Reg = Reg::dsp8(cl_sd(AZX_REG_SD_CTL));
const SD_CTL32: Reg = Reg::dsp32(cl_sd(AZX_REG_SD_CTL));

fn adspcs_poll(mask: u32, want: u32) -> Op {
    Op::Poll(PollSpec::new(ADSPCS, mask, want, AVS_ADSPCS_INTERVAL_US, AVS_ADSPCS_TIMEOUT_US))
}

/// `avs_dsp_core_power` (dsp.c:17-44): set SPA, wait, poll CPA.
pub fn core_power(core_mask: u32, power: bool, on_fail: OnFail) -> Program {
    let spa = adspcs_spa_mask(core_mask);
    let cpa = adspcs_cpa_mask(core_mask);
    let (l1, l2, l3) = if power { ("core-power-on", "core-power-on-settle", "core-power-on-cpa") } else { ("core-power-off", "core-power-off-settle", "core-power-off-cpa") };
    let mut p = Program::new();
    p.push(Step::new(l1, Op::Update { reg: ADSPCS, mask: spa, value: if power { spa } else { 0 } }, on_fail));
    p.push(Step::new(l2, Op::DelayUs(AVS_ADSPCS_DELAY_US), OnFail::Tolerate)); // dsp.c:30 "avoid false positives"
    p.push(Step::new(l3, adspcs_poll(cpa, if power { cpa } else { 0 }), on_fail));
    p
}

/// `avs_dsp_core_reset` (dsp.c:46-68): set or clear CRST, poll it.
pub fn core_reset(core_mask: u32, reset: bool, on_fail: OnFail) -> Program {
    let crst = adspcs_crst_mask(core_mask);
    let (l1, l2) = if reset { ("core-reset-enter", "core-reset-enter-ack") } else { ("core-reset-exit", "core-reset-exit-ack") };
    let mut p = Program::new();
    p.push(Step::new(l1, Op::Update { reg: ADSPCS, mask: crst, value: if reset { crst } else { 0 } }, on_fail));
    p.push(Step::new(l2, adspcs_poll(crst, if reset { crst } else { 0 }), on_fail));
    p
}

/// `avs_dsp_core_stall` (dsp.c:70-96): set or clear CSTALL, poll it, then give the hardware 1 ms to propagate.
pub fn core_stall(core_mask: u32, stall: bool, on_fail: OnFail) -> Program {
    let cs = adspcs_cstall_mask(core_mask);
    let (l1, l2, l3) = if stall { ("core-stall", "core-stall-ack", "core-stall-settle") } else { ("core-unstall", "core-unstall-ack", "core-unstall-settle") };
    let mut p = Program::new();
    p.push(Step::new(l1, Op::Update { reg: ADSPCS, mask: cs, value: if stall { cs } else { 0 } }, on_fail));
    p.push(Step::new(l2, adspcs_poll(cs, if stall { cs } else { 0 }), on_fail));
    p.push(Step::new(l3, Op::DelayUs(AVS_ADSPCS_DELAY_US), OnFail::Tolerate)); // dsp.c:92-93
    p
}

/// `avs_dsp_core_disable` (dsp.c:113-121): "No error checks to allow for complete DSP shutdown" - stall, reset, power off.
pub fn core_disable(core_mask: u32) -> Program {
    let mut p = Program::new();
    p.extend(&core_stall(core_mask, true, OnFail::Tolerate));
    p.extend(&core_reset(core_mask, true, OnFail::Tolerate));
    p.extend(&core_power(core_mask, false, OnFail::Tolerate));
    p
}

/// What `OnFail::AbortDisable` runs: `avs_dsp_core_disable(adev, AVS_MAIN_CORE_MASK)`.
pub fn core_disable_cleanup() -> Program {
    core_disable(AVS_MAIN_CORE_MASK)
}

/// `hda_cldma_stop` (cldma.c:151-166): interrupts off, DMA run bit off, wait for the engine to stop. The
/// `cancel_delayed_work_sync` is the caller's: there is no worker in a planner.
pub fn cldma_stop(on_fail: OnFail) -> Program {
    let mut p = Program::new();
    p.push(Step::new("cldma-irq-off", Op::Update { reg: ADSPIC, mask: AVS_ADSP_ADSPIC_CLDMA, value: 0 }, on_fail));
    p.push(Step::new("cldma-run-off", Op::Update { reg: SD_CTL8, mask: SD_INT_MASK | SD_CTL_DMA_START, value: 0 }, on_fail));
    p.push(Step::new("cldma-stopped", Op::Poll(PollSpec::new(SD_CTL8, SD_CTL_DMA_START, 0, AVS_CL_OP_INTERVAL_US, AVS_CL_OP_TIMEOUT_US)), on_fail));
    p
}

/// What `OnFail::AbortStopDisable` runs: `hda_cldma_stop(cl)` then the core disable (loader.c:183-188).
pub fn stop_then_disable_cleanup() -> Program {
    let mut p = cldma_stop(OnFail::Tolerate);
    p.extend(&core_disable(AVS_MAIN_CORE_MASK));
    p
}

/// `hda_cldma_reset` (cldma.c:168-196): stop, then pulse the stream reset bit and wait for each edge.
pub fn cldma_reset(on_fail: OnFail) -> Program {
    let mut p = cldma_stop(on_fail);
    p.push(Step::new("cldma-srst-set", Op::Update { reg: SD_CTL8, mask: SD_CTL_STREAM_RESET, value: SD_CTL_STREAM_RESET }, on_fail));
    p.push(Step::new("cldma-srst-ack", Op::Poll(PollSpec::new(SD_CTL8, SD_CTL_STREAM_RESET, SD_CTL_STREAM_RESET, AVS_CL_OP_INTERVAL_US, AVS_CL_OP_TIMEOUT_US)), on_fail));
    p.push(Step::new("cldma-srst-clear", Op::Update { reg: SD_CTL8, mask: SD_CTL_STREAM_RESET, value: 0 }, on_fail));
    p.push(Step::new("cldma-srst-clear-ack", Op::Poll(PollSpec::new(SD_CTL8, SD_CTL_STREAM_RESET, 0, AVS_CL_OP_INTERVAL_US, AVS_CL_OP_TIMEOUT_US)), on_fail));
    p
}

/// `hda_cldma_setup` register writes (cldma.c:234-249). The BDL ENTRIES themselves are memory the caller fills from
/// [`ClDmaLayout::entries`] before this program runs.
pub fn cldma_setup(l: &ClDmaLayout) -> Program {
    let mut p = Program::new();
    let f = OnFail::Abort;
    p.push(Step::new("cldma-bdlpl", Op::Write { reg: Reg::dsp32(cl_sd(AZX_REG_SD_BDLPL)), value: (l.bdl_addr as u32) & AZX_SD_BDLPL_BDLPLBA_MASK }, f));
    p.push(Step::new("cldma-bdlpu", Op::Write { reg: Reg::dsp32(cl_sd(AZX_REG_SD_BDLPU)), value: (l.bdl_addr >> 32) as u32 }, f));
    p.push(Step::new("cldma-cbl", Op::Write { reg: Reg::dsp32(cl_sd(AZX_REG_SD_CBL)), value: l.buffer_size }, f));
    p.push(Step::new("cldma-lvi", Op::Write { reg: Reg::dsp8(cl_sd(AZX_REG_SD_LVI)), value: l.lvi() }, f));
    p.push(Step::new("cldma-stream-tag", Op::Update { reg: SD_CTL32, mask: AZX_SD_CTL_STRM_MASK, value: (l.stream_tag << AZX_SD_CTL_STRM_SHIFT) & AZX_SD_CTL_STRM_MASK }, f));
    p.push(Step::new("cldma-spib-enable", Op::Write { reg: Reg::dsp32(cl_sd(AZX_REG_CL_SPBFCTL)), value: 1 }, f)); // "enable spib"
    p
}

/// `avs_dsp_interrupt_control(adev, enable)` (ipc.c:541-558): the IPC interrupt in ADSPIC and the HIPC DONE/BUSY enables.
pub fn int_control(enable: bool, on_fail: OnFail) -> Program {
    let mask = AVS_ADSP_HIPCCTL_DONE | AVS_ADSP_HIPCCTL_BUSY;
    let mut p = Program::new();
    p.push(Step::new("int-ipc", Op::Update { reg: ADSPIC, mask: AVS_ADSP_ADSPIC_IPC, value: if enable { AVS_ADSP_ADSPIC_IPC } else { 0 } }, on_fail));
    p.push(Step::new("int-hipcctl", Op::Update { reg: Reg::dsp32(SKL_ADSP_REG_HIPCCTL), mask, value: if enable { mask } else { 0 } }, on_fail));
    p
}

/// `avs_hda_power_gating_enable`, `avs_hda_clock_gating_enable`, `avs_hda_l1sen_enable` (core.c:54-88). Linux brackets every
/// code load with the three set to DISABLED and puts them back afterwards (loader.c:625-627, :643-646).
pub fn gating(enable: bool) -> Program {
    let pci = |off| Reg { space: Space::PciConfig, off, width: Width::D32 };
    let mut p = Program::new();
    let t = OnFail::Tolerate;
    // value = enable ? 0 : pgctl_mask (core.c:56); value = enable ? cgctl_mask : 0 (core.c:64)
    p.push(Step::new(if enable { "power-gating-on" } else { "power-gating-off" }, Op::Update { reg: pci(AZX_PCIREG_PGCTL), mask: AZX_PGCTL_LSRMD_MASK, value: if enable { 0 } else { AZX_PGCTL_LSRMD_MASK } }, t));
    p.push(Step::new(if enable { "clock-gating-on" } else { "clock-gating-off" }, Op::Update { reg: pci(AZX_PCIREG_CGCTL), mask: AZX_CGCTL_MISCBDCGE_MASK, value: if enable { AZX_CGCTL_MISCBDCGE_MASK } else { 0 } }, t));
    let hda = Reg { space: Space::Hda, off: AZX_REG_VS_EM2, width: Width::D32 };
    p.push(Step::new(if enable { "l1sen-on" } else { "l1sen-off" }, Op::Update { reg: hda, mask: AZX_VS_EM2_L1SEN, value: if enable { AZX_VS_EM2_L1SEN } else { 0 } }, t));
    p
}

/// The front half of `avs_cldma_load_basefw` (loader.c:138-175): power the main core, release its reset, reset and program
/// the code loader, un-stall, enable IPC interrupts, and wait for the ROM to announce itself.
pub fn basefw_pre(l: &ClDmaLayout) -> Program {
    let main = AVS_MAIN_CORE_MASK;
    let mut p = Program::new();
    p.extend(&core_power(main, true, OnFail::Abort)); // loader.c:144-146
    p.extend(&core_reset(main, false, OnFail::Abort)); // loader.c:148-150
    p.extend(&cldma_reset(OnFail::Abort)); // loader.c:152-156
    p.extend(&cldma_setup(l)); // loader.c:157
    p.extend(&core_stall(main, false, OnFail::Abort)); // loader.c:159-161
    p.extend(&int_control(true, OnFail::Abort)); // loader.c:163-164
    p.push(Step::new(
        "rom-init",
        Op::Poll(PollSpec::new(Reg::dsp32(AVS_FW_REG_STATUS), AVS_ROM_INIT_DONE, AVS_ROM_INIT_DONE, AVS_ROM_INIT_POLLING_US, SKL_ROM_INIT_TIMEOUT_US)),
        OnFail::AbortDisable, // loader.c:170-175
    ));
    p
}

/// The back half of `avs_cldma_load_basefw` plus `avs_dsp_load_basefw`'s wait (loader.c:180-189, :589-598): wait for the ROM
/// to report the base firmware entered, stop the code loader, wait for FW_READY.
pub fn basefw_post() -> Program {
    let mut p = Program::new();
    p.push(Step::new(
        "basefw-entered",
        Op::Poll(PollSpec::new(Reg::dsp32(AVS_FW_REG_STATUS), AVS_ROM_STS_MASK, SKL_ROM_BASEFW_ENTERED, AVS_FW_INIT_POLLING_US, AVS_FW_INIT_TIMEOUT_US)),
        OnFail::AbortStopDisable, // loader.c:180-189
    ));
    p.extend(&cldma_stop(OnFail::Tolerate)); // loader.c:183: `hda_cldma_stop(cl)`, result ignored
    p.push(Step::new("fw-ready", Op::Wait { event: Event::FwReady, timeout_ms: AVS_FW_INIT_TIMEOUT_MS }, OnFail::AbortDisable)); // loader.c:589-596
    p
}
