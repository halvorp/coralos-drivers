// SPDX-License-Identifier: GPL-2.0-only
//! The workaround batch against the dwords Linux's OWN `gen8_init_indirectctx_bb` / `gen9_init_indirectctx_bb` /
//! `gen8_emit_flush_coherentl3_wa` / `emit_lri` write (intel_lrc.c:1636-1803 compiled by GCC against the real
//! intel_gpu_commands.h, intel_gt_regs.h and gen8_engine_cs.h; only the engine / GT structures are stubbed). Each array is the
//! whole batch including its NOOP padding to a cacheline.
use i915_submit_core::wa_bb::*;
use i915_submit_core::SubmitError;

/// gen9, IS_BROADWELL=0, HAS_POOLED_EU=0, scratch ggtt 0x1000
const KBL_9: [u32; 48] = [
    0x04000000, 0x12400002, 0x0000b118, 0x00001100, 0x00000000, 0x11000001, 0x0000b118, 0x40600000, 0x7a000004, 0x00100020,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x14c00002, 0x0000b118, 0x00001100, 0x00000000, 0x7a000004, 0x08304000,
    0x000000d0, 0x00000000, 0x00000000, 0x00000000, 0x11000005, 0x00007014, 0x10000000, 0x00002088, 0x00020002, 0x00002090,
    0x10001000, 0x00000000, 0x04000001, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
];
/// gen9, IS_BROADWELL=0, HAS_POOLED_EU=1, scratch ggtt 0x2000
const POOLED_9: [u32; 48] = [
    0x04000000, 0x12400002, 0x0000b118, 0x00002100, 0x00000000, 0x11000001, 0x0000b118, 0x40600000, 0x7a000004, 0x00100020,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x14c00002, 0x0000b118, 0x00002100, 0x00000000, 0x7a000004, 0x08304000,
    0x000000d0, 0x00000000, 0x00000000, 0x00000000, 0x11000005, 0x00007014, 0x10000000, 0x00002088, 0x00020002, 0x00002090,
    0x10001000, 0x00000000, 0x70050004, 0x80000000, 0x00777000, 0x00000000, 0x00000000, 0x00000000, 0x04000001, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
];
/// gen8, IS_BROADWELL=0, HAS_POOLED_EU=0, scratch ggtt 0x1000
const GEN8_CHV: [u32; 16] = [
    0x04000000, 0x7a000004, 0x08304000, 0x000000d0, 0x00000000, 0x00000000, 0x00000000, 0x04000001, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
];
/// gen8, IS_BROADWELL=1, HAS_POOLED_EU=0, scratch ggtt 0x1000
const GEN8_BDW: [u32; 32] = [
    0x04000000, 0x12400002, 0x0000b118, 0x00001100, 0x00000000, 0x11000001, 0x0000b118, 0x40600000, 0x7a000004, 0x00100020,
    0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x14c00002, 0x0000b118, 0x00001100, 0x00000000, 0x7a000004, 0x08304000,
    0x000000d0, 0x00000000, 0x00000000, 0x00000000, 0x04000001, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000,
];

fn build(gen: u8, bdw: bool, pooled: bool, scratch: u32) -> (Vec<u32>, WaCtx) {
    let mut page = vec![0xdead_beefu32; 1024];
    let wa = build_wa_ctx(&WaBbParams { gen, is_broadwell: bdw, has_pooled_eu: pooled, scratch_ggtt: scratch }, &mut page).unwrap();
    page.truncate((wa.indirect_ctx.size / 4) as usize);
    (page, wa)
}

#[test]
fn kaby_lake_batch_equals_gen9_init_indirectctx_bb() {
    let (dw, wa) = build(9, false, false, 0x1000);
    assert_eq!(dw, KBL_9);
    assert_eq!(wa.indirect_ctx, WaBb { offset: 0, size: 192 });
    assert_eq!(wa.per_ctx, WaBb { offset: 192, size: 0 }, "no per-context batch on Gen9 (wa_bb_fn[1] = NULL)");
}

