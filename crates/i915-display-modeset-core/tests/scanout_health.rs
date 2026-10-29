// SPDX-License-Identifier: GPL-2.0-only
//! The post-resize health readback and the CPU FIFO underrun watch of the adopted pipe (`scanout_health`): which registers the watch
//! writes (GEN8_DE_PIPE_IIR clear, GEN8_DE_PIPE_IMR unmask of GEN8_PIPE_FIFO_UNDERRUN only) and when it refuses to (an interrupt nobody
//! services), what it puts back, and the one health line decoded from the latched registers.
use i915_display_link_core::plan::{written_regs, Effect};
use i915_display_modeset_core::scanout_health::*;
use i915_kbl_display_core::regs_write::{gen8_de_pipe_ier, gen8_de_pipe_iir, gen8_de_pipe_imr, gen8_de_pipe_isr, GEN8_PIPE_FIFO_UNDERRUN};
use i915_kbl_display_core::types::Pipe;
use std::collections::BTreeMap;

const IMR_A: u32 = 0x44404;
const IIR_A: u32 = 0x44408;

fn writes(p: &[Effect]) -> Vec<(u32, u32)> {
    p.iter().filter_map(|e| if let Effect::Write { reg, value } = e { Some((reg.offset(), *value)) } else { None }).collect()
}

#[test]
fn the_de_pipe_interrupt_bank_is_linux_s() {
    assert_eq!((gen8_de_pipe_isr(Pipe::A), gen8_de_pipe_imr(Pipe::A), gen8_de_pipe_iir(Pipe::A), gen8_de_pipe_ier(Pipe::A)), (0x44400, 0x44404, 0x44408, 0x4440c));
    assert_eq!(gen8_de_pipe_iir(Pipe::B), 0x44418);
    assert_eq!(GEN8_PIPE_FIFO_UNDERRUN, 0x8000_0000);
    let allow: Vec<u32> = UNDERRUN_WRITE_ALLOWLIST.iter().map(|r| r.offset()).collect();
    assert_eq!(allow, [IMR_A, IIR_A]);
}

#[test]
fn a_masked_underrun_bit_is_unmasked_in_imr_only_and_put_back() {
    let (w, plan) = UnderrunWatch::arm(Pipe::A, 0xffff_ffff, 0).unwrap();
    // the stale event cleared first (write-1-to-clear, as gen8_de_irq_handler acks), then the one bit unmasked
    assert_eq!(writes(&plan), [(IIR_A, GEN8_PIPE_FIFO_UNDERRUN), (IMR_A, 0x7fff_ffff)]);
    assert!(written_regs(&plan).iter().all(|r| UNDERRUN_WRITE_ALLOWLIST.contains(r)));
    assert_eq!(writes(&w.disarm_plan()), [(IIR_A, GEN8_PIPE_FIFO_UNDERRUN), (IMR_A, 0xffff_ffff)], "the firmware's IMR goes back");
}

#[test]
fn an_unmasked_bit_is_left_alone_and_an_enabled_ier_is_refused_by_name() {
    let (w, plan) = UnderrunWatch::arm(Pipe::A, 0x7fff_ffff, 0).unwrap();
    assert_eq!(writes(&plan), [(IIR_A, GEN8_PIPE_FIFO_UNDERRUN)], "IMR already reports it: only the stale event is cleared");
    assert_eq!(writes(&w.disarm_plan()), [(IIR_A, GEN8_PIPE_FIFO_UNDERRUN)]);
    // masked, but enabled in IER: unmasking would raise an interrupt no handler of this device acks
    let e = UnderrunWatch::arm(Pipe::A, 0xffff_ffff, GEN8_PIPE_FIFO_UNDERRUN).unwrap_err();
    assert_eq!(e.name(), "underrun-ier-enabled");
    assert_eq!(UnderrunWatch::arm(Pipe::B, 0xffff_ffff, 0).unwrap_err().name(), "underrun-pipe-not-in-allow-list");
}

fn latched_regs() -> BTreeMap<u32, u32> {
    [
        (0x6001cu32, (2559u32 << 16) | 1439), // PIPESRC
        (0x68180, 0x9000_0000),               // PS_1A_CTRL
        (0x68174, (3840 << 16) | 2160),       // PS_WIN_SZ
        (0x70190, (1439 << 16) | 2559),       // PLANE_SIZE
        (0x70188, 160),                       // PLANE_STRIDE (64-byte units)
        (0x7027c, 859 << 16),                 // PLANE_BUF_CFG 0..859
        (0x70240, 0x8000_0010),               // PLANE_WM_1 level 0: enabled, 0 lines, 16 blocks
        (0x70244, 0x8000_4020),
    ]
    .into_iter()
    .collect()
}

#[test]
fn a_healthy_readback_is_one_line_with_underrun_0() {
    let regs = latched_regs();
    let (w, _) = UnderrunWatch::arm(Pipe::A, 0xffff_ffff, 0).unwrap();
    let h = read_health(|r| regs.get(&r).copied().unwrap_or(0), Pipe::A, 0, 0, Some(&w), 52);
    assert_eq!(h.underrun(), Some(false));
    let line = format!("{h}");
    assert_eq!(
        line,
        "[I915-SCANOUT-HEALTH pipe=A src=2560x1440 ps=ps1:ctrl=0x90000000,win=3840x2160 plane_size=2560x1440 stride=160 buf=0-859 wm0=en,lines=0,blocks=16 wm_levels=2 underrun=0 after_ms=52]"
    );
}

#[test]
fn an_underrun_latched_in_iir_reads_underrun_1_and_an_unwatched_pipe_says_why() {
    let mut regs = latched_regs();
    regs.insert(IIR_A, GEN8_PIPE_FIFO_UNDERRUN);
    let (w, _) = UnderrunWatch::arm(Pipe::A, 0xffff_ffff, 0).unwrap();
    let h = read_health(|r| regs.get(&r).copied().unwrap_or(0), Pipe::A, 0, 0, Some(&w), 52);
    assert_eq!(h.underrun(), Some(true));
    assert!(format!("{h}").contains(" underrun=1 after_ms=52]"));
    // no watch (refused by name): the line says the bit is not observable, never a 0 it did not see
    let h = read_health(|r| regs.get(&r).copied().unwrap_or(0), Pipe::A, 0, 0, None, 52);
    assert_eq!(h.underrun(), None);
    assert!(format!("{h}").contains(" underrun=unobserved after_ms=52]"));
}
