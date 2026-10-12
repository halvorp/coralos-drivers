// SPDX-License-Identifier: GPL-2.0-only
//! Per-object semantics: madvise and purgeable reclaim, caching on an LLC part, set_domain, tiling, userptr.
mod common;
use common::*;
use i915_gem_core::*;
use i915_memory_core::CacheLevel;
use i915_uapi_core::consts::misc as m;

type R = Rig<16, 16>;

const WILLNEED: u32 = m::I915_MADV_WILLNEED;
const DONTNEED: u32 = m::I915_MADV_DONTNEED;

fn with_pages(r: &mut R, size: u64) -> (u32, ObjId) {
    let h = r.create(size);
    let id = r.gem.lookup_object(r.s, h).unwrap();
    r.gem.set_domain(r.s, h, 1, 1, &mut r.pages).unwrap(); // pins the pages (gem/i915_gem_domain.c:619)
    (h, id)
}

// ---- madvise / purgeable reclaim --------------------------------------------------------------------------------------------

#[test]
fn madvise_flips_between_willneed_and_dontneed_and_reports_retained() {
    let mut r = R::new();
    let (h, _) = with_pages(&mut r, 8192);
    assert_eq!(r.gem.madvise(r.s, h, DONTNEED), Ok(true), "has pages: still retained");
    assert_eq!(r.gem.madvise(r.s, h, WILLNEED), Ok(true));
    assert_eq!(r.gem.madvise(r.s, h, 7), Err(GemError::MadviseInvalid { madv: 7 }));
    assert_eq!(r.gem.madvise(r.s, h, 7).unwrap_err().errno(), 22);
    assert_eq!(r.gem.madvise(r.s, 0x1234, DONTNEED).unwrap_err().errno(), 2);
}

#[test]
fn dontneed_on_an_object_without_pages_discards_it_at_once() {
    let mut r = R::new();
    let h = r.create(4096); // never touched: no pages yet
    assert_eq!(r.gem.madvise(r.s, h, DONTNEED), Ok(false), "i915_gem_object_truncate (i915_gem.c:1092-1095)");
    assert_eq!(r.gem.madvise(r.s, h, WILLNEED), Ok(false), "a purged object stays purged (i915_gem.c:1068)");
    assert!(r.gem.is_purged(r.gem.lookup_object(r.s, h).unwrap()));
}

#[test]
fn shrink_purges_only_idle_dontneed_objects_and_the_object_is_gone_for_good() {
    let mut r = R::new();
    let (keep, keep_id) = with_pages(&mut r, 4096);
    let (gone, gone_id) = with_pages(&mut r, 8192);
    let (also, also_id) = with_pages(&mut r, 4096);
    r.gem.madvise(r.s, gone, DONTNEED).unwrap();
    r.gem.madvise(r.s, also, DONTNEED).unwrap();
    r.sink.clear();
    let rep = r.gem.shrink(2, &mut r.sink); // wants 2 pages: the 8 KiB object covers it
    assert_eq!(rep.purged, 1, "stops once the target is met (`count < target`, i915_gem_shrinker.c:197)");
    assert_eq!(rep.pages_freed, 2);
    assert!(r.sink.effects().contains(&Effect::FreePages { obj: gone_id }));
    assert!(!r.sink.effects().contains(&Effect::FreePages { obj: keep_id }), "WILLNEED objects are not purgeable");
    assert!(!r.sink.effects().contains(&Effect::FreePages { obj: also_id }));
    assert_eq!(r.gem.madvise(r.s, gone, WILLNEED), Ok(false), "retained = false");
    assert_eq!(r.gem.madvise(r.s, also, WILLNEED), Ok(true), "not reached by this pass");
    let rep = r.gem.shrink(u64::MAX, &mut r.sink);
    assert_eq!(rep.purged, 0, "everything purgeable is gone; `also` went back to WILLNEED");
    // set_domain on the purged object: -EFAULT (gem/i915_gem_pages.c:109-112)
    assert_eq!(r.gem.set_domain(r.s, gone, 1, 1, &mut r.pages), Err(GemError::ObjectPurged { handle: gone }));
    assert_eq!(GemError::ObjectPurged { handle: 0 }.errno(), 14);
    let _ = keep;
}

