// SPDX-License-Identifier: GPL-2.0-only
//! Ring space / direction / offset arithmetic against Linux's OWN inline functions (intel_ring.h:54-78, :129-139 compiled by
//! GCC), the `intel_ring_begin` packet / reserved-space / wrap rules at their boundaries, and the named refusals: ring-full,
//! odd dwords, oversize, bad ring size, tail in the head's cacheline.
use i915_submit_core::ring::*;
use i915_submit_core::SubmitError;

/// (size, head, tail, `__intel_ring_space(head, tail, size)`).
const SPACE: [(u32, u32, u32, u32); 140] = [
    (0x1000, 0, 0, 0xfc0),
    (0x1000, 0, 0x8, 0xfb8),
    (0x1000, 0, 0x40, 0xf80),
    (0x1000, 0, 0xf80, 0x40),
    (0x1000, 0, 0x800, 0x7c0),
    (0x1000, 0, 0xfc0, 0),
    (0x1000, 0, 0xff8, 0xfc8),
    (0x1000, 0x40, 0, 0),
    (0x1000, 0x40, 0x8, 0xff8),
    (0x1000, 0x40, 0x40, 0xfc0),
    (0x1000, 0x40, 0xf80, 0x80),
    (0x1000, 0x40, 0x800, 0x800),
    (0x1000, 0x40, 0xfc0, 0x40),
    (0x1000, 0x40, 0xff8, 0x8),
    (0x1000, 0x80, 0, 0x40),
    (0x1000, 0x80, 0x8, 0x38),
    (0x1000, 0x80, 0x40, 0),
    (0x1000, 0x80, 0xf80, 0xc0),
    (0x1000, 0x80, 0x800, 0x840),
    (0x1000, 0x80, 0xfc0, 0x80),
    (0x1000, 0x80, 0xff8, 0x48),
    (0x1000, 0xfc0, 0, 0xf80),
    (0x1000, 0xfc0, 0x8, 0xf78),
    (0x1000, 0xfc0, 0x40, 0xf40),
    (0x1000, 0xfc0, 0xf80, 0),
    (0x1000, 0xfc0, 0x800, 0x780),
    (0x1000, 0xfc0, 0xfc0, 0xfc0),
    (0x1000, 0xfc0, 0xff8, 0xf88),
    (0x1000, 0x800, 0, 0x7c0),
    (0x1000, 0x800, 0x8, 0x7b8),
    (0x1000, 0x800, 0x40, 0x780),
    (0x1000, 0x800, 0xf80, 0x840),
    (0x1000, 0x800, 0x800, 0xfc0),
    (0x1000, 0x800, 0xfc0, 0x800),
    (0x1000, 0x800, 0xff8, 0x7c8),
    (0x1000, 0xff8, 0, 0xfb8),
    (0x1000, 0xff8, 0x8, 0xfb0),
    (0x1000, 0xff8, 0x40, 0xf78),
    (0x1000, 0xff8, 0xf80, 0x38),
    (0x1000, 0xff8, 0x800, 0x7b8),
    (0x1000, 0xff8, 0xfc0, 0xff8),
    (0x1000, 0xff8, 0xff8, 0xfc0),
    (0x2000, 0, 0, 0x1fc0),
    (0x2000, 0, 0x8, 0x1fb8),
    (0x2000, 0, 0x40, 0x1f80),
    (0x2000, 0, 0xf80, 0x1040),
    (0x2000, 0, 0x1000, 0xfc0),
    (0x2000, 0, 0x1fc0, 0),
    (0x2000, 0, 0x1ff8, 0x1fc8),
    (0x2000, 0x40, 0, 0),
    (0x2000, 0x40, 0x8, 0x1ff8),
    (0x2000, 0x40, 0x40, 0x1fc0),
    (0x2000, 0x40, 0xf80, 0x1080),
    (0x2000, 0x40, 0x1000, 0x1000),
    (0x2000, 0x40, 0x1fc0, 0x40),
    (0x2000, 0x40, 0x1ff8, 0x8),
    (0x2000, 0x80, 0, 0x40),
    (0x2000, 0x80, 0x8, 0x38),
    (0x2000, 0x80, 0x40, 0),
    (0x2000, 0x80, 0xf80, 0x10c0),
    (0x2000, 0x80, 0x1000, 0x1040),
    (0x2000, 0x80, 0x1fc0, 0x80),
    (0x2000, 0x80, 0x1ff8, 0x48),
    (0x2000, 0xfc0, 0, 0xf80),
    (0x2000, 0xfc0, 0x8, 0xf78),
    (0x2000, 0xfc0, 0x40, 0xf40),
    (0x2000, 0xfc0, 0xf80, 0),
    (0x2000, 0xfc0, 0x1000, 0x1f80),
    (0x2000, 0xfc0, 0x1fc0, 0xfc0),
    (0x2000, 0xfc0, 0x1ff8, 0xf88),
    (0x2000, 0x1fc0, 0, 0x1f80),
    (0x2000, 0x1fc0, 0x8, 0x1f78),
    (0x2000, 0x1fc0, 0x40, 0x1f40),
    (0x2000, 0x1fc0, 0xf80, 0x1000),
    (0x2000, 0x1fc0, 0x1000, 0xf80),
    (0x2000, 0x1fc0, 0x1fc0, 0x1fc0),
    (0x2000, 0x1fc0, 0x1ff8, 0x1f88),
    (0x2000, 0x1000, 0, 0xfc0),
    (0x2000, 0x1000, 0x8, 0xfb8),
    (0x2000, 0x1000, 0x40, 0xf80),
    (0x2000, 0x1000, 0xf80, 0x40),
    (0x2000, 0x1000, 0x1000, 0x1fc0),
    (0x2000, 0x1000, 0x1fc0, 0x1000),
    (0x2000, 0x1000, 0x1ff8, 0xfc8),
    (0x2000, 0x1ff8, 0, 0x1fb8),
    (0x2000, 0x1ff8, 0x8, 0x1fb0),
    (0x2000, 0x1ff8, 0x40, 0x1f78),
    (0x2000, 0x1ff8, 0xf80, 0x1038),
    (0x2000, 0x1ff8, 0x1000, 0xfb8),
    (0x2000, 0x1ff8, 0x1fc0, 0x1ff8),
    (0x2000, 0x1ff8, 0x1ff8, 0x1fc0),
    (0x200000, 0, 0, 0x1fffc0),
    (0x200000, 0, 0x8, 0x1fffb8),
    (0x200000, 0, 0x40, 0x1fff80),
    (0x200000, 0, 0xf80, 0x1ff040),
    (0x200000, 0, 0x100000, 0xfffc0),
    (0x200000, 0, 0x1fffc0, 0),
    (0x200000, 0, 0x1ffff8, 0x1fffc8),
    (0x200000, 0x40, 0, 0),
    (0x200000, 0x40, 0x8, 0x1ffff8),
    (0x200000, 0x40, 0x40, 0x1fffc0),
    (0x200000, 0x40, 0xf80, 0x1ff080),
    (0x200000, 0x40, 0x100000, 0x100000),
    (0x200000, 0x40, 0x1fffc0, 0x40),
    (0x200000, 0x40, 0x1ffff8, 0x8),
    (0x200000, 0x80, 0, 0x40),
    (0x200000, 0x80, 0x8, 0x38),
    (0x200000, 0x80, 0x40, 0),
    (0x200000, 0x80, 0xf80, 0x1ff0c0),
    (0x200000, 0x80, 0x100000, 0x100040),
    (0x200000, 0x80, 0x1fffc0, 0x80),
    (0x200000, 0x80, 0x1ffff8, 0x48),
    (0x200000, 0xfc0, 0, 0xf80),
    (0x200000, 0xfc0, 0x8, 0xf78),
    (0x200000, 0xfc0, 0x40, 0xf40),
    (0x200000, 0xfc0, 0xf80, 0),
    (0x200000, 0xfc0, 0x100000, 0x100f80),
    (0x200000, 0xfc0, 0x1fffc0, 0xfc0),
    (0x200000, 0xfc0, 0x1ffff8, 0xf88),
    (0x200000, 0x1fffc0, 0, 0x1fff80),
    (0x200000, 0x1fffc0, 0x8, 0x1fff78),
    (0x200000, 0x1fffc0, 0x40, 0x1fff40),
    (0x200000, 0x1fffc0, 0xf80, 0x1ff000),
    (0x200000, 0x1fffc0, 0x100000, 0xfff80),
    (0x200000, 0x1fffc0, 0x1fffc0, 0x1fffc0),
    (0x200000, 0x1fffc0, 0x1ffff8, 0x1fff88),
    (0x200000, 0x100000, 0, 0xfffc0),
    (0x200000, 0x100000, 0x8, 0xfffb8),
    (0x200000, 0x100000, 0x40, 0xfff80),
    (0x200000, 0x100000, 0xf80, 0xff040),
    (0x200000, 0x100000, 0x100000, 0x1fffc0),
    (0x200000, 0x100000, 0x1fffc0, 0x100000),
    (0x200000, 0x100000, 0x1ffff8, 0xfffc8),
    (0x200000, 0x1ffff8, 0, 0x1fffb8),
    (0x200000, 0x1ffff8, 0x8, 0x1fffb0),
    (0x200000, 0x1ffff8, 0x40, 0x1fff78),
    (0x200000, 0x1ffff8, 0xf80, 0x1ff038),
    (0x200000, 0x1ffff8, 0x100000, 0xfffb8),
    (0x200000, 0x1ffff8, 0x1fffc0, 0x1ffff8),
    (0x200000, 0x1ffff8, 0x1ffff8, 0x1fffc0),
];

