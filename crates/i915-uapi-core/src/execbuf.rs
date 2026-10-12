// SPDX-License-Identifier: GPL-2.0-only
//! `DRM_IOCTL_I915_GEM_EXECBUFFER2` VALIDATION: every refusal `i915_gem_execbuffer2_ioctl` / `i915_gem_do_execbuffer` makes
//! before it touches the GPU, over a decoded [`ExecbufRequest`]. No execution: this module returns an [`ExecPlan`] the GEM and
//! submit crates carry out (pinning, relocation patching, request construction) or the first named refusal.
//!
//! State Linux consults in the middle of validation (the context table, the object table, syncobjs, sync files) is reached through
//! [`ExecEnv`], which the device implements over its session tables; this crate holds none of it.
//! Ported from drivers/gpu/drm/i915/gem/i915_gem_execbuffer.c (v7.0-rc6): `i915_gem_do_execbuffer` (:3339-3534),
//! `eb_select_context` (:823), `eb_select_engine` / `eb_select_legacy_ring` (:2626-2754), `eb_lookup_vmas` (:941),
//! `eb_validate_vma` (:500), `eb_add_vma` (:561), `add_fence_array` (:2919), `add_timeline_fence_array` (:2786) and
//! `eb_relocate_entry` (:1409), plus the softpin range test of `i915_vma_insert` (i915_vma.c:854-858).
use crate::consts::domain as d;
use crate::consts::exec as ef;
use crate::consts::exec_fence as xf;
use crate::consts::exec_object as eo;
use crate::device::UapiDevice;
use crate::error::UapiError;
use crate::execbuf_wire::ExecbufRequest;
use crate::types::{engine_class, ExecObject2, RelocationEntry};

/// `I915_GTT_PAGE_SIZE` (gt/intel_gtt.h:45).
pub const GTT_PAGE_SIZE: u64 = 4096; // intel_gtt.h:45
/// `GEN8_HIGH_ADDRESS_BIT` (gt/intel_gpu_commands.h:456).
pub const GEN8_HIGH_ADDRESS_BIT: u32 = 47; // intel_gpu_commands.h:456
/// `I915_GEM_GPU_DOMAINS` (i915_gem.h:42-47).
pub const GPU_DOMAINS: u32 = d::I915_GEM_DOMAIN_RENDER | d::I915_GEM_DOMAIN_SAMPLER | d::I915_GEM_DOMAIN_COMMAND
    | d::I915_GEM_DOMAIN_INSTRUCTION | d::I915_GEM_DOMAIN_VERTEX; // i915_gem.h:42
/// `__EXEC_OBJECT_UNKNOWN_FLAGS` = `-(EXEC_OBJECT_CAPTURE << 1)` (i915_drm.h:1308).
pub const EXEC_OBJECT_UNKNOWN_FLAGS: u64 = (eo::EXEC_OBJECT_CAPTURE << 1).wrapping_neg(); // i915_drm.h:1308
/// `__I915_EXEC_FENCE_UNKNOWN_FLAGS` = `-(I915_EXEC_FENCE_SIGNAL << 1)` (i915_drm.h:1343).
pub const EXEC_FENCE_UNKNOWN_FLAGS: u32 = (xf::I915_EXEC_FENCE_SIGNAL << 1).wrapping_neg(); // i915_drm.h:1343

/// `gen8_canonical_addr` (gt/intel_gpu_commands.h:457-460): sign-extend from bit 47.
pub const fn canonical_addr(a: u64) -> u64 {
    (((a << (63 - GEN8_HIGH_ADDRESS_BIT)) as i64) >> (63 - GEN8_HIGH_ADDRESS_BIT)) as u64
}
/// `gen8_noncanonical_addr` (gt/intel_gpu_commands.h:462-465): keep bits 47:0.
pub const fn noncanonical_addr(a: u64) -> u64 {
    a & ((1u64 << (GEN8_HIGH_ADDRESS_BIT + 1)) - 1)
}

