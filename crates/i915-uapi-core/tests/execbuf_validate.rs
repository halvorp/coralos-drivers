// SPDX-License-Identifier: GPL-2.0-only
//! EXECBUFFER2 validation: each case Linux refuses is refused here with the same errno, each cited to the check in
//! gem/i915_gem_execbuffer.c (v7.0-rc6); and the iris / anv shaped submits are accepted. Validation only: nothing is executed.
mod common;
use common::*;
use i915_uapi_core::consts::exec as ef;
use i915_uapi_core::consts::exec_fence as xf;
use i915_uapi_core::consts::exec_object as eo;
use i915_uapi_core::types::*;
use i915_uapi_core::{decode_execbuf2, validate_execbuf2, ContextInfo, EngineChoice, Errno, ExecLimits, ExecPlan, InFence, ObjectInfo, SyncobjInfo, UapiDevice};

const MIB: u64 = 1 << 20;

fn check(dev: &UapiDevice, env: &Env, e: &Exec) -> Result<ExecPlan, (&'static str, Errno)> {
    let b = e.build();
    let req = decode_execbuf2(&b, &ExecLimits::DEFAULT).map_err(|x| (x.name(), x.errno()))?;
    validate_execbuf2(dev, env, &req).map_err(|x| (x.name(), x.errno()))
}
fn bo(handle: u32) -> ExecObject2 {
    ExecObject2 { handle, ..Default::default() }
}
fn flags(f: u64) -> ExecObject2 {
    ExecObject2 { handle: 10, flags: f, ..Default::default() }
}
/// A one-object submit whose only object is `o`.
fn one(o: ExecObject2) -> Exec {
    Exec::new(0, vec![o])
}
fn err(r: Result<ExecPlan, (&'static str, Errno)>) -> (&'static str, Errno) {
    r.unwrap_err()
}

#[test]
fn iris_shaped_submit_is_accepted_and_planned() {
    // iris_kmd_backend.c:395-410: BATCH_FIRST|HANDLE_LUT|NO_RELOC|RENDER, all objects softpinned 48-bit, batch first.
    let f = ef::I915_EXEC_RENDER | ef::I915_EXEC_NO_RELOC | ef::I915_EXEC_BATCH_FIRST | ef::I915_EXEC_HANDLE_LUT;
    let mut o = vec![pinned(10, 0x10_0000), pinned(11, 0x20_0000), pinned(12, 0x1_0000_0000)];
    o[1].flags |= eo::EXEC_OBJECT_WRITE;
    let mut e = Exec::new(f, o);
    e.len = 4096;
    let p = check(&gt2(), &Env::basic(), &e).unwrap();
    assert_eq!(p.context_id, 1);
    assert_eq!(p.engine, EngineChoice::Fixed { class: 0, instance: 0 });
    assert_eq!((p.batch_index, p.batch_start_offset, p.batch_len), (0, 0, 4096));
    assert_eq!((p.pinned_objects, p.relocs_total, p.relocs_to_patch), (3, 0, 0));
    assert!(p.handle_lut && !p.reports_offsets && !p.out_fence);
    assert_eq!(p.in_fence, InFence::None);
}

#[test]
fn anv_shaped_submit_with_timeline_fences_is_accepted() {
    // anv_batch_chain.c:561,577-581: HANDLE_LUT|NO_RELOC, USE_EXTENSIONS with a TIMELINE_FENCES block; batch is the LAST object.
    let mut env = Env::basic();
    env.syncobjs = vec![
        (1, SyncobjInfo { has_fence: true, chain_seqno: Some(10) }),
        (2, SyncobjInfo { has_fence: false, chain_seqno: None }),
    ];
    let f = ef::I915_EXEC_HANDLE_LUT | ef::I915_EXEC_NO_RELOC | ef::I915_EXEC_USE_EXTENSIONS;
    let mut e = Exec::new(f, vec![pinned(10, 0x10_0000), pinned(11, 0x20_0000)]);
    e.exts = vec![TimelineExt::new(
        vec![ExecFence { handle: 1, flags: xf::I915_EXEC_FENCE_WAIT }, ExecFence { handle: 2, flags: xf::I915_EXEC_FENCE_SIGNAL }],
        vec![7, 11],
    )];
    let p = check(&gt2(), &env, &e).unwrap();
    assert_eq!(p.batch_index, 1);
    assert_eq!((p.fence_waits, p.fence_signals), (1, 1));
}

#[test]
fn secure_batches_are_refused_on_gen9() {
    // :3384-3391: Gen11+ -> ENODEV; no secure-batch support (Gen6+) -> EPERM "to trigger fallback code"
    let e = Exec::new(ef::I915_EXEC_SECURE, vec![bo(10)]);
    assert_eq!(err(check(&gt2(), &Env::basic(), &e)), ("EXEC-SECURE-UNSUPPORTED", Errno::EPERM));
}

#[test]
fn context_lookup() {
    // eb_select_context :823-829 -> i915_gem_context_lookup -> -ENOENT
    let mut e = one(bo(10));
    e.rsvd1 = 99;
    assert_eq!(err(check(&gt2(), &Env::basic(), &e)), ("EXEC-CONTEXT-MISSING", Errno::ENOENT));
    // only the low 32 bits of rsvd1 select the context
    e.rsvd1 = (0xffff_ffffu64 << 32) | 1;
    assert!(check(&gt2(), &Env::basic(), &e).is_ok());
}

#[test]
fn in_fence_rules() {
    let mut env = Env::basic();
    env.sync_files = vec![42];
    // FENCE_IN together with FENCE_SUBMIT -> -EINVAL (:3442-3443)
    let mut e = Exec::new(ef::I915_EXEC_FENCE_IN | ef::I915_EXEC_FENCE_SUBMIT, vec![bo(10)]);
    e.rsvd2 = 42;
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-IN-FENCE-BOTH", Errno::EINVAL));
    // an in-fence token with no sync file behind it -> -EINVAL (:3445-3448)
    let mut e = Exec::new(ef::I915_EXEC_FENCE_IN, vec![bo(10)]);
    e.rsvd2 = 41;
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-IN-FENCE-MISSING", Errno::EINVAL));
    // both forms are planned when the sync file exists; only the low half of rsvd2 is the token
    e.rsvd2 = (7u64 << 32) | 42;
    assert_eq!(check(&gt2(), &env, &e).unwrap().in_fence, InFence::Wait(42));
    let mut e = Exec::new(ef::I915_EXEC_FENCE_SUBMIT | ef::I915_EXEC_FENCE_OUT, vec![bo(10)]);
    e.rsvd2 = 42;
    let p = check(&gt2(), &env, &e).unwrap();
    assert_eq!((p.in_fence, p.out_fence), (InFence::Submit(42), true));
}

#[test]
fn object_lookup_and_flags() {
    // eb_lookup_vma -> -ENOENT (:907)
    assert_eq!(err(check(&gt2(), &Env::basic(), &one(bo(77)))), ("EXEC-OBJECT-MISSING", Errno::ENOENT));
    // eb_validate_vma :511-512: unknown bits are invalid; NEEDS_GTT is invalid under full PPGTT (eb_select_context :833) -- Gen9 is full
    for f in [1u64 << 8, 1 << 40, 1 << 63] {
        assert_eq!(err(check(&gt2(), &Env::basic(), &one(flags(f)))), ("EXEC-OBJECT-FLAGS", Errno::EINVAL), "flags {f:#x}");
    }
    assert_eq!(err(check(&gt2(), &Env::basic(), &one(flags(eo::EXEC_OBJECT_NEEDS_GTT)))), ("EXEC-OBJECT-FLAGS", Errno::EINVAL));
    // the defined ones that are legal here are accepted
    let ok = eo::EXEC_OBJECT_NEEDS_FENCE | eo::EXEC_OBJECT_WRITE | eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS | eo::EXEC_OBJECT_ASYNC | eo::EXEC_OBJECT_CAPTURE;
    let mut e = one(flags(ok));
    e.objects[0].flags &= !eo::EXEC_OBJECT_WRITE; // the only object is the batch, which may not be written
    let p = check(&gt2(), &Env::basic(), &e).unwrap();
    assert_eq!(p.captured_objects, 1);
}

#[test]
fn alignment_must_be_a_power_of_two() {
    // :514-516
    for a in [3u64, 0x3000, 0x1_0001] {
        let mut o = bo(10);
        o.alignment = a;
        assert_eq!(err(check(&gt2(), &Env::basic(), &one(o))), ("EXEC-ALIGNMENT-NOT-POW2", Errno::EINVAL), "alignment {a:#x}");
    }
    for a in [0u64, 1, 0x1000, 0x1_0000, 1 << 32] {
        let mut o = bo(10);
        o.alignment = a;
        assert!(check(&gt2(), &Env::basic(), &one(o)).is_ok(), "alignment {a:#x}");
    }
}

#[test]
fn softpin_offset_must_be_page_aligned_and_canonical() {
    // :522-524: offset == canonical(offset & PAGE_MASK)
    for off in [0x1001u64, 0x10_0800, 0x8000_0000_0000 + 0x1000 /* bit 47 set but bits 63:48 clear */, 0xffff_0000_0000_1000 /* bit 47 clear, bits 63:48 set */] {
        assert_eq!(err(check(&gt2(), &Env::basic(), &one(pinned(10, off)))), ("EXEC-PINNED-OFFSET", Errno::EINVAL), "offset {off:#x}");
    }
    // the same bad offsets are NOT checked without PINNED (the offset is only a hint)
    let mut o = bo(10);
    o.offset = 0x1001;
    assert!(check(&gt2(), &Env::basic(), &one(o)).is_ok());
    // a canonical high address (bit 47 set, 63:48 set) is a valid softpin; its non-canonical form is below the 48-bit end
    assert!(check(&gt2(), &Env::basic(), &one(pinned(10, 0xffff_8000_0000_0000))).is_ok());
}

#[test]
fn pad_to_size_must_be_page_aligned() {
    // :527-529; without the flag the field is ignored (:530-532 zeroes it)
    let mut o = bo(10);
    o.flags = eo::EXEC_OBJECT_PAD_TO_SIZE;
    o.pad_to_size = 0x1800;
    assert_eq!(err(check(&gt2(), &Env::basic(), &one(o))), ("EXEC-PAD-TO-SIZE", Errno::EINVAL));
    o.pad_to_size = 0x2000;
    assert!(check(&gt2(), &Env::basic(), &one(o)).is_ok());
    o.flags = 0;
    o.pad_to_size = 0x1800;
    assert!(check(&gt2(), &Env::basic(), &one(o)).is_ok());
}

#[test]
fn softpin_range_must_fit_the_address_space() {
    let env = Env::basic(); // 1 MiB objects
    // i915_vma_insert :854-858: range_overflows(offset, size, end) with end = vm->total = 1 << 48
    let top = (1u64 << 48) - MIB;
    // the canonical form of the top 1 MiB of the 48-bit space has bit 47 set (and so bits 63:48)
    let mut o = pinned(10, i915_uapi_core::execbuf::canonical_addr(top));
    assert!(check(&gt2(), &env, &one(o)).is_ok(), "ends exactly at 1 << 48");
    o.offset = i915_uapi_core::execbuf::canonical_addr(top + 0x1000);
    assert_eq!(err(check(&gt2(), &env, &one(o))), ("EXEC-PINNED-RANGE", Errno::EINVAL), "one page past the end");
    // an object without SUPPORTS_48B_ADDRESS must end below 4 GiB - 4 KiB (PIN_ZONE_4G, :831-832)
    let mut o = pinned(10, (1 << 32) - MIB - 0x1000);
    o.flags &= !eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS;
    assert!(check(&gt2(), &env, &one(o)).is_ok(), "ends exactly at 4 GiB - 4 KiB");
    o.offset += 0x1000;
    assert_eq!(err(check(&gt2(), &env, &one(o))), ("EXEC-PINNED-RANGE", Errno::EINVAL), "ends at 4 GiB");
    // the same offset is fine with SUPPORTS_48B_ADDRESS
    o.flags |= eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS;
    assert!(check(&gt2(), &env, &one(o)).is_ok());
    // !IS_ALIGNED(offset, alignment) -> -EINVAL (:857)
    let mut o = pinned(10, 0x1000);
    o.alignment = 0x1_0000;
    assert_eq!(err(check(&gt2(), &env, &one(o))), ("EXEC-PINNED-RANGE", Errno::EINVAL));
    o.offset = 0x1_0000;
    assert!(check(&gt2(), &env, &one(o)).is_ok());
    // pad_to_size grows the range that must fit
    let mut o = pinned(10, i915_uapi_core::execbuf::canonical_addr(top));
    o.flags |= eo::EXEC_OBJECT_PAD_TO_SIZE;
    o.pad_to_size = 2 * MIB;
    assert_eq!(err(check(&gt2(), &env, &one(o))), ("EXEC-PINNED-RANGE", Errno::EINVAL));
}

#[test]
fn two_softpinned_objects_may_not_overlap() {
    let env = Env::basic();
    // eb_reserve_vma :667-676 / drm_mm reserve: a second reservation of an occupied range fails with -ENOSPC
    let e = Exec::new(0, vec![pinned(10, 0x10_0000), pinned(11, 0x10_0000 + MIB - 0x1000)]);
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-PINNED-OVERLAP", Errno::ENOSPC));
    // touching ranges do not overlap
    let e = Exec::new(0, vec![pinned(10, 0x10_0000), pinned(11, 0x10_0000 + MIB)]);
    assert!(check(&gt2(), &env, &e).is_ok());
    // a pinned object entirely BEFORE an earlier-listed one is disjoint (both halves of the interval test matter)
    let e = Exec::new(0, vec![pinned(11, 0x30_0000), pinned(10, 0x10_0000)]);
    assert!(check(&gt2(), &env, &e).is_ok());
    // order does not matter
    let e = Exec::new(0, vec![pinned(11, 0x20_0000), pinned(10, 0x10_0000 + MIB / 2 * 3)]);
    assert!(check(&gt2(), &env, &e).is_err());
    // an unpinned object never collides at validation time
    let e = Exec::new(0, vec![pinned(10, 0x10_0000), bo(11)]);
    assert!(check(&gt2(), &env, &e).is_ok());
}

#[test]
fn batch_object_rules() {
    let env = Env::basic(); // 1 MiB objects
    // self-modifying batch (:603-607)
    let mut o = bo(10);
    o.flags = eo::EXEC_OBJECT_WRITE;
    assert_eq!(err(check(&gt2(), &env, &one(o))), ("EXEC-BATCH-WRITABLE", Errno::EINVAL));
    // a WRITE flag on a NON-batch object is fine
    let mut w = bo(10);
    w.flags = eo::EXEC_OBJECT_WRITE;
    assert!(check(&gt2(), &env, &Exec::new(0, vec![w, bo(11)])).is_ok());
    // out-of-bounds (:610-614): range_overflows_t(u64, start, len, size)
    let mut e = one(bo(10));
    e.start = (MIB - 8) as u32;
    e.len = 16;
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-BATCH-OUT-OF-BOUNDS", Errno::EINVAL));
    e.len = 8;
    assert_eq!(check(&gt2(), &env, &e).unwrap().batch_len, 8, "ends exactly at the object's end");
    // batch_len == 0 means "to the end" (:616-620)
    let mut e = one(bo(10));
    e.start = 4096;
    assert_eq!(check(&gt2(), &env, &e).unwrap().batch_len, MIB - 4096);
    // ... which is zero when start == size (:621-624)
    e.start = MIB as u32;
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-BATCH-LEN-ZERO", Errno::EINVAL));
    // start beyond the object
    e.start = MIB as u32 + 8;
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-BATCH-OUT-OF-BOUNDS", Errno::EINVAL));
}

#[test]
fn batch_position_follows_batch_first() {
    let env = Env::basic();
    // is_batch_buffer (:550-554): last object by default, first with BATCH_FIRST. The WRITE flag marks which one was checked.
    let mut first = bo(10);
    first.flags = eo::EXEC_OBJECT_WRITE;
    let last = bo(11);
    let e = Exec::new(0, vec![first, last]);
    assert_eq!(check(&gt2(), &env, &e).unwrap().batch_index, 1);
    let e = Exec::new(ef::I915_EXEC_BATCH_FIRST, vec![first, last]);
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-BATCH-WRITABLE", Errno::EINVAL));
}

fn ring(f: u64) -> Result<EngineChoice, (&'static str, Errno)> {
    check(&gt2(), &Env::basic(), &Exec::new(f, vec![bo(10)])).map(|p| p.engine)
}

#[test]
fn legacy_ring_selection_on_gt2() {
    // user_ring_map[] (:2457-2463)
    let fixed = |class, instance| Ok(EngineChoice::Fixed { class, instance });
    assert_eq!(ring(ef::I915_EXEC_DEFAULT), fixed(0, 0));
    assert_eq!(ring(ef::I915_EXEC_RENDER), fixed(0, 0));
    assert_eq!(ring(ef::I915_EXEC_BLT), fixed(1, 0));
    assert_eq!(ring(ef::I915_EXEC_BSD), fixed(2, 0));
    assert_eq!(ring(ef::I915_EXEC_VEBOX), fixed(3, 0));
    // an id beyond the map -> -EINVAL (:2660-2664)
    for r in [5u64, 6, 0x3f] {
        assert_eq!(ring(r), Err(("EXEC-ENGINE-INVALID", Errno::EINVAL)), "ring {r}");
    }
    // BSD dispatch bits on a non-BSD ring -> -EINVAL (:2633-2638)
    assert_eq!(ring(ef::I915_EXEC_RENDER | ef::I915_EXEC_BSD_RING1), Err(("EXEC-ENGINE-INVALID", Errno::EINVAL)));
    // GT2 has ONE video engine: the BSD bits are ignored and the ring is VCS0 (the multi-VCS branch :2640-2656 is not taken)
    assert_eq!(ring(ef::I915_EXEC_BSD | ef::I915_EXEC_BSD_RING2), fixed(2, 0));
}

#[test]
fn legacy_bsd_dispatch_on_a_two_vdbox_part() {
    let run = |f: u64| check(&gt3(), &Env::basic(), &Exec::new(f, vec![bo(10)])).map(|p| p.engine).map_err(|e| e.0);
    let fixed = |class, instance| Ok(EngineChoice::Fixed { class, instance });
    // :2640-2656
    assert_eq!(run(ef::I915_EXEC_BSD | ef::I915_EXEC_BSD_DEFAULT), Ok(EngineChoice::BalancedVideo));
    assert_eq!(run(ef::I915_EXEC_BSD | ef::I915_EXEC_BSD_RING1), fixed(2, 0));
    assert_eq!(run(ef::I915_EXEC_BSD | ef::I915_EXEC_BSD_RING2), fixed(2, 1));
    assert_eq!(run(ef::I915_EXEC_BSD | ef::I915_EXEC_BSD_MASK), Err("EXEC-ENGINE-INVALID"), "BSD ring selector 3");
}

#[test]
fn engine_must_exist_on_the_device() {
    // i915_gem_context_get_engine: a ring the device does not have -> -EINVAL (i915_gem_context.h:213-216)
    let mut d = gt2();
    d.desc.info.engine_mask &= !(1 << i915_devinfo_core::engine::VECS0);
    assert_eq!(err(check(&d, &Env::basic(), &Exec::new(ef::I915_EXEC_VEBOX, vec![bo(10)]))), ("EXEC-ENGINE-INVALID", Errno::EINVAL));
    assert!(check(&d, &Env::basic(), &Exec::new(ef::I915_EXEC_RENDER, vec![bo(10)])).is_ok());
}

#[test]
fn user_engine_maps() {
    // :2674-2675: with a CONTEXT_PARAM_ENGINES map the ring field is the slot index
    let mut env = Env::basic();
    env.contexts = vec![(1, ContextInfo { user_engines: Some(2), user_engine_present: true })];
    let go = |f: u64| check(&gt2(), &env, &Exec::new(f, vec![bo(10)])).map(|p| p.engine).map_err(|e| e.0);
    assert_eq!(go(1), Ok(EngineChoice::UserSlot(1)));
    assert_eq!(go(0), Ok(EngineChoice::UserSlot(0)));
    assert_eq!(go(2), Err("EXEC-ENGINE-INVALID"), "slot 2 of 2");
    // legacy ring flags are NOT interpreted in a user-engine context: BSD (2) is just slot 2
    assert_eq!(go(ef::I915_EXEC_BSD), Err("EXEC-ENGINE-INVALID"));
    // an INVALID placeholder slot is -EINVAL
    env.contexts = vec![(1, ContextInfo { user_engines: Some(2), user_engine_present: false })];
    assert_eq!(check(&gt2(), &env, &Exec::new(0, vec![bo(10)])).unwrap_err().0, "EXEC-ENGINE-INVALID");
}

fn fence_env() -> Env {
    let mut env = Env::basic();
    env.syncobjs = vec![
        (1, SyncobjInfo { has_fence: true, chain_seqno: None }),
        (2, SyncobjInfo { has_fence: false, chain_seqno: None }),
        (3, SyncobjInfo { has_fence: true, chain_seqno: Some(10) }),
    ];
    env
}
fn with_fences(f: Vec<ExecFence>) -> Exec {
    let mut e = Exec::new(ef::I915_EXEC_FENCE_ARRAY, vec![bo(10)]);
    e.fences = f;
    e
}

#[test]
fn fence_array_rules() {
    let env = fence_env();
    let (w, s) = (xf::I915_EXEC_FENCE_WAIT, xf::I915_EXEC_FENCE_SIGNAL);
    // unknown flags (:2959-2960)
    assert_eq!(err(check(&gt2(), &env, &with_fences(vec![ExecFence { handle: 1, flags: 4 }]))), ("EXEC-FENCE-FLAGS", Errno::EINVAL));
    assert_eq!(err(check(&gt2(), &env, &with_fences(vec![ExecFence { handle: 1, flags: 0x8000_0000 }]))).0, "EXEC-FENCE-FLAGS");
    // missing syncobj -> -ENOENT (:2962-2967)
    assert_eq!(err(check(&gt2(), &env, &with_fences(vec![ExecFence { handle: 9, flags: w }]))), ("EXEC-SYNCOBJ-MISSING", Errno::ENOENT));
    // a WAIT on a syncobj with no fence -> -EINVAL (:2970-2975); a SIGNAL on it is fine
    assert_eq!(err(check(&gt2(), &env, &with_fences(vec![ExecFence { handle: 2, flags: w }]))), ("EXEC-SYNCOBJ-NO-FENCE", Errno::EINVAL));
    let p = check(&gt2(), &env, &with_fences(vec![ExecFence { handle: 2, flags: s }, ExecFence { handle: 1, flags: w | s }])).unwrap();
    assert_eq!((p.fence_waits, p.fence_signals), (1, 2));
    // FENCE_ARRAY with zero entries is no fences (:2926-2931)
    assert!(check(&gt2(), &env, &with_fences(vec![])).is_ok());
}

fn with_timeline(f: Vec<(u32, u32, u64)>) -> Exec {
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, vec![bo(10)]);
    e.exts = vec![TimelineExt::new(f.iter().map(|x| ExecFence { handle: x.0, flags: x.1 }).collect(), f.iter().map(|x| x.2).collect())];
    e
}

