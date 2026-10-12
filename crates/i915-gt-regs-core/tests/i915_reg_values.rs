// SPDX-License-Identifier: GPL-2.0-only
//! i915_reg: every constant against the value GCC computes from the Linux header itself (macro-expanded with
//! REG_BIT / REG_GENMASK / _MMIO stubs), written out literally. Generated once from the real header; the expected side is
//! Linux's number, the actual side travels through the crate's constant.
use i915_gt_regs_core::*;

#[test]
fn i915_reg_constants_equal_the_values_gcc_computes_from_the_linux_header() {
    assert_eq!(RENDER_RING_BASE, 0x00002000, "RENDER_RING_BASE");
    assert_eq!(GEN6_BSD_RING_BASE, 0x00012000, "GEN6_BSD_RING_BASE");
    assert_eq!(GEN8_BSD2_RING_BASE, 0x0001c000, "GEN8_BSD2_RING_BASE");
    assert_eq!(BSD_RING_BASE, 0x00004000, "BSD_RING_BASE");
    assert_eq!(VEBOX_RING_BASE, 0x0001a000, "VEBOX_RING_BASE");
    assert_eq!(BLT_RING_BASE, 0x00022000, "BLT_RING_BASE");
    assert_eq!(GEN11_BSD_RING_BASE, 0x001c0000, "GEN11_BSD_RING_BASE");
    assert_eq!(GEN11_BSD2_RING_BASE, 0x001c4000, "GEN11_BSD2_RING_BASE");
    assert_eq!(GEN11_VEBOX_RING_BASE, 0x001c8000, "GEN11_VEBOX_RING_BASE");
    assert_eq!(GT_WAIT_SEMAPHORE_INTERRUPT, 0x00000800, "GT_WAIT_SEMAPHORE_INTERRUPT");
    assert_eq!(GT_CONTEXT_SWITCH_INTERRUPT, 0x00000100, "GT_CONTEXT_SWITCH_INTERRUPT");
    assert_eq!(GT_RENDER_PIPECTL_NOTIFY_INTERRUPT, 0x00000010, "GT_RENDER_PIPECTL_NOTIFY_INTERRUPT");
    assert_eq!(GT_CS_MASTER_ERROR_INTERRUPT, 0x00000008, "GT_CS_MASTER_ERROR_INTERRUPT");
    assert_eq!(GT_RENDER_SYNC_STATUS_INTERRUPT, 0x00000004, "GT_RENDER_SYNC_STATUS_INTERRUPT");
    assert_eq!(GT_RENDER_DEBUG_INTERRUPT, 0x00000002, "GT_RENDER_DEBUG_INTERRUPT");
    assert_eq!(GT_RENDER_USER_INTERRUPT, 0x00000001, "GT_RENDER_USER_INTERRUPT");
    assert_eq!(GEN8_MASTER_IRQ.offset(), 0x00044200, "GEN8_MASTER_IRQ");
    assert_eq!(GEN8_MASTER_IRQ_CONTROL, 0x80000000, "GEN8_MASTER_IRQ_CONTROL");
    assert_eq!(GEN8_GT_VECS_IRQ, 0x00000040, "GEN8_GT_VECS_IRQ");
    assert_eq!(GEN8_GT_GUC_IRQ, 0x00000020, "GEN8_GT_GUC_IRQ");
    assert_eq!(GEN8_GT_PM_IRQ, 0x00000010, "GEN8_GT_PM_IRQ");
    assert_eq!(GEN8_GT_VCS1_IRQ, 0x00000008, "GEN8_GT_VCS1_IRQ");
    assert_eq!(GEN8_GT_VCS0_IRQ, 0x00000004, "GEN8_GT_VCS0_IRQ");
    assert_eq!(GEN8_GT_BCS_IRQ, 0x00000002, "GEN8_GT_BCS_IRQ");
    assert_eq!(GEN8_GT_RCS_IRQ, 0x00000001, "GEN8_GT_RCS_IRQ");
    assert_eq!(GEN8_RCS_IRQ_SHIFT, 0x00000000, "GEN8_RCS_IRQ_SHIFT");
    assert_eq!(GEN8_BCS_IRQ_SHIFT, 0x00000010, "GEN8_BCS_IRQ_SHIFT");
    assert_eq!(GEN8_VCS0_IRQ_SHIFT, 0x00000000, "GEN8_VCS0_IRQ_SHIFT");
    assert_eq!(GEN8_VCS1_IRQ_SHIFT, 0x00000010, "GEN8_VCS1_IRQ_SHIFT");
    assert_eq!(GEN8_VECS_IRQ_SHIFT, 0x00000000, "GEN8_VECS_IRQ_SHIFT");
    assert_eq!(CHICKEN_PAR1_1.offset(), 0x00042080, "CHICKEN_PAR1_1");
    assert_eq!(SKL_DE_COMPRESSED_HASH_MODE, 0x00008000, "SKL_DE_COMPRESSED_HASH_MODE");
    assert_eq!(SKL_EDP_PSR_FIX_RDWRAP, 0x00000008, "SKL_EDP_PSR_FIX_RDWRAP");
    assert_eq!(DISP_ARB_CTL.offset(), 0x00045000, "DISP_ARB_CTL");
    assert_eq!(DISP_FBC_MEMORY_WAKE, 0x80000000, "DISP_FBC_MEMORY_WAKE");
    assert_eq!(DISP_FBC_WM_DIS, 0x00008000, "DISP_FBC_WM_DIS");
    assert_eq!(GEN8_CHICKEN_DCPR_1.offset(), 0x00046430, "GEN8_CHICKEN_DCPR_1");
    assert_eq!(MASK_WAKEMEM, 0x00002000, "MASK_WAKEMEM");
}

