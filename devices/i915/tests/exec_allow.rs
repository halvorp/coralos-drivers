// SPDX-License-Identifier: GPL-2.0-only
//! The compile-time allow-list. DEFAULT build: exactly the three forcewake SET registers and a closed GGTT window. With
//! `p2-memory` / `p3-exec`: exactly the registers THAT increment's planners write, proved by running the planners and requiring set
//! EQUALITY (a register no planner writes cannot be granted; a planner write that is not granted fails here, not on silicon).
//! Expected offsets are Linux's own, written out by hand.
mod exec_common;
use i915_device::exec::allow::*;
use i915_device::regio::BAR0_WRITE_RANGES;
#[cfg(not(feature = "p2-memory"))]
use i915_device::regio::BAR0_NEED_BYTES;

const FORCEWAKE_SET: [u32; 3] = [0xa188, 0xa270, 0xa278]; // intel_uncore.c:135-136 / intel_gt_regs.h: GT, MEDIA, RENDER Gen9

fn offsets(r: &[i915_common_core::Reg]) -> Vec<u32> {
    let mut v: Vec<u32> = r.iter().map(|r| r.offset()).collect();
    v.sort();
    v
}

#[test]
fn the_three_forcewake_set_registers_are_always_granted() {
    let all = offsets(&ALLOW);
    for r in FORCEWAKE_SET {
        assert!(all.contains(&r), "forcewake set {r:#x} must always be on the list");
    }
}

#[cfg(not(feature = "p2-memory"))]
#[test]
fn the_default_build_writes_only_forcewake_and_has_no_ggtt_window() {
    assert_eq!(offsets(&ALLOW), FORCEWAKE_SET);
    assert_eq!(N_ALLOW, 3);
    assert_eq!(compiled_ggtt_window(), None);
    assert_eq!(BAR0_WRITE_RANGES.len(), 3, "the window grant is the three registers, no GGTT range");
    assert_eq!(BAR0_NEED_BYTES, 0x14_59a0);
    assert!(!P2_ON && !P3_ON);
}

#[cfg(all(feature = "p2-memory", not(feature = "p3-exec")))]
#[test]
fn p2_adds_exactly_the_ggtt_invalidate_register_and_the_ggtt_window() {
    let mut want = FORCEWAKE_SET.to_vec();
    want.push(0x101008); // GFX_FLSH_CNTL_GEN6, intel_gt_regs.h:1474
    want.sort();
    assert_eq!(offsets(&ALLOW), want);
    assert_eq!(compiled_ggtt_window(), Some(GgttWindow { base: 0x80_0000, bytes: 0x80_0000 }));
    assert_eq!(BAR0_WRITE_RANGES.len(), 5);
    let last = BAR0_WRITE_RANGES[4];
    assert_eq!((last.off, last.len), (0x80_0000, 0x80_0000), "the GGTT window is the upper half of the 16 MiB BAR0 (intel_ggtt.c:1187-1192)");
    assert_eq!(i915_device::regio::BAR0_NEED_BYTES, 16 << 20);
}

#[cfg(feature = "p3-exec")]
#[test]
fn p3_adds_exactly_the_execlist_and_reset_registers() {
    let mut want = FORCEWAKE_SET.to_vec();
    want.push(0x101008);
    want.extend([
        0x2230,   // RING_ELSP(RENDER_RING_BASE 0x2000), intel_engine_regs.h:134 (+0x230)
        0x2098,   // RING_HWSTAM
        0x229c,   // RING_MODE
        0x209c,   // RING_MI_MODE
        0x2080,   // RING_HWS_PGA
        0x23a0,   // RING_CONTEXT_STATUS_PTR
        0x20d0,   // RING_RESET_CTL(RCS0)
        0x220d0,  // RING_RESET_CTL(BCS0, base 0x22000)
        0x120d0,  // RING_RESET_CTL(VCS0, base 0x12000)
        0x1a0d0,  // RING_RESET_CTL(VECS0, base 0x1a000)
        0x941c,   // GEN6_GDRST, intel_gt_regs.h:663
    ]);
    want.sort();
    assert_eq!(offsets(&ALLOW), want);
    assert_eq!(BAR0_WRITE_RANGES.len(), want.len() + 1);
}