#[test]
fn timeline_fence_rules() {
    let env = fence_env();
    let (w, s) = (xf::I915_EXEC_FENCE_WAIT, xf::I915_EXEC_FENCE_SIGNAL);
    assert_eq!(err(check(&gt2(), &env, &with_timeline(vec![(1, 4, 0)]))).0, "EXEC-FENCE-FLAGS"); // :2837
    assert_eq!(err(check(&gt2(), &env, &with_timeline(vec![(9, w, 1)]))), ("EXEC-SYNCOBJ-MISSING", Errno::ENOENT)); // :2843-2847
    // no fence and a WAIT (flags != 0, no SIGNAL) -> -EINVAL (:2852-2858)
    assert_eq!(err(check(&gt2(), &env, &with_timeline(vec![(2, w, 0)]))), ("EXEC-SYNCOBJ-NO-FENCE", Errno::EINVAL));
    // no fence and flags == 0: "a point might have been signaled and garbage collected": skipped, accepted, counts nothing
    let p = check(&gt2(), &env, &with_timeline(vec![(2, 0, 5)])).unwrap();
    assert_eq!((p.fence_waits, p.fence_signals), (0, 0));
    // waiting on a timeline point the chain has not reached -> the dma_fence_chain_find_seqno error, -EINVAL (:2866-2875)
    assert_eq!(err(check(&gt2(), &env, &with_timeline(vec![(3, w, 11)]))), ("EXEC-TIMELINE-POINT-MISSING", Errno::EINVAL));
    assert!(check(&gt2(), &env, &with_timeline(vec![(3, w, 10)])).is_ok(), "the newest point exists");
    assert!(check(&gt2(), &env, &with_timeline(vec![(3, w, 3)])).is_ok(), "older points exist");
    // a non-chain fence cannot satisfy a non-zero point
    assert_eq!(err(check(&gt2(), &env, &with_timeline(vec![(1, w, 1)]))).0, "EXEC-TIMELINE-POINT-MISSING");
    // ... but point 0 means "the latest" and is always satisfiable by a fence
    assert!(check(&gt2(), &env, &with_timeline(vec![(1, w, 0)])).is_ok());
    // SIGNAL of a point the syncobj does not have yet is the normal case
    assert!(check(&gt2(), &env, &with_timeline(vec![(3, s, 99)])).is_ok());
    // wait AND signal of the same non-zero point would break the timeline (:2889-2896)
    assert_eq!(err(check(&gt2(), &env, &with_timeline(vec![(3, w | s, 5)]))), ("EXEC-TIMELINE-WAIT-AND-SIGNAL", Errno::EINVAL));
    assert!(check(&gt2(), &env, &with_timeline(vec![(3, w | s, 0)])).is_ok(), "point 0 is a binary syncobj semantic");
}

