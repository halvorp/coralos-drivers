// SPDX-License-Identifier: GPL-2.0-only
//! The register convention against Linux's own literals. Expected values are written out by hand.
use i915_common_core::reg::*;
use i915_common_core::{Reg, RegIo};

#[test]
fn reg_is_an_offset_and_nth_is_base_plus_stride() {
    assert_eq!(Reg::new(0xa188).offset(), 0xa188);
    // GEN9_EU_DISABLE(slice) = 0x9134 + slice * 4 (intel_gt_regs.h:609)
    assert_eq!(Reg::nth(0x9134, 0, 4).offset(), 0x9134);
    assert_eq!(Reg::nth(0x9134, 1, 4).offset(), 0x9138);
    assert_eq!(Reg::nth(0x9134, 2, 4).offset(), 0x913c);
}

#[test]
fn bit_and_genmask_are_linux_literals() {
    assert_eq!(bit(0), 0x1); // FORCEWAKE_KERNEL BIT(0) intel_gt_regs.h:1497
    assert_eq!(bit(15), 0x8000); // FORCEWAKE_KERNEL_FALLBACK BIT(15) intel_gt_regs.h:1499
    assert_eq!(bit(31), 0x8000_0000);
    assert_eq!(genmask(2, 1), 0x6); // CTC_SHIFT_PARAMETER_MASK intel_gt_regs.h:882
    assert_eq!(genmask(23, 20), 0x00f0_0000); // GEN9_F2_SS_DIS_MASK intel_gt_regs.h:605
    assert_eq!(genmask(27, 25), 0x0e00_0000); // GEN8_F2_S_ENA_MASK intel_gt_regs.h:604
    assert_eq!(genmask(31, 24), 0xff00_0000); // GEN8_EU_DIS0_S1_MASK intel_gt_regs.h:611
    assert_eq!(genmask(23, 0), 0x00ff_ffff); // GEN8_EU_DIS0_S0_MASK intel_gt_regs.h:612
    assert_eq!(genmask(31, 0), 0xffff_ffff);
    assert_eq!(genmask(0, 0), 1);
}

#[test]
fn field_get_and_prep_round_trip() {
    // GEN9_F2_SS_DIS_MASK is bits 23:20
    assert_eq!(field_get(genmask(23, 20), 0x00a0_0000), 0xa);
    assert_eq!(field_get(genmask(23, 20), 0xff0f_ffff), 0);
    assert_eq!(field_prep(genmask(23, 20), 0x3), 0x0030_0000);
    assert_eq!(field_prep(genmask(23, 20), 0x1f), 0x00f0_0000); // clipped to the field, like REG_FIELD_PREP's mask
    assert_eq!(field_get(genmask(2, 1), 0x6), 3);
    assert_eq!(field_get(bit(3), 0x8), 1);
}

#[test]
fn masked_encoding_is_linux_literal() {
    // _MASKED_BIT_ENABLE(FORCEWAKE_KERNEL) / _DISABLE: the words intel_uncore.c:135-136 writes to the set register
    assert_eq!(masked_bit_enable(0x1), 0x0001_0001);
    assert_eq!(masked_bit_disable(0x1), 0x0001_0000);
    assert_eq!(masked_bit_enable(0x8000), 0x8000_8000); // FORCEWAKE_KERNEL_FALLBACK
    assert_eq!(masked_bit_disable(0x8000), 0x8000_0000);
    assert_eq!(masked_bit_disable(0xffff), 0xffff_0000); // WaRsClearFWBitsAtReset, intel_uncore.c:150
    assert_eq!(masked_field(0xf0, 0x30), 0x00f0_0030);
    assert_eq!(try_masked_field(0xf0, 0x30), Some(0x00f0_0030));
}

#[test]
fn masked_encoding_refuses_what_linux_build_bugs_on() {
    assert_eq!(try_masked_field(0x1_0000, 0), None, "Incorrect mask");
    assert_eq!(try_masked_field(0xff, 0x1_0000), None, "Incorrect value");
    assert_eq!(try_masked_field(0x0f, 0x10), None, "Incorrect value for mask");
}

struct One(u32, u32);
impl coralos_hal::Mmio for One {
    fn r8(&mut self, _: u32) -> u8 { 0 }
    fn r16(&mut self, _: u32) -> u16 { 0 }
    fn r32(&mut self, r: u32) -> u32 { if r == self.0 { self.1 } else { 0 } }
    fn w8(&mut self, _: u32, _: u8) {}
    fn w16(&mut self, _: u32, _: u16) {}
    fn w32(&mut self, r: u32, v: u32) { self.0 = r; self.1 = v; }
}

#[test]
fn regio_goes_through_the_hal_mmio_trait() {
    let mut m = One(0, 0);
    m.wr(Reg::new(0xa188), 0x0001_0001);
    assert_eq!((m.0, m.1), (0xa188, 0x0001_0001));
    assert_eq!(m.rd(Reg::new(0xa188)), 0x0001_0001);
    assert_eq!(m.rd(Reg::new(0xd84)), 0);
}
