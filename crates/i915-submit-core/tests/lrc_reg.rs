// SPDX-License-Identifier: GPL-2.0-only
//! Every layout / descriptor / status constant against the value GCC computes from the REAL Linux sources (intel_lrc_reg.h
//! included whole; the `#define` blocks of intel_lrc.h, intel_engine.h and the head of intel_execlists_submission.c compiled
//! with kernel-macro stubs). The expected side is Linux's number written out literally; the actual side travels through the
//! crate's constant.
use i915_gt_core::emit::{I915_GEM_HWS_PREEMPT_ADDR, LRC_PPHWSP_SCRATCH_ADDR};
use i915_submit_core::lrc_reg::*;

#[test]
fn layout_descriptor_and_status_constants_equal_the_values_gcc_computes_from_the_linux_sources() {
    assert_eq!(CTX_DESC_FORCE_RESTORE, 0x4, "CTX_DESC_FORCE_RESTORE");
    assert_eq!(CTX_CONTEXT_CONTROL, 0x3, "CTX_CONTEXT_CONTROL");
    assert_eq!(CTX_RING_HEAD, 0x5, "CTX_RING_HEAD");
    assert_eq!(CTX_RING_TAIL, 0x7, "CTX_RING_TAIL");
    assert_eq!(CTX_RING_START, 0x9, "CTX_RING_START");
    assert_eq!(CTX_RING_CTL, 0xb, "CTX_RING_CTL");
    assert_eq!(CTX_BB_STATE, 0x11, "CTX_BB_STATE");
    assert_eq!(CTX_TIMESTAMP, 0x23, "CTX_TIMESTAMP");
    assert_eq!(CTX_PDP3_UDW, 0x25, "CTX_PDP3_UDW");
    assert_eq!(CTX_PDP3_LDW, 0x27, "CTX_PDP3_LDW");
    assert_eq!(CTX_PDP2_UDW, 0x29, "CTX_PDP2_UDW");
    assert_eq!(CTX_PDP2_LDW, 0x2b, "CTX_PDP2_LDW");
    assert_eq!(CTX_PDP1_UDW, 0x2d, "CTX_PDP1_UDW");
    assert_eq!(CTX_PDP1_LDW, 0x2f, "CTX_PDP1_LDW");
    assert_eq!(CTX_PDP0_UDW, 0x31, "CTX_PDP0_UDW");
    assert_eq!(CTX_PDP0_LDW, 0x33, "CTX_PDP0_LDW");
    assert_eq!(CTX_R_PWR_CLK_STATE, 0x43, "CTX_R_PWR_CLK_STATE");
    assert_eq!(GEN9_CTX_RING_MI_MODE, 0x54, "GEN9_CTX_RING_MI_MODE");
    assert_eq!(GEN8_CTX_RCS_INDIRECT_CTX_OFFSET_DEFAULT, 0x17, "GEN8_CTX_RCS_INDIRECT_CTX_OFFSET_DEFAULT");
    assert_eq!(GEN9_CTX_RCS_INDIRECT_CTX_OFFSET_DEFAULT, 0x26, "GEN9_CTX_RCS_INDIRECT_CTX_OFFSET_DEFAULT");
    assert_eq!(GEN8_EXECLISTS_STATUS_BUF, 0x370, "GEN8_EXECLISTS_STATUS_BUF");
    assert_eq!(GEN8_CSB_ENTRIES, 0x6, "GEN8_CSB_ENTRIES");
    assert_eq!(GEN8_CSB_PTR_MASK, 0x7, "GEN8_CSB_PTR_MASK");
    assert_eq!(GEN8_CSB_READ_PTR_MASK, 0x700, "GEN8_CSB_READ_PTR_MASK");
    assert_eq!(GEN8_CSB_WRITE_PTR_MASK, 0x7, "GEN8_CSB_WRITE_PTR_MASK");
    assert_eq!(MAX_CONTEXT_HW_ID, 0x200000, "MAX_CONTEXT_HW_ID");
    assert_eq!(GEN12_MAX_CONTEXT_HW_ID, 0x7ff, "GEN12_MAX_CONTEXT_HW_ID");
    assert_eq!(LRC_PPHWSP_PN, 0, "LRC_PPHWSP_PN");
    assert_eq!(LRC_PPHWSP_SZ, 0x1, "LRC_PPHWSP_SZ");
    assert_eq!(LRC_STATE_PN, 0x1, "LRC_STATE_PN");
    assert_eq!(LRC_STATE_OFFSET, 0x1000, "LRC_STATE_OFFSET");
    assert_eq!(LRC_PPHWSP_SCRATCH, 0x34, "LRC_PPHWSP_SCRATCH");
    assert_eq!(INTEL_ADVANCED_CONTEXT, 0, "INTEL_ADVANCED_CONTEXT");
    assert_eq!(INTEL_LEGACY_32B_CONTEXT, 0x1, "INTEL_LEGACY_32B_CONTEXT");
    assert_eq!(INTEL_ADVANCED_AD_CONTEXT, 0x2, "INTEL_ADVANCED_AD_CONTEXT");
    assert_eq!(INTEL_LEGACY_64B_CONTEXT, 0x3, "INTEL_LEGACY_64B_CONTEXT");
    assert_eq!(CTX_GTT_ADDRESS_MASK, 0xfffff000, "CTX_GTT_ADDRESS_MASK");
    assert_eq!(GEN8_CTX_VALID, 0x1, "GEN8_CTX_VALID");
    assert_eq!(GEN8_CTX_FORCE_PD_RESTORE, 0x2, "GEN8_CTX_FORCE_PD_RESTORE");
    assert_eq!(GEN8_CTX_FORCE_RESTORE, 0x4, "GEN8_CTX_FORCE_RESTORE");
    assert_eq!(GEN8_CTX_L3LLC_COHERENT, 0x20, "GEN8_CTX_L3LLC_COHERENT");
    assert_eq!(GEN8_CTX_PRIVILEGE, 0x100, "GEN8_CTX_PRIVILEGE");
    assert_eq!(GEN8_CTX_ADDRESSING_MODE_SHIFT, 0x3, "GEN8_CTX_ADDRESSING_MODE_SHIFT");
    assert_eq!(GEN8_CTX_ID_SHIFT, 0x20, "GEN8_CTX_ID_SHIFT");
    assert_eq!(GEN8_CTX_ID_WIDTH, 0x15, "GEN8_CTX_ID_WIDTH");
    assert_eq!(GEN11_SW_CTX_ID_SHIFT, 0x25, "GEN11_SW_CTX_ID_SHIFT");
    assert_eq!(GEN11_SW_CTX_ID_WIDTH, 0xb, "GEN11_SW_CTX_ID_WIDTH");
    assert_eq!(RING_EXECLIST_QFULL, 0x4, "RING_EXECLIST_QFULL");
    assert_eq!(RING_EXECLIST1_VALID, 0x8, "RING_EXECLIST1_VALID");
    assert_eq!(RING_EXECLIST0_VALID, 0x10, "RING_EXECLIST0_VALID");
    assert_eq!(RING_EXECLIST_ACTIVE_STATUS, 0xc000, "RING_EXECLIST_ACTIVE_STATUS");
    assert_eq!(RING_EXECLIST1_ACTIVE, 0x20000, "RING_EXECLIST1_ACTIVE");
    assert_eq!(RING_EXECLIST0_ACTIVE, 0x40000, "RING_EXECLIST0_ACTIVE");
    assert_eq!(GEN8_CTX_STATUS_IDLE_ACTIVE, 0x1, "GEN8_CTX_STATUS_IDLE_ACTIVE");
    assert_eq!(GEN8_CTX_STATUS_PREEMPTED, 0x2, "GEN8_CTX_STATUS_PREEMPTED");
    assert_eq!(GEN8_CTX_STATUS_ELEMENT_SWITCH, 0x4, "GEN8_CTX_STATUS_ELEMENT_SWITCH");
    assert_eq!(GEN8_CTX_STATUS_ACTIVE_IDLE, 0x8, "GEN8_CTX_STATUS_ACTIVE_IDLE");
    assert_eq!(GEN8_CTX_STATUS_COMPLETE, 0x10, "GEN8_CTX_STATUS_COMPLETE");
    assert_eq!(GEN8_CTX_STATUS_LITE_RESTORE, 0x8000, "GEN8_CTX_STATUS_LITE_RESTORE");
    assert_eq!(GEN8_CTX_STATUS_COMPLETED_MASK, 0x12, "GEN8_CTX_STATUS_COMPLETED_MASK");
    assert_eq!(I915_GEM_HWS_PREEMPT, 0x32, "I915_GEM_HWS_PREEMPT");
    assert_eq!(I915_GEM_HWS_SEQNO, 0x40, "I915_GEM_HWS_SEQNO");
    assert_eq!(I915_GEM_HWS_GGTT_BIND, 0x46, "I915_GEM_HWS_GGTT_BIND");
    assert_eq!(I915_GEM_HWS_PXP, 0x60, "I915_GEM_HWS_PXP");
    assert_eq!(I915_GEM_HWS_GSC, 0x62, "I915_GEM_HWS_GSC");
    assert_eq!(I915_GEM_HWS_SCRATCH, 0x80, "I915_GEM_HWS_SCRATCH");
    assert_eq!(I915_HWS_CSB_BUF0_INDEX, 0x10, "I915_HWS_CSB_BUF0_INDEX");
    assert_eq!(I915_HWS_CSB_WRITE_INDEX, 0x1f, "I915_HWS_CSB_WRITE_INDEX");
    assert_eq!(CACHELINE_BYTES, 0x40, "CACHELINE_BYTES");
    assert_eq!(CACHELINE_DWORDS, 0x10, "CACHELINE_DWORDS");
    assert_eq!(EXECLISTS_REQUEST_SIZE, 0x40, "EXECLISTS_REQUEST_SIZE");
}