fn reloc(target: u32, offset: u64, presumed: u64) -> RelocationEntry {
    RelocationEntry { target_handle: target, delta: 0, offset, presumed_offset: presumed, read_domains: 2, write_domain: 0 }
}
fn with_relocs(flags_: u64, objects: Vec<ExecObject2>, relocs: Vec<RelocationEntry>) -> Exec {
    let mut e = Exec::new(flags_, objects);
    e.relocs = relocs;
    e
}

#[test]
fn relocation_target_lookup() {
    let env = Env::basic();
    let mut owner = bo(10);
    owner.relocation_count = 1;
    // a GEM handle that is not in this exec list -> -ENOENT (eb_get_vma :1417-1420)
    let e = with_relocs(0, vec![owner, bo(11)], vec![reloc(99, 0, 1)]);
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-RELOC-TARGET-MISSING", Errno::ENOENT));
    // under HANDLE_LUT the target is an INDEX into the list: 1 is the second object; 2 is out of range
    let e = with_relocs(ef::I915_EXEC_HANDLE_LUT, vec![owner, bo(11)], vec![reloc(1, 0, 1)]);
    assert!(check(&gt2(), &env, &e).is_ok());
    let e = with_relocs(ef::I915_EXEC_HANDLE_LUT, vec![owner, bo(11)], vec![reloc(2, 0, 1)]);
    assert_eq!(err(check(&gt2(), &env, &e)), ("EXEC-RELOC-TARGET-MISSING", Errno::ENOENT));
    // ... and the handle 11 is NOT valid as an index-form target here (index 11 >= 2)
    let e = with_relocs(ef::I915_EXEC_HANDLE_LUT, vec![owner, bo(11)], vec![reloc(11, 0, 1)]);
    assert_eq!(err(check(&gt2(), &env, &e)).0, "EXEC-RELOC-TARGET-MISSING");
    // without LUT the target is the GEM handle
    let e = with_relocs(0, vec![owner, bo(11)], vec![reloc(11, 0, 1)]);
    assert!(check(&gt2(), &env, &e).is_ok());
}

