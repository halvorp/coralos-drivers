// SPDX-License-Identifier: GPL-2.0-only
//! THE ADOPTED PIPE'S HEALTH AFTER A LOGICAL-SIZE RESIZE: a readback of what latched (pipe source, scaler, plane size / stride / DDB /
//! level-0 watermark) and the pipe's CPU FIFO underrun status, decoded into one ring line.
//!
//! WHERE LINUX SEES AN UNDERRUN ON SKL/KBL (Linux v7.0-rc6, drivers/gpu/drm/i915/display). Display ver 9 is neither GMCH (PIPESTAT's
//! `PIPE_FIFO_UNDERRUN_STATUS`, `i9xx_check_fifo_underruns`) nor ver 7 (`GEN7_ERR_INT`, `ivb_check_fifo_underruns`), so
//! `intel_check_cpu_fifo_underruns` polls nothing there (intel_fifo_underrun.c:529-541); reporting is `bdw_set_fifo_underrun_reporting`
//! = `bdw_enable_pipe_irq(.., GEN8_PIPE_FIFO_UNDERRUN)` (:284-291, :378-379), which clears the bit in `GEN8_DE_PIPE_IMR(pipe)`
//! (`bdw_update_pipe_irq`, intel_display_irq.c:227-249), and the event is read from `GEN8_DE_PIPE_IIR(pipe)` bit 31 and acked by
//! writing it back (`gen8_de_irq_handler`, intel_display_irq.c:1493-1528). An event masked in IMR is not reported in IIR - which is
//! why Linux masks it to stop reporting - so the watch first UNMASKS that one bit, after acking any stale event, and puts the
//! firmware's IMR back when the readback is done ([`UnderrunWatch`]). It never touches `GEN8_DE_PIPE_IER`: with the bit off in IER
//! the event is latched in IIR without raising an interrupt. If the firmware left the bit ENABLED in IER while masked, unmasking it
//! would raise an interrupt no handler of this device acks: the watch is refused by name and the line says `underrun=unobserved`.
//!
//! Nothing here performs MMIO: the reads go through a closure, the writes are plans the device runs through its write guard
//! ([`UNDERRUN_WRITE_ALLOWLIST`]). Copyright (c) Intel Corporation.
use crate::resize::{ddb_entry, WmWord};
use i915_common_core::Reg;
use i915_display_link_core::plan::{Effect, Plan};
use i915_kbl_display_core::regs::*;
use i915_kbl_display_core::regs_write::*;
use i915_kbl_display_core::skl_wm::NUM_LEVELS;
use i915_kbl_display_core::types::{Pipe, Transcoder};

/// What the watch writes: pipe A's `GEN8_DE_PIPE_IMR` (the underrun bit only) and `GEN8_DE_PIPE_IIR` (write-1-to-clear ack).
pub const UNDERRUN_WRITE_ALLOWLIST: [Reg; 2] = [Reg::new(gen8_de_pipe_imr(Pipe::A)), Reg::new(gen8_de_pipe_iir(Pipe::A))];

/// Why the underrun bit cannot be watched, named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unobservable {
    /// Masked in IMR but enabled in IER: unmasking would raise an interrupt this device does not service.
    IerEnabled,
    /// Only pipe A's bank is on the write allow-list (as for the resize itself).
    PipeNotInAllowList,
}

impl Unobservable {
    pub const fn name(&self) -> &'static str {
        match self {
            Unobservable::IerEnabled => "underrun-ier-enabled",
            Unobservable::PipeNotInAllowList => "underrun-pipe-not-in-allow-list",
        }
    }
}

/// The armed underrun watch of one pipe: what to read, and what to put back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnderrunWatch {
    pub pipe: Pipe,
    /// `GEN8_DE_PIPE_IMR` as the firmware left it.
    pub fw_imr: u32,
}

fn write(reg: u32, value: u32) -> Effect {
    Effect::Write { reg: Reg::new(reg), value }
}

impl UnderrunWatch {
    /// Arm from the firmware's IMR / IER: ack a stale event in IIR, then unmask `GEN8_PIPE_FIFO_UNDERRUN` in IMR if it is masked.
    pub fn arm(pipe: Pipe, imr: u32, ier: u32) -> Result<(UnderrunWatch, Plan), Unobservable> {
        if pipe != Pipe::A {
            return Err(Unobservable::PipeNotInAllowList);
        }
        let masked = imr & GEN8_PIPE_FIFO_UNDERRUN != 0;
        if masked && ier & GEN8_PIPE_FIFO_UNDERRUN != 0 {
            return Err(Unobservable::IerEnabled);
        }
        let mut p = Plan::new();
        p.push(write(gen8_de_pipe_iir(pipe), GEN8_PIPE_FIFO_UNDERRUN));
        if masked {
            p.push(write(gen8_de_pipe_imr(pipe), imr & !GEN8_PIPE_FIFO_UNDERRUN));
            p.push(Effect::PostingRead { reg: Reg::new(gen8_de_pipe_imr(pipe)) }); // bdw_update_pipe_irq's posting read
        }
        Ok((UnderrunWatch { pipe, fw_imr: imr }, p))
    }