#[test]
fn ring_space_equals_linux_for_every_head_tail_pair() {
    for (size, head, tail, want) in SPACE {
        assert_eq!(Ring::space_between(head, tail, size), want, "size {size:#x} head {head:#x} tail {tail:#x}");
    }
}

/// (next, prev, `intel_ring_direction()` on a 4 KiB ring).
const DIRECTION: [(u32, u32, i32); 12] = [
    (8, 0, 8388608),
    (0, 8, -8388608),
    (0, 0, 0),
    (2048, 0, -2147483648),
    (0, 2048, -2147483648),
    (2040, 0, 2139095040),
    (0, 2040, -2139095040),
    (0, 4088, 8388608),
    (4088, 0, -8388608),
    (16, 4080, 33554432),
    (4080, 16, -33554432),
    (1024, 3072, -2147483648),
];

#[test]
fn ring_direction_equals_linux_including_the_half_ring_boundary() {
    let r = Ring::new(4096).unwrap();
    for (next, prev, want) in DIRECTION {
        assert_eq!(r.direction(next, prev), want, "{next} vs {prev}");
    }
    // exactly half a ring apart is "behind" in both directions (the sign bit), which is why a ring may never be filled past half
    assert!(r.direction(2048, 0) < 0 && r.direction(0, 2048) < 0);
}

