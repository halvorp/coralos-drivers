// SPDX-License-Identifier: GPL-2.0-only
//! The flattened EXECBUFFER2 decode: structural bounds, Linux's pre-checks (check_buffer_count, i915_gem_check_execbuffer,
//! i915_user_extensions) and this port's resource bounds. Every bound has a test that goes RED if its check is removed.
mod common;
use common::*;
use i915_uapi_core::consts::exec as ef;
use i915_uapi_core::consts::exec_fence as xf;
use i915_uapi_core::execbuf_wire::{ExecLimits, MAX_EXTENSIONS, MAX_RELOCS_PER_OBJECT_LINUX};
use i915_uapi_core::types::*;
use i915_uapi_core::{decode_execbuf2, Errno};

fn obj(handle: u32) -> ExecObject2 {
    ExecObject2 { handle, ..Default::default() }
}
fn objs(n: u32) -> Vec<ExecObject2> {
    (0..n).map(|i| obj(10 + i)).collect()
}
fn dec(e: &Exec, l: &ExecLimits) -> Result<(), (&'static str, Errno)> {
    let b = e.build();
    decode_execbuf2(&b, l).map(|_| ()).map_err(|x| (x.name(), x.errno()))
}
const L: ExecLimits = ExecLimits::DEFAULT;

#[test]
fn a_well_formed_request_decodes_every_table() {
    let mut e = Exec::new(ef::I915_EXEC_FENCE_ARRAY | ef::I915_EXEC_NO_RELOC, vec![obj(10), ExecObject2 { handle: 11, relocation_count: 2, ..Default::default() }]);
    e.relocs = vec![
        RelocationEntry { target_handle: 10, delta: 1, offset: 8, presumed_offset: 0x1000, read_domains: 2, write_domain: 0 },
        RelocationEntry { target_handle: 10, delta: 2, offset: 16, presumed_offset: 0x2000, read_domains: 2, write_domain: 2 },
    ];
    e.fences = vec![ExecFence { handle: 4, flags: 1 }, ExecFence { handle: 5, flags: 2 }];
    e.rsvd1 = 0xdead_beef_0000_0007; // only the low 32 bits are the context id
    e.start = 8;
    e.len = 64;
    let b = e.build();
    let r = decode_execbuf2(&b, &L).unwrap();
    assert_eq!((r.objects.len(), r.relocs.len(), r.fences.len()), (2, 2, 2));
    assert_eq!(r.context_id(), 7);
    assert_eq!(r.objects.get(1).unwrap().relocation_count, 2);
    assert_eq!(r.relocs.get(1).unwrap().delta, 2);
    assert_eq!(r.fences.get(1).unwrap(), ExecFence { handle: 5, flags: 2 });
    assert_eq!((r.batch_start_offset, r.batch_len, r.fence_total()), (8, 64, 2));
}

#[test]
fn timeline_blocks_decode_with_their_points() {
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.exts = vec![
        TimelineExt::new(vec![ExecFence { handle: 1, flags: 1 }, ExecFence { handle: 2, flags: 2 }], vec![5, 6]),
        TimelineExt::new(vec![ExecFence { handle: 3, flags: 1 }], vec![0x1_0000_0000]),
    ];
    let b = e.build();
    let r = decode_execbuf2(&b, &L).unwrap();
    let blocks: Vec<_> = r.timeline_blocks().collect();
    assert_eq!(blocks.len(), 2);
    assert_eq!((blocks[0].len(), blocks[0].value(1), blocks[0].fences.get(1).unwrap().handle), (2, Some(6), 2));
    assert_eq!((blocks[1].len(), blocks[1].value(0)), (1, Some(0x1_0000_0000)));
    assert_eq!(r.fence_total(), 3);
}

