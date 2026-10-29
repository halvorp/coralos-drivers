// SPDX-License-Identifier: GPL-2.0-only
//! MOCS on a simulated GPU: the Gen9 control tables and the L3CC words are programmed as Linux programs them - `intel_mocs_init`
//! (gt/intel_mocs.c:666-685, from `intel_gt_init_hw`) and `intel_mocs_init_engine` (:631-649, from `execlists_resume`, so at engine
//! bring-up BEFORE the first request and again after every engine or full reset) - every word is read back, and a register that does not
//! hold what was written is named in the `[I915-MOCS-PROGRAM ...]` marker. Expected words are Linux's table values (intel_mocs.c:130-155)
//! written out by hand.
mod exec_common;
use exec_common::*;
use i915_device::exec::dma::{ObjKind, PAGE};
use i915_device::exec::engine::EngineRig;
use i915_device::exec::Executor;
use i915_gt_core::emit::FlushQuirks;
use i915_gt_core::engine::{engines_for, EngineClass, EngineInfo};
use i915_gt_core::mocs::mocs_settings;
use i915_gt_core::reset::ResetScope;
use i915_memory_core::probe::GgttGeometry;
use i915_memory_core::{BindRequest, CacheLevel, Gtt, NoTablePages, Placement};
use i915_submit_core::EngineConfig;

fn rcs() -> EngineInfo {
    *engines_for(&kbl().info).unwrap().as_slice().iter().find(|e| e.class == EngineClass::Render).unwrap()
}

/// The 96 words a KBL render-engine resume writes: `GEN9_GFX_MOCS(0..64)` then `GEN9_LNCFCMOCS(0..32)`.
fn linux_rcs_program() -> Vec<(u32, u32)> {
    let mut v = vec![];
    for i in 0..64u32 {
        let control = match i {
            0 => 0x9,  // I915_MOCS_UNCACHED: LE_1_UC | LE_TC_2_LLC_ELLC (intel_mocs.c:131-133)
            2 => 0x3b, // I915_MOCS_CACHED: LE_3_WB | LE_TC_2_LLC_ELLC | LE_LRUM(3) (:140-142)
            63 => 0x37, // mocs:63: LE_3_WB | LE_TC_1_LLC | LE_LRUM(3) (:152-154)
            _ => 0x30, // I915_MOCS_PTE, and every unused index (unused_entries_index = I915_MOCS_PTE, :461, :542-548)
        };
        v.push((0xc800 + i * 4, control));
    }
    for i in 0..32u32 {
        let word = match i {
            0 => 0x0030_0010,  // entry 0 L3_1_UC low, entry 1 L3_3_WB high (l3cc_combine, :602-605)
            31 => 0x0010_0030, // entry 62 (PTE) low, entry 63 L3_1_UC high
            _ => 0x0030_0030,
        };
        v.push((0xb020 + i * 4, word));
    }
    v
}

fn is_mocs(r: u32) -> bool {
    (0xc800..0xc900).contains(&r) || (0xb020..0xb0a0).contains(&r)
}

fn config(status_page_ggtt: u32) -> EngineConfig {
    EngineConfig { status_page_ggtt, quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, wa_ctx: None, stall_bound: 1000 }
}

struct W {
    chip: Chip,
    ex: Executor<Chip>,
    gm: Gm,
    log: Lines,
    rig: EngineRig,
}

/// A GGTT with the status page bound and the render engine's rig, NOT yet brought up.
fn rig_world() -> W {
    let (chip, _mem, mut ex, mut gm) = built();
    let mut log = Lines::default();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    ex.set_ggtt_window(i915_device::exec::allow::GgttWindow::for_table(geom.pte_base, geom.entries).unwrap()).unwrap();
    let scratch = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
    let (mut ggtt, create) = Gtt::new_ggtt(&kbl(), &geom, scratch.device_addr).unwrap();
    ex.run_bind_step(&mut log, &mut gm, &create, true, &mut |_| true).unwrap();
    let hwsp = gm.alloc.alloc_hwsp().unwrap();
    let pages: Vec<u64> = (0..hwsp.pages()).map(|i| hwsp.page(i)).collect();
    let req = BindRequest { pages: &pages, size: pages.len() as u64 * 4096, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place: Placement::best() };
    let plan = ggtt.plan_bind(&mut NoTablePages, &req).unwrap();
    ex.run_bind_step(&mut log, &mut gm, &plan, true, &mut |_| true).unwrap();
    let hwsp_ggtt = plan.offset() as u32;
    let rig = EngineRig::new(&rcs(), &kbl(), config(hwsp_ggtt), hwsp, hwsp_ggtt).unwrap();
    W { chip, ex, gm, log, rig }
}

fn mocs_writes(chip: &Chip, from: usize) -> Vec<(u32, u32)> {
    chip.all_writes()[from..].iter().copied().filter(|(r, _)| is_mocs(*r)).collect()
}

#[cfg(feature = "p3-exec")]
mod p3 {
    use super::*;

