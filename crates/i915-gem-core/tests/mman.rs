// SPDX-License-Identifier: GPL-2.0-only
//! CPU-mapping policy: which mmap type each object may use on this device, offsets and who may use them, and leases.
mod common;
use common::*;
use i915_gem_core::limits::*;
use i915_gem_core::*;
use i915_uapi_core::consts::misc as m;

type R = Rig<16, 16>;

const GTT: u64 = m::I915_MMAP_OFFSET_GTT as u64;
const WC: u64 = m::I915_MMAP_OFFSET_WC as u64;
const WB: u64 = m::I915_MMAP_OFFSET_WB as u64;
const UC: u64 = m::I915_MMAP_OFFSET_UC as u64;
const FIXED: u64 = m::I915_MMAP_OFFSET_FIXED as u64;

fn rig_with(pat: bool, aperture: bool) -> R {
    let mut d = dev();
    d.ggtt_has_aperture = aperture;
    let mut gem: Gem<16, 16> = Gem::new(d, GemConfig { pat_enabled: pat });
    let mut sink = Sink::default();
    let mut pool = Pool::new();
    let init = pool.init();
    let s = gem.open_session(SessionCaps::default(), init, &mut sink).unwrap();
    let mut eng = Engines::new();
    eng.attach(&mut gem, s, 0);
    Rig { gem, s, sink, pages: Pages::new(), pool, eng, sync: Sync::default() }
}

#[test]
fn each_mmap_type_maps_to_its_cpu_cache_mode_on_kbl() {
    let mut r = R::new();
    let h = r.create(8192);
    let kinds = [(GTT, MmapKind::Gtt, CpuCache::Aperture), (WC, MmapKind::Wc, CpuCache::Wc), (WB, MmapKind::Wb, CpuCache::Wb), (UC, MmapKind::Uc, CpuCache::Uc)];
    let mut offsets = vec![];
    for (flag, kind, cache) in kinds {
        let g = r.gem.mmap_offset(r.s, h, flag).unwrap();
        assert_eq!((g.kind, g.cache), (kind, cache), "flag {flag}");
        offsets.push(g.offset);
    }
    offsets.sort();
    offsets.dedup();
    assert_eq!(offsets.len(), 4, "one fake offset per (object, type)");
    // asking again returns the same node (lookup_mmo, gem/i915_gem_mman.c:727-729)
    assert_eq!(r.gem.mmap_offset(r.s, h, WB).unwrap().offset, r.gem.mmap_offset(r.s, h, WB).unwrap().offset);
    assert!(offsets.iter().all(|o| *o >= 0x1_0000_0000 && o % 4096 == 0), "DRM_FILE_PAGE_OFFSET_START and page aligned: {offsets:x?}");
}

#[test]
fn offsets_of_different_objects_never_overlap() {
    let mut r = R::new();
    let a = r.create(3 * 4096);
    let b = r.create(4096);
    let oa = r.gem.mmap_offset(r.s, a, WB).unwrap().offset;
    let ob = r.gem.mmap_offset(r.s, b, WB).unwrap().offset;
    assert!(ob >= oa + 3 * 4096 || oa >= ob + 4096, "a={oa:x} b={ob:x}");
}

#[test]
fn types_the_device_cannot_offer_are_refused_by_name() {
    // WC and UC need PAT (gem/i915_gem_mman.c:895-898, gem/i915_gem_mman.c:901-905)
    let mut r = rig_with(false, true);
    let h = r.create(4096);
    assert_eq!(r.gem.mmap_offset(r.s, h, WC), Err(GemError::MmapTypeUnavailable { kind: "WC without PAT" }));
    assert_eq!(r.gem.mmap_offset(r.s, h, UC), Err(GemError::MmapTypeUnavailable { kind: "UC without PAT" }));
    assert!(r.gem.mmap_offset(r.s, h, WB).is_ok(), "WB never needs PAT");
    assert!(r.gem.mmap_offset(r.s, h, GTT).is_ok());
    assert_eq!(GemError::MmapTypeUnavailable { kind: "" }.errno(), 19);
    // GTT needs the aperture (gem/i915_gem_mman.c:889-890)
    let mut r = rig_with(true, false);
    let h = r.create(4096);
    assert_eq!(r.gem.mmap_offset(r.s, h, GTT), Err(GemError::MmapTypeUnavailable { kind: "GTT without an aperture" }));
    assert!(r.gem.mmap_offset(r.s, h, WC).is_ok());
    // FIXED belongs to objects with their own mmap_offset op; shmem has none (gem/i915_gem_mman.c:789-790); anything else is -EINVAL
    assert_eq!(r.gem.mmap_offset(r.s, h, FIXED), Err(GemError::MmapTypeUnavailable { kind: "FIXED on a shmem object" }));
    assert_eq!(r.gem.mmap_offset(r.s, h, 5), Err(GemError::MmapFlags { flags: 5 }));
    assert_eq!(r.gem.mmap_offset(r.s, h, 1 << 40).unwrap_err().errno(), 22);
}

#[test]
fn an_offset_is_usable_only_by_the_session_it_was_granted_to() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let b = r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    let h = r.create(4096);
    let g = r.gem.mmap_offset(r.s, h, WB).unwrap();
    // drm_vma_node_is_allowed (-EACCES): another session guessing the fake offset gets nothing
    let err = r.gem.mmap_open(b, g.offset, 4096, &mut r.pages).unwrap_err();
    assert_eq!((err, err.errno()), (GemError::MmapOffsetDenied { offset: g.offset }, 13));
    assert_eq!(r.gem.mmap_open(r.s, g.offset + 4096, 4096, &mut r.pages).unwrap_err().name(), "MMAP-OFFSET-DENIED", "an offset inside the node but not its start is not a grant");
    assert!(r.gem.mmap_open(r.s, g.offset, 4096, &mut r.pages).is_ok());
}

