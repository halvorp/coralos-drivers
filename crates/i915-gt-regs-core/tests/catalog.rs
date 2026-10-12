// SPDX-License-Identifier: GPL-2.0-only
//! Register offsets and fields against Linux's literals (expected values written out by hand), and the "one home per
//! register" invariant: every offset is aligned and unique, every duplicate is a listed alias.
use i915_gt_regs_core::*;

#[test]
fn forcewake_gen9_registers_are_linux_literals() {
    assert_eq!(FORCEWAKE_GT_GEN9.offset(), 0xa188);
    assert_eq!(FORCEWAKE_MEDIA_GEN9.offset(), 0xa270);
    assert_eq!(FORCEWAKE_RENDER_GEN9.offset(), 0xa278);
    assert_eq!(FORCEWAKE_ACK_GT_GEN9.offset(), 0x130044);
    assert_eq!(FORCEWAKE_ACK_RENDER_GEN9.offset(), 0xd84);
    assert_eq!(FORCEWAKE_ACK_MEDIA_GEN9.offset(), 0xd88);
}

#[test]
fn forcewake_bits_are_linux_literals() {
    assert_eq!(FORCEWAKE_KERNEL, 0x1);
    assert_eq!(FORCEWAKE_USER, 0x2);
    assert_eq!(FORCEWAKE_KERNEL_FALLBACK, 0x8000);
    // distinct bits (trap 2: a zero or colliding value would be indistinguishable)
    assert_eq!(FORCEWAKE_KERNEL & FORCEWAKE_USER, 0);
    assert_eq!(FORCEWAKE_KERNEL & FORCEWAKE_KERNEL_FALLBACK, 0);
    assert_eq!(FORCEWAKE_USER & FORCEWAKE_KERNEL_FALLBACK, 0);
}

#[test]
fn chv_vlv_forcewake_registers_are_linux_literals() {
    assert_eq!(FORCEWAKE_VLV.offset(), 0x1300b0);
    assert_eq!(FORCEWAKE_ACK_VLV.offset(), 0x1300b4);
    assert_eq!(FORCEWAKE_MEDIA_VLV.offset(), 0x1300b8);
    assert_eq!(FORCEWAKE_ACK_MEDIA_VLV.offset(), 0x1300bc);
}

#[test]
fn fuse_and_topology_registers_are_linux_literals() {
    assert_eq!(GEN8_FUSE2.offset(), 0x9120);
    assert_eq!(GEN8_EU_DISABLE0.offset(), 0x9134);
    assert_eq!(GEN8_EU_DISABLE1.offset(), 0x9138);
    assert_eq!(GEN8_EU_DISABLE2.offset(), 0x913c);
    assert_eq!(gen9_eu_disable(0).offset(), 0x9134);
    assert_eq!(gen9_eu_disable(1).offset(), 0x9138);
    assert_eq!(gen9_eu_disable(2).offset(), 0x913c);
    assert_eq!(GEN8_F2_S_ENA_MASK, 0x0e00_0000);
    assert_eq!(GEN9_F2_SS_DIS_MASK, 0x00f0_0000);
    assert_eq!(GEN8_F2_SS_DIS_MASK, 0x00e0_0000);
}

#[test]
fn clock_and_pstate_registers_are_linux_literals() {
    assert_eq!(RPM_CONFIG0.offset(), 0xd00);
    assert_eq!(GEN9_RPM_CONFIG0_CRYSTAL_CLOCK_FREQ_MASK, 0x8);
    assert_eq!(GEN10_RPM_CONFIG0_CTC_SHIFT_PARAMETER_MASK, 0x6);
    assert_eq!(CTC_MODE.offset(), 0xa26c);
    assert_eq!(CTC_SHIFT_PARAMETER_MASK, 0x6);
    assert_eq!(CTC_SOURCE_PARAMETER_MASK, 0x1);
    assert_eq!(CTC_SOURCE_CRYSTAL_CLOCK, 0);
    assert_eq!(CTC_SOURCE_DIVIDE_LOGIC, 1);
    assert_ne!(CTC_SOURCE_CRYSTAL_CLOCK, CTC_SOURCE_DIVIDE_LOGIC, "zero-valued field value is pinned by being distinct from its sibling");
    assert_eq!(GEN9_TIMESTAMP_OVERRIDE.offset(), 0x44074);
    assert_eq!(GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DIVIDER_MASK, 0x3ff);
    assert_eq!(GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DENOMINATOR_SHIFT, 12);
    assert_eq!(GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DENOMINATOR_MASK, 0xf000);
    assert_eq!(MCHBAR_MIRROR_BASE_SNB, 0x140000);
    assert_eq!(GEN6_RP_STATE_CAP.offset(), 0x145998);
    assert_eq!(RP0_CAP_MASK, 0xff);
    assert_eq!(RP1_CAP_MASK, 0xff00);
    assert_eq!(RPN_CAP_MASK, 0xff_0000);
}

#[test]
fn aliases_are_the_only_duplicate_offsets() {
    assert_eq!(FORCEWAKE_MT.offset(), 0xa188);
    assert_eq!(FORCEWAKE_ACK_HSW.offset(), 0x130044);
    for (alias, home) in ALIASES {
        assert_eq!(alias, home);
    }
    assert_eq!(ALIASES.len(), 2);
    for (i, (na, a)) in CATALOG.iter().enumerate() {
        assert_eq!(a.offset() % 4, 0, "{na} misaligned");
        for (nb, b) in &CATALOG[i + 1..] {
            assert_ne!(na, nb, "duplicate name");
            assert_ne!(a, b, "{na} and {nb} share an offset without being a listed alias");
        }
    }
    assert_eq!(CATALOG.len(), 18);
}
