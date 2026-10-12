// SPDX-License-Identifier: GPL-2.0-only
//! The argument-taking register macros and the catalog invariants of the GT workaround / engine / i915_reg.h modules.
//! Expected values are what GCC computes from the Linux headers (macro-expanded), written out literally.
use i915_common_core::Reg;
use i915_gt_regs_core::*;

#[test]
fn mcr_selector_fields_are_linux_literals() {
    assert_eq!(gen8_mcr_slice(0), 0x0000_0000);
    assert_eq!(gen8_mcr_slice(1), 0x0400_0000);
    assert_eq!(gen8_mcr_slice(2), 0x0800_0000);
    assert_eq!(gen8_mcr_slice(3), 0x0c00_0000);
    assert_eq!(gen8_mcr_slice(5), 0x0400_0000, "slice is masked to 2 bits (& 3)");
    assert_eq!(GEN8_MCR_SLICE_MASK, 0x0c00_0000);
    assert_eq!(gen8_mcr_subslice(0), 0);
    assert_eq!(gen8_mcr_subslice(1), 0x0100_0000);
    assert_eq!(gen8_mcr_subslice(3), 0x0300_0000);
    assert_eq!(GEN8_MCR_SUBSLICE_MASK, 0x0300_0000);
    assert_eq!(GEN8_MCR_SLICE_MASK & GEN8_MCR_SUBSLICE_MASK, 0);
}

#[test]
fn preempt_gpgpu_levels_are_distinct_linux_literals() {
    assert_eq!(GEN9_PREEMPT_GPGPU_MID_THREAD_LEVEL, 0);
    assert_eq!(GEN9_PREEMPT_GPGPU_THREAD_GROUP_LEVEL, 2);
    assert_eq!(GEN9_PREEMPT_GPGPU_COMMAND_LEVEL, 4);
    assert_eq!(GEN9_PREEMPT_GPGPU_LEVEL_MASK, 6);
    // the zero-valued family member is pinned by distinctness from every sibling
    for sibling in [GEN9_PREEMPT_GPGPU_THREAD_GROUP_LEVEL, GEN9_PREEMPT_GPGPU_COMMAND_LEVEL, GEN9_PREEMPT_GPGPU_LEVEL_MASK] {
        assert_ne!(GEN9_PREEMPT_GPGPU_MID_THREAD_LEVEL, sibling);
    }
}

#[test]
fn lncfcmocs_words_are_linux_literals() {
    assert_eq!(gen9_lncfcmocs(0).offset(), 0xb020);
    assert_eq!(gen9_lncfcmocs(31).offset(), 0xb09c);
    assert_eq!(LNCFCMOCS_REG_COUNT, 32);
}

#[test]
fn ring_registers_are_linux_literals() {
    assert_eq!(ring_nopid(0x2000).offset(), 0x2094);
    assert_eq!(ring_eir(0x2000).offset(), 0x20b0);
    assert_eq!(ring_emr(0x22000).offset(), 0x220b4);
    assert_eq!(ring_reset_ctl(0x2000).offset(), 0x20d0);
    assert_eq!(ring_reset_ctl(0x12000).offset(), 0x120d0);
    assert_eq!(ring_sema_wait_poll(0x1a000).offset(), 0x1a24c);
    assert_eq!(ring_force_to_nonpriv(0x2000, 0).offset(), 0x24d0);
    assert_eq!(ring_force_to_nonpriv(0x2000, 11).offset(), 0x24fc);
    assert_eq!(RING_FORCE_TO_NONPRIV_MASK_VALID, 0x7000_0003);
}

