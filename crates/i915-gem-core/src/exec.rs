// SPDX-License-Identifier: GPL-2.0-only
//! `DRM_IOCTL_I915_GEM_EXECBUFFER2`, EXECUTION side: after `i915-uapi-core` has made Linux's pre-execution refusals, look the
//! objects up in the session, reserve address space for them (softpin placement through `i915-memory-core`, with eviction on
//! ENOSPC), apply relocations, build the request on the engine (`i915-submit-core`), register the fences and the activity, and
//! hand back the ELSP submission. Everything is a plan: binds and unbinds go through the [`EffectSink`], and nothing here writes
//! a register or a page.
//!
//! Ported from gem/i915_gem_execbuffer.c (v7.0-rc6): `eb_lookup_vmas` (gem/i915_gem_execbuffer.c:941), `eb_vma_misplaced` (gem/i915_gem_execbuffer.c:379), `eb_pin_flags` (gem/i915_gem_execbuffer.c:411),
//! `eb_pin_vma` (gem/i915_gem_execbuffer.c:438), `eb_validate_vmas` (gem/i915_gem_execbuffer.c:997), `eb_reserve_vma` (gem/i915_gem_execbuffer.c:655), `eb_unbind` (gem/i915_gem_execbuffer.c:696), `eb_reserve` (gem/i915_gem_execbuffer.c:735),
//! `eb_relocate_entry` (gem/i915_gem_execbuffer.c:1409), `eb_relocate_parse` (gem/i915_gem_execbuffer.c:1890), `eb_move_to_gpu` (gem/i915_gem_execbuffer.c:2080), `eb_request_submit` (gem/i915_gem_execbuffer.c:2369), `eb_submit`
//! (gem/i915_gem_execbuffer.c:2418), `eb_pin_engine` (gem/i915_gem_execbuffer.c:2550), `add_fence_array`/`await_fence_array`/`signal_fence_array` (gem/i915_gem_execbuffer.c:2919, :3000, :3018), `eb_fences_add` (gem/i915_gem_execbuffer.c:3231) and
//! `eb_request_add` (gem/i915_gem_execbuffer.c:3068); i915_vma.c `_i915_vma_move_to_active` (i915_vma.c:1970).
//!
//! RELOCATION (the ruling asked for, named): Gen9 Mesa does not relocate (docs/I915-UAPI-INVENTORY.md section 8), but the plan keeps
//! relocations "implemented anyway". This is the MINIMAL FAITHFUL path: every entry the uAPI crate validated and that does not
//! already hold the right address becomes one [`Effect::PatchReloc`] carrying `gen8_canonical_addr((int)delta + target_offset)` (gem/i915_gem_execbuffer.c:1100),
//! applied by the executor through its CPU mapping of the owner. No GPU relocation, no `reloc_cache`, no slow path.
//!
//! NOT PORTED, by name: the command parser (`eb_parse`, gem/i915_gem_execbuffer.c:2263: Gen9 render has none), secure dispatch (`I915_EXEC_SECURE` is
//! refused by the uAPI crate), parallel/composite fences and engine bonding, the capture list (`eb_capture_*`: error capture is
//! the GT reset path's), `eb_throttle` (gem/i915_gem_execbuffer.c:2465), the ww-mutex backoff (`-EDEADLK`; this planner is single-threaded), pass 3 of
//! `eb_reserve` (contention on object locks), and the fence-register pin (`EXEC_OBJECT_NEEDS_FENCE` is a pre-Gen9 thing).
use crate::active::ActiveReq;
use crate::context::UserEngine;
use crate::error::GemError;
use crate::exec_env::ExecView;
use crate::gem::Gem;
use crate::ids::{ObjId, RequestId, SessionId, VmId};
use crate::limits::*;
use crate::seam::{Effect, EffectSink, EngineSet, PageSource, SyncTables};
use i915_memory_core::{BindRequest, MemError, Mode, Placement, TablePageSource, VmaId};
use i915_submit_core::BatchSpec;
use i915_uapi_core::consts::{exec as ef, exec_fence as xf, exec_object as eo};
use i915_uapi_core::execbuf::{canonical_addr, noncanonical_addr, EXEC_FENCE_UNKNOWN_FLAGS};
use i915_uapi_core::types::{engine_class, ExecObject2, RelocationEntry};
use i915_uapi_core::{validate_execbuf2, EngineChoice, ExecbufRequest, InFence, UapiError};

