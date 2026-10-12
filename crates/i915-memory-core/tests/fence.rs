// SPDX-License-Identifier: GPL-2.0-only
//! `i965_write_fence_reg` (intel_ggtt_fencing.c:61-113) on the Gen6+ path, values worked out by hand.
use i915_common_core::Reg;
use i915_memory_core::fence::*;
use i915_memory_core::MemError;

fn w(reg: u32, value: u32) -> FenceStep {
    FenceStep::Write { reg: Reg::new(reg), value }
}
fn r(reg: u32) -> FenceStep {
    FenceStep::PostingRead { reg: Reg::new(reg) }
}

#[test]
fn x_tiled_fence_is_five_steps_off_first_valid_last() {
    // start 1 MiB, size 1 MiB, stride 512 (pitch field 3), X: end = 0x100000 + 0x100000 - 0x1000 = 0x1ff000.
    // val = (0x1ff000 << 32) | 0x100000 | (3 << 32) | VALID(1)  =>  hi = 0x001ff003 (end sits in bits 63:32, pitch in 43:32), lo = 0x00100001
    let p = plan_fence(0, 0x100000, 0x100000, Tiling::X, 512).unwrap();
    assert_eq!(p.steps, [w(0x100000, 0), r(0x100000), w(0x100004, 0x001f_f003), w(0x100000, 0x0010_0001), r(0x100000)]);
}

#[test]
fn y_tiled_sets_bit_1_and_the_last_fence_uses_the_last_slot() {
    let p = plan_fence(31, 0x100000, 0x100000, Tiling::Y, 512).unwrap();
    assert_eq!(p.steps, [w(0x1000f8, 0), r(0x1000f8), w(0x1000fc, 0x001f_f003), w(0x1000f8, 0x0010_0003), r(0x1000f8)]);
}

#[test]
fn pitch_field_is_stride_over_128_minus_one() {
    // stride 128 -> 0, stride 4096 -> 31, max pitch val 0x800 -> stride 0x40000 -> field 0x7ff
    let hi = |stride| match plan_fence(2, 0x1000, 0x1000, Tiling::X, stride).unwrap().steps[2] {
        FenceStep::Write { value, .. } => value & 0xfff, // pitch field only (end address is 4 KiB aligned above it)
        _ => unreachable!(),
    };
    assert_eq!(hi(128), 0x0000_0000);
    assert_eq!(hi(4096), 31);
    assert_eq!(hi(128 * 0x800), 0x7ff);
    assert!(plan_fence(2, 0x1000, 0x1000, Tiling::X, 128 * 0x801).is_err());
}

#[test]
fn clearing_a_fence_writes_zero_to_both_halves() {
    let p = plan_fence(5, 0, 0, Tiling::None, 0).unwrap();
    assert_eq!(p.steps, [w(0x100028, 0), r(0x100028), w(0x10002c, 0), w(0x100028, 0), r(0x100028)]);
}

#[test]
fn bad_parameters_are_refused_by_name() {
    let e = |r: Result<FencePlan, MemError>| r.unwrap_err();
    assert_eq!(e(plan_fence(32, 0, 4096, Tiling::X, 128)), MemError::BadFence { what: "fence id", value: 32 });
    assert_eq!(e(plan_fence(0, 0, 4096, Tiling::X, 100)), MemError::BadFence { what: "stride", value: 100 });
    assert_eq!(e(plan_fence(0, 0, 4096, Tiling::X, 0)), MemError::BadFence { what: "stride", value: 0 });
    assert_eq!(e(plan_fence(0, 0, 0, Tiling::X, 128)), MemError::BadFence { what: "size", value: 0 });
    assert_eq!(e(plan_fence(0, 0, 4097, Tiling::X, 128)), MemError::BadFence { what: "size", value: 4097 });
    assert_eq!(e(plan_fence(0, 100, 4096, Tiling::X, 128)), MemError::BadFence { what: "start", value: 100 });
    assert_eq!(e(plan_fence(0, 0xffff_f000, 0x2000, Tiling::X, 128)).name(), "BAD-FENCE");
    // the last fence of a 4 GiB GGTT is fine: end = 2^32
    assert!(plan_fence(0, 0xffff_e000, 0x2000, Tiling::X, 128).is_ok());
}