    /// Done watching: ack the event, put the firmware's IMR back (only when the watch changed it).
    pub fn disarm_plan(&self) -> Plan {
        let mut p = Plan::new();
        p.push(write(gen8_de_pipe_iir(self.pipe), GEN8_PIPE_FIFO_UNDERRUN));
        if self.fw_imr & GEN8_PIPE_FIFO_UNDERRUN != 0 {
            p.push(write(gen8_de_pipe_imr(self.pipe), self.fw_imr));
        }
        p
    }
}

/// The health readback, raw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanoutHealth {
    pub pipe: Pipe,
    pub scaler_id: u8,
    pub pipesrc: u32,
    pub ps_ctrl: u32,
    pub ps_win_sz: u32,
    pub plane_size: u32,
    pub plane_stride: u32,
    pub buf_cfg: u32,
    pub wm: [u32; NUM_LEVELS],
    /// `GEN8_DE_PIPE_IIR`, when the underrun bit was watched.
    pub iir: Option<u32>,
    pub after_ms: u64,
}

/// Read the health of `pipe` / plane `plane_id` / scaler `scaler_id` through `rd` (reads only).
pub fn read_health<R: FnMut(u32) -> u32>(mut rd: R, pipe: Pipe, plane_id: u8, scaler_id: u8, watch: Option<&UnderrunWatch>, after_ms: u64) -> ScanoutHealth {
    let p = |base| plane_reg(base, pipe, plane_id as u32);
    let s = |base| scaler_reg(base, pipe, scaler_id as u32);
    let mut wm = [0u32; NUM_LEVELS];
    for (level, v) in wm.iter_mut().enumerate() {
        *v = rd(plane_wm(pipe, plane_id as u32, level as u32));
    }
    ScanoutHealth {
        pipe,
        scaler_id,
        pipesrc: rd(pipesrc(Transcoder::of_pipe(pipe))),
        ps_ctrl: rd(s(PS_1A_CTRL)),
        ps_win_sz: rd(s(PS_WIN_SZ_1A)),
        plane_size: rd(p(PLANE_SIZE_1_A)),
        plane_stride: rd(p(PLANE_STRIDE_1_A)),
        buf_cfg: rd(p(PLANE_BUF_CFG_1_A)),
        wm,
        iir: watch.map(|w| rd(gen8_de_pipe_iir(w.pipe))),
        after_ms,
    }
}

impl ScanoutHealth {
    /// `Some(true)` = the pipe underran since the watch was armed; `None` = not observable.
    pub fn underrun(&self) -> Option<bool> {
        self.iir.map(|v| v & GEN8_PIPE_FIFO_UNDERRUN != 0)
    }
}

impl core::fmt::Display for ScanoutHealth {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // PIPESRC: width - 1 high, height - 1 low; PLANE_SIZE: height - 1 high, width - 1 low; PS_WIN_SZ: width high, height low
        let src = ((self.pipesrc >> 16) + 1, (self.pipesrc & 0xffff) + 1);
        let size = ((self.plane_size & 0xffff) + 1, (self.plane_size >> 16) + 1);
        let (bs, be) = ddb_entry(self.buf_cfg);
        let w0 = WmWord::decode(self.wm[0]);
        let levels = self.wm.iter().filter(|&&v| WmWord::decode(v).enable).count();
        write!(
            f,
            "[I915-SCANOUT-HEALTH pipe={} src={}x{} ps=ps{}:ctrl={:#x},win={}x{} plane_size={}x{} stride={} buf={}-{} wm0={},lines={},blocks={} wm_levels={} underrun=",
            self.pipe.letter(), src.0, src.1, self.scaler_id as u32 + 1, self.ps_ctrl, self.ps_win_sz >> 16, self.ps_win_sz & 0xffff,
            size.0, size.1, self.plane_stride, bs, be.saturating_sub(1), if w0.enable { "en" } else { "off" }, w0.lines, w0.blocks, levels
        )?;
        match self.underrun() {
            Some(u) => write!(f, "{}", u as u8)?,
            None => f.write_str("unobserved")?,
        }
        write!(f, " after_ms={}]", self.after_ms)
    }
}
