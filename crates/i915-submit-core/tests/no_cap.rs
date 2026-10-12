// SPDX-License-Identifier: GPL-2.0-only
//! RUNTIME-TABLES-HAVE-NO-CAP, the allocation-failure side: the context table and the request table grow fallibly. A global
//! allocator that fails the n-th allocation of THIS thread proves that a refusal is the named `OUT-OF-MEMORY` and that nothing
//! half-exists afterwards (no context slot, no request, ring and timeline untouched). An infallible growth would abort here.
use i915_devinfo_core::lookup;
use i915_gt_core::emit::FlushQuirks;
use i915_gt_core::engine::{engines_for, EngineClass};
use i915_submit_core::engine::*;
use i915_submit_core::SubmitError;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static FAIL_AT: Cell<u32> = const { Cell::new(0) };
    static SEEN: Cell<u32> = const { Cell::new(0) };
}

struct Flaky;
fn should_fail() -> bool {
    FAIL_AT.with(|f| {
        let at = f.get();
        if at == 0 {
            return false;
        }
        let n = SEEN.with(|s| {
            s.set(s.get() + 1);
            s.get()
        });
        n >= at
    })
}
unsafe impl GlobalAlloc for Flaky {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if should_fail() {
            return std::ptr::null_mut();
        }
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if should_fail() {
            return std::ptr::null_mut();
        }
        System.realloc(p, l, n)
    }
}
#[global_allocator]
static A: Flaky = Flaky;

fn failing_at<R>(n: u32, f: impl FnOnce() -> R) -> R {
    SEEN.with(|s| s.set(0));
    FAIL_AT.with(|a| a.set(n));
    let r = f();
    FAIL_AT.with(|a| a.set(0));
    r
}

fn engine() -> Engine {
    let d = lookup(0x591c).unwrap();
    let info = engines_for(&d.info).unwrap().as_slice().iter().copied().find(|e| e.class == EngineClass::Render).unwrap();
    let cfg = EngineConfig { status_page_ggtt: 0x4_0000, quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, wa_ctx: None, stall_bound: 1000 };
    Engine::new(&info, 9, cfg).unwrap()
}

fn spec(n: u32) -> ContextSpec<'static> {
    ContextSpec {
        lrca: 0x0100_0000 + n * 0x0001_0000,
        ring_size: 4096,
        ring_ggtt: 0x0200_0000 + n * 0x1000,
        pml4: 0x1_0000_0000 + n as u64 * 0x1000,
        rpcs: 0,
        hwsp_ggtt: 0x0003_0000 + n * 0x40,
        initial_breadcrumb: true,
        default_state: None,
    }
}
const BATCH: BatchSpec = BatchSpec { offset: 0x2345_6000, flags: 0, nopreempt: false };

#[test]
fn creating_a_context_refuses_by_name_when_the_table_cannot_grow() {
    let mut e = engine();
    // 520 live contexts: far past any old cap. Every create runs with the first allocation failing: it either needs none (spare
    // capacity) or is refused by name, and the retry then succeeds.
    let mut refused = 0;
    for n in 0..520 {
        let r = failing_at(1, || e.create_context(&spec(n)));
        match r {
            Ok(_) => {} // a spare slot of capacity existed: no allocation was needed
            Err(err) => {
                assert_eq!(err, SubmitError::OutOfMemory { what: "context table" });
                assert_eq!(err.name(), "OUT-OF-MEMORY");
                assert!(e.context_lrca(n).is_err(), "no context slot was created");
                refused += 1;
                // and the very same create succeeds once memory is back
                assert_eq!(e.create_context(&spec(n)).unwrap(), n);
            }
        }
    }
    assert!(refused >= 5, "the table must have needed to grow under injection repeatedly, got {refused}");
    assert!(e.context_lrca(519).is_ok());
}

#[test]
fn building_a_request_refuses_by_name_when_the_table_cannot_grow_and_leaves_the_ring_alone() {
    let mut e = engine();
    let mut mem = vec![0u32; 1024];
    let ctxs: Vec<u32> = (0..100).map(|n| e.create_context(&spec(n)).unwrap()).collect();
    let mut refused = 0;
    for round in 0..8 {
        for &c in &ctxs {
            let (emit0, seq0) = (e.context_ring(c).unwrap().emit(), e.context_timeline(c).unwrap().seqno());
            let n_reqs = e.requests().len();
            match failing_at(1, || e.build_request(c, &mut mem, &BATCH)) {
                Ok(_) => {}
                Err(err) => {
                    assert_eq!(err, SubmitError::OutOfMemory { what: "request table" });
                    assert_eq!(e.requests().len(), n_reqs, "no request was created");
                    assert_eq!((e.context_ring(c).unwrap().emit(), e.context_timeline(c).unwrap().seqno()), (emit0, seq0), "ring and timeline untouched");
                    refused += 1;
                    if round < 7 {
                        e.build_request(c, &mut mem, &BATCH).unwrap();
                    }
                }
            }
        }
        if e.requests().len() > 600 {
            break;
        }
    }
    assert!(refused >= 3, "got {refused}");
    assert!(e.requests().len() > 300, "well past the old caps of 3 / 16 requests, got {}", e.requests().len());
}