/// The registers the planners WRITE, found by running them.
#[cfg(feature = "p3-exec")]
mod planner_write_sets {
    use super::*;
    use i915_common_core::Reg;
    use i915_devinfo_core::lookup;
    use i915_gt_core::engine::{engines_for, EngineClass};
    use i915_gt_core::reset::{Input, Output, ResetPlanner, ResetScope};
    use i915_gt_core::Effect;
    use i915_memory_core::probe::GgttGeometry;
    use i915_memory_core::{Flush, Gtt};
    use i915_submit_core::execlists::{elsp_writes, plan_enable_execlists, plan_reset_csb_pointers, Step};

    fn reset_writes(scope: ResetScope, engines: &[i915_gt_core::engine::EngineInfo]) -> Vec<Reg> {
        let mut p = ResetPlanner::new(scope, engines, 8).unwrap();
        let mut seen = vec![];
        let mut input = Input::Begin;
        for _ in 0..10_000 {
            input = match p.step(input).unwrap() {
                Output::Read(_) => Input::Read(0),
                Output::Write(w) => {
                    seen.push(w.reg);
                    Input::Wrote
                }
                Output::Poll(q) => Input::Polled(q.want),
                Output::DelayUs(_) => Input::Delayed,
                Output::Done(_) => return seen,
            };
        }
        panic!("reset planner did not finish");
    }

    #[test]
    fn the_allow_list_is_the_union_of_what_the_p2_and_p3_planners_write() {
        let d = lookup(0x591c).unwrap();
        let engines = engines_for(&d.info).unwrap();
        let rcs = *engines.as_slice().iter().find(|e| e.class == EngineClass::Render).unwrap();
        let mut writes: Vec<Reg> = vec![];
        // P2: the GGTT bind's flush
        let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
        let (_, plan) = Gtt::<8, 4>::new_ggtt(&d, &geom, 0x9000).unwrap();
        for f in plan.flushes() {
            if let Flush::GgttInvalidate(w) = f {
                writes.push(w.reg);
            }
        }
        // P3: submit-core
        writes.extend(elsp_writes(rcs.mmio_base, [1, 2]).iter().map(|w| w.reg));
        for e in plan_enable_execlists(rcs.mmio_base, 0x1000).unwrap().as_slice() {
            if let Effect::Write(w) = e {
                writes.push(w.reg);
            }
        }
        for s in plan_reset_csb_pointers(rcs.mmio_base).steps.as_slice() {
            if let Step::Mmio(Effect::Write(w)) = s {
                writes.push(w.reg);
            }
        }
        // P3: the reset planner, both scopes
        writes.extend(reset_writes(ResetScope::Engines, &[rcs]));
        writes.extend(reset_writes(ResetScope::AllEngines, engines.as_slice()));
        let mut set: Vec<u32> = writes.iter().map(|r| r.offset()).chain(FORCEWAKE_SET).collect();
        set.sort();
        set.dedup();
        assert_eq!(offsets(&ALLOW), set, "ALLOW must equal forcewake + the planners' write sets, no more and no less");
    }
}

#[test]
fn the_ggtt_window_holds_only_whole_aligned_entries_inside_it() {
    let w = GgttWindow { base: 0x80_0000, bytes: 0x1000 };
    assert!(w.holds_entry(0x80_0000));
    assert!(w.holds_entry(0x80_0ff8), "the last entry");
    assert!(!w.holds_entry(0x80_1000), "one past");
    assert!(!w.holds_entry(0x7f_fff8), "one before");
    assert!(!w.holds_entry(0x80_0004), "entries are 8-byte aligned");
    assert!(!w.holds_entry(u32::MAX - 3), "no overflow wraps into the window");
    // narrowed from the probed table, never wider than the half of BAR0
    assert_eq!(GgttWindow::for_table(0x80_0000, 0x40000), Some(GgttWindow { base: 0x80_0000, bytes: 0x20_0000 }));
    assert_eq!(GgttWindow::for_table(0x80_0000, 0x100001), None, "a table wider than the upper half of BAR0");
    assert_eq!(GgttWindow::for_table(0x70_0000, 8), None);
    assert_eq!(GgttWindow::for_table(0x80_0000, 0), None);
}