#[test]
fn shrink_unbinds_the_object_first_and_waits_for_the_gpu() {
    let mut r = R::new();
    let victim = r.create(4096);
    let batch = r.create(4096);
    let e = Exec::new(0, vec![pinned(victim, 0x10_0000), pinned(batch, 0x20_0000)]);
    let d = r.exec(&e, 0).unwrap();
    let vid = r.gem.lookup_object(r.s, victim).unwrap();
    r.gem.madvise(r.s, victim, DONTNEED).unwrap();
    // in use by an unretired request: left alone (drop_pages without UNBIND_ACTIVE fails, i915_gem_shrinker.c:41-58)
    r.sink.clear();
    let rep = r.gem.shrink(100, &mut r.sink);
    assert_eq!((rep.purged, rep.skipped_active), (0, 1));
    assert!(r.sink.log.is_empty(), "nothing was unbound or freed");
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno).unwrap();
    // completed but not retired is still "active": retirement is what releases the object
    assert_eq!(r.gem.shrink(100, &mut r.sink).skipped_active, 1);
    r.gem.retire(&mut r.sink);
    let rep = r.gem.shrink(100, &mut r.sink);
    assert_eq!(rep.purged, 1);
    let log = &r.sink.log;
    let unbind = log.iter().position(|x| matches!(x, Rec::Unbind { .. })).expect("unbound from its VM");
    let free = log.iter().position(|x| matches!(x, Rec::Eff(Effect::FreePages { obj }) if *obj == vid)).expect("pages freed");
    assert!(unbind < free, "unbind, then release the pages");
    let vm = r.gem.context_vm(r.s, 0).unwrap();
    assert_eq!(r.gem.bound_offset(vm, vid), None);
}

#[test]
fn shrink_skips_scanout_buffers_and_revokes_cpu_mappings_of_what_it_purges() {
    let mut r = R::new();
    let (fb, fb_id) = with_pages(&mut r, 4096);
    let (mapped, mapped_id) = with_pages(&mut r, 4096);
    r.gem.set_framebuffer(fb_id, true).unwrap();
    r.gem.madvise(r.s, fb, DONTNEED).unwrap();
    r.gem.madvise(r.s, mapped, DONTNEED).unwrap();
    let g = r.gem.mmap_offset(r.s, mapped, 2).unwrap();
    let lease = r.gem.mmap_open(r.s, g.offset, 4096, &mut r.pages).unwrap();
    assert_eq!(lease.cache, CpuCache::Wb);
    r.sink.clear();
    let rep = r.gem.shrink(u64::MAX, &mut r.sink);
    assert_eq!((rep.purged, rep.skipped_framebuffer), (1, 1), "i915_gem_object_is_framebuffer is skipped (gem/i915_gem_shrinker.c:207-208)");
    let eff = r.sink.effects();
    assert!(eff.contains(&Effect::RevokeMmap { obj: mapped_id }), "truncate zaps the user mapping");
    assert!(eff.contains(&Effect::FreePages { obj: mapped_id }));
}

#[test]
fn a_purged_object_cannot_be_mapped_or_submitted_but_can_still_be_closed() {
    let mut r = R::new();
    let (h, id) = with_pages(&mut r, 4096);
    r.gem.madvise(r.s, h, DONTNEED).unwrap();
    r.gem.shrink(u64::MAX, &mut r.sink);
    let g = r.gem.mmap_offset(r.s, h, 2).unwrap(); // an offset can still be assigned (__assign_mmap_offset does not look at pages)
    assert_eq!(r.gem.mmap_open(r.s, g.offset, 4096, &mut r.pages), Err(GemError::ObjectPurged { handle: id.index() }), "the fault path pins the pages");
    r.sink.clear();
    r.gem.close(r.s, h, &mut r.sink).unwrap();
    assert!(r.sink.effects().contains(&Effect::ObjectFreed { obj: id }));
    assert!(!r.sink.effects().iter().any(|e| matches!(e, Effect::FreePages { .. })), "no pages to free twice");
}