#[test]
fn relocation_domains() {
    let env = Env::basic();
    let mut owner = bo(10);
    owner.relocation_count = 1;
    let mk = |rd: u32, wd: u32| {
        let mut r = reloc(11, 0, 1);
        r.read_domains = rd;
        r.write_domain = wd;
        with_relocs(0, vec![owner, bo(11)], vec![r])
    };
    // more than one bit in write_domain (:1423-1433)
    assert_eq!(err(check(&gt2(), &env, &mk(2, 2 | 8))), ("EXEC-RELOC-WRITE-DOMAINS", Errno::EINVAL));
    // domains outside I915_GEM_GPU_DOMAINS: CPU, GTT, WC (:1434-1445)
    for bad in [1u32, 0x40, 0x80, 0x100] {
        assert_eq!(err(check(&gt2(), &env, &mk(bad, 0))), ("EXEC-RELOC-DOMAIN", Errno::EINVAL), "read {bad:#x}");
        assert_eq!(err(check(&gt2(), &env, &mk(2, bad))).1, Errno::EINVAL, "write {bad:#x}");
    }
    assert!(check(&gt2(), &env, &mk(2 | 4 | 8 | 0x10 | 0x20, 0x10)).is_ok());
    assert!(check(&gt2(), &env, &mk(0, 0)).is_ok());
}