/// `BATCH_OFFSET_BIAS` (gem/i915_gem_execbuffer.c:71): a batch that carries relocations is kept above 256 KiB.
pub const BATCH_OFFSET_BIAS: u64 = 256 * 1024; // i915_gem_execbuffer.c:71
const PAGE: u64 = 4096;

/// What a successful `EXECBUFFER2` produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecDone {
    pub request: RequestId,
    /// Index in the device's engine order, and the request's identity on that engine.
    pub engine: usize,
    pub engine_ctx: u32,
    pub seqno: u32,
    /// Objects in the submission and their final GPU addresses, canonical form (what `execbuffer2` writes back, gem/i915_gem_execbuffer.c:3630).
    pub n_objects: usize,
    pub offsets: [u64; MAX_EXEC_OBJECTS],
    /// Objects whose address differs from the `offset` the user passed (`UPDATE`, gem/i915_gem_execbuffer.c:1019): the user must re-read offsets.
    pub moved: u32,
    pub relocs_patched: u32,
    /// VMAs evicted to make room, across all passes.
    pub evicted: u32,
    /// How many `eb_reserve` passes were needed (0 = everything fitted without eviction).
    pub passes: u8,
    pub waits: u32,
    pub signals: u32,
    pub out_fence: bool,
    /// An ELSP submission was produced (`Effect::Submit` emitted).
    pub submitted: bool,
}

#[derive(Clone, Copy)]
struct Eb {
    exec: ExecObject2,
    obj: ObjId,
    vma: Option<VmaId>,
    /// Non-canonical GPU address once placed.
    offset: u64,
    write: bool,
    asynchronous: bool,
    needs_bias: bool,
}

const EB_EMPTY: Eb = Eb {
    exec: ExecObject2 { handle: 0, relocation_count: 0, alignment: 0, offset: 0, flags: 0, pad_to_size: 0, rsvd2: 0 },
    obj: ObjId(0),
    vma: None,
    offset: 0,
    write: false,
    asynchronous: false,
    needs_bias: false,
};

/// `eb_vma_misplaced` (gem/i915_gem_execbuffer.c:379-409) for a bound VMA at `start` covering `size` bytes.
fn misplaced(eb: &Eb, start: u64, size: u64) -> bool {
    let e = &eb.exec;
    if size < e.pad_to_size && e.flags & eo::EXEC_OBJECT_PAD_TO_SIZE != 0 {
        return true; // gem/i915_gem_execbuffer.c:385-386
    }
    if e.alignment != 0 && !start.is_multiple_of(e.alignment) {
        return true; // gem/i915_gem_execbuffer.c:388-389
    }
    if e.flags & eo::EXEC_OBJECT_PINNED != 0 && start != noncanonical_addr(e.offset) {
        return true; // gem/i915_gem_execbuffer.c:391-393
    }
    if eb.needs_bias && start < BATCH_OFFSET_BIAS {
        return true; // gem/i915_gem_execbuffer.c:395-397
    }
    if e.flags & eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS == 0 && (start + size + PAGE - 1) >> 32 != 0 {
        return true; // gem/i915_gem_execbuffer.c:399-401
    }
    false
}

fn retryable(e: &MemError) -> bool {
    matches!(e, MemError::NoSpace { .. } | MemError::RangeOccupied { .. }) // -ENOSPC drives the next pass, gem/i915_gem_execbuffer.c:816-817
}

impl<const V: usize, const T: usize> Gem<V, T> {
    /// A uAPI "no such object" for a handle that belongs to another session keeps its name (`CROSS-SESSION-HANDLE`).
    fn reclassify(&self, slot: usize, e: UapiError) -> GemError {
        if let UapiError::ExecObjectMissing { handle } = e {
            if let Err(le) = self.lookup(slot, handle) {
                return le;
            }
        }
        GemError::Uapi(e)
    }

