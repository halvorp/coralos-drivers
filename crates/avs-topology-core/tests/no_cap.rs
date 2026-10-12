// SPDX-License-Identifier: GPL-2.0-only
//! RUNTIME-TABLES-HAVE-NO-CAP (DECISIONS.md D10): a topology FILE decides how many entries each table holds. The port's former
//! fixed capacities (templates 16, paths 32, pipelines 32, modules 64, routes 64, controls 16, pcms 8, plus the manifest
//! dictionaries, see boundaries.rs) are gone: this parses a synthetic topology with TEN TIMES each, and then proves that running out
//! of memory at ANY allocation of that parse is the named refusal `OutOfMemory`, never a panic or an abort.
mod common;
use avs_topology_core::error::TplgError;
use avs_topology_core::topology::Topology;
use common::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// Fails the n-th allocation made on this thread while armed (n counts down; 0 = disarmed). Const-initialised thread-locals do
/// not allocate, so the hook cannot recurse.
struct Failing;
thread_local! {
    static COUNTDOWN: Cell<u64> = const { Cell::new(0) };
    static SEEN: Cell<u64> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Failing {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if COUNTDOWN.with(|c| {
            let n = c.get();
            if n == 0 {
                return false;
            }
            SEEN.with(|s| s.set(s.get() + 1));
            c.set(n - 1);
            n == 1
        }) {
            return std::ptr::null_mut();
        }
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        if COUNTDOWN.with(|c| {
            let n = c.get();
            if n == 0 {
                return false;
            }
            c.set(n - 1);
            n == 1
        }) {
            return std::ptr::null_mut();
        }
        System.realloc(p, l, new)
    }
}
#[global_allocator]
static A: Failing = Failing;

/// 10x the old caps: 160 templates (old 16) each with one path (320 > 32 is reached by two paths per template), one pipeline per
/// path, two modules per pipeline (old 64 -> 640), 640 routes (old 64), 160 mixer controls on one widget (old 16), 80 PCMs (old 8).
fn big() -> Vec<u8> {
    let mut widgets = Vec::new();
    for t in 0..160u32 {
        let name = format!("w{t}");
        let mut p = tmpl_hdr(0, &name);
        for k in 0..2u32 {
            p.extend(cat(&[path_hdr(k, 0, 1), pipeline_hdr(k, 0, 0), module(0, 0, 0, 0), module(1, 0, 0, 0)]));
        }
        let ctls: Vec<Vec<u8>> = if t == 0 { (0..160).map(|i| mixer(&format!("m{i}"), 1, &[])).collect() } else { vec![] };
        widgets.extend(widget(&name, &p, &ctls));
    }
    let routes: Vec<(String, String, String)> = (0..640).map(|i| (format!("s{i}"), String::new(), format!("r{i}"))).collect();
    let r: Vec<(&str, &str, &str)> = routes.iter().map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str())).collect();
    let pcms: Vec<Vec<u8>> = (0..80u32).map(|i| pcm(&format!("p{i}"), &format!("d{i}"), i, 1, 0, &[])).collect();
    cat(&[manifest_block(&small_manifest()), block(5, 160, &widgets), block(7, 80, &cat(&pcms)), block(4, 640, &graph(&r))])
}

#[test]
fn ten_times_every_old_cap_parses_with_every_count_intact() {
    let bytes: &'static [u8] = Box::leak(big().into_boxed_slice());
    let t = Topology::parse(bytes, None).expect("ten times the old caps");
    assert_eq!(t.templates.len(), 160);
    assert_eq!(t.paths.len(), 320);
    assert_eq!(t.pipelines.len(), 320);
    assert_eq!(t.modules.len(), 640);
    assert_eq!(t.routes.len(), 640);
    assert_eq!(t.controls.len(), 160);
    assert_eq!(t.pcms.len(), 80);
}

#[test]
fn running_out_of_memory_at_any_allocation_is_the_named_refusal() {
    let bytes: &'static [u8] = Box::leak(big().into_boxed_slice());
    let clean = Topology::parse(bytes, None).unwrap();
    let (mut refused, mut k) = (0u32, 1u64);
    loop {
        COUNTDOWN.with(|c| c.set(k));
        let r = Topology::parse(bytes, None);
        COUNTDOWN.with(|c| c.set(0));
        match r {
            Ok(t) => {
                // k is past the last allocation: the parse is complete and identical to the unfailed one
                assert_eq!((t.templates.len(), t.paths.len(), t.modules.len(), t.routes.len()), (clean.templates.len(), clean.paths.len(), clean.modules.len(), clean.routes.len()));
                break;
            }
            Err(TplgError::OutOfMemory { what }) => {
                assert!(!what.is_empty());
                refused += 1;
            }
            Err(e) => panic!("allocation {k} failed -> {e:?}, not OutOfMemory"),
        }
        k += 1;
        assert!(k < 100_000, "never converged");
    }
    assert!(refused > 20, "only {refused} allocation points were exercised");
}
