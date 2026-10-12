// SPDX-License-Identifier: GPL-2.0-only
//! The former fixed capacities (VMA table, page-table table, ops per plan, spare pool) are GONE (RUNTIME-TABLES-HAVE-NO-CAP): each
//! test drives the population well past the old cap. The allocation-failure side is in no_cap.rs.
mod common;
use common::*;
use i915_memory_core::*;

fn bind(pages: &[u64], at: u64) -> BindRequest<'_> {
    BindRequest {
        pages,
        size: pages.len() as u64 * 4096,
        alignment: 0,
        guard: 0,
        cache: CacheLevel::Llc,
        flags: 0,
        place: Placement::with_mode(Mode::Fixed(at)),
    }
}

#[test]
fn the_page_table_table_grows_past_the_old_cap() {
    // RUNTIME-TABLES-HAVE-NO-CAP: the old T was 1024. One page per fresh 2 MiB needs one PT each; 3000 of them is ~3x the old cap.
    let (mut g, _) = Gtt::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap();
    let mut src = Pool::new(POOL0);
    for i in 0..3000u64 {
        g.plan_bind(&mut src, &bind(&[0x1000], i * 0x20_0000)).unwrap();
    }
    assert!(g.table_count() > 3000, "got {}", g.table_count());
    assert_eq!(g.live_nodes(), 3000);
    // an address inside an already-mapped PT still binds with no new table
    let before = g.table_count();
    g.plan_bind(&mut src, &bind(&[0x1000], 0x1000)).unwrap();
    assert_eq!(g.table_count(), before);
}

#[test]
fn a_plan_grows_past_the_old_op_buffer_and_every_rolled_back_table_is_parked() {
    let (mut g, _) = Gtt::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap();
    let mut src = Pool::new(POOL0);
    // ~3 ops per fresh 2 MiB (fill, link, run): the old MAX_OPS was 512, so 200 PTs (~600 ops) was refused
    let big: Vec<u64> = (0..200 * 512).map(|i| 0x1_0000_0000 + i * 4096).collect();
    let plan = g.plan_bind(&mut src, &bind(&big, 0)).unwrap();
    assert!(plan.ops().len() > 512, "got {} ops", plan.ops().len());
    // a failed plan rolls back: the source gives 150 pages then refuses, so 150 tables were made and must ALL be parked
    // (the old spare cap was 64 and the rest were counted as leaked)
    let tables = g.table_count();
    let mut limited = Pool::new(0x4000_0000);
    limited.limit = 150;
    let e = g.plan_bind(&mut limited, &bind(&big, 1 << 40)).err();
    assert!(matches!(e, Some(MemError::TablePageRefused { .. })));
    assert_eq!(g.table_count(), tables, "the tables a failed plan made are forgotten");
    assert_eq!(g.live_nodes(), 1, "and so is the node");
    assert_eq!(g.spare_pages(), 150, "every granted page is parked: no spare cap");
    // a smaller bind afterwards reuses the parked pages before asking the source
    let before = limited.asked.len();
    g.plan_bind(&mut limited, &bind(&[0x1000], 1 << 41)).unwrap();
    assert_eq!(limited.asked.len(), before, "3 tables came from the spare pool");
    assert_eq!(g.spare_pages(), 147);
}

#[test]
fn the_vma_table_grows_past_the_old_cap_for_ppgtt_too() {
    let (mut g, _) = Gtt::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap();
    let mut src = Pool::new(POOL0);
    let p = Placement::best();
    for _ in 0..500 {
        g.plan_bind(&mut src, &BindRequest { place: p, ..bind(&[0x1000], 0) }).unwrap();
    }
    assert_eq!(g.live_nodes(), 500);
}

#[test]
fn every_error_has_a_distinct_stable_name() {
    use std::collections::HashSet;
    let all = [
        MemError::UnsupportedGeneration { gen: 8 },
        MemError::GgttAbsent { gmch_ctl: 0 },
        MemError::Bar0Size { got: 0, want: 0 },
        MemError::GgttOverflowsBar { table: 0 },
        MemError::StolenReservedOutside { reserved_base: 0, base: 0, top: 0 },
        MemError::Beyond48Bit { va: 0 },
        MemError::BeyondSpace { va: 0, len: 0, total: 0 },
        MemError::Misaligned { what: "", value: 0, align: 0 },
        MemError::BadAlignment { align: 0 },
        MemError::ZeroLength,
        MemError::PagesExceedNode { pages: 0, node_pages: 0 },
        MemError::DeviceAddrBeyondMask { addr: 0, bits: 0 },
        MemError::NoSpace { size: 0 },
        MemError::RangeOccupied { start: 0, len: 0 },
        MemError::StaleMapping { va: 0 },
        MemError::GuardPageAccess { va: 0 },
        MemError::NotBound { id: 0 },
        MemError::Pinned { id: 0 },
        MemError::OutOfMemory { what: "" },
        MemError::TablePageRefused { level: 0 },
        MemError::BadFence { what: "", value: 0 },
        MemError::UnsupportedFlag { flag: 0 },
        MemError::NotFullPpgtt { size_bits: 0 },
        MemError::DuplicatePage { addr: 0 },
    ];
    let names: HashSet<&str> = all.iter().map(|e| e.name()).collect();
    assert_eq!(names.len(), all.len(), "24 variants, 24 names");
    assert_eq!(all[5].name(), "VA-BEYOND-48BIT");
    assert_eq!(all[14].name(), "STALE-MAPPING");
    assert_eq!(all[5].to_i915(), i915_common_core::I915Error::BadValue { what: "VA-BEYOND-48BIT", value: 0 });
}