/// What the context table knows about a context id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextInfo {
    /// `Some(n)` once userspace set `I915_CONTEXT_PARAM_ENGINES` with `n` slots (`i915_gem_context_user_engines`); `None` for a
    /// legacy ring-selector context.
    pub user_engines: Option<u32>,
    /// `true` when `engines[idx]` for the slot used is a real engine (not an `INVALID` placeholder).
    pub user_engine_present: bool,
}

/// What the object table knows about a GEM handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectInfo {
    /// `vma->size`: the object size in bytes.
    pub size: u64,
    /// `i915_vma_offset(vma)` if the object is currently bound in the context's address space.
    pub bound_offset: Option<u64>,
}

/// What the syncobj table knows about a syncobj handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncobjInfo {
    /// `drm_syncobj_fence_get()` returned a fence.
    pub has_fence: bool,
    /// `Some(seqno)` when that fence is a `dma_fence_chain` (a timeline syncobj) whose newest point is `seqno`.
    pub chain_seqno: Option<u64>,
}

/// The session state the validator reads. The device implements it over its tables; tests over fixed ones.
pub trait ExecEnv {
    fn context(&self, id: u32) -> Option<ContextInfo>;
    fn object(&self, handle: u32) -> Option<ObjectInfo>;
    fn syncobj(&self, handle: u32) -> Option<SyncobjInfo>;
    /// `sync_file_get_fence(fd)` for the flattened in-fence token.
    fn sync_file(&self, token: u32) -> bool;
}

/// Which engine the submit runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineChoice {
    /// A specific user-visible engine of a legacy context.
    Fixed { class: u16, instance: u16 },
    /// `I915_EXEC_BSD` with `BSD_DEFAULT` on a part with several VCS: the device balances (`gen8_dispatch_bsd_engine`).
    BalancedVideo,
    /// Slot `idx` of the context's own engine map.
    UserSlot(u32),
}

/// How the request waits on an in-fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InFence {
    None,
    /// `I915_EXEC_FENCE_IN`: wait for completion.
    Wait(u32),
    /// `I915_EXEC_FENCE_SUBMIT`: wait for submission.
    Submit(u32),
}

/// A validated submit, ready for the GEM and submit crates to execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecPlan {
    pub context_id: u32,
    pub engine: EngineChoice,
    /// Index into the object list of the batch (`BATCH_FIRST` -> 0, else the last).
    pub batch_index: usize,
    pub batch_start_offset: u32,
    /// The resolved length (`batch_len == 0` means "to the end of the object", i915_gem_execbuffer.c:618-620).
    pub batch_len: u64,
    pub handle_lut: bool,
    /// `I915_EXEC_NO_RELOC` was NOT given: offsets may change and must be written back (`__EXEC_HAS_RELOC`, :3358-3359).
    pub reports_offsets: bool,
    pub pinned_objects: u32,
    pub captured_objects: u32,
    pub relocs_total: u32,
    /// Relocations whose `presumed_offset` does not match the target's current placement: the ones to patch.
    pub relocs_to_patch: u32,
    pub fence_waits: u32,
    pub fence_signals: u32,
    pub in_fence: InFence,
    pub out_fence: bool,
}