#[test]
fn nonpriv_access_and_range_families_are_distinct_and_cover_their_masks() {
    // the access field has four members (RW is zero-valued): each must be distinct and inside the mask
    let access = [
        RING_FORCE_TO_NONPRIV_ACCESS_RW,
        RING_FORCE_TO_NONPRIV_ACCESS_RD,
        RING_FORCE_TO_NONPRIV_ACCESS_WR,
        RING_FORCE_TO_NONPRIV_ACCESS_INVALID,
    ];
    for (i, a) in access.iter().enumerate() {
        assert_eq!(a & !RING_FORCE_TO_NONPRIV_ACCESS_MASK, 0);
        assert_eq!(*a >> 28, i as u32, "access member {i} selects bits 29:28 with value {i}");
    }
    let range = [
        RING_FORCE_TO_NONPRIV_RANGE_1,
        RING_FORCE_TO_NONPRIV_RANGE_4,
        RING_FORCE_TO_NONPRIV_RANGE_16,
        RING_FORCE_TO_NONPRIV_RANGE_64,
    ];
    for (i, r) in range.iter().enumerate() {
        assert_eq!(r & !RING_FORCE_TO_NONPRIV_RANGE_MASK, 0);
        assert_eq!(*r, i as u32);
    }
    assert_eq!(RING_FORCE_TO_NONPRIV_ADDRESS_MASK, 0x03ff_fffc);
    assert_eq!(RING_FORCE_TO_NONPRIV_ADDRESS_MASK & RING_FORCE_TO_NONPRIV_ACCESS_MASK, 0);
}

#[test]
fn gen8_gt_interrupt_banks_are_linux_literals() {
    assert_eq!(gen8_gt_isr(0).offset(), 0x44300);
    assert_eq!(gen8_gt_imr(0).offset(), 0x44304);
    assert_eq!(gen8_gt_iir(0).offset(), 0x44308);
    assert_eq!(gen8_gt_ier(0).offset(), 0x4430c);
    assert_eq!(gen8_gt_isr(3).offset(), 0x44330);
    assert_eq!(gen8_gt_imr(1).offset(), 0x44314);
    assert_eq!(gen8_gt_iir(2).offset(), 0x44328);
    assert_eq!(gen8_gt_ier(3).offset(), 0x4433c);
}

#[test]
fn gen8_master_irq_bank_bits_are_distinct_single_bits() {
    let bits = [
        GEN8_GT_RCS_IRQ,
        GEN8_GT_BCS_IRQ,
        GEN8_GT_VCS0_IRQ,
        GEN8_GT_VCS1_IRQ,
        GEN8_GT_PM_IRQ,
        GEN8_GT_GUC_IRQ,
        GEN8_GT_VECS_IRQ,
    ];
    for (i, b) in bits.iter().enumerate() {
        assert_eq!(b.count_ones(), 1);
        assert_eq!(b.trailing_zeros(), i as u32, "master-control bank bits are consecutive from 0");
    }
    // shift members: RCS / VCS0 / VECS are zero-valued; pinned by the behaviour that BCS / VCS1 are the upper half
    assert_eq!(GEN8_RCS_IRQ_SHIFT, 0);
    assert_eq!(GEN8_VCS0_IRQ_SHIFT, 0);
    assert_eq!(GEN8_VECS_IRQ_SHIFT, 0);
    assert_eq!(GEN8_BCS_IRQ_SHIFT, 16);
    assert_eq!(GEN8_VCS1_IRQ_SHIFT, 16);
}

#[test]
fn new_modules_have_aligned_unique_offsets_and_listed_aliases() {
    let mut all: Vec<(&str, Reg)> = Vec::new();
    all.extend(GT_WA_CATALOG.iter().copied());
    all.extend(I915_REG_CATALOG.iter().copied());
    all.extend(CATALOG.iter().copied());
    for (i, (na, a)) in all.iter().enumerate() {
        assert_eq!(a.offset() % 4, 0, "{na} misaligned");
        for (nb, b) in &all[i + 1..] {
            assert_ne!(na, nb, "duplicate name across modules");
            assert_ne!(a, b, "{na} and {nb} share an offset without being a listed alias");
        }
    }
    for (alias, home) in WA_ALIASES {
        assert_eq!(alias, home);
    }
    assert_eq!(WA_ALIASES.len(), 1);
    assert_eq!(BDW_SCRATCH1, GEN9_SCRATCH1, "same register, two Linux names (intel_gt_regs.h:998,1001)");
}