    /// `eb_select_engine` result -> the physical engine (index, class, instance).
    fn resolve_engine(&mut self, slot: usize, cslot: usize, choice: EngineChoice) -> Result<(usize, u16, u16), GemError> {
        let (class, instance) = match choice {
            EngineChoice::Fixed { class, instance } => (class, instance),
            EngineChoice::UserSlot(i) => match self.contexts[cslot].as_ref().and_then(|c| c.engines) {
                Some((e, _)) => match e[i as usize] {
                    UserEngine::Physical { class, instance } => (class, instance),
                    UserEngine::Invalid => return Err(GemError::EngineIndex { index: i as usize }),
                },
                None => return Err(GemError::EngineIndex { index: i as usize }),
            },
            EngineChoice::BalancedVideo => {
                // gen8_dispatch_bsd_engine (gem/i915_gem_execbuffer.c:2443-2455) picks a RANDOM instance once per client. A planner
                // is deterministic: the client's session number picks it instead; the choice is still sticky per client.
                let n = self.dev.engines().class_count(engine_class::VIDEO).max(1) as u64;
                let pick = match self.session(slot).bsd_engine {
                    Some(p) => p,
                    None => {
                        let p = (self.session(slot).id.0 % n) as u8;
                        self.session_mut(slot).bsd_engine = Some(p);
                        p
                    }
                };
                (engine_class::VIDEO, pick as u16)
            }
        };
        let idx = self.engine_index(class, instance).ok_or(GemError::EngineIndex { index: instance as usize })?;
        Ok((idx, class, instance))
    }

    /// Unbind every unpinned VMA of a VM (`i915_gem_evict_vm`, i915_gem_evict.c; pass 2 of `eb_reserve`, gem/i915_gem_execbuffer.c:781-788).
    fn evict_vm(&mut self, vi: usize, sink: &mut dyn EffectSink) -> u32 {
        let mut n = 0;
        let Some(vm) = self.vms[vi].as_mut() else { return 0 };
        for i in 0..V {
            let Some(v) = vm.vmas[i] else { continue };
            if let Ok(plan) = vm.gtt.plan_unbind(v.id) {
                vm.vmas[i] = None;
                sink.unbind(VmId(vi as u8), &plan);
                n += 1;
            } // Pinned (in use by this submission): left alone
        }
        n
    }

    /// One `i915_vma_pin_ww` with `PIN_USER | PIN_VALIDATE` and the flags of `eb_pin_flags` (gem/i915_gem_execbuffer.c:411-435): reserve address space for
    /// the object, bind its pages, pin the VMA for the duration of the submission.
    #[allow(clippy::too_many_arguments)]
    fn bind_entry<P: PageSource, TS: TablePageSource>(
        &mut self,
        vi: usize,
        eb: &mut Eb,
        evict: bool,
        pages: &P,
        tables: &mut TS,
        sink: &mut dyn EffectSink,
        evicted: &mut u32,
    ) -> Result<(), MemError> {
        let (osize, cache) = match self.obj(eb.obj) {
            Some(o) => (o.size, o.cache),
            None => return Err(MemError::NotBound { id: eb.obj.0 }),
        };
        let e = eb.exec;
        let pad = if e.flags & eo::EXEC_OBJECT_PAD_TO_SIZE != 0 { e.pad_to_size } else { 0 };
        let size = osize.max(pad);
        let mode = if e.flags & eo::EXEC_OBJECT_PINNED != 0 { Mode::Fixed(noncanonical_addr(e.offset)) } else { Mode::Best }; // PIN_OFFSET_FIXED, gem/i915_gem_execbuffer.c:429-430
        let place = Placement {
            mode,
            bias: if e.flags & eo::EXEC_OBJECT_PINNED == 0 && eb.needs_bias { BATCH_OFFSET_BIAS } else { 0 }, // gem/i915_gem_execbuffer.c:431-432
            mappable: false,
            zone_4g: e.flags & eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS == 0, // Wa32bitGeneralStateOffset, gem/i915_gem_execbuffer.c:419-424
            evict,
        };
        let pg = pages.pages(eb.obj).ok_or(MemError::NotBound { id: eb.obj.0 })?;
        let req = BindRequest { pages: pg, size, alignment: e.alignment, guard: 0, cache, flags: 0, place };
        let vm = self.vms[vi].as_mut().ok_or(MemError::NotBound { id: vi as u32 })?;
        let plan = vm.gtt.plan_bind(tables, &req)?;
        for v in plan.evicted() {
            vm.forget_vma(*v); // victims are unbound by this plan
            *evicted += 1;
        }
        vm.record(eb.obj, plan.vma(), size).map_err(|_| MemError::VmaTableFull { cap: V })?;
        let _ = vm.gtt.set_pinned(plan.vma(), true);
        eb.vma = Some(plan.vma());
        eb.offset = plan.offset();
        sink.bind(VmId(vi as u8), eb.obj, &plan);
        Ok(())
    }