/// (pos, `intel_ring_offset_valid()` on a 4 KiB ring).
const OFFSET_VALID: [(u32, i32); 8] = [
    (0, 1),
    (0x8, 1),
    (0x4, 0),
    (0xff8, 1),
    (0x1000, 0),
    (0x1008, 0),
    (0x2000, 0),
    (0xfffffff8, 0),
];

#[test]
fn offset_valid_equals_linux() {
    let r = Ring::new(4096).unwrap();
    for (pos, want) in OFFSET_VALID {
        assert_eq!(r.offset_valid(pos), want != 0, "{pos:#x}");
    }
}

#[test]
fn ring_sizes_follow_ring_ctl_size_and_ring_nr_pages() {
    // intel_engine_create_ring: GEM_BUG_ON(!is_power_of_2(size)); GEM_BUG_ON(RING_CTL_SIZE(size) & ~RING_NR_PAGES)
    for size in [4096u32, 8192, 16384, 0x10000, 0x200000] {
        let r = Ring::new(size).unwrap();
        assert_eq!(r.size(), size);
        assert_eq!(r.wrap_shift(), 32 - size.trailing_zeros());
    }
    for size in [0u32, 1, 2048, 4095, 4097, 12288, 0x400000, 0x8000_0000] {
        let e = Ring::new(size).unwrap_err();
        assert_eq!(e, SubmitError::BadRingSize { size });
        assert_eq!(e.name(), "RING-BAD-SIZE");
    }
    // RING_CTL_SIZE is the byte size less one page, already in the field's position
    assert_eq!(ring_ctl_size(4096), 0);
    assert_eq!(ring_ctl_size(0x200000), 0x1ff000);
    assert_eq!(Ring::new(4096).unwrap().ctl_value(), 1);
    assert_eq!(Ring::new(0x200000).unwrap().ctl_value(), 0x1ff001);
}

#[test]
fn a_fresh_ring_has_all_but_one_cacheline_free() {
    // (head - tail - CACHELINE_BYTES) & (size - 1) with head == tail == 0
    let r = Ring::new(4096).unwrap();
    assert_eq!(r.space(), 4096 - 64);
    assert_eq!((r.head(), r.tail(), r.emit()), (0, 0, 0));
}

#[test]
fn begin_reserves_dwords_and_advances_emit() {
    let mut r = Ring::new(4096).unwrap();
    let b = r.begin(6, 0).unwrap();
    assert_eq!(b, Begin { at: 0, wrap_fill: None });
    assert_eq!(r.emit(), 24);
    assert_eq!(r.space(), 4096 - 64 - 24);
    let b = r.begin(2, 0).unwrap();
    assert_eq!(b.at, 24);
    assert_eq!(r.emit(), 32);
}

#[test]
fn begin_refuses_odd_dwords_and_oversize_by_name() {
    let mut r = Ring::new(4096).unwrap();
    assert_eq!(r.begin(5, 0).unwrap_err(), SubmitError::RingOddDwords { dwords: 5 });
    assert_eq!(r.emit(), 0, "a refused begin changes nothing");
    // packet + reserved larger than the whole ring can never fit
    let e = r.begin(1024, 8).unwrap_err();
    assert_eq!(e, SubmitError::RingRequestTooLarge { need: 4096 + 8, effective: 4096 });
    assert_eq!(e.name(), "RING-REQUEST-TOO-LARGE");
    assert_eq!(r.emit(), 0);
}

#[test]
fn begin_refuses_when_full_where_linux_would_wait() {
    let mut r = Ring::new(4096).unwrap();
    // fill until only the cacheline of slack remains
    r.begin(1008, 0).unwrap(); // 4032 bytes: space is now 4096 - 64 - 4032 = 0
    assert_eq!(r.space(), 0);
    let e = r.begin(2, 0).unwrap_err();
    assert_eq!(e, SubmitError::RingFull { need: 8, space: 0 });
    assert_eq!(e.name(), "RING-FULL");
    assert_eq!(r.emit(), 4032, "the refusal changed nothing");
    // the reserved space counts: 4 dwords fit alone but not with 64 bytes reserved
    let mut r = Ring::new(4096).unwrap();
    r.begin(1000, 0).unwrap(); // 4000 bytes, space 32
    assert_eq!(r.begin(4, 0).unwrap().at, 4000); // 16 bytes, leaves 16
    let mut r = Ring::new(4096).unwrap();
    r.begin(1000, 0).unwrap();
    assert_eq!(r.begin(4, 24).unwrap_err(), SubmitError::RingFull { need: 40, space: 32 });
}