/// Validate a decoded request against the device and the session state.
pub fn validate_execbuf2(dev: &UapiDevice, env: &dyn ExecEnv, req: &ExecbufRequest<'_>) -> Result<ExecPlan, UapiError> {
    let flags = req.flags;
    let gen = dev.gen();

    // i915_gem_do_execbuffer :3384-3394
    if flags & ef::I915_EXEC_SECURE != 0 {
        if gen >= 11 {
            return Err(UapiError::ExecSecureGen11 { ver: gen }); // :3385-3386
        }
        if gen >= 6 {
            // !HAS_SECURE_BATCHES(i915) (GRAPHICS_VER < 6, i915_drv.h:556): "-EPERM to trigger fallback code on old binaries"
            return Err(UapiError::ExecSecureUnsupported { ver: gen }); // :3389-3390
        }
    }

    // parse_execbuf2_extensions already ran in the decode; its syncobj lookups are state, so they run here, timeline blocks
    // first (:3400-3402), then the plain fence array (:3404-3406).
    let mut waits = 0u32;
    let mut signals = 0u32;
    for blk in req.timeline_blocks() {
        for i in 0..blk.len() {
            let f = blk.fences.get(i).ok_or(UapiError::Truncated { what: "timeline.fences", need: (i + 1) * 8, have: 0 })?;
            let point = blk.value(i).ok_or(UapiError::Truncated { what: "timeline.values", need: (i + 1) * 8, have: 0 })?;
            check_timeline_fence(env, f.handle, f.flags, point, &mut waits, &mut signals)?;
        }
    }
    for i in 0..req.fences.len() {
        let f = req.fences.get(i).ok_or(UapiError::Truncated { what: "fences", need: (i + 1) * 8, have: 0 })?;
        if f.flags & EXEC_FENCE_UNKNOWN_FLAGS != 0 {
            return Err(UapiError::ExecFenceFlags { flags: f.flags }); // :2959-2960
        }
        let s = env.syncobj(f.handle).ok_or(UapiError::ExecSyncobjMissing { handle: f.handle })?; // :2962-2967
        if f.flags & xf::I915_EXEC_FENCE_WAIT != 0 {
            if !s.has_fence {
                return Err(UapiError::ExecSyncobjNoFence { handle: f.handle }); // :2969-2977
            }
            waits += 1;
        }
        if f.flags & xf::I915_EXEC_FENCE_SIGNAL != 0 {
            signals += 1;
        }
    }

    // in-fence (:3408-3419)
    let in_fences = ef::I915_EXEC_FENCE_IN | ef::I915_EXEC_FENCE_SUBMIT;
    let in_fence = if flags & in_fences != 0 {
        if flags & in_fences == in_fences {
            return Err(UapiError::ExecInFenceBoth); // :3409-3411
        }
        let token = req.in_fence_token();
        if !env.sync_file(token) {
            return Err(UapiError::ExecInFenceMissing { token }); // :3413-3417
        }
        if flags & ef::I915_EXEC_FENCE_IN != 0 {
            InFence::Wait(token)
        } else {
            InFence::Submit(token)
        }
    } else {
        InFence::None
    };

    // eb_select_context (:823-836)
    let context_id = req.context_id();
    let ctx = env.context(context_id).ok_or(UapiError::ExecContextMissing { id: context_id })?;
    let mut invalid_flags = EXEC_OBJECT_UNKNOWN_FLAGS; // :3367
    if dev.has_full_ppgtt() {
        invalid_flags |= eo::EXEC_OBJECT_NEEDS_GTT; // :833
    }

    let engine = select_engine(dev, &ctx, flags)?;

    // eb_lookup_vmas (:941-978): lookup, eb_validate_vma, eb_add_vma, per object, in order.
    let count = req.objects.len();
    let batch_index = if flags & ef::I915_EXEC_BATCH_FIRST != 0 { 0 } else { count - 1 }; // is_batch_buffer :553-558
    let mut batch_len = 0u64;
    let mut pinned = 0u32;
    let mut captured = 0u32;
    for i in 0..count {
        let e = req.objects.get(i).ok_or(UapiError::Truncated { what: "objects", need: (i + 1) * ExecObject2::WIRE_BYTES, have: 0 })?;
        let info = env.object(e.handle).ok_or(UapiError::ExecObjectMissing { handle: e.handle })?; // :907
        validate_object(gen, &e, invalid_flags)?;
        if e.flags & eo::EXEC_OBJECT_PINNED != 0 {
            pinned += 1;
        }
        if e.flags & eo::EXEC_OBJECT_CAPTURE != 0 {
            captured += 1;
        }
        if i == batch_index {
            batch_len = check_batch(&e, &info, req.batch_start_offset, req.batch_len)?;
        }
    }

    // eb_reserve: a softpinned range must fit the address space and not collide with another pinned object (i915_vma.c:854-858).
    check_pinned_ranges(dev, env, req)?;

    // eb_relocate_parse: per-relocation checks (eb_relocate_entry :1409-1509)
    let relocs_to_patch = check_relocs(dev, env, req)?;

    Ok(ExecPlan {
        context_id,
        engine,
        batch_index,
        batch_start_offset: req.batch_start_offset,
        batch_len,
        handle_lut: flags & ef::I915_EXEC_HANDLE_LUT != 0,
        reports_offsets: flags & ef::I915_EXEC_NO_RELOC == 0,
        pinned_objects: pinned,
        captured_objects: captured,
        relocs_total: req.relocs.len() as u32,
        relocs_to_patch,
        fence_waits: waits,
        fence_signals: signals,
        in_fence,
        out_fence: flags & ef::I915_EXEC_FENCE_OUT != 0,
    })
}