/// PCU mailbox words moved in from i915-display-clock-core (sentinel audit 2026-10-12), pinned to the Linux literals.
#[test]
fn pcu_mailbox_words_have_linux_values() {
    assert_eq!(GEN6_PCODE_MAILBOX, 0x138124); // i915_reg.h:1079
    assert_eq!(GEN6_PCODE_DATA, 0x138128); // :1179
    assert_eq!(GEN6_PCODE_DATA1, 0x13812c); // :1182
    assert_eq!(GEN6_PCODE_READY, 0x8000_0000); // :1080
    assert_eq!(GEN6_PCODE_ERROR_MASK, 0xff); // :1084
    assert_eq!(GEN6_PCODE_SUCCESS, 0x0); // :1085
    assert_eq!(GEN6_PCODE_ILLEGAL_CMD, 0x1); // :1086
    assert_eq!(GEN7_PCODE_TIMEOUT, 0x2); // :1090
    assert_eq!(GEN7_PCODE_ILLEGAL_DATA, 0x3); // :1091
    assert_eq!(GEN11_PCODE_ILLEGAL_SUBCOMMAND, 0x4); // :1092
    assert_eq!(GEN11_PCODE_LOCKED, 0x6); // :1093
    assert_eq!(GEN11_PCODE_REJECTED, 0x11); // :1094
    assert_eq!(GEN7_PCODE_MIN_FREQ_TABLE_GT_RATIO_OUT_OF_RANGE, 0x10); // :1095
    assert_eq!((SKL_PCODE_CDCLK_CONTROL, SKL_CDCLK_PREPARE_FOR_CHANGE, SKL_CDCLK_READY_FOR_CHANGE), (0x7, 0x3, 0x1)); // :1107-1109
}

/// Re-homed from i915-display-link-core (2026-10-12): i915_reg.h:1035 / :1055, values from GCC over the Linux header.
#[test]
fn south_chicken1_and_pwm_granularity_have_linux_values() {
    assert_eq!(SOUTH_CHICKEN1.offset(), 0xc2000, "SOUTH_CHICKEN1");
    assert_eq!(SPT_PWM_GRANULARITY, 0x1, "SPT_PWM_GRANULARITY");
}