#[test]
fn pooled_eu_parts_add_the_media_pool_state_command() {
    let (dw, wa) = build(9, false, true, 0x2000);
    assert_eq!(dw, POOLED_9);
    assert_eq!(wa.indirect_ctx.size, 192);
    assert_ne!(KBL_9, POOLED_9);
}

#[test]
fn gen8_batch_has_the_l3_flush_only_on_broadwell() {
    let (dw, wa) = build(8, false, false, 0x1000);
    assert_eq!(dw, GEN8_CHV);
    assert_eq!(wa.indirect_ctx.size, 64);
    let (dw, wa) = build(8, true, false, 0x1000);
    assert_eq!(dw, GEN8_BDW);
    assert_eq!(wa.indirect_ctx.size, 128);
}

#[test]
fn every_batch_is_whole_cachelines_and_ends_in_noop_padding() {
    for (gen, bdw, pooled) in [(9u8, false, false), (9, false, true), (8, false, false), (8, true, false)] {
        let (dw, wa) = build(gen, bdw, pooled, 0x1000);
        assert_eq!(wa.indirect_ctx.size % 64, 0, "the indirect batch length is in cachelines (CACHELINE_BYTES)");
        assert_eq!(wa.per_ctx.offset, wa.indirect_ctx.size);
        assert_eq!(*dw.last().unwrap(), 0, "padded with MI_NOOP");
        // MI_ARB_ON_OFF | MI_ARB_ENABLE sits before the padding; MI_ARB_ON_OFF | MI_ARB_DISABLE opens the batch
        assert_eq!(dw[0], 0x0400_0000);
    }
}

#[test]
fn the_scratch_dword_sits_256_bytes_into_the_scratch_object() {
    let (dw, _) = build(9, false, false, 0x1000);
    // the SRM and LRM operands (dwords 3 and 16 of the Linux listing): scratch + INTEL_GT_SCRATCH_FIELD_COHERENTL3_WA
    assert_eq!(dw[3], 0x1000 + INTEL_GT_SCRATCH_FIELD_COHERENTL3_WA);
    assert_eq!(dw[3], 0x1100);
    assert_eq!(dw[16], 0x1100);
    let (dw, _) = build(9, false, false, 0x4000_0000);
    assert_eq!(dw[3], 0x4000_0100);
}

#[test]
fn a_scratch_address_that_overflows_is_refused_by_name() {
    let mut page = vec![0u32; 1024];
    let e = build_wa_ctx(&WaBbParams { gen: 9, is_broadwell: false, has_pooled_eu: false, scratch_ggtt: 0xffff_ff80 }, &mut page).unwrap_err();
    assert_eq!(e, SubmitError::ScratchOverflow { scratch: 0xffff_ff80 });
    assert_eq!(e.name(), "SCRATCH-OVERFLOW");
}

#[test]
fn a_page_too_small_for_the_batch_page_is_refused() {
    let mut page = vec![0u32; 512];
    let e = build_wa_ctx(&WaBbParams { gen: 9, is_broadwell: false, has_pooled_eu: false, scratch_ggtt: 0x1000 }, &mut page).unwrap_err();
    assert_eq!(e.name(), "CS-OVERFLOW");
}

#[test]
fn only_gen8_and_gen9_have_a_batch() {
    let mut page = vec![0u32; 1024];
    for gen in [7u8, 10, 11, 12] {
        let e = build_wa_ctx(&WaBbParams { gen, is_broadwell: false, has_pooled_eu: false, scratch_ggtt: 0x1000 }, &mut page).unwrap_err();
        assert_eq!(e, SubmitError::UnsupportedGeneration { gen });
    }
}

#[test]
fn the_wa_page_constant_is_one_page() {
    assert_eq!(CTX_WA_BB_SIZE, 4096); // intel_lrc.c:1805: #define CTX_WA_BB_SIZE (PAGE_SIZE)
    assert_eq!(INTEL_GT_SCRATCH_FIELD_COHERENTL3_WA, 256); // intel_gt_types.h:319
}