    /// `DRM_IOCTL_I915_GEM_EXECBUFFER2`: validate, place, relocate, build the request, register fences and activity, and produce
    /// the ELSP submission. `now` is the executor's clock (for the engine's stall bound).
    ///
    /// On any error nothing is half-done that the caller must undo: binds already emitted stay bound (a cache, as in Linux after a
    /// failed execbuffer), every VMA is unpinned again, and no request is registered unless the engine built it.
    #[allow(clippy::too_many_arguments)]
    pub fn execbuffer2<P: PageSource, TS: TablePageSource, E: EngineSet>(
        &mut self,
        session: SessionId,
        sync: &dyn SyncTables,
        req: &ExecbufRequest<'_>,
        pages: &mut P,
        tables: &mut TS,
        engines: &mut E,
        sink: &mut dyn EffectSink,
        now: u64,
    ) -> Result<ExecDone, GemError> {
        let slot = self.session_slot(session)?;
        let ctx_id = req.context_id();
        let ring_slot = (req.flags & ef::I915_EXEC_RING_MASK) as u32;
        let vm_hint = self.context_slot(slot, ctx_id).ok().and_then(|c| self.contexts[c].map(|c| c.vm));
        // The uAPI crate makes every refusal Linux makes before it touches the GPU (gem/i915_gem_execbuffer.c:3339-3534 up to eb_relocate_parse).
        let plan = {
            let view = ExecView { gem: &*self, slot, vm: vm_hint, ring_slot, sync };
            validate_execbuf2(&self.dev, &view, req).map_err(|e| self.reclassify(slot, e))?
        };
        let cslot = self.context_slot(slot, ctx_id)?;
        let ctx = self.contexts[cslot].expect("validated");
        if ctx.banned {
            return Err(GemError::ContextBanned { id: ctx_id }); // eb_pin_engine, gem/i915_gem_execbuffer.c:2558-2559
        }
        let (eidx, class, instance) = self.resolve_engine(slot, cslot, plan.engine)?;
        let ectx = ctx.engine_ctx[eidx].ok_or(GemError::ExecEngineContextMissing { ctx: ctx_id, class, instance })?; // gem/i915_gem_execbuffer.c:2566
        let aslot = self.active.free_slot().ok_or(GemError::ActiveTableFull { cap: MAX_ACTIVE })?;
        let vi = ctx.vm as usize;

        // eb_lookup_vmas (gem/i915_gem_execbuffer.c:941-978): the session's own table, then page residency.
        let n = req.objects.len();
        if n > MAX_EXEC_OBJECTS {
            return Err(GemError::ExecTooManyObjects { count: n, max: MAX_EXEC_OBJECTS });
        }
        let mut eb = [EB_EMPTY; MAX_EXEC_OBJECTS];
        for i in 0..n {
            let exec = req.objects.get(i).ok_or(GemError::EngineIndex { index: i })?;
            let obj = self.lookup(slot, exec.handle)?;
            if eb[..i].iter().any(|x| x.obj == obj) {
                return Err(GemError::ExecObjectDuplicate { handle: exec.handle });
            }
            self.ensure_pages(obj, exec.handle, pages)?; // a purged object is -EFAULT here, gem/i915_gem_pages.c:109-112
            let is_batch = i == plan.batch_index;
            eb[i] = Eb {
                exec,
                obj,
                vma: None,
                offset: 0,
                write: exec.flags & eo::EXEC_OBJECT_WRITE != 0,
                asynchronous: exec.flags & eo::EXEC_OBJECT_ASYNC != 0,
                // eb_add_vma gem/i915_gem_execbuffer.c:593-596: a batch with relocations that is not softpinned is kept above BATCH_OFFSET_BIAS.
                needs_bias: is_batch && exec.relocation_count != 0 && exec.flags & eo::EXEC_OBJECT_PINNED == 0,
            };
        }

        let mut evicted = 0u32;
        let mut passes = 0u8;
        let result = self.exec_placed(
            slot, cslot, vi, ectx, eidx, class, instance, &plan, req, sync, &mut eb[..n], aslot, pages, tables, engines, sink, now, &mut evicted, &mut passes,
        );
        // Whatever happened, the VMAs are no longer in use by this submission (i915_vma_unpin, eb_release_vmas gem/i915_gem_execbuffer.c:1068-1088).
        if let Some(vm) = self.vms[vi].as_mut() {
            for e in eb[..n].iter().filter_map(|e| e.vma) {
                let _ = vm.gtt.set_pinned(e, false);
            }
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn exec_placed<P: PageSource, TS: TablePageSource, E: EngineSet>(
        &mut self,
        slot: usize,
        cslot: usize,
        vi: usize,
        ectx: u32,
        eidx: usize,
        class: u16,
        instance: u16,
        plan: &i915_uapi_core::ExecPlan,
        req: &ExecbufRequest<'_>,
        sync: &dyn SyncTables,
        eb: &mut [Eb],
        aslot: usize,
        pages: &mut P,
        tables: &mut TS,
        engines: &mut E,
        sink: &mut dyn EffectSink,
        now: u64,
        evicted: &mut u32,
        passes: &mut u8,
    ) -> Result<ExecDone, GemError> {
        let n = eb.len();
        let pages: &P = pages; // allocation is over; from here the pages are only read
        let mut unbound = [false; MAX_EXEC_OBJECTS];

        // eb_validate_vmas -> eb_pin_vma (gem/i915_gem_execbuffer.c:997-1045, gem/i915_gem_execbuffer.c:438-489): reuse the current location when it still satisfies the entry.
        for i in 0..n {
            let vm = self.vms[vi].as_mut().expect("context holds its VM");
            match vm.vma_of(eb[i].obj) {
                Some((ri, vid)) => {
                    let rec = vm.vmas[ri].expect("found");
                    let (start, _, _) = vm.gtt.offset_of(vid).expect("recorded vma is live");
                    if misplaced(&eb[i], start, rec.size) {
                        // "Failing that pick any _free_ space": unbind and put it on the unbound list (gem/i915_gem_execbuffer.c:1022-1030).
                        let p = vm.gtt.plan_unbind(vid).map_err(|why| GemError::ExecReserve { handle: eb[i].exec.handle, why })?;
                        vm.vmas[ri] = None;
                        sink.unbind(VmId(vi as u8), &p);
                        unbound[i] = true;
                    } else {
                        let _ = vm.gtt.set_pinned(vid, true);
                        let _ = vm.gtt.touch(vid);
                        eb[i].vma = Some(vid);
                        eb[i].offset = start;
                    }
                }
                None => unbound[i] = true,
            }
        }

        // eb_reserve (gem/i915_gem_execbuffer.c:735-821): pass 0 binds without evicting; pass 1 may evict the least recently used unpinned VMAs; pass 2
        // unpins everything, evicts the whole VM and binds again. Softpinned entries go first ("Pinned must have their slot", gem/i915_gem_execbuffer.c:719).
        if unbound[..n].iter().any(|u| *u) {
            let mut pass = 0u8;
            loop {
                if pass == 2 {
                    // eb_unbind(force) + i915_gem_evict_vm (gem/i915_gem_execbuffer.c:781-788): every entry is unbound again, then the VM is emptied.
                    for i in 0..n {
                        if let (Some(v), Some(vm)) = (eb[i].vma.take(), self.vms[vi].as_mut()) {
                            let _ = vm.gtt.set_pinned(v, false);
                        }
                        unbound[i] = true;
                    }
                    *evicted += self.evict_vm(vi, sink);
                }
                let mut failed: Option<MemError> = None;
                for pinned_first in [true, false] {
                    for i in 0..n {
                        if !unbound[i] || (eb[i].exec.flags & eo::EXEC_OBJECT_PINNED != 0) != pinned_first {
                            continue;
                        }
                        let mut e = eb[i];
                        let r = self.bind_entry(vi, &mut e, pass >= 1, pages, tables, sink, evicted);
                        match r {
                            Ok(()) => {
                                eb[i] = e;
                                unbound[i] = false;
                            }
                            Err(why) if retryable(&why) && pass < 2 => {
                                failed = Some(why);
                                break;
                            }
                            Err(why) => return Err(GemError::ExecReserve { handle: eb[i].exec.handle, why }),
                        }
                    }
                    if failed.is_some() {
                        break;
                    }
                }
                *passes = pass + 1;
                if failed.is_none() {
                    break;
                }
                pass += 1;
            }
        }
        // A softpinned object must sit exactly where it asked (eb_vma_misplaced -> -EBADSLT, gem/i915_gem_execbuffer.c:484-485).
        for e in eb.iter() {
            if e.exec.flags & eo::EXEC_OBJECT_PINNED != 0 && e.offset != noncanonical_addr(e.exec.offset) {
                return Err(GemError::ExecPinnedMisplaced { handle: e.exec.handle, want: noncanonical_addr(e.exec.offset), have: e.offset });
            }
        }
        let mut offsets = [0u64; MAX_EXEC_OBJECTS];
        let mut moved = 0u32;
        for (i, e) in eb.iter().enumerate() {
            offsets[i] = canonical_addr(e.offset);
            if canonical_addr(e.offset) != canonical_addr(noncanonical_addr(e.exec.offset)) {
                moved += 1; // entry->offset = i915_vma_offset(vma) | UPDATE, gem/i915_gem_execbuffer.c:1019
            }
        }

        // eb_relocate_vma (gem/i915_gem_execbuffer.c:1511-1594) -> eb_relocate_entry: relocations in object order (the wire order).
        let patched = self.relocate(slot, req, eb, sink)?;

        // eb_move_to_gpu (gem/i915_gem_execbuffer.c:2080-2178): clflush what the GPU will read through a non-coherent cache.
        for e in eb.iter() {
            if let Some(o) = self.obj_mut(e.obj) {
                // `unlikely(obj->cache_dirty & ~obj->cache_coherent)` with the bitwise `&` of the C source (gem/i915_gem_execbuffer.c:2117).
                if (o.cache_dirty as u8) & !o.coherent != 0 {
                    o.cache_dirty = false;
                    sink.effect(Effect::Clflush { obj: e.obj });
                }
            }
        }

        // eb_requests_create -> i915_request_create (gem/i915_gem_execbuffer.c:3293-): the request on the engine's ring.
        let batch = &eb[plan.batch_index];
        let spec = BatchSpec {
            offset: batch.offset + u64::from(plan.batch_start_offset), // i915_vma_offset(batch) + eb->batch_start_offset, gem/i915_gem_execbuffer.c:2397-2400
            flags: 0,                                                    // I915_DISPATCH_*: SECURE and PINNED are refused earlier
            nopreempt: false,                                            // intel_context_nopreempt: never set on this port
        };
        let rq = engines.build_request(class, instance, ectx, &spec)?;
        let id = RequestId(self.active.next_id);
        self.active.next_id += 1;

        // _i915_vma_move_to_active (i915_vma.c:1970-2040): the activity and the CPU-domain bookkeeping.
        let mut act = ActiveReq { id, engine: eidx as u8, ectx, seqno: rq.seqno, ctx: cslot as u8, objs: [(ObjId(0), false); MAX_EXEC_OBJECTS], n: n as u8, error: None };
        for (i, e) in eb.iter().enumerate() {
            act.objs[i] = (e.obj, e.write);
        }
        // i915_request_await_object (i915_request.c:1583-): wait for earlier writers (and, for a write, readers) on OTHER engines;
        // the same engine runs requests in submission order. EXEC_OBJECT_ASYNC opts out (gem/i915_gem_execbuffer.c:2123).
        let mut seen = 0u64;
        for e in eb.iter().filter(|e| !e.asynchronous) {
            for (si, prev) in self.active.reqs.iter().enumerate() {
                let Some(prev) = prev else { continue };
                if prev.engine as usize == eidx || self.active.completed(prev) || seen & (1 << si) != 0 {
                    continue;
                }
                if let Some(prev_writes) = prev.touches(e.obj) {
                    if e.write || prev_writes {
                        seen |= 1 << si;
                        sink.effect(Effect::AwaitRequest { rq: id, on: prev.id });
                    }
                }
            }
        }
        for e in eb.iter() {
            if let Some(o) = self.obj_mut(e.obj) {
                o.move_to_active(e.write); // i915_vma.c:2022-2039
            }
        }
        self.active.reqs[aslot] = Some(act);
        if let Some(vm) = self.vms[vi].as_mut() {
            for e in eb.iter().filter_map(|e| e.vma) {
                let _ = vm.gtt.touch(e); // LRU: used now
            }
        }

        // eb_fences_add (gem/i915_gem_execbuffer.c:3231-3273): in-fence, syncobj arrays, out-fence.
        let (waits, signals) = self.emit_fences(id, plan, req, sync, sink);

        // __i915_request_queue -> the tasklet's execlists_dequeue (eb_request_add gem/i915_gem_execbuffer.c:3068-3112).
        let submitted = match engines.dequeue(class, instance, now)? {
            Some(sp) => {
                sink.effect(Effect::Submit { engine: eidx as u8, plan: sp });
                true
            }
            None => false,
        };
        Ok(ExecDone {
            request: id,
            engine: eidx,
            engine_ctx: ectx,
            seqno: rq.seqno,
            n_objects: n,
            offsets,
            moved,
            relocs_patched: patched,
            evicted: *evicted,
            passes: *passes,
            waits,
            signals,
            out_fence: plan.out_fence,
            submitted,
        })
    }

    /// `eb_relocate_entry` for every relocation (gem/i915_gem_execbuffer.c:1409-1509), with the target's FINAL placement.
    fn relocate(&mut self, _slot: usize, req: &ExecbufRequest<'_>, eb: &mut [Eb], sink: &mut dyn EffectSink) -> Result<u32, GemError> {
        let mut next = 0usize;
        let mut patched = 0u32;
        let by_lut = req.flags & ef::I915_EXEC_HANDLE_LUT != 0;
        for oi in 0..eb.len() {
            for _ in 0..eb[oi].exec.relocation_count {
                let idx = next;
                next += 1;
                let r: RelocationEntry = req.relocs.get(idx).ok_or(GemError::Uapi(UapiError::Truncated { what: "relocs", need: (idx + 1) * 32, have: 0 }))?;
                // eb_get_vma (gem/i915_gem_execbuffer.c:1049-1066)
                let ti = if by_lut { (r.target_handle as usize).checked_lt(eb.len()) } else { eb.iter().position(|e| e.exec.handle == r.target_handle) };
                let ti = ti.ok_or(GemError::Uapi(UapiError::ExecRelocTargetMissing { handle: eb[oi].exec.handle, target: r.target_handle }))?; // gem/i915_gem_execbuffer.c:1418-1420
                if r.write_domain != 0 {
                    eb[ti].write = true; // target->flags |= EXEC_OBJECT_WRITE, gem/i915_gem_execbuffer.c:1445-1446
                }
                let target_offset = eb[ti].offset;
                // "If the relocation already has the right value in it, no more work needs to be done." (gem/i915_gem_execbuffer.c:1472-1477)
                if canonical_addr(target_offset) == r.presumed_offset {
                    continue;
                }
                let owner_size = self.obj(eb[oi].obj).map_or(0, |o| o.size);
                if r.offset > owner_size.wrapping_sub(8) {
                    return Err(GemError::Uapi(UapiError::ExecRelocBounds { handle: eb[oi].exec.handle, offset: r.offset, size: owner_size })); // gem/i915_gem_execbuffer.c:1480-1488
                }
                if r.offset & 3 != 0 {
                    return Err(GemError::Uapi(UapiError::ExecRelocUnaligned { handle: eb[oi].exec.handle, offset: r.offset })); // gem/i915_gem_execbuffer.c:1489-1494
                }
                eb[oi].asynchronous = false; // gem/i915_gem_execbuffer.c:1505
                // relocation_target (gem/i915_gem_execbuffer.c:1098-1101): gen8_canonical_addr((int)reloc->delta + i915_vma_offset(target))
                let value = canonical_addr((r.delta as i32 as i64 as u64).wrapping_add(target_offset));
                sink.effect(Effect::PatchReloc {
                    owner: eb[oi].obj,
                    byte_offset: r.offset,
                    value,
                    width: 8, // use_64bit_reloc: GRAPHICS_VER >= 8
                    reloc_index: idx as u32,
                    presumed: canonical_addr(target_offset),
                });
                patched += 1;
            }
        }
        Ok(patched)
    }

    /// The fence side of `eb_fences_add` (gem/i915_gem_execbuffer.c:3231-3273), `await_fence_array` (gem/i915_gem_execbuffer.c:3000-3016) and `signal_fence_array` (gem/i915_gem_execbuffer.c:3018-3050).
    fn emit_fences(&mut self, id: RequestId, plan: &i915_uapi_core::ExecPlan, req: &ExecbufRequest<'_>, sync: &dyn SyncTables, sink: &mut dyn EffectSink) -> (u32, u32) {
        let (mut waits, mut signals) = (0u32, 0u32);
        match plan.in_fence {
            InFence::None => {}
            InFence::Wait(token) => sink.effect(Effect::AwaitInFence { rq: id, token, submit: false }),
            InFence::Submit(token) => sink.effect(Effect::AwaitInFence { rq: id, token, submit: true }),
        }
        // Timeline blocks first, then the plain array (gem/i915_gem_execbuffer.c:3400-3406).
        for blk in req.timeline_blocks() {
            for i in 0..blk.len() {
                let (Some(f), Some(point)) = (blk.fences.get(i), blk.value(i)) else { continue };
                let signal = f.flags & xf::I915_EXEC_FENCE_SIGNAL != 0;
                let has_fence = sync.syncobj(f.handle).is_some_and(|s| s.has_fence);
                if f.flags & EXEC_FENCE_UNKNOWN_FLAGS != 0 || (!has_fence && !signal) {
                    continue; // "a point might have been signaled already and garbage collected", gem/i915_gem_execbuffer.c:2872-2880
                }
                if f.flags & xf::I915_EXEC_FENCE_WAIT != 0 {
                    waits += 1;
                    sink.effect(Effect::AwaitSyncobj { rq: id, handle: f.handle, point });
                }
                if signal {
                    signals += 1;
                    sink.effect(Effect::SignalSyncobj { rq: id, handle: f.handle, point });
                }
            }
        }
        for i in 0..req.fences.len() {
            let Some(f) = req.fences.get(i) else { continue };
            if f.flags & xf::I915_EXEC_FENCE_WAIT != 0 {
                waits += 1;
                sink.effect(Effect::AwaitSyncobj { rq: id, handle: f.handle, point: 0 });
            }
            if f.flags & xf::I915_EXEC_FENCE_SIGNAL != 0 {
                signals += 1;
                sink.effect(Effect::SignalSyncobj { rq: id, handle: f.handle, point: 0 });
            }
        }
        if plan.out_fence {
            sink.effect(Effect::OutFence { rq: id }); // sync_file_create(&rq->fence), gem/i915_gem_execbuffer.c:3266-3270
        }
        (waits, signals)
    }
}

trait CheckedLt {
    fn checked_lt(self, n: usize) -> Option<usize>;
}
impl CheckedLt for usize {
    fn checked_lt(self, n: usize) -> Option<usize> {
        if self < n {
            Some(self)
        } else {
            None
        }
    }
}