// ---- caching -------------------------------------------------------------------------------------------------------------------

#[test]
fn objects_start_cached_on_an_llc_part_and_set_caching_rebinds() {
    let mut r = R::new();
    let a = r.create(4096);
    let b = r.create(4096);
    assert_eq!(r.gem.get_caching(r.s, a), Ok(m::I915_CACHING_CACHED), "shmem_object_init: I915_CACHE_LLC when HAS_LLC (gem/i915_gem_shmem.c:589-606)");
    r.exec(&Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]), 0).unwrap();
    r.sink.clear();
    r.gem.set_caching(r.s, a, m::I915_CACHING_CACHED, &mut r.sink).unwrap();
    assert!(r.sink.log.is_empty(), "same level: nothing to do (i915_gem_object_has_cache_level, gem/i915_gem_domain.c:277-278)");
    r.gem.set_caching(r.s, a, m::I915_CACHING_NONE, &mut r.sink).unwrap();
    assert_eq!(r.gem.get_caching(r.s, a), Ok(m::I915_CACHING_NONE));
    assert_eq!(r.sink.count_unbinds(), 1, "the mapping is dropped so it is rebound with the new PTE cache bits (gem/i915_gem_domain.c:292)");
    // the rebind encodes uncached PTEs: PPAT_UNCACHED = PWT | PCD (gt/intel_gtt.h:135)
    r.sink.clear();
    let ba = r.create(4096);
    let _ = ba;
    r.exec(&Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]), 1).unwrap();
    let Rec::Bind { writes, .. } = r.sink.binds()[0].clone() else { unreachable!() };
    let page = r.pages.map[&r.gem.lookup_object(r.s, a).unwrap()][0];
    assert!(writes.iter().any(|w| w.value == page | 0x1 | 0x2 | 0x8 | 0x10), "{writes:x?}");
    assert_eq!(r.gem.set_caching(r.s, a, 9, &mut r.sink), Err(GemError::CachingInvalid { caching: 9 }));
    // DISPLAY without HAS_WT is NONE (gem/i915_gem_domain.c:367)
    r.gem.set_caching(r.s, a, m::I915_CACHING_DISPLAY, &mut r.sink).unwrap();
    assert_eq!(r.gem.get_caching(r.s, a), Ok(m::I915_CACHING_NONE));
    let _ = CacheLevel::Llc;
}

// ---- set_domain ---------------------------------------------------------------------------------------------------------------

#[test]
fn set_domain_requests_the_pages_with_a_typed_request_and_follows_the_checks() {
    let mut r = R::new();
    let h = r.create(12288);
    let id = r.gem.lookup_object(r.s, h).unwrap();
    assert!(r.pages.asked.is_empty(), "creation allocates nothing: pages are lazy");
    assert_eq!(r.gem.set_domain(r.s, h, 1, 0, &mut r.pages), Ok(false)); // CPU read
    assert_eq!(r.pages.asked, vec![PageRequest { obj: id, bytes: 12288, cache: CacheLevel::Llc }]);
    // GPU domains are refused (gem/i915_gem_domain.c:550-551); a write domain must equal the read set (gem/i915_gem_domain.c:557-558); an empty read set is a no-op (gem/i915_gem_domain.c:560)
    assert_eq!(r.gem.set_domain(r.s, h, 2, 0, &mut r.pages), Err(GemError::SetDomainGpuDomain { read: 2, write: 0 }));
    assert_eq!(r.gem.set_domain(r.s, h, 1, 2, &mut r.pages), Err(GemError::SetDomainGpuDomain { read: 1, write: 2 }));
    assert_eq!(r.gem.set_domain(r.s, h, 1 | 0x40, 1, &mut r.pages), Err(GemError::SetDomainWriteNotRead { read: 0x41, write: 1 }));
    assert_eq!(r.gem.set_domain(r.s, h, 0, 0, &mut r.pages), Ok(false));
    assert_eq!(r.gem.set_domain(r.s, h, 0, 1, &mut r.pages), Err(GemError::SetDomainWriteNotRead { read: 0, write: 1 }));
    // iris calls it right after create with the CPU read domain and it must not fail (inventory section 7)
    let h2 = r.create(4096);
    assert!(r.gem.set_domain(r.s, h2, 1, 0, &mut r.pages).is_ok());
    // an allocator that refuses surfaces as a named error and leaves the object page-less
    let h3 = r.create(4096);
    r.pages.refuse = true;
    assert_eq!(r.gem.set_domain(r.s, h3, 1, 1, &mut r.pages), Err(GemError::PagesRefused { bytes: 4096 }));
    r.pages.refuse = false;
    assert!(r.gem.set_domain(r.s, h3, 1, 1, &mut r.pages).is_ok());
}