#[test]
fn retiring_frees_space_and_the_next_begin_sees_it() {
    let mut r = Ring::new(4096).unwrap();
    r.begin(1008, 0).unwrap();
    assert!(r.begin(2, 0).is_err());
    r.set_head(2048).unwrap(); // the GPU got halfway
    // `space` is refreshed from the head by begin (wait_for_space's intel_ring_update_space), not by set_head
    assert_eq!(r.space(), 0);
    assert_eq!(r.begin(2, 0).unwrap().at, 4032);
    assert_eq!(r.space(), Ring::space_between(2048, 4032, 4096) - 8);
}

#[test]
fn begin_wraps_by_filling_the_tail_with_noops() {
    let mut r = Ring::new(4096).unwrap();
    // advance emit to 4000, then retire everything so there is space
    r.begin(1000, 0).unwrap();
    r.set_head(4000).unwrap();
    r.update_space();
    // 4000 + 96 > 4096: a 24-dword (96 byte) packet does not fit in the 96 bytes left? it does (exact fit), so no wrap
    let b = r.begin(24, 0).unwrap();
    assert_eq!(b, Begin { at: 4000, wrap_fill: None });
    assert_eq!(r.emit(), 4096);
    let mut r = Ring::new(4096).unwrap();
    r.begin(1004, 0).unwrap(); // emit 4016
    r.set_head(4016).unwrap();
    r.update_space();
    let b = r.begin(24, 0).unwrap(); // needs 96, only 80 remain: wrap 80 bytes of NOOPs
    assert_eq!(b, Begin { at: 0, wrap_fill: Some((4016, 80)) });
    assert_eq!(r.emit(), 96);
}

#[test]
fn reserved_space_that_falls_off_the_end_waits_for_the_start_of_the_ring() {
    // The base packet fits before the end but the reserved space does not: no wrap now, yet the space needed is
    // reserved + remain_actual (intel_ring.c:253-265).
    let mut r = Ring::new(4096).unwrap();
    r.begin(1000, 0).unwrap(); // emit 4000
    r.set_head(4000).unwrap();
    r.update_space(); // space = 4000 - 4000 - 64 = 4032 (mod 4096)
    let b = r.begin(8, 128).unwrap(); // 32 bytes fit (remain_usable 96) but 32 + 128 > 96
    assert_eq!(b, Begin { at: 4000, wrap_fill: None });
    assert_eq!(r.emit(), 4032);
    // 32 + 128 would fit in 200 bytes of space, but the rule demands 128 + the 96 bytes left at the end = 224
    let mut r = Ring::new(4096).unwrap();
    r.begin(1000, 0).unwrap(); // emit 4000
    r.set_head(168).unwrap(); // space = (168 - 4000 - 64) & 4095 = 200
    let e = r.begin(8, 128).unwrap_err();
    assert_eq!(e, SubmitError::RingFull { need: 224, space: 200 });
}

#[test]
fn tail_may_not_enter_the_heads_cacheline_from_behind() {
    // assert_ring_tail_valid (intel_ring.h:105-129): same cacheline and tail < head is refused
    let mut r = Ring::new(4096).unwrap();
    r.set_head(72).unwrap();
    assert_eq!(r.set_tail(64).unwrap_err(), SubmitError::RingTailInvalid { tail: 64, head: 72 });
    assert_eq!(r.set_tail(72).unwrap(), 72, "equal is allowed");
    assert_eq!(r.set_tail(120).unwrap(), 120);
    assert_eq!(r.set_tail(128).unwrap(), 128, "the next cacheline is fine");
    assert_eq!(r.set_tail(0).unwrap(), 0, "a different cacheline behind the head is fine");
    assert_eq!(r.set_tail(4).unwrap_err(), SubmitError::RingOffsetInvalid { pos: 4, size: 4096 });
    assert_eq!(r.set_tail(4096).unwrap_err(), SubmitError::RingOffsetInvalid { pos: 4096, size: 4096 });
}

#[test]
fn reset_collapses_head_tail_and_emit() {
    let mut r = Ring::new(4096).unwrap();
    r.begin(100, 0).unwrap();
    r.reset(4096 + 128); // wrapped like intel_ring_wrap
    assert_eq!((r.head(), r.tail(), r.emit()), (128, 128, 128));
    assert_eq!(r.space(), Ring::space_between(128, 128, 4096));
}

#[test]
fn set_head_refuses_an_offset_outside_the_ring() {
    let mut r = Ring::new(4096).unwrap();
    assert_eq!(r.set_head(4096).unwrap_err(), SubmitError::RingOffsetInvalid { pos: 4096, size: 4096 });
    assert_eq!(r.set_head(12).unwrap_err().name(), "RING-OFFSET-INVALID");
}

#[test]
fn offset_never_returns_the_ring_size() {
    // intel_ring_offset: "Don't write ring->size (equivalent to 0) as that hangs some GPUs"
    let r = Ring::new(4096).unwrap();
    assert_eq!(r.offset(4096), 0);
    assert_eq!(r.offset(4088), 4088);
    assert_eq!(r.wrap(8192 + 16), 16);
}

/// One step of a scripted run: (kind, arg, reserved, result, emit, space, head) where kind 0 is `intel_ring_begin(rq, arg dwords)`
/// with `rq->reserved_space = reserved` (result = the byte offset it returned, or -1 for the error return), kind 1 sets the ring's
/// head to `arg` (the GPU/retire moved it). The trailing fields are the ring state AFTER the step. Every row is the real
/// `intel_ring_begin` of intel_ring.c (:230-309) compiled by GCC, with `wait_for_space` reduced to its first statement
/// (`intel_ring_update_space`) followed by -ENOSPC.
type Step = (u8, i32, u32, i32, u32, u32, u32);

