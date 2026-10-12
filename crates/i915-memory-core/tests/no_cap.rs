// SPDX-License-Identifier: GPL-2.0-only
//! RUNTIME-TABLES-HAVE-NO-CAP, the allocation-failure side: every growth is fallible. A global allocator that can be told to fail
//! the n-th allocation of THIS thread drives each planner through every failure point; the call must either succeed or refuse with
//! the named `OUT-OF-MEMORY`, and a refusal must leave the space exactly as it was (no node, no table, no victim removed).
//! Any growth that is not fallible would abort the test binary here instead of refusing.
mod common;
use common::*;
use i915_memory_core::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// 0 = never fail; n > 0 = the n-th allocation from now fails (and every one after it).
    static FAIL_AT: Cell<u32> = const { Cell::new(0) };
    static SEEN: Cell<u32> = const { Cell::new(0) };
}

struct Flaky;
unsafe impl GlobalAlloc for Flaky {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let fail = FAIL_AT.with(|f| {
            let at = f.get();
            if at == 0 {
                return false;
            }
            SEEN.with(|s| s.set(s.get() + 1));
            SEEN.with(|s| s.get()) >= at
        });
        if fail {
            return std::ptr::null_mut();
        }
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let fail = FAIL_AT.with(|f| {
            let at = f.get();
            if at == 0 {
                return false;
            }
            SEEN.with(|s| s.set(s.get() + 1));
            SEEN.with(|s| s.get()) >= at
        });
        if fail {
            return std::ptr::null_mut();
        }
        System.realloc(p, l, n)
    }
}
#[global_allocator]
static A: Flaky = Flaky;

/// Run `f` with the `n`-th allocation (1-based) failing. Disarmed again before returning.
fn failing_at<R>(n: u32, f: impl FnOnce() -> R) -> R {
    SEEN.with(|s| s.set(0));
    FAIL_AT.with(|a| a.set(n));
    let r = f();
    FAIL_AT.with(|a| a.set(0));
    r
}

/// A table-page source that itself never allocates (the shared `Pool` records requests in a Vec, which would trip the injector).
struct Seq(u64);
impl TablePageSource for Seq {
    fn table_page(&mut self, _: TablePageRequest) -> Result<TablePage, MemError> {
        self.0 += 0x1000;
        Ok(TablePage { device_addr: self.0 })
    }
}

fn bind(pages: &[u64], at: u64, evict: bool) -> BindRequest<'_> {
    let mut place = Placement::with_mode(Mode::Fixed(at));
    place.evict = evict;
    BindRequest { pages, size: pages.len() as u64 * 4096, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place }
}

#[test]
fn a_ppgtt_bind_refuses_by_name_at_every_allocation_and_changes_nothing() {
    let (mut g, _) = Gtt::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap();
    let mut src = Pool::new(POOL0);
    // well past every old cap: 1100+ page tables, 1100 VMAs
    for i in 0..1100u64 {
        g.plan_bind(&mut src, &bind(&[0x1000], i * 0x20_0000, false)).unwrap();
    }
    assert!(g.table_count() > 1100);
    let pages: Vec<u64> = (0..1500).map(|i| 0x1_0000_0000 + i * 4096).collect(); // 3 PTs, > the old 512-op... per plan growth
    let (nodes, tables) = (g.live_nodes(), g.table_count());
    let mut s2 = Seq(0x4000_0000); // one source across attempts: parked pages must not collide with fresh ones
    let mut refused = 0;
    let mut n = 1;
    loop {
        let r = failing_at(n, || g.plan_bind(&mut s2, &bind(&pages, 1 << 40, false)));
        match r {
            Ok(_) => break,
            Err(e) => {
                assert_eq!(e, MemError::OutOfMemory { what: e_what(&e) }, "failure point {n}");
                assert_eq!(e.name(), "OUT-OF-MEMORY");
                assert_eq!((g.live_nodes(), g.table_count()), (nodes, tables), "failure point {n}: nothing half-created");
                refused += 1;
            }
        }
        n += 1;
        assert!(n < 200, "the bind never succeeded");
    }
    assert!(refused >= 3, "the sweep must have hit real allocation points, got {refused}");
    assert_eq!(g.live_nodes(), nodes + 1);
}

fn e_what(e: &MemError) -> &'static str {
    match e {
        MemError::OutOfMemory { what } => what,
        other => panic!("expected OUT-OF-MEMORY, got {other:?}"),
    }
}

#[test]
fn a_ggtt_eviction_refuses_by_name_at_every_allocation_and_evicts_nothing() {
    let (mut g, _) = Gtt::new_ggtt(&kbl(), &geom_4g(), 0x9000).unwrap();
    let pg = [0x10_0000u64; 1];
    for i in 0..300u64 {
        g.plan_bind(&mut NoTablePages, &bind(&pg, 0x10_0000 + i * 4096, false)).unwrap();
    }
    let big_pages = [0x20_0000u64; 1];
    let nodes = g.live_nodes();
    let mut n = 1;
    let mut refused = 0;
    loop {
        let req = BindRequest { size: 300 * 4096, ..bind(&big_pages, 0x10_0000, true) };
        let r = failing_at(n, || g.plan_bind(&mut NoTablePages, &req));
        match r {
            Ok(p) => {
                assert_eq!(p.evicted().len(), 300);
                break;
            }
            Err(e) => {
                e_what(&e);
                assert_eq!(g.live_nodes(), nodes, "failure point {n}: no victim was removed");
                refused += 1;
            }
        }
        n += 1;
        assert!(n < 200);
    }
    assert!(refused >= 2, "got {refused}");
    assert_eq!(g.live_nodes(), 2);
}

#[test]
fn an_unconstrained_placement_scan_refuses_by_name_when_its_scratch_lists_cannot_be_had() {
    let (mut g, _) = Gtt::new_ggtt(&kbl(), &geom_4g(), 0x9000).unwrap();
    let pg = [0x10_0000u64; 1];
    for _ in 0..200 {
        g.plan_bind(&mut NoTablePages, &BindRequest { place: Placement::best(), ..bind(&pg, 0, false) }).unwrap();
    }
    let nodes = g.live_nodes();
    // allocation #1 of a non-fixed bind is the scan's mark list
    let r = failing_at(1, || g.plan_bind(&mut NoTablePages, &BindRequest { place: Placement::best(), ..bind(&pg, 0, false) }));
    assert_eq!(r.err(), Some(MemError::OutOfMemory { what: "scratch" }));
    assert_eq!(g.live_nodes(), nodes);
}

#[test]
fn creating_a_ppgtt_refuses_by_name_at_every_allocation() {
    let mut refused = 0;
    for n in 1..50 {
        match failing_at(n, || Gtt::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S })) {
            Ok(_) => break,
            Err(e) => {
                e_what(&e);
                refused += 1;
            }
        }
    }
    assert!(refused >= 2, "got {refused}");
}