#[test]
fn set_domain_on_a_busy_object_reports_that_the_caller_must_wait() {
    let mut r = R::new();
    let (a, b) = (r.create(4096), r.create(4096));
    let d = r.exec(&Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]), 0).unwrap();
    assert_eq!(r.gem.set_domain(r.s, a, 1, 0, &mut r.pages), Ok(true), "i915_gem_object_wait before the move (gem/i915_gem_domain.c:572-576)");
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno).unwrap();
    assert_eq!(r.gem.set_domain(r.s, a, 1, 0, &mut r.pages), Ok(false));
}

#[test]
fn a_page_source_that_returns_the_wrong_number_of_pages_is_refused() {
    struct Bad;
    impl PageSource for Bad {
        fn alloc_pages(&mut self, _r: PageRequest) -> Result<(), PagesRefused> {
            Ok(())
        }
        fn pages(&self, _o: ObjId) -> Option<&[u64]> {
            Some(&[0x1000])
        }
    }
    let mut r = R::new();
    let h = r.create(8192);
    assert_eq!(r.gem.set_domain(r.s, h, 1, 0, &mut Bad), Err(GemError::PagesMalformed { have: 1, want: 2 }));
}

// ---- tiling -------------------------------------------------------------------------------------------------------------------

#[test]
fn tiling_follows_i915_tiling_ok_for_gen9() {
    let mut r = R::new();
    let h = r.create(1 << 20);
    let set = |r: &mut R, mode, stride| r.gem.set_tiling(r.s, h, mode, stride, &mut Sink::default());
    let ok = |mode, stride, swz| TilingReply { tiling_mode: mode, stride, swizzle_mode: swz };
    assert_eq!(set(&mut r, m::I915_TILING_X, 512), Ok(ok(1, 512, m::I915_BIT_6_SWIZZLE_NONE)), "X tiles are 512 bytes wide; swizzle is NONE on Gen8+");
    assert_eq!(set(&mut r, m::I915_TILING_Y, 128), Ok(ok(2, 128, 0)), "HAS_128_BYTE_Y_TILING: Y tiles are 128 wide (i915_drv.h:589)");
    assert_eq!(set(&mut r, m::I915_TILING_Y, 1280), Ok(ok(2, 1280, 0)));
    assert_eq!(r.gem.get_tiling(r.s, h), Ok(ok(2, 1280, 0)).map(|t| TilingReply { stride: 1280, ..t }));
    // refusals (gem/i915_gem_tiling.c:118-159)
    assert_eq!(set(&mut r, m::I915_TILING_X, 100), Err(GemError::TilingNotOk { tiling: 1, stride: 100 }));
    assert_eq!(set(&mut r, m::I915_TILING_X, 256), Err(GemError::TilingNotOk { tiling: 1, stride: 256 }), "512-byte tiles");
    assert_eq!(set(&mut r, m::I915_TILING_X, 0), Err(GemError::TilingNotOk { tiling: 1, stride: 0 }));
    assert_eq!(set(&mut r, m::I915_TILING_Y, 64), Err(GemError::TilingNotOk { tiling: 2, stride: 64 }));
    assert_eq!(set(&mut r, 3, 512), Err(GemError::TilingNotOk { tiling: 3, stride: 512 }), "> I915_TILING_LAST");
    // GEN7_FENCE_MAX_PITCH_VAL = 0x800: stride / 128 must not exceed it
    assert!(set(&mut r, m::I915_TILING_X, 128 * 0x800).is_ok());
    assert_eq!(set(&mut r, m::I915_TILING_X, 128 * 0x801), Err(GemError::TilingNotOk { tiling: 1, stride: 128 * 0x801 }));
    assert_eq!(set(&mut r, m::I915_TILING_X, 128 * 0x801).unwrap_err().errno(), 22);
    // back to linear: stride is forced to 0 (gem/i915_gem_tiling.c:373-376)
    assert_eq!(set(&mut r, m::I915_TILING_NONE, 4096), Ok(ok(0, 0, 0)));
    assert_eq!(r.gem.get_tiling(r.s, h), Ok(ok(0, 0, 0)));
}