#[test]
fn buffer_count_bounds() {
    // check_buffer_count (i915_gem_execbuffer.c:3541-3553): count < 1 -> -EINVAL
    let mut e = Exec::new(0, vec![]);
    e.buffer_count = Some(0);
    assert_eq!(dec(&e, &L), Err(("EXEC-BUFFER-COUNT", Errno::EINVAL)));
    // count > INT_MAX -> -EINVAL (before any table is read)
    let mut e = Exec::new(0, vec![]);
    e.buffer_count = Some(i32::MAX as u32 + 1);
    assert_eq!(dec(&e, &L), Err(("EXEC-BUFFER-COUNT", Errno::EINVAL)));
    // this port's bound: max_buffers is accepted, one more is refused with the value and the bound, ENOMEM
    let lim = ExecLimits { max_buffers: 4, ..L };
    assert_eq!(dec(&Exec::new(0, objs(4)), &lim), Ok(()));
    assert_eq!(dec(&Exec::new(0, objs(5)), &lim), Err(("LIMIT-EXCEEDED", Errno::ENOMEM)));
    let e = decode_execbuf2(&Exec::new(0, objs(5)).build(), &lim).unwrap_err();
    assert_eq!(e, i915_uapi_core::UapiError::LimitExceeded { what: "execbuf.buffer_count", got: 5, max: 4 });
    // the default bound is reachable and enforced against a real payload
    assert_eq!(dec(&Exec::new(0, objs(L.max_buffers)), &L), Ok(()));
    assert_eq!(dec(&Exec::new(0, objs(L.max_buffers + 1)), &L), Err(("LIMIT-EXCEEDED", Errno::ENOMEM)));
}

#[test]
fn payload_cap_is_the_wire_max_payload() {
    let big = vec![0u8; i915_wire::MAX_PAYLOAD as usize + 1];
    assert_eq!(decode_execbuf2(&big, &L).unwrap_err().name(), "LIMIT-EXCEEDED");
    // a caller cannot raise the cap above the wire's
    let lim = ExecLimits { max_payload: u32::MAX, ..L };
    assert_eq!(decode_execbuf2(&big, &lim).unwrap_err().name(), "LIMIT-EXCEEDED");
    // a smaller cap is honoured
    let lim = ExecLimits { max_payload: 100, ..L };
    assert_eq!(dec(&Exec::new(0, objs(1)), &lim), Err(("LIMIT-EXCEEDED", Errno::ENOMEM)));
}

#[test]
fn relocation_count_bounds() {
    // N_RELOC(INT_MAX) per object (i915_gem_execbuffer.c:1520,1628): 0x7fffffff / 32 = 67108863; one more is -EINVAL, and it is
    // reported BEFORE the table that cannot possibly be present is read.
    assert_eq!(MAX_RELOCS_PER_OBJECT_LINUX, 67_108_863);
    let mut e = Exec::new(0, vec![ExecObject2 { handle: 10, relocation_count: MAX_RELOCS_PER_OBJECT_LINUX + 1, ..Default::default() }]);
    e.reloc_total = Some(0);
    assert_eq!(dec(&e, &L), Err(("EXEC-RELOC-COUNT", Errno::EINVAL)));
    // exactly Linux's bound passes Linux's check and meets this port's total bound instead (ENOMEM), never a read past the payload
    let mut e = Exec::new(0, vec![ExecObject2 { handle: 10, relocation_count: MAX_RELOCS_PER_OBJECT_LINUX, ..Default::default() }]);
    e.reloc_total = Some(0);
    assert_eq!(dec(&e, &L), Err(("LIMIT-EXCEEDED", Errno::ENOMEM)));
    // the port's total over ALL objects is bounded
    let lim = ExecLimits { max_relocs_total: 3, ..L };
    let r = |t| RelocationEntry { target_handle: t, ..Default::default() };
    let mut e = Exec::new(0, vec![ExecObject2 { handle: 10, relocation_count: 2, ..Default::default() }, ExecObject2 { handle: 11, relocation_count: 2, ..Default::default() }]);
    e.relocs = vec![r(10), r(10), r(10), r(10)];
    assert_eq!(dec(&e, &lim), Err(("LIMIT-EXCEEDED", Errno::ENOMEM)));
    e.relocs.pop();
    e.objects[1].relocation_count = 1;
    assert_eq!(dec(&e, &lim), Ok(()));
    // the declared total must equal the sum of the counts
    e.reloc_total = Some(2);
    assert_eq!(dec(&e, &lim), Err(("WIRE-COUNT-MISMATCH", Errno::EINVAL)));
    // relocations promised by the counts but absent from the payload: truncated, EFAULT
    let mut e = Exec::new(0, vec![ExecObject2 { handle: 10, relocation_count: 2, ..Default::default() }]);
    e.relocs = vec![r(10)];
    e.reloc_total = Some(2);
    assert_eq!(dec(&e, &L), Err(("WIRE-TRUNCATED", Errno::EFAULT)));
}