#[test]
fn byte_addresses_owned_by_gt_core_agree_with_the_dword_indices_owned_here() {
    // intel_lrc.h:31 and intel_engine.h:189 define the byte forms as "index * sizeof(u32)"; gt-core owns the byte forms.
    assert_eq!(LRC_PPHWSP_SCRATCH_ADDR, LRC_PPHWSP_SCRATCH * 4);
    assert_eq!(LRC_PPHWSP_SCRATCH_ADDR, 0xd0);
    assert_eq!(I915_GEM_HWS_PREEMPT_ADDR, I915_GEM_HWS_PREEMPT * 4);
    assert_eq!(I915_GEM_HWS_PREEMPT_ADDR, 0xc8);
}

#[test]
fn zero_valued_addressing_mode_is_distinct_from_its_siblings() {
    // INTEL_ADVANCED_CONTEXT is 0 (trap 2): pin it by distinctness and by the order of the enum.
    let modes = [INTEL_ADVANCED_CONTEXT, INTEL_LEGACY_32B_CONTEXT, INTEL_ADVANCED_AD_CONTEXT, INTEL_LEGACY_64B_CONTEXT];
    for i in 0..modes.len() {
        for j in 0..modes.len() {
            assert_eq!(modes[i] == modes[j], i == j);
        }
    }
    assert_eq!(modes, [0, 1, 2, 3]);
}

#[test]
fn csb_entries_and_pointer_masks_are_consistent() {
    // intel_lrc_reg.h:64-67: 6 entries, 3-bit pointers (the pointer wraps after 5), read pointer at bit 8.
    assert!(GEN8_CSB_ENTRIES <= GEN8_CSB_PTR_MASK, "every entry index fits a pointer");
    assert_eq!(GEN8_CSB_READ_PTR_MASK, GEN8_CSB_PTR_MASK << 8);
    assert_eq!(GEN8_CSB_WRITE_PTR_MASK, GEN8_CSB_PTR_MASK);
    assert_eq!(csb_dwords_end(), I915_HWS_CSB_BUF0_INDEX + 2 * GEN8_CSB_ENTRIES);
    assert_eq!(csb_dwords_end(), 0x1c, "the six qwords at 0x10..0x1b end below the write-pointer dword 0x1f");
    assert!(csb_dwords_end() <= I915_HWS_CSB_WRITE_INDEX);
}

fn csb_dwords_end() -> u32 {
    I915_HWS_CSB_BUF0_INDEX + 2 * GEN8_CSB_ENTRIES
}