fn replay(script: &[Step]) {
    let mut r = Ring::new(4096).unwrap();
    for (i, (kind, arg, reserved, result, emit, space, head)) in script.iter().copied().enumerate() {
        if kind == 0 {
            match r.begin(arg as u32, reserved) {
                Ok(b) => assert_eq!(b.at as i32, result, "step {i}: begin({arg}, {reserved})"),
                Err(e) => {
                    assert_eq!(result, -1, "step {i}: Linux returned {result}, the port refused with {e:?}");
                    assert_eq!(e.name(), "RING-FULL");
                }
            }
        } else {
            r.set_head(arg as u32).unwrap();
        }
        assert_eq!((r.emit(), r.space(), r.head()), (emit, space, head), "step {i}: state after");
    }
}

const FILL_UNTIL_FULL: &[Step] = &[
    (0, 12, 240, 0, 48, 3984, 0),
    (0, 6, 176, 48, 72, 3960, 0),
    (0, 6, 176, 72, 96, 3936, 0),
    (0, 22, 0, 96, 184, 3848, 0),
    (0, 12, 240, 184, 232, 3800, 0),
    (0, 6, 176, 232, 256, 3776, 0),
    (0, 6, 176, 256, 280, 3752, 0),
    (0, 22, 0, 280, 368, 3664, 0),
    (0, 12, 240, 368, 416, 3616, 0),
    (0, 6, 176, 416, 440, 3592, 0),
    (0, 6, 176, 440, 464, 3568, 0),
    (0, 22, 0, 464, 552, 3480, 0),
    (0, 12, 240, 552, 600, 3432, 0),
    (0, 6, 176, 600, 624, 3408, 0),
    (0, 6, 176, 624, 648, 3384, 0),
    (0, 22, 0, 648, 736, 3296, 0),
    (0, 12, 240, 736, 784, 3248, 0),
    (0, 6, 176, 784, 808, 3224, 0),
    (0, 6, 176, 808, 832, 3200, 0),
    (0, 22, 0, 832, 920, 3112, 0),
    (0, 12, 240, 920, 968, 3064, 0),
    (0, 6, 176, 968, 992, 3040, 0),
    (0, 6, 176, 992, 1016, 3016, 0),
    (0, 22, 0, 1016, 1104, 2928, 0),
    (0, 12, 240, 1104, 1152, 2880, 0),
    (0, 6, 176, 1152, 1176, 2856, 0),
    (0, 6, 176, 1176, 1200, 2832, 0),
    (0, 22, 0, 1200, 1288, 2744, 0),
    (0, 12, 240, 1288, 1336, 2696, 0),
    (0, 6, 176, 1336, 1360, 2672, 0),
    (0, 6, 176, 1360, 1384, 2648, 0),
    (0, 22, 0, 1384, 1472, 2560, 0),
    (0, 12, 240, 1472, 1520, 2512, 0),
    (0, 6, 176, 1520, 1544, 2488, 0),
    (0, 6, 176, 1544, 1568, 2464, 0),
    (0, 22, 0, 1568, 1656, 2376, 0),
    (0, 12, 240, 1656, 1704, 2328, 0),
    (0, 6, 176, 1704, 1728, 2304, 0),
    (0, 6, 176, 1728, 1752, 2280, 0),
    (0, 22, 0, 1752, 1840, 2192, 0),
    (0, 12, 240, 1840, 1888, 2144, 0),
    (0, 6, 176, 1888, 1912, 2120, 0),
    (0, 6, 176, 1912, 1936, 2096, 0),
    (0, 22, 0, 1936, 2024, 2008, 0),
    (0, 12, 240, 2024, 2072, 1960, 0),
    (0, 6, 176, 2072, 2096, 1936, 0),
    (0, 6, 176, 2096, 2120, 1912, 0),
    (0, 22, 0, 2120, 2208, 1824, 0),
    (0, 12, 240, 2208, 2256, 1776, 0),
    (0, 6, 176, 2256, 2280, 1752, 0),
    (0, 6, 176, 2280, 2304, 1728, 0),
    (0, 22, 0, 2304, 2392, 1640, 0),
    (0, 12, 240, 2392, 2440, 1592, 0),
    (0, 6, 176, 2440, 2464, 1568, 0),
    (0, 6, 176, 2464, 2488, 1544, 0),
    (0, 22, 0, 2488, 2576, 1456, 0),
    (0, 12, 240, 2576, 2624, 1408, 0),
    (0, 6, 176, 2624, 2648, 1384, 0),
    (0, 6, 176, 2648, 2672, 1360, 0),
    (0, 22, 0, 2672, 2760, 1272, 0),
    (0, 12, 240, 2760, 2808, 1224, 0),
    (0, 6, 176, 2808, 2832, 1200, 0),
    (0, 6, 176, 2832, 2856, 1176, 0),
    (0, 22, 0, 2856, 2944, 1088, 0),
    (0, 12, 240, 2944, 2992, 1040, 0),
    (0, 6, 176, 2992, 3016, 1016, 0),
    (0, 6, 176, 3016, 3040, 992, 0),
    (0, 22, 0, 3040, 3128, 904, 0),
    (0, 12, 240, 3128, 3176, 856, 0),
    (0, 6, 176, 3176, 3200, 832, 0),
    (0, 6, 176, 3200, 3224, 808, 0),
    (0, 22, 0, 3224, 3312, 720, 0),
    (0, 12, 240, 3312, 3360, 672, 0),
    (0, 6, 176, 3360, 3384, 648, 0),
    (0, 6, 176, 3384, 3408, 624, 0),
    (0, 22, 0, 3408, 3496, 536, 0),
    (0, 12, 240, 3496, 3544, 488, 0),
    (0, 6, 176, 3544, 3568, 464, 0),
    (0, 6, 176, 3568, 3592, 440, 0),
    (0, 22, 0, 3592, 3680, 352, 0),
    (0, 12, 240, 3680, 3728, 304, 0),
    (0, 6, 176, 3728, 3752, 280, 0),
    (0, 6, 176, 3752, 3776, 256, 0),
    (0, 22, 0, 3776, 3864, 168, 0),
    (0, 12, 240, -1, 3864, 168, 0),
];