#[test]
fn relocation_offset_bounds_and_alignment() {
    let env = Env::basic(); // owner object is 1 MiB
    let mut owner = bo(10);
    owner.relocation_count = 1;
    let go = |off: u64| check(&gt2(), &env, &with_relocs(0, vec![owner, bo(11)], vec![reloc(11, off, 0xdead_0000)])).map(|p| p.relocs_to_patch).map_err(|e| e.0);
    // Gen8+ relocations are 64-bit: offset <= size - 8 (:1480-1488)
    assert_eq!(go(MIB - 8), Ok(1));
    assert_eq!(go(MIB - 4), Err("EXEC-RELOC-BOUNDS"));
    assert_eq!(go(MIB), Err("EXEC-RELOC-BOUNDS"));
    assert_eq!(go(u64::MAX - 3), Err("EXEC-RELOC-BOUNDS"));
    // 4-byte alignment (:1489-1494)
    assert_eq!(go(2), Err("EXEC-RELOC-UNALIGNED"));
    assert_eq!(go(0x1001), Err("EXEC-RELOC-UNALIGNED"));
    assert_eq!(go(4), Ok(1));
}

#[test]
fn a_correct_presumed_offset_skips_the_offset_checks_like_linux() {
    let env = Env::basic();
    let mut owner = bo(10);
    owner.relocation_count = 1;
    // :1470-1476: when the target already sits at presumed_offset nothing is patched and nothing further is checked -- even an
    // offset Linux would otherwise refuse. The target is softpinned, so its placement is its (canonical) offset.
    let target = pinned(11, 0x40_0000);
    let e = with_relocs(0, vec![owner, target], vec![reloc(11, MIB * 4, 0x40_0000)]);
    let p = check(&gt2(), &env, &e).unwrap();
    assert_eq!((p.relocs_total, p.relocs_to_patch), (1, 0));
    // a stale presumed_offset falls through to the checks and counts as work
    let e = with_relocs(0, vec![owner, target], vec![reloc(11, 8, 0x50_0000)]);
    assert_eq!(check(&gt2(), &env, &e).unwrap().relocs_to_patch, 1);
    let e = with_relocs(0, vec![owner, target], vec![reloc(11, MIB * 4, 0x50_0000)]);
    assert_eq!(err(check(&gt2(), &env, &e)).0, "EXEC-RELOC-BOUNDS");
    // a bound-but-not-pinned target uses its current placement
    let mut env2 = Env::basic();
    env2.objects[1].1 = ObjectInfo { size: MIB, bound_offset: Some(0x80_0000) };
    let e = with_relocs(0, vec![owner, bo(11)], vec![reloc(11, MIB * 4, 0x80_0000)]);
    assert_eq!(check(&gt2(), &env2, &e).unwrap().relocs_to_patch, 0);
    // domain errors are NOT skipped by a matching presumed offset (they are checked first, :1423-1445)
    let mut r = reloc(11, 8, 0x40_0000);
    r.write_domain = 1;
    let e = with_relocs(0, vec![owner, target], vec![r]);
    assert_eq!(err(check(&gt2(), &env, &e)).0, "EXEC-RELOC-DOMAIN");
}