/// `add_timeline_fence_array` per-entry rules (i915_gem_execbuffer.c:2837-2909) for entries that are kept.
fn check_timeline_fence(env: &dyn ExecEnv, handle: u32, flags: u32, point: u64, waits: &mut u32, signals: &mut u32) -> Result<(), UapiError> {
    if flags & EXEC_FENCE_UNKNOWN_FLAGS != 0 {
        return Err(UapiError::ExecFenceFlags { flags }); // :2837-2838
    }
    let s = env.syncobj(handle).ok_or(UapiError::ExecSyncobjMissing { handle })?; // :2843-2847
    let signal = flags & xf::I915_EXEC_FENCE_SIGNAL != 0;
    if !s.has_fence && flags != 0 && !signal {
        return Err(UapiError::ExecSyncobjNoFence { handle }); // :2852-2858
    }
    // dma_fence_chain_find_seqno: point 0 is "the latest"; otherwise the head must be a chain at least that new (-EINVAL).
    let find_err = s.has_fence && point != 0 && !matches!(s.chain_seqno, Some(head) if head >= point);
    if find_err && !signal {
        return Err(UapiError::ExecTimelinePointMissing { handle, point }); // :2860-2870
    }
    if !s.has_fence && !signal {
        return Ok(()); // :2872-2880 "a point might have been signaled already and garbage collected": skipped, not kept
    }
    if point != 0 && signal && flags & xf::I915_EXEC_FENCE_WAIT != 0 {
        return Err(UapiError::ExecTimelineWaitAndSignal { handle, point }); // :2886-2897
    }
    if flags & xf::I915_EXEC_FENCE_WAIT != 0 {
        *waits += 1;
    }
    if signal {
        *signals += 1;
    }
    Ok(())
}

/// `eb_select_engine` -> `eb_select_legacy_ring` (i915_gem_execbuffer.c:2626-2700) with `i915_gem_context_get_engine`
/// (i915_gem_context.h:205-220).
fn select_engine(dev: &UapiDevice, ctx: &ContextInfo, flags: u64) -> Result<EngineChoice, UapiError> {
    let invalid = |why: &'static str| UapiError::ExecEngineInvalid { flags, why };
    if let Some(n) = ctx.user_engines {
        let idx = (flags & ef::I915_EXEC_RING_MASK) as u32; // :2677-2678
        if idx >= n || !ctx.user_engine_present {
            return Err(invalid("engine slot outside the context's engine map")); // i915_gem_context.h:213-216
        }
        return Ok(EngineChoice::UserSlot(idx));
    }
    let engines = dev.engines();
    let ring = flags & ef::I915_EXEC_RING_MASK;
    if ring != ef::I915_EXEC_BSD && flags & ef::I915_EXEC_BSD_MASK != 0 {
        return Err(invalid("BSD dispatch flags on a non-BSD ring")); // :2632-2638
    }
    if ring == ef::I915_EXEC_BSD && engines.class_count(engine_class::VIDEO) > 1 {
        let bsd = flags & ef::I915_EXEC_BSD_MASK;
        if bsd == ef::I915_EXEC_BSD_DEFAULT {
            return Ok(EngineChoice::BalancedVideo); // :2644-2645
        }
        if bsd == ef::I915_EXEC_BSD_RING1 || bsd == ef::I915_EXEC_BSD_RING2 {
            let instance = ((bsd >> ef::I915_EXEC_BSD_SHIFT) - 1) as u16; // :2646-2649
            return match engines.lookup(engine_class::VIDEO, instance) {
                Some(_) => Ok(EngineChoice::Fixed { class: engine_class::VIDEO, instance }),
                None => Err(invalid("no such VCS instance")),
            };
        }
        return Err(invalid("unknown BSD ring")); // :2650-2655
    }
    // user_ring_map[] (:2457-2463): DEFAULT and RENDER -> RCS0, BLT -> BCS0, BSD -> VCS0, VEBOX -> VECS0
    let (class, instance) = match ring {
        ef::I915_EXEC_DEFAULT | ef::I915_EXEC_RENDER => (engine_class::RENDER, 0),
        ef::I915_EXEC_BLT => (engine_class::COPY, 0),
        ef::I915_EXEC_BSD => (engine_class::VIDEO, 0),
        ef::I915_EXEC_VEBOX => (engine_class::VIDEO_ENHANCE, 0),
        _ => return Err(invalid("unknown ring")), // :2660-2664
    };
    match engines.lookup(class, instance) {
        Some(_) => Ok(EngineChoice::Fixed { class, instance }),
        None => Err(invalid("this device has no such engine")),
    }
}

