// SPDX-License-Identifier: GPL-2.0-only
//! Plane / watermark / cursor effect plans and the restore helpers.
mod common;
use common::*;
use i915_common_core::Reg;
use i915_display_link_core::plan::Effect;
use i915_display_modeset_core::plan::*;
use i915_display_modeset_core::plane_plan::*;
use i915_kbl_display_core::skl_plane::*;
use i915_kbl_display_core::skl_wm::*;
use i915_kbl_display_core::types::Pipe;

fn w(reg: u32, value: u32) -> Effect {
    Effect::Write { reg: Reg::new(reg), value }
}

fn level(blocks: u16, lines: u16) -> WmLevel {
    WmLevel { enable: true, ignore_lines: false, blocks, lines, min_ddb_alloc: blocks + 1 }
}

#[test]
fn wm_block_is_levels_then_trans_then_buf_cfgs() {
    let mut pw = PlaneWm::default();
    for l in 0..8 {
        pw.wm[l] = level(10 + l as u16, l as u16);
    }
    let ddb = Ddb { start: 0, end: 860 };
    let ddb_y = Ddb { start: 100, end: 200 };
    let b = plane_wm_writes(Pipe::A, 0, &pw, &ddb, &ddb_y);
    let regs: Vec<u32> = b.as_slice().iter().map(|x| x.0).collect();
    assert_eq!(regs, [0x70240, 0x70244, 0x70248, 0x7024c, 0x70250, 0x70254, 0x70258, 0x7025c, 0x70268, 0x7027c, 0x70278]);
    assert_eq!(b.as_slice()[0].1, 0x8000_000a);
    assert_eq!(b.as_slice()[9].1, 0x035b_0000, "PLANE_BUF_CFG = plane_ddb");
    assert_eq!(b.as_slice()[10].1, 0x00c7_0064, "PLANE_NV12_BUF_CFG = plane_ddb_y: END(199) | START(100)");
    // pipe B plane 2
    let b = plane_wm_writes(Pipe::B, 1, &pw, &ddb, &ddb_y);
    assert_eq!(b.as_slice()[0].0, 0x71340);
    assert_eq!(b.as_slice()[10].0, 0x71378);
}

#[test]
fn commit_splits_noarm_and_arm_and_waits_on_surflive() {
    let regs = encode_plane(&plane_cfg()).unwrap();
    let wm = plane_wm_writes(Pipe::A, 0, &PlaneWm::default(), &Ddb::default(), &Ddb::default());
    let c = plane_commit(&regs, &wm);
    assert_eq!(c.noarm.len(), 3 + 11);
    assert_eq!(c.arm.len(), 8 + 1);
    assert_eq!(c.arm.last(), Some(&Effect::PostingRead { reg: Reg::new(0x7019c) }));
    assert_eq!(c.arm[7], w(0x7019c, 0x0040_0000), "PLANE_SURF is the last WRITE");
    assert_eq!((c.latch.reg.offset(), c.latch.mask, c.latch.want), (0x701ac, 0xffff_f000, 0x0040_0000));
    assert_eq!(c.latch.max_polls, LATCH_TIMEOUT_US / LATCH_INTERVAL_US + 1);
}

#[test]
fn disable_writes_watermarks_first_then_ctl_then_surf() {
    let wm = plane_wm_writes(Pipe::A, 0, &PlaneWm::default(), &Ddb::default(), &Ddb::default());
    let p = plane_disable_plan(Pipe::A, 0, &wm);
    assert_eq!(p.len(), 11 + 2);
    assert_eq!(&p[11..], &[w(0x70180, 0), w(0x7019c, 0)]);
}

#[test]
fn cursor_block_for_a_hidden_cursor() {
    let pm = PipeWm {
        planes: [PlaneWm::default(); 3],
        plane_ddb: [Ddb::default(); 3],
        plane_ddb_y: [Ddb::default(); 3],
        cursor_ddb: Ddb { start: 860, end: 892 },
        max_level: 7,
    };
    let p = cursor_hidden_plan(Pipe::A, &pm);
    let regs: Vec<u32> = p
        .iter()
        .map(|e| match e {
            Effect::Write { reg, .. } => reg.offset(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(regs, [0x70140, 0x70144, 0x70148, 0x7014c, 0x70150, 0x70154, 0x70158, 0x7015c, 0x70168, 0x7017c, 0x70088, 0x70084]);
    assert_eq!(p[9], w(0x7017c, 0x037b_035c));
    // a pipe B cursor sits one pipe block higher
    let q = cursor_hidden_plan(Pipe::B, &pm);
    assert_eq!(q[0], w(0x71140, 0));
    assert_eq!(q[11], w(0x71084, 0));
}

#[test]
fn restore_plan_is_reverse_first_write_order_once_per_register() {
    let plan = vec![w(0x10, 1), w(0x20, 2), Effect::Rmw { reg: Reg::new(0x10), clear: 1, set: 2 }, Effect::PostingRead { reg: Reg::new(0x30) }, w(0x40, 3)];
    let snap = |r: u32| r + 0x1000;
    let rb = restore_plan(&plan, &snap);
    assert_eq!(rb, vec![w(0x40, 0x1040), w(0x20, 0x1020), w(0x10, 0x1010)]);
    assert!(restore_plan(&vec![], &snap).is_empty());
    // restore_all concatenates first
    let a = vec![w(0x10, 1)];
    let b = vec![w(0x20, 1), w(0x10, 2)];
    assert_eq!(restore_all(&[&a, &b], &snap), vec![w(0x20, 0x1020), w(0x10, 0x1010)]);
}

#[test]
fn readbacks_check_the_last_write_of_each_register() {
    let plan = vec![w(0x10, 0xf0), w(0x20, 0), w(0x10, 0x0f), Effect::PostingRead { reg: Reg::new(0x10) }];
    let rb = readbacks_of(&plan, false);
    assert_eq!(rb, vec![Readback { reg: Reg::new(0x10), mask: 0x0f, want: 0x0f }], "the zero write has no set bits to check, and only the LAST write of 0x10 counts");
    let full = readbacks_of(&plan, true);
    assert_eq!(full.len(), 2, "a zero word is checked when the whole word is defined");
    assert!(full.contains(&Readback { reg: Reg::new(0x20), mask: u32::MAX, want: 0 }));
    assert!(full.contains(&Readback { reg: Reg::new(0x10), mask: u32::MAX, want: 0x0f }));
}

#[test]
fn body_helpers_see_writes_and_polls() {
    use i915_display_link_core::plan::PollPlan;
    let p: Plan = vec![w(0x10, 1), Effect::Poll(PollPlan::with_timeout(Reg::new(0x20), 1, 1, 10, 100, "x")), Effect::Rmw { reg: Reg::new(0x30), clear: 0, set: 1 }];
    let b = Body::Link(p);
    assert_eq!(b.written_regs(), vec![Reg::new(0x10), Reg::new(0x30)]);
    assert_eq!(b.polls(), vec![(11, "x")]);
    assert!(Body::Empty("why").written_regs().is_empty());
    assert!(Body::Interactive(Interactive::LinkTraining).polls().is_empty());
    for i in [Interactive::PpsPanelOn, Interactive::PpsPanelOff, Interactive::PpsBacklightOn, Interactive::PpsBacklightOff, Interactive::SinkPowerD0, Interactive::SinkPowerD3, Interactive::LinkTraining] {
        assert!(!i.name().is_empty());
    }
}