#[test]
fn truncation_and_trailing_bytes_are_named() {
    let b = Exec::new(0, objs(2)).build();
    assert_eq!(decode_execbuf2(&b[..b.len() - 1], &L).unwrap_err().name(), "WIRE-TRUNCATED");
    assert_eq!(decode_execbuf2(&b[..63], &L).unwrap_err().name(), "WIRE-TRUNCATED");
    let mut t = b.clone();
    t.push(0);
    assert_eq!(decode_execbuf2(&t, &L).unwrap_err().name(), "WIRE-TRAILING-BYTES");
    // reserved header word and the cliprects flag are exact
    let mut t = b.clone();
    t[60] = 1;
    assert_eq!(decode_execbuf2(&t, &L).unwrap_err().name(), "WIRE-RESERVED-NONZERO");
    let mut e = Exec::new(0, objs(1));
    e.cliprects_present = 2;
    assert_eq!(dec(&e, &L), Err(("WIRE-RESERVED-NONZERO", Errno::EINVAL)));
}

#[test]
fn i915_gem_check_execbuffer_rules() {
    // :2183-2184 unknown flag bits, CONSTANTS_MASK, RESOURCE_STREAMER
    for f in [1u64 << 22, 1 << 63, ef::I915_EXEC_CONSTANTS_ABSOLUTE, ef::I915_EXEC_CONSTANTS_REL_SURFACE, ef::I915_EXEC_RESOURCE_STREAMER] {
        assert_eq!(dec(&Exec::new(f, objs(1)), &L), Err(("EXEC-ILLEGAL-FLAGS", Errno::EINVAL)), "flags {f:#x}");
    }
    // every defined, legal flag is accepted by the pre-check (CONSTANTS_REL_GENERAL is 0)
    let legal = ef::I915_EXEC_RING_MASK | ef::I915_EXEC_GEN7_SOL_RESET | ef::I915_EXEC_SECURE | ef::I915_EXEC_IS_PINNED | ef::I915_EXEC_NO_RELOC
        | ef::I915_EXEC_HANDLE_LUT | ef::I915_EXEC_BSD_MASK | ef::I915_EXEC_FENCE_IN | ef::I915_EXEC_FENCE_OUT | ef::I915_EXEC_BATCH_FIRST
        | ef::I915_EXEC_FENCE_SUBMIT;
    assert_eq!(dec(&Exec::new(legal, objs(1)), &L), Ok(()));
    // :2187-2191 cliprects without FENCE_ARRAY / USE_EXTENSIONS
    let mut e = Exec::new(0, objs(1));
    e.num_cliprects = Some(1);
    assert_eq!(dec(&e, &L), Err(("EXEC-CLIPRECTS", Errno::EINVAL)));
    let mut e = Exec::new(0, objs(1));
    e.cliprects_present = 1;
    assert_eq!(dec(&e, &L), Err(("EXEC-CLIPRECTS", Errno::EINVAL)));
    // :2193-2198 DR1/DR4; the UXA value DR4 = 0xffffffff is fixed up to zero
    let mut e = Exec::new(0, objs(1));
    e.dr4 = 0xffff_ffff;
    assert_eq!(dec(&e, &L), Ok(()));
    e.dr4 = 1;
    assert_eq!(dec(&e, &L), Err(("EXEC-DR-NONZERO", Errno::EINVAL)));
    e.dr4 = 0;
    e.dr1 = 1;
    assert_eq!(dec(&e, &L), Err(("EXEC-DR-NONZERO", Errno::EINVAL)));
    // :2200-2201 batch_start_offset and batch_len are 8-byte aligned
    for (s, l) in [(4, 0), (0, 4), (1, 0), (0, 7)] {
        let mut e = Exec::new(0, objs(1));
        e.start = s;
        e.len = l;
        assert_eq!(dec(&e, &L), Err(("EXEC-BATCH-UNALIGNED", Errno::EINVAL)), "start {s} len {l}");
    }
    let mut e = Exec::new(0, objs(1));
    e.start = 8;
    e.len = 16;
    assert_eq!(dec(&e, &L), Ok(()));
}

