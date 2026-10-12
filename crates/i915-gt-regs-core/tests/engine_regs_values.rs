// SPDX-License-Identifier: GPL-2.0-only
//! engine_regs: every constant against the value GCC computes from the Linux header itself (macro-expanded with
//! REG_BIT / REG_GENMASK / _MMIO stubs), written out literally. Generated once from the real header; the expected side is
//! Linux's number, the actual side travels through the crate's constant.
use i915_gt_regs_core::*;

#[test]
fn engine_regs_constants_equal_the_values_gcc_computes_from_the_linux_header() {
    assert_eq!(RESET_CTL_CAT_ERROR, 0x00000004, "RESET_CTL_CAT_ERROR");
    assert_eq!(RESET_CTL_READY_TO_RESET, 0x00000002, "RESET_CTL_READY_TO_RESET");
    assert_eq!(RESET_CTL_REQUEST_RESET, 0x00000001, "RESET_CTL_REQUEST_RESET");
    assert_eq!(RING_FORCE_TO_NONPRIV_DENY, 0x40000000, "RING_FORCE_TO_NONPRIV_DENY");
    assert_eq!(RING_FORCE_TO_NONPRIV_ADDRESS_MASK, 0x03fffffc, "RING_FORCE_TO_NONPRIV_ADDRESS_MASK");
    assert_eq!(RING_FORCE_TO_NONPRIV_ACCESS_RW, 0x00000000, "RING_FORCE_TO_NONPRIV_ACCESS_RW");
    assert_eq!(RING_FORCE_TO_NONPRIV_ACCESS_RD, 0x10000000, "RING_FORCE_TO_NONPRIV_ACCESS_RD");
    assert_eq!(RING_FORCE_TO_NONPRIV_ACCESS_WR, 0x20000000, "RING_FORCE_TO_NONPRIV_ACCESS_WR");
    assert_eq!(RING_FORCE_TO_NONPRIV_ACCESS_INVALID, 0x30000000, "RING_FORCE_TO_NONPRIV_ACCESS_INVALID");
    assert_eq!(RING_FORCE_TO_NONPRIV_ACCESS_MASK, 0x30000000, "RING_FORCE_TO_NONPRIV_ACCESS_MASK");
    assert_eq!(RING_FORCE_TO_NONPRIV_RANGE_1, 0x00000000, "RING_FORCE_TO_NONPRIV_RANGE_1");
    assert_eq!(RING_FORCE_TO_NONPRIV_RANGE_4, 0x00000001, "RING_FORCE_TO_NONPRIV_RANGE_4");
    assert_eq!(RING_FORCE_TO_NONPRIV_RANGE_16, 0x00000002, "RING_FORCE_TO_NONPRIV_RANGE_16");
    assert_eq!(RING_FORCE_TO_NONPRIV_RANGE_64, 0x00000003, "RING_FORCE_TO_NONPRIV_RANGE_64");
    assert_eq!(RING_FORCE_TO_NONPRIV_RANGE_MASK, 0x00000003, "RING_FORCE_TO_NONPRIV_RANGE_MASK");
    assert_eq!(RING_MAX_NONPRIV_SLOTS, 0x0000000c, "RING_MAX_NONPRIV_SLOTS");
}