/// `eb_validate_vma` (i915_gem_execbuffer.c:500-558), the parts that are properties of the entry itself.
fn validate_object(gen: u8, e: &ExecObject2, invalid_flags: u64) -> Result<(), UapiError> {
    if e.relocation_count != 0 && gen >= 12 {
        return Err(UapiError::ExecRelocsGen12 { ver: gen }); // :507-509 (Tigerlake, the one exception, is not a carried row)
    }
    if e.flags & invalid_flags != 0 {
        return Err(UapiError::ExecObjectFlags { handle: e.handle, flags: e.flags, invalid: e.flags & invalid_flags }); // :511-512
    }
    if e.alignment != 0 && !e.alignment.is_power_of_two() {
        return Err(UapiError::ExecAlignmentNotPow2 { handle: e.handle, alignment: e.alignment }); // :514-516
    }
    if e.flags & eo::EXEC_OBJECT_PINNED != 0 && e.offset != canonical_addr(e.offset & !(GTT_PAGE_SIZE - 1)) {
        return Err(UapiError::ExecPinnedOffset { handle: e.handle, offset: e.offset }); // :522-524
    }
    if e.flags & eo::EXEC_OBJECT_PAD_TO_SIZE != 0 && e.pad_to_size & (GTT_PAGE_SIZE - 1) != 0 {
        return Err(UapiError::ExecPadToSize { handle: e.handle, pad_to_size: e.pad_to_size }); // :527-529
    }
    Ok(())
}

/// The batch checks of `eb_add_vma` (:593-626). Returns the resolved batch length.
fn check_batch(e: &ExecObject2, info: &ObjectInfo, start: u32, len: u32) -> Result<u64, UapiError> {
    if e.flags & eo::EXEC_OBJECT_WRITE != 0 {
        return Err(UapiError::ExecBatchWritable { handle: e.handle }); // :602-606
    }
    let (start64, len64, size) = (u64::from(start), u64::from(len), info.size);
    // range_overflows_t(u64, start, len, size): start > size || len > size - start
    if start64 > size || len64 > size - start64 {
        return Err(UapiError::ExecBatchOutOfBounds { start, len, size }); // :608-614
    }
    let resolved = if len == 0 { size - start64 } else { len64 }; // :616-620
    if resolved == 0 {
        return Err(UapiError::ExecBatchLenZero); // :621-624
    }
    Ok(resolved)
}