#[test]
fn offsets_are_reported_back_unless_no_reloc() {
    // :3365-3366: !NO_RELOC -> __EXEC_HAS_RELOC -> the updated offsets are written back
    let env = Env::basic();
    assert!(check(&gt2(), &env, &Exec::new(0, vec![bo(10)])).unwrap().reports_offsets);
    assert!(!check(&gt2(), &env, &Exec::new(ef::I915_EXEC_NO_RELOC, vec![bo(10)])).unwrap().reports_offsets);
}

#[test]
fn validation_order_follows_linux() {
    // Context lookup (:3470) precedes object lookup (:3480): a missing context wins over a missing object.
    let mut e = one(bo(77));
    e.rsvd1 = 99;
    assert_eq!(err(check(&gt2(), &Env::basic(), &e)).0, "EXEC-CONTEXT-MISSING");
    // The engine is selected before objects are looked up.
    let e = Exec::new(5, vec![bo(77)]);
    assert_eq!(err(check(&gt2(), &Env::basic(), &e)).0, "EXEC-ENGINE-INVALID");
    // Per-object checks run in list order: the first failing object is the one reported.
    let mut bad = bo(11);
    bad.alignment = 3;
    let e = Exec::new(0, vec![bad, flags(1 << 50)]);
    assert_eq!(err(check(&gt2(), &Env::basic(), &e)).0, "EXEC-ALIGNMENT-NOT-POW2");
}