#[test]
fn closing_the_handle_revokes_new_mappings_but_not_existing_leases() {
    let mut r = R::new();
    let h = r.create(4096);
    let id = r.gem.lookup_object(r.s, h).unwrap();
    let g = r.gem.mmap_offset(r.s, h, WB).unwrap();
    let lease = r.gem.mmap_open(r.s, g.offset, 4096, &mut r.pages).unwrap();
    assert_eq!((lease.obj, lease.len, lease.cache), (id, 4096, CpuCache::Wb));
    assert_eq!(r.gem.lease_count(id), 1);
    r.sink.clear();
    r.gem.close(r.s, h, &mut r.sink).unwrap();
    // drm_vma_node_revoke: no NEW mapping on that offset (gem/i915_gem_object.c:267-270)
    assert_eq!(r.gem.mmap_open(r.s, g.offset, 4096, &mut r.pages), Err(GemError::MmapOffsetDenied { offset: g.offset }));
    // but the mapping that exists keeps the object and its pages alive (vm_open holds a reference, gem/i915_gem_mman.c:921-926)
    assert!(!r.sink.effects().iter().any(|e| matches!(e, Effect::FreePages { .. } | Effect::ObjectFreed { .. })), "{:?}", r.sink.effects());
    assert_eq!(r.gem.lookup_object(r.s, h).unwrap_err().name(), "STALE-HANDLE");
    // the mapping ends: now it goes
    r.gem.mmap_close(lease.lease, &mut r.sink).unwrap();
    assert!(r.sink.effects().contains(&Effect::FreePages { obj: id }));
    assert!(r.sink.effects().contains(&Effect::ObjectFreed { obj: id }));
    assert_eq!(r.gem.mmap_close(lease.lease, &mut r.sink), Err(GemError::LeaseUnknown { lease: lease.lease }));
}

#[test]
fn a_lease_longer_than_the_object_is_refused() {
    let mut r = R::new();
    let h = r.create(4096);
    let g = r.gem.mmap_offset(r.s, h, WB).unwrap();
    assert_eq!(r.gem.mmap_open(r.s, g.offset, 8192, &mut r.pages), Err(GemError::MmapRange { offset: 0, size: 8192, object: 4096 }));
    assert!(matches!(r.gem.mmap_open(r.s, g.offset, 0, &mut r.pages), Err(GemError::MmapRange { .. })));
}

#[test]
fn legacy_gem_mmap_checks_flags_range_and_pat() {
    let mut r = R::new();
    let h = r.create(8192);
    let l = r.gem.mmap_legacy(r.s, h, 4096, 4096, 0, &mut r.pages).unwrap();
    assert_eq!((l.start, l.len, l.cache), (4096, 4096, CpuCache::Wb));
    let l = r.gem.mmap_legacy(r.s, h, 0, 8192, 1, &mut r.pages).unwrap();
    assert_eq!(l.cache, CpuCache::Wc, "I915_MMAP_WC");
    assert_eq!(r.gem.mmap_legacy(r.s, h, 0, 8192, 2, &mut r.pages), Err(GemError::MmapFlags { flags: 2 })); // gem/i915_gem_mman.c:75-76
    assert!(matches!(r.gem.mmap_legacy(r.s, h, 4096, 8192, 0, &mut r.pages), Err(GemError::MmapRange { .. }))); // range_overflows, gem/i915_gem_mman.c:93-96
    assert!(matches!(r.gem.mmap_legacy(r.s, h, u64::MAX, 2, 0, &mut r.pages), Err(GemError::MmapRange { .. })), "offset + size must not wrap");
    let mut nopat = rig_with(false, true);
    let h = nopat.create(4096);
    assert_eq!(nopat.gem.mmap_legacy(nopat.s, h, 0, 4096, 1, &mut nopat.pages), Err(GemError::MmapTypeUnavailable { kind: "WC without PAT" }));
}

#[test]
fn mapping_requests_the_pages_and_the_tables_are_bounded() {
    let mut r = R::new();
    let hs: Vec<u32> = (0..MAX_MMAP_OFFSETS + 1).map(|_| r.create(4096)).collect();
    for h in &hs[..MAX_MMAP_OFFSETS] {
        r.gem.mmap_offset(r.s, *h, WB).unwrap();
    }
    let err = r.gem.mmap_offset(r.s, hs[MAX_MMAP_OFFSETS], WB).unwrap_err();
    assert_eq!(err, GemError::MmapTableFull { cap: MAX_MMAP_OFFSETS });
    // an offset already granted can still be asked for again
    assert!(r.gem.mmap_offset(r.s, hs[0], WB).is_ok());
    // closing an object frees its offsets
    r.gem.close(r.s, hs[1], &mut r.sink).unwrap();
    assert!(r.gem.mmap_offset(r.s, hs[MAX_MMAP_OFFSETS], WB).is_ok());

    // leases: one object, MAX_LEASES mappings
    let mut r = R::new();
    let h = r.create(4096);
    let g = r.gem.mmap_offset(r.s, h, WB).unwrap();
    let mut leases = vec![];
    for _ in 0..MAX_LEASES {
        leases.push(r.gem.mmap_open(r.s, g.offset, 4096, &mut r.pages).unwrap().lease);
    }
    assert_eq!(r.gem.mmap_open(r.s, g.offset, 4096, &mut r.pages), Err(GemError::LeaseTableFull { cap: MAX_LEASES }));
    r.gem.mmap_close(leases.pop().unwrap(), &mut r.sink).unwrap();
    assert!(r.gem.mmap_open(r.s, g.offset, 4096, &mut r.pages).is_ok());
}