#[test]
fn a_scanout_buffer_cannot_change_tiling_and_a_gtt_mapping_is_revoked_when_it_does() {
    let mut r = R::new();
    let h = r.create(1 << 20);
    let id = r.gem.lookup_object(r.s, h).unwrap();
    r.gem.set_tiling(r.s, h, 1, 512, &mut r.sink).unwrap();
    r.gem.mmap_offset(r.s, h, 0).unwrap(); // GTT type
    r.sink.clear();
    r.gem.set_tiling(r.s, h, 2, 512, &mut r.sink).unwrap();
    assert!(r.sink.effects().contains(&Effect::RevokeMmap { obj: id }), "i915_gem_object_release_mmap_gtt (gem/i915_gem_tiling.c:323)");
    r.sink.clear();
    r.gem.set_tiling(r.s, h, 2, 512, &mut r.sink).unwrap();
    assert!(r.sink.log.is_empty(), "unchanged: (tiling | stride) == obj->tiling_and_stride -> 0 (gem/i915_gem_tiling.c:244)");
    r.gem.set_framebuffer(id, true).unwrap();
    assert_eq!(r.gem.set_tiling(r.s, h, 1, 512, &mut r.sink), Err(GemError::TilingFramebuffer { handle: h }));
    assert_eq!(GemError::TilingFramebuffer { handle: 0 }.errno(), 16, "-EBUSY");
    assert!(r.gem.set_tiling(r.s, h, 2, 512, &mut r.sink).is_ok(), "re-setting the same mode is still fine");
}

#[test]
fn without_an_aperture_there_are_no_fences_and_tiling_is_unsupported() {
    let mut d = dev();
    d.ggtt_has_aperture = false; // gt/intel_ggtt_fencing.c:853-854
    let mut gem: Gem<8, 8> = Gem::new(d, GemConfig { pat_enabled: true });
    let mut pool = Pool::new();
    let mut sink = Sink::default();
    let s = gem.open_session(SessionCaps::default(), pool.init(), &mut sink).unwrap();
    let h = gem.create(s, 4096).unwrap().handle;
    assert_eq!(gem.get_tiling(s, h), Err(GemError::TilingNoFences));
    assert_eq!(gem.set_tiling(s, h, 1, 512, &mut sink), Err(GemError::TilingNoFences));
    assert_eq!(GemError::TilingNoFences.errno(), 95);
}

// ---- userptr -------------------------------------------------------------------------------------------------------------------

