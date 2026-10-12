// SPDX-License-Identifier: GPL-2.0-only
//! Declared bounds, each with a test that fails if its check is removed: page tables per space (T), ops per plan, the spare pool.
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
fn the_table_table_is_bounded() {
    // T = 4: the PML4 plus one PDP, one PD, one PT
    let (mut g, _) = Gtt::<16, 4>::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap();
    let mut src = Pool::new(POOL0);
    g.plan_bind(&mut src, &bind(&[0x1000], 0)).unwrap();
    assert_eq!(g.table_count(), 4);
    // same PD, next PT: needs a fifth table
    assert_eq!(g.plan_bind(&mut src, &bind(&[0x1000], 0x20_0000)).err(), Some(MemError::TableTableFull { cap: 4 }));
    assert_eq!(g.table_count(), 4, "nothing changed");
    // an address inside the same PT still binds with no new table
    assert!(g.plan_bind(&mut src, &bind(&[0x1000], 0x1000)).is_ok());
}

#[test]
fn the_op_buffer_is_bounded_and_a_too_large_plan_is_rolled_back() {
    let (mut g, _) = Gtt::<16, 1024>::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap();
    let mut src = Pool::new(POOL0);
    // ~3 ops per fresh 2 MiB (fill, link, run): 100 PTs fit, 200 do not
    let ok: Vec<u64> = (0..100 * 512).map(|i| 0x1_0000_0000 + i * 4096).collect();
    let plan = g.plan_bind(&mut src, &bind(&ok, 0)).unwrap();
    assert!(plan.ops().len() <= MAX_OPS && plan.ops().len() > 300);
    let tables_after_ok = g.table_count();
    let too_big: Vec<u64> = (0..200 * 512).map(|i| 0x1_0000_0000 + i * 4096).collect();
    let e = g.plan_bind(&mut src, &bind(&too_big, 1 << 40)).err();
    assert_eq!(e, Some(MemError::PlanTooLarge { what: "ops", cap: MAX_OPS }));
    assert_eq!(MAX_OPS, 512);
    assert_eq!(g.table_count(), tables_after_ok, "the tables a failed plan made are forgotten");
    assert_eq!(g.live_nodes(), 1, "and so is the node");
    assert_eq!(g.spare_pages(), MAX_SPARE, "their pages are parked up to the spare cap");
    assert!(g.leaked_pages() > 0, "beyond the cap they are counted, not silently dropped");
    // a smaller bind afterwards reuses the parked pages before asking the source
    let before = src.asked.len();
    g.plan_bind(&mut src, &bind(&[0x1000], 1 << 41)).unwrap();
    assert_eq!(src.asked.len(), before, "3 tables came from the spare pool");
    assert_eq!(g.spare_pages(), MAX_SPARE - 3);
}

#[test]
fn the_vma_table_is_bounded_for_ppgtt_too() {
    let (mut g, _) = Gtt::<2, 64>::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap();
    let mut src = Pool::new(POOL0);
    let p = Placement::best();
    let r = |pg: &'static [u64]| BindRequest { place: p, ..bind(pg, 0) };
    g.plan_bind(&mut src, &r(&[0x1000])).unwrap();
    g.plan_bind(&mut src, &r(&[0x2000])).unwrap();
    assert_eq!(g.plan_bind(&mut src, &r(&[0x3000])).err(), Some(MemError::VmaTableFull { cap: 2 }));
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
        MemError::VmaTableFull { cap: 0 },
        MemError::TableTableFull { cap: 0 },
        MemError::PlanTooLarge { what: "", cap: 0 },
        MemError::TablePageRefused { level: 0 },
        MemError::BadFence { what: "", value: 0 },
        MemError::UnsupportedFlag { flag: 0 },
        MemError::NotFullPpgtt { size_bits: 0 },
        MemError::DuplicatePage { addr: 0 },
    ];
    let names: HashSet<&str> = all.iter().map(|e| e.name()).collect();
    assert_eq!(names.len(), all.len(), "26 variants, 26 names");
    assert_eq!(all[5].name(), "VA-BEYOND-48BIT");
    assert_eq!(all[14].name(), "STALE-MAPPING");
    assert_eq!(all[5].to_i915(), i915_common_core::I915Error::BadValue { what: "VA-BEYOND-48BIT", value: 0 });
}