#[test]
fn extension_chain_rules() {
    let tl = |_n: u32| TimelineExt::new(vec![], vec![]);
    // parse_execbuf2_extensions :3147-3151
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS | ef::I915_EXEC_FENCE_ARRAY, objs(1));
    e.fences = vec![ExecFence { handle: 1, flags: 1 }];
    assert_eq!(dec(&e, &L), Err(("EXEC-EXT-WITH-FENCE-ARRAY", Errno::EINVAL)));
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.num_cliprects = Some(1);
    assert_eq!(dec(&e, &L), Err(("EXEC-EXT-WITH-CLIPRECTS", Errno::EINVAL)));
    // the chain depth: 512 records pass, the 513th is -E2BIG (i915_user_extensions.c:21,28-29)
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.exts = (0..MAX_EXTENSIONS).map(tl).collect();
    assert_eq!(dec(&e, &L), Ok(()));
    e.exts = (0..=MAX_EXTENSIONS).map(tl).collect();
    assert_eq!(dec(&e, &L), Err(("EXEC-EXT-CHAIN-TOO-LONG", Errno::E2BIG)));
    // the declared count must match the records and the flag
    let mut e = Exec::new(0, objs(1));
    e.ext_count = Some(1);
    assert_eq!(dec(&e, &L), Err(("WIRE-COUNT-MISMATCH", Errno::EINVAL)));
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.ext_count = Some(2);
    e.exts = vec![tl(0)];
    assert_eq!(dec(&e, &L), Err(("WIRE-TRUNCATED", Errno::EFAULT)));
    // check_user_mbz on flags and every rsvd word (:31-39), BEFORE the name is looked at (:41-49)
    for variant in 0..5 {
        let mut x = tl(0);
        match variant {
            0 => x.header.flags = 1,
            n => x.header.rsvd[n - 1] = 1,
        }
        let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
        e.exts = vec![x];
        assert_eq!(dec(&e, &L), Err(("EXEC-EXT-RESERVED", Errno::EINVAL)), "variant {variant}");
    }
    let mut x = tl(0);
    x.header.name = 7;
    x.header.flags = 1; // both wrong: the mbz check wins
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.exts = vec![x];
    assert_eq!(dec(&e, &L), Err(("EXEC-EXT-RESERVED", Errno::EINVAL)));
    // an unknown name (only TIMELINE_FENCES = 0 exists, :3135-3137)
    let mut x = tl(0);
    x.header.name = xf::DRM_I915_GEM_EXECBUFFER_EXT_TIMELINE_FENCES + 1;
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.exts = vec![x];
    assert_eq!(dec(&e, &L), Err(("EXEC-EXT-UNKNOWN", Errno::EINVAL)));
}

#[test]
fn fence_count_bounds() {
    let lim = ExecLimits { max_fences: 3, ..L };
    // FENCE_ARRAY: max_fences accepted, one more is refused
    let f = |n: u32| (0..n).map(|h| ExecFence { handle: h + 1, flags: 1 }).collect::<Vec<_>>();
    let mut e = Exec::new(ef::I915_EXEC_FENCE_ARRAY, objs(1));
    e.fences = f(3);
    assert_eq!(dec(&e, &lim), Ok(()));
    e.fences = f(4);
    assert_eq!(dec(&e, &lim), Err(("LIMIT-EXCEEDED", Errno::ENOMEM)));
    // the bound is over the running total: array + every timeline block
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.exts = vec![TimelineExt::new(f(2), vec![0, 0]), TimelineExt::new(f(2), vec![0, 0])];
    assert_eq!(dec(&e, &lim), Err(("LIMIT-EXCEEDED", Errno::ENOMEM)));
    e.exts = vec![TimelineExt::new(f(2), vec![0, 0]), TimelineExt::new(f(1), vec![0])];
    assert_eq!(dec(&e, &lim), Ok(()));
    // a count that cannot even be a size (Linux's overflow guard, :2801-2803) is EINVAL, not an allocation attempt
    let mut x = TimelineExt::new(vec![], vec![]);
    x.declared_count = Some(1 << 33);
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.exts = vec![x];
    assert_eq!(dec(&e, &L), Err(("EXEC-FENCE-COUNT", Errno::EINVAL)));
    // a declared count with no fences behind it is truncated
    let mut x = TimelineExt::new(vec![], vec![]);
    x.declared_count = Some(2);
    let mut e = Exec::new(ef::I915_EXEC_USE_EXTENSIONS, objs(1));
    e.exts = vec![x];
    assert_eq!(dec(&e, &L), Err(("WIRE-TRUNCATED", Errno::EFAULT)));
}