#[test]
fn userptr_is_refused_by_name() {
    let r = R::new();
    // the iris probe sends a zero-size request and expects -EINVAL to learn the ioctl exists (inventory section 7)
    let probe = r.gem.userptr(r.s, 0).unwrap_err();
    assert_eq!((probe, probe.errno()), (GemError::UserptrRefused { size: 0 }, 22));
    let real = r.gem.userptr(r.s, 1 << 20).unwrap_err();
    assert_eq!((real.name(), real.errno()), ("USERPTR-REFUSED", 19));
}

// ---- CPU / GPU domain bookkeeping ---------------------------------------------------------------------------------------------

const CPU: u32 = 0x1;
const RENDER: u32 = 0x2;
const GPU_DOMAINS: u32 = 0x2 | 0x4 | 0x8 | 0x10 | 0x20;
const GTT_D: u32 = 0x40;
const WC_D: u32 = 0x80;

#[test]
fn domains_follow_the_object_through_cpu_and_gpu_use() {
    let mut r = R::new();
    let (a, b) = (r.create(4096), r.create(4096));
    let st = |r: &R, h| {
        let s = r.gem.object_state(r.s, h).unwrap();
        (s.read_domains, s.write_domain)
    };
    assert_eq!(st(&r, a), (CPU, CPU), "shmem_object_init: both CPU (gem/i915_gem_shmem.c:585-587)");
    // a GPU read: the write domain is dropped and the GPU read domains join (i915_vma.c:2028, :2038)
    let mut ra = pinned(a, 0x10_0000);
    ra.flags &= !i915_uapi_core::consts::exec_object::EXEC_OBJECT_ASYNC;
    r.exec(&Exec::new(0, vec![ra, pinned(b, 0x20_0000)]), 0).unwrap();
    assert_eq!(st(&r, a), (CPU | GPU_DOMAINS, 0));
    // a GPU write: the GPU is the only writer and the only reader (i915_vma.c:2024-2025, :2038)
    r.exec(&Exec::new(0, vec![pinned_write(a, 0x10_0000), pinned(b, 0x20_0000)]), 1).unwrap();
    assert_eq!(st(&r, a), (GPU_DOMAINS, RENDER));
    assert!(!r.gem.object_state(r.s, a).unwrap().cache_dirty, "an LLC object starts coherent and clean");
    // CPU read: the GPU write domain is flushed away (flush_write_domain, gem/i915_gem_domain.c:54-89) and CPU joins the readers
    assert_eq!(r.gem.set_domain(r.s, a, CPU, 0, &mut r.pages), Ok(true));
    assert_eq!(st(&r, a), (GPU_DOMAINS | CPU, 0));
    assert!(r.gem.object_state(r.s, a).unwrap().cache_dirty, "gpu_write_needs_clflush: a GPU-written cached object is marked CPU-cache dirty (:82-84)");
    // CPU write: exclusive (__start_cpu_write, gem/i915_gem_object.h:795-801)
    r.gem.set_domain(r.s, a, CPU, CPU, &mut r.pages).unwrap();
    assert_eq!(st(&r, a), (CPU, CPU));
    // the same request again is a no-op: already in that write domain (gem/i915_gem_domain.c:633-634)
    r.gem.set_domain(r.s, a, CPU, CPU, &mut r.pages).unwrap();
    assert_eq!(st(&r, a), (CPU, CPU));
    // WC and GTT: first access joins the read set, a write makes it exclusive (:170-175, :232-238); WC wins over GTT (:638)
    r.gem.set_domain(r.s, a, WC_D, 0, &mut r.pages).unwrap();
    assert_eq!(st(&r, a), (CPU | WC_D, 0), "flush_write_domain(~WC) drops the CPU write domain (:157)");
    r.gem.set_domain(r.s, a, GTT_D, GTT_D, &mut r.pages).unwrap();
    assert_eq!(st(&r, a), (GTT_D, GTT_D));
    r.gem.set_domain(r.s, a, GTT_D | WC_D, 0, &mut r.pages).unwrap();
    assert_eq!(st(&r, a), (GTT_D | WC_D, 0), "both bits asked: WC is handled first");
}