    #[test]
    fn engine_bring_up_programs_the_rcs_table_and_the_l3cc_words_before_execlists_are_enabled() {
        let mut w = rig_world();
        let before = w.chip.all_writes().len();
        w.rig.bring_up(&mut w.ex, &mut w.gm, &mut w.log).unwrap();
        assert_eq!(mocs_writes(&w.chip, before), linux_rcs_program(), "intel_mocs_init_engine(rcs0) = skl_mocs_table, entry by entry");
        let order: Vec<u32> = w.chip.all_writes()[before..].iter().map(|(r, _)| *r).collect();
        let first_mocs = order.iter().position(|r| is_mocs(*r)).unwrap();
        let last_mocs = order.iter().rposition(|r| is_mocs(*r)).unwrap();
        let enable = order.iter().position(|r| *r == 0x229c).unwrap();
        let csb = order.iter().rposition(|r| *r == 0x23a0).unwrap();
        assert!(csb < first_mocs && last_mocs < enable, "execlists_resume: intel_mocs_init_engine, then enable_execlists (:2952-2956)");
        assert!(w.log.has("[I915-MOCS-PROGRAM table=skl engine=rcs0 entries=64 l3cc=32 readback=ok]"), "{:?}", w.log.0);
    }

    #[test]
    fn the_gt_program_writes_the_l3cc_words_only_as_its_own_step() {
        let (chip, _mem, mut ex, _gm) = built();
        let mut log = Lines::default();
        let s = mocs_settings(&kbl()).unwrap().unwrap();
        let rep = ex.program_gt_mocs(&mut log, &s).unwrap();
        assert_eq!((rep.entries, rep.l3cc, rep.first_bad), (0, 32, None));
        assert_eq!(chip.all_writes(), linux_rcs_program()[64..].to_vec(), "intel_mocs_init on Gen9: init_l3cc_table only (:683-684)");
        assert!(log.has("[I915-STEP begun id=0x2005 name=MOCS-PROGRAM"));
        assert!(log.has("[I915-STEP done id=0x2005 name=MOCS-PROGRAM"));
        assert!(log.has("[I915-MOCS-PROGRAM table=skl engine=gt entries=0 l3cc=32 readback=ok]"), "{:?}", log.0);
    }

    #[test]
    fn an_engine_reset_programs_the_table_again() {
        let mut w = rig_world();
        w.rig.bring_up(&mut w.ex, &mut w.gm, &mut w.log).unwrap();
        // the reset clears the engine's MOCS (a chip that forgets them), then the resume must put them back
        for (r, _) in linux_rcs_program() {
            w.chip.set(r, 0);
        }
        let before = w.chip.all_writes().len();
        w.rig.recover(&mut w.ex, &mut w.gm, &mut w.log, ResetScope::Engines, &[]).unwrap();
        assert_eq!(mocs_writes(&w.chip, before), linux_rcs_program(), "execlists_resume after __intel_engine_reset_bh (intel_reset.c:1367)");
        for (r, v) in linux_rcs_program() {
            assert_eq!(w.chip.0.borrow().regs.get(&r).copied(), Some(v), "{r:#x}");
        }
        assert_eq!(w.log.find("[I915-MOCS-PROGRAM table=skl engine=rcs0").len(), 2, "bring-up + reset");
    }

    #[test]
    fn a_full_reset_programs_the_l3cc_words_and_the_rcs_table_again() {
        let mut w = rig_world();
        w.rig.bring_up(&mut w.ex, &mut w.gm, &mut w.log).unwrap();
        let engines: Vec<EngineInfo> = engines_for(&kbl().info).unwrap().as_slice().to_vec();
        let before = w.chip.all_writes().len();
        w.rig.recover(&mut w.ex, &mut w.gm, &mut w.log, ResetScope::AllEngines, &engines).unwrap();
        let mut want = linux_rcs_program()[64..].to_vec(); // intel_gt_init_hw -> intel_mocs_init (intel_reset.c:1264)
        want.extend(linux_rcs_program()); // resume(gt) -> execlists_resume(rcs0) (intel_reset.c:1270)
        assert_eq!(mocs_writes(&w.chip, before), want);
        assert!(w.log.has("[I915-MOCS-PROGRAM table=skl engine=gt entries=0 l3cc=32 readback=ok]"), "{:?}", w.log.0);
        assert!(w.log.has("[I915-STEP done id=0x3005 name=FULL-RESET"));
    }

    #[test]
    fn a_word_that_reads_back_differently_is_named_in_the_marker() {
        let mut w = rig_world();
        // MOCS 3 does not latch (reads back 0), and one L3CC word keeps a stale value
        w.chip.0.borrow_mut().on_write = Some(Box::new(|regs, r, _| {
            if r == 0xc80c {
                regs.insert(r, 0);
            }
            if r == 0xb040 {
                regs.insert(r, 0xdead);
            }
        }));
        w.rig.bring_up(&mut w.ex, &mut w.gm, &mut w.log).unwrap();
        assert!(
            w.log.has("[I915-MOCS-PROGRAM table=skl engine=rcs0 entries=64 l3cc=32 readback=MISMATCH bad=2 first_bad=0xc80c want=0x00000030 got=0x00000000]"),
            "{:?}",
            w.log.0
        );
    }
}

#[cfg(not(feature = "p3-exec"))]
#[test]
fn refused_without_the_feature_no_mocs_word_is_written() {
    let (chip, _mem, mut ex, _gm) = built();
    let mut log = Lines::default();
    let s = mocs_settings(&kbl()).unwrap().unwrap();
    assert_eq!(ex.program_gt_mocs(&mut log, &s).unwrap_err().name(), "WRITE-NOT-ALLOWED");
    assert!(chip.all_writes().is_empty());
    assert_eq!(ex.stats().first_refusal.unwrap().offset, 0xb020);
}
