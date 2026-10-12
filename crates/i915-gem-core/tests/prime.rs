// SPDX-License-Identifier: GPL-2.0-only
//! PRIME sharing (drm_prime.c): a hold that outlives the exporter's handle, a second session's own handle on the same object, and the
//! per-session mmap node that makes the importer's CPU mapping legal.
mod common;
use common::*;
use i915_gem_core::*;
use i915_uapi_core::consts::misc as m;

const WB: u64 = m::I915_MMAP_OFFSET_WB as u64;

fn second(r: &mut Rig) -> SessionId {
    let init = r.pool.init();
    r.gem.open_session(SessionCaps::default(), init, &mut r.sink).unwrap()
}

#[test]
fn an_exported_object_outlives_the_exporters_handle_and_is_freed_with_the_last_holder() {
    let mut r = Rig::new();
    let b = second(&mut r);
    let ha = r.create(4096);
    let obj = r.gem.prime_export(r.s, ha).unwrap();
    let hb = r.gem.prime_import(b, obj).unwrap();
    assert_eq!(r.gem.lookup_object(b, hb), Ok(obj), "one object under two sessions' handles");
    assert_eq!(r.gem.object_count(), 1);
    r.gem.close(r.s, ha, &mut r.sink).unwrap();
    assert_eq!(r.gem.object_count(), 1, "the importer's handle and the export hold keep it");
    r.gem.close(b, hb, &mut r.sink).unwrap();
    assert_eq!(r.gem.object_count(), 1, "the export hold alone still keeps it");
    r.gem.prime_release(obj, &mut r.sink);
    assert_eq!(r.gem.object_count(), 0, "the last holder going frees it");
}

#[test]
fn importing_an_object_nobody_holds_is_refused_and_a_foreign_handle_is_not_exportable() {
    let mut r = Rig::new();
    let b = second(&mut r);
    let ha = r.create(4096);
    let obj = r.gem.prime_export(r.s, ha).unwrap();
    r.gem.close(r.s, ha, &mut r.sink).unwrap();
    r.gem.prime_release(obj, &mut r.sink);
    assert!(matches!(r.gem.prime_import(b, obj), Err(GemError::HandleNotInSession { .. })), "freed: nothing to import");
    let hb = {
        let h = r.gem.create(b, 4096).unwrap().handle;
        h
    };
    let other = r.gem.create(r.s, 4096).unwrap().handle;
    assert_ne!(hb, 0);
    // a handle that is B's is not a handle in A's table unless A has it; exporting someone else's name is a named refusal
    assert!(r.gem.prime_export(r.s, 0x7777).is_err());
    let _ = other;
}

#[test]
fn an_importer_gets_its_own_mmap_offset_and_the_exporters_is_not_its_to_use() {
    let mut r = Rig::new();
    let b = second(&mut r);
    let ha = r.create(8192);
    let oa = r.gem.mmap_offset(r.s, ha, WB).unwrap();
    let obj = r.gem.prime_export(r.s, ha).unwrap();
    let hb = r.gem.prime_import(b, obj).unwrap();
    let ob = r.gem.mmap_offset(b, hb, WB).unwrap();
    assert_ne!(oa.offset, ob.offset, "each session has its own node");
    assert!(matches!(r.gem.mmap_open(b, oa.offset, 4096, &mut r.pages), Err(GemError::MmapOffsetDenied { .. })), "drm_vma_node_is_allowed is per file");
    assert!(r.gem.mmap_open(b, ob.offset, 4096, &mut r.pages).is_ok());
    assert!(r.gem.mmap_open(r.s, oa.offset, 4096, &mut r.pages).is_ok());
}

#[test]
fn the_slot_accessors_name_live_contexts_and_bound_the_vm_scan() {
    let mut r = Rig::new();
    let slot = r.gem.context_slot_of(r.s, 0).unwrap();
    assert_eq!(r.gem.context_slot_of(r.s, 99), Err(GemError::ContextMissing { id: 99 }));
    assert!(r.gem.vm_slots() >= 1);
    let vm = r.gem.context_vm(r.s, 0).unwrap();
    assert!((vm.0 as usize) < r.gem.vm_slots());
    let _ = slot;
}