const HEAD_FOLLOWS: &[Step] = &[
    (0, 12, 240, 0, 48, 3984, 0),
    (0, 6, 176, 48, 72, 3960, 0),
    (0, 6, 176, 72, 96, 3936, 0),
    (0, 22, 0, 96, 184, 3848, 0),
    (1, 96, 0, 0, 184, 3848, 96),
    (0, 12, 240, 184, 232, 3800, 96),
    (0, 6, 176, 232, 256, 3776, 96),
    (0, 6, 176, 256, 280, 3752, 96),
    (0, 22, 0, 280, 368, 3664, 96),
    (1, 280, 0, 0, 368, 3664, 280),
    (0, 12, 240, 368, 416, 3616, 280),
    (0, 6, 176, 416, 440, 3592, 280),
    (0, 6, 176, 440, 464, 3568, 280),
    (0, 22, 0, 464, 552, 3480, 280),
    (1, 464, 0, 0, 552, 3480, 464),
    (0, 12, 240, 552, 600, 3432, 464),
    (0, 6, 176, 600, 624, 3408, 464),
    (0, 6, 176, 624, 648, 3384, 464),
    (0, 22, 0, 648, 736, 3296, 464),
    (1, 648, 0, 0, 736, 3296, 648),
    (0, 12, 240, 736, 784, 3248, 648),
    (0, 6, 176, 784, 808, 3224, 648),
    (0, 6, 176, 808, 832, 3200, 648),
    (0, 22, 0, 832, 920, 3112, 648),
    (1, 832, 0, 0, 920, 3112, 832),
    (0, 12, 240, 920, 968, 3064, 832),
    (0, 6, 176, 968, 992, 3040, 832),
    (0, 6, 176, 992, 1016, 3016, 832),
    (0, 22, 0, 1016, 1104, 2928, 832),
    (1, 1016, 0, 0, 1104, 2928, 1016),
    (0, 12, 240, 1104, 1152, 2880, 1016),
    (0, 6, 176, 1152, 1176, 2856, 1016),
    (0, 6, 176, 1176, 1200, 2832, 1016),
    (0, 22, 0, 1200, 1288, 2744, 1016),
    (1, 1200, 0, 0, 1288, 2744, 1200),
    (0, 12, 240, 1288, 1336, 2696, 1200),
    (0, 6, 176, 1336, 1360, 2672, 1200),
    (0, 6, 176, 1360, 1384, 2648, 1200),
    (0, 22, 0, 1384, 1472, 2560, 1200),
    (1, 1384, 0, 0, 1472, 2560, 1384),
    (0, 12, 240, 1472, 1520, 2512, 1384),
    (0, 6, 176, 1520, 1544, 2488, 1384),
    (0, 6, 176, 1544, 1568, 2464, 1384),
    (0, 22, 0, 1568, 1656, 2376, 1384),
    (1, 1568, 0, 0, 1656, 2376, 1568),
    (0, 12, 240, 1656, 1704, 2328, 1568),
    (0, 6, 176, 1704, 1728, 2304, 1568),
    (0, 6, 176, 1728, 1752, 2280, 1568),
    (0, 22, 0, 1752, 1840, 2192, 1568),
    (1, 1752, 0, 0, 1840, 2192, 1752),
    (0, 12, 240, 1840, 1888, 2144, 1752),
    (0, 6, 176, 1888, 1912, 2120, 1752),
    (0, 6, 176, 1912, 1936, 2096, 1752),
    (0, 22, 0, 1936, 2024, 2008, 1752),
    (1, 1936, 0, 0, 2024, 2008, 1936),
    (0, 12, 240, 2024, 2072, 1960, 1936),
    (0, 6, 176, 2072, 2096, 1936, 1936),
    (0, 6, 176, 2096, 2120, 1912, 1936),
    (0, 22, 0, 2120, 2208, 1824, 1936),
    (1, 2120, 0, 0, 2208, 1824, 2120),
    (0, 12, 240, 2208, 2256, 1776, 2120),
    (0, 6, 176, 2256, 2280, 1752, 2120),
    (0, 6, 176, 2280, 2304, 1728, 2120),
    (0, 22, 0, 2304, 2392, 1640, 2120),
    (1, 2304, 0, 0, 2392, 1640, 2304),
    (0, 12, 240, 2392, 2440, 1592, 2304),
    (0, 6, 176, 2440, 2464, 1568, 2304),
    (0, 6, 176, 2464, 2488, 1544, 2304),
    (0, 22, 0, 2488, 2576, 1456, 2304),
    (1, 2488, 0, 0, 2576, 1456, 2488),
    (0, 12, 240, 2576, 2624, 1408, 2488),
    (0, 6, 176, 2624, 2648, 1384, 2488),
    (0, 6, 176, 2648, 2672, 1360, 2488),
    (0, 22, 0, 2672, 2760, 1272, 2488),
    (1, 2672, 0, 0, 2760, 1272, 2672),
    (0, 12, 240, 2760, 2808, 1224, 2672),
    (0, 6, 176, 2808, 2832, 1200, 2672),
    (0, 6, 176, 2832, 2856, 1176, 2672),
    (0, 22, 0, 2856, 2944, 1088, 2672),
    (1, 2856, 0, 0, 2944, 1088, 2856),
    (0, 12, 240, 2944, 2992, 1040, 2856),
    (0, 6, 176, 2992, 3016, 1016, 2856),
    (0, 6, 176, 3016, 3040, 992, 2856),
    (0, 22, 0, 3040, 3128, 904, 2856),
    (1, 3040, 0, 0, 3128, 904, 3040),
    (0, 12, 240, 3128, 3176, 856, 3040),
    (0, 6, 176, 3176, 3200, 832, 3040),
    (0, 6, 176, 3200, 3224, 808, 3040),
    (0, 22, 0, 3224, 3312, 720, 3040),
    (1, 3224, 0, 0, 3312, 720, 3224),
    (0, 12, 240, 3312, 3360, 672, 3224),
    (0, 6, 176, 3360, 3384, 648, 3224),
    (0, 6, 176, 3384, 3408, 624, 3224),
    (0, 22, 0, 3408, 3496, 536, 3224),
    (1, 3408, 0, 0, 3496, 536, 3408),
    (0, 12, 240, 3496, 3544, 488, 3408),
    (0, 6, 176, 3544, 3568, 464, 3408),
    (0, 6, 176, 3568, 3592, 440, 3408),
    (0, 22, 0, 3592, 3680, 352, 3408),
    (1, 3592, 0, 0, 3680, 352, 3592),
    (0, 12, 240, 3680, 3728, 304, 3592),
    (0, 6, 176, 3728, 3752, 280, 3592),
    (0, 6, 176, 3752, 3776, 256, 3592),
    (0, 22, 0, 3776, 3864, 168, 3592),
    (1, 3776, 0, 0, 3864, 168, 3776),
    (0, 12, 240, 3864, 3912, 3896, 3776),
    (0, 6, 176, 3912, 3936, 3872, 3776),
    (0, 6, 176, 3936, 3960, 3848, 3776),
    (0, 22, 0, 3960, 4048, 3760, 3776),
    (1, 3960, 0, 0, 4048, 3760, 3960),
    (0, 12, 240, 4048, 4096, 3712, 3960),
    (0, 6, 176, 0, 24, 3688, 3960),
    (0, 6, 176, 24, 48, 3664, 3960),
    (0, 22, 0, 48, 136, 3576, 3960),
    (1, 48, 0, 0, 136, 3576, 48),
    (0, 12, 240, 136, 184, 3528, 48),
    (0, 6, 176, 184, 208, 3504, 48),
    (0, 6, 176, 208, 232, 3480, 48),
    (0, 22, 0, 232, 320, 3392, 48),
    (1, 232, 0, 0, 320, 3392, 232),
    (0, 12, 240, 320, 368, 3344, 232),
    (0, 6, 176, 368, 392, 3320, 232),
    (0, 6, 176, 392, 416, 3296, 232),
    (0, 22, 0, 416, 504, 3208, 232),
    (1, 416, 0, 0, 504, 3208, 416),
    (0, 12, 240, 504, 552, 3160, 416),
    (0, 6, 176, 552, 576, 3136, 416),
    (0, 6, 176, 576, 600, 3112, 416),
    (0, 22, 0, 600, 688, 3024, 416),
    (1, 600, 0, 0, 688, 3024, 600),
    (0, 12, 240, 688, 736, 2976, 600),
    (0, 6, 176, 736, 760, 2952, 600),
    (0, 6, 176, 760, 784, 2928, 600),
    (0, 22, 0, 784, 872, 2840, 600),
    (1, 784, 0, 0, 872, 2840, 784),
    (0, 12, 240, 872, 920, 2792, 784),
    (0, 6, 176, 920, 944, 2768, 784),
    (0, 6, 176, 944, 968, 2744, 784),
    (0, 22, 0, 968, 1056, 2656, 784),
    (1, 968, 0, 0, 1056, 2656, 968),
    (0, 12, 240, 1056, 1104, 2608, 968),
    (0, 6, 176, 1104, 1128, 2584, 968),
    (0, 6, 176, 1128, 1152, 2560, 968),
    (0, 22, 0, 1152, 1240, 2472, 968),
    (1, 1152, 0, 0, 1240, 2472, 1152),
    (0, 12, 240, 1240, 1288, 2424, 1152),
    (0, 6, 176, 1288, 1312, 2400, 1152),
    (0, 6, 176, 1312, 1336, 2376, 1152),
    (0, 22, 0, 1336, 1424, 2288, 1152),
    (1, 1336, 0, 0, 1424, 2288, 1336),
    (0, 12, 240, 1424, 1472, 2240, 1336),
    (0, 6, 176, 1472, 1496, 2216, 1336),
    (0, 6, 176, 1496, 1520, 2192, 1336),
    (0, 22, 0, 1520, 1608, 2104, 1336),
    (1, 1520, 0, 0, 1608, 2104, 1520),
    (0, 12, 240, 1608, 1656, 2056, 1520),
    (0, 6, 176, 1656, 1680, 2032, 1520),
    (0, 6, 176, 1680, 1704, 2008, 1520),
    (0, 22, 0, 1704, 1792, 1920, 1520),
    (1, 1704, 0, 0, 1792, 1920, 1704),
    (0, 12, 240, 1792, 1840, 1872, 1704),
    (0, 6, 176, 1840, 1864, 1848, 1704),
    (0, 6, 176, 1864, 1888, 1824, 1704),
    (0, 22, 0, 1888, 1976, 1736, 1704),
    (1, 1888, 0, 0, 1976, 1736, 1888),
    (0, 12, 240, 1976, 2024, 1688, 1888),
    (0, 6, 176, 2024, 2048, 1664, 1888),
    (0, 6, 176, 2048, 2072, 1640, 1888),
    (0, 22, 0, 2072, 2160, 1552, 1888),
    (1, 2072, 0, 0, 2160, 1552, 2072),
    (0, 12, 240, 2160, 2208, 1504, 2072),
    (0, 6, 176, 2208, 2232, 1480, 2072),
    (0, 6, 176, 2232, 2256, 1456, 2072),
    (0, 22, 0, 2256, 2344, 1368, 2072),
    (1, 2256, 0, 0, 2344, 1368, 2256),
    (0, 12, 240, 2344, 2392, 1320, 2256),
    (0, 6, 176, 2392, 2416, 1296, 2256),
    (0, 6, 176, 2416, 2440, 1272, 2256),
    (0, 22, 0, 2440, 2528, 1184, 2256),
    (1, 2440, 0, 0, 2528, 1184, 2440),
    (0, 12, 240, 2528, 2576, 1136, 2440),
    (0, 6, 176, 2576, 2600, 1112, 2440),
    (0, 6, 176, 2600, 2624, 1088, 2440),
    (0, 22, 0, 2624, 2712, 1000, 2440),
    (1, 2624, 0, 0, 2712, 1000, 2624),
    (0, 12, 240, 2712, 2760, 952, 2624),
    (0, 6, 176, 2760, 2784, 928, 2624),
    (0, 6, 176, 2784, 2808, 904, 2624),
    (0, 22, 0, 2808, 2896, 816, 2624),
    (1, 2808, 0, 0, 2896, 816, 2808),
    (0, 12, 240, 2896, 2944, 768, 2808),
    (0, 6, 176, 2944, 2968, 744, 2808),
    (0, 6, 176, 2968, 2992, 720, 2808),
    (0, 22, 0, 2992, 3080, 632, 2808),
    (1, 2992, 0, 0, 3080, 632, 2992),
    (0, 12, 240, 3080, 3128, 584, 2992),
    (0, 6, 176, 3128, 3152, 560, 2992),
    (0, 6, 176, 3152, 3176, 536, 2992),
    (0, 22, 0, 3176, 3264, 448, 2992),
    (1, 3176, 0, 0, 3264, 448, 3176),
];