/// The softpin placement test: `i915_vma_insert` with `PIN_OFFSET_FIXED` (i915_vma.c:828-858) and the reservation collision of
/// `i915_gem_gtt_reserve` (i915_gem_gtt.c:115-120, reached from `eb_reserve_vma` :655-694). Offsets are compared in non-canonical form (:538).
fn check_pinned_ranges(dev: &UapiDevice, env: &dyn ExecEnv, req: &ExecbufRequest<'_>) -> Result<(), UapiError> {
    let total = 1u64 << dev.desc.info.ppgtt_size; // vm->total
    let zone_4g = (1u64 << 32) - GTT_PAGE_SIZE; // PIN_ZONE_4G (:831-832), i915_vma.c
    for i in 0..req.objects.len() {
        let Some(e) = req.objects.get(i) else { continue };
        if e.flags & eo::EXEC_OBJECT_PINNED == 0 {
            continue;
        }
        let Some(info) = env.object(e.handle) else { continue };
        let pad = if e.flags & eo::EXEC_OBJECT_PAD_TO_SIZE != 0 { e.pad_to_size } else { 0 };
        let size = info.size.max(pad);
        let alignment = e.alignment.max(GTT_PAGE_SIZE);
        let end = if e.flags & eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS != 0 { total } else { total.min(zone_4g) };
        let off = noncanonical_addr(e.offset);
        // !IS_ALIGNED(offset, alignment) || range_overflows(offset, size, end)
        if !off.is_multiple_of(alignment) || off > end || size > end - off {
            return Err(UapiError::ExecPinnedRange { handle: e.handle, offset: off, size, end });
        }
        for j in 0..i {
            let Some(o) = req.objects.get(j) else { continue };
            if o.flags & eo::EXEC_OBJECT_PINNED == 0 || o.handle == e.handle {
                continue;
            }
            let Some(oi) = env.object(o.handle) else { continue };
            let opad = if o.flags & eo::EXEC_OBJECT_PAD_TO_SIZE != 0 { o.pad_to_size } else { 0 };
            let (ooff, osize) = (noncanonical_addr(o.offset), oi.size.max(opad));
            if off < ooff.saturating_add(osize) && ooff < off.saturating_add(size) {
                return Err(UapiError::ExecPinnedOverlap { a: o.handle, b: e.handle });
            }
        }
    }
    Ok(())
}

/// Resolve a relocation's `target_handle`: an index under `HANDLE_LUT`, a GEM handle otherwise (`eb_get_vma`, :1049-1066).
fn find_target(req: &ExecbufRequest<'_>, target: u32) -> Option<ExecObject2> {
    if req.flags & ef::I915_EXEC_HANDLE_LUT != 0 {
        req.objects.get(target as usize)
    } else {
        req.objects.iter().find(|o| o.handle == target)
    }
}

/// `eb_relocate_entry` for every relocation, in object order. Returns how many need patching.
fn check_relocs(dev: &UapiDevice, env: &dyn ExecEnv, req: &ExecbufRequest<'_>) -> Result<u32, UapiError> {
    let reloc_width: u64 = if dev.gen() >= 8 { 8 } else { 4 }; // use_64bit_reloc
    let mut next = 0usize;
    let mut to_patch = 0u32;
    for i in 0..req.objects.len() {
        let Some(owner) = req.objects.get(i) else { continue };
        let owner_size = env.object(owner.handle).map(|o| o.size).unwrap_or(0);
        for _ in 0..owner.relocation_count {
            let r: RelocationEntry = req.relocs.get(next).ok_or(UapiError::Truncated { what: "relocs", need: (next + 1) * 32, have: 0 })?;
            next += 1;
            let target = find_target(req, r.target_handle)
                .ok_or(UapiError::ExecRelocTargetMissing { handle: owner.handle, target: r.target_handle })?; // :1418-1420
            if r.write_domain & r.write_domain.wrapping_sub(1) != 0 {
                return Err(UapiError::ExecRelocWriteDomains { write_domain: r.write_domain }); // :1423-1433
            }
            if (r.write_domain | r.read_domains) & !GPU_DOMAINS != 0 {
                return Err(UapiError::ExecRelocDomain { read_domains: r.read_domains, write_domain: r.write_domain }); // :1434-1445
            }
            // :1472-1477 already right: the target's placement equals presumed_offset -> nothing to patch, no further checks.
            let placement = if target.flags & eo::EXEC_OBJECT_PINNED != 0 {
                Some(noncanonical_addr(target.offset))
            } else {
                env.object(target.handle).and_then(|o| o.bound_offset)
            };
            if placement.map(canonical_addr) == Some(r.presumed_offset) {
                continue;
            }
            if r.offset > owner_size.wrapping_sub(reloc_width) {
                return Err(UapiError::ExecRelocBounds { handle: owner.handle, offset: r.offset, size: owner_size }); // :1480-1488
            }
            if r.offset & 3 != 0 {
                return Err(UapiError::ExecRelocUnaligned { handle: owner.handle, offset: r.offset }); // :1489-1494
            }
            to_patch += 1;
        }
    }
    Ok(to_patch)
}