const BOUNDARIES: &[Step] = &[
    (0, 1000, 0, 0, 4000, 32, 0),
    (1, 4000, 0, 0, 4000, 32, 4000),
    (0, 24, 64, 4000, 4096, 3936, 4000),
    (0, 2, 64, 0, 8, 3928, 4000),
    (1, 4088, 0, 0, 8, 3928, 4088),
    (0, 16, 64, 8, 72, 3864, 4088),
    (0, 46, 176, 72, 256, 3680, 4088),
    (1, 2000, 0, 0, 256, 3680, 2000),
    (0, 100, 64, 256, 656, 3280, 2000),
    (0, 2, 3000, 656, 664, 3272, 2000),
    (1, 0, 0, 0, 664, 3272, 0),
    (0, 500, 64, 664, 2664, 1272, 0),
    (0, 2, 64, 2664, 2672, 1264, 0),
];

#[test]
fn a_ring_filled_until_it_refuses_matches_intel_ring_begin_step_by_step() {
    replay(FILL_UNTIL_FULL);
    // the last row is the refusal: whole requests of 184 bytes fit until the next flush (48 + 240 reserved) no longer does
    assert_eq!(FILL_UNTIL_FULL.last().unwrap().3, -1);
}

#[test]
fn a_ring_whose_head_follows_the_retiring_requests_wraps_exactly_as_intel_ring_begin_does() {
    replay(HEAD_FOLLOWS);
    assert!(HEAD_FOLLOWS.iter().skip(1).any(|s| s.0 == 0 && s.3 == 0), "the script wraps the ring several times");
}

#[test]
fn packets_at_the_boundaries_match_intel_ring_begin() {
    replay(BOUNDARIES);
}
