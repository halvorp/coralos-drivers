// SPDX-License-Identifier: GPL-2.0-only
//! The DDI DP-transport / buffer-translation / balance-leg / raw-clock registers re-homed from i915-display-link-core,
//! pinned to the numbers GCC computes from intel_display_regs.h (macros expanded with REG_BIT/REG_GENMASK/_PICK_EVEN stubs
//! and written out literally here). Expected side = Linux; actual side = the crate's constant or function.
use i915_kbl_display_core::regs::*;
use i915_kbl_display_core::types::Ddi;

#[test]
fn dp_tp_registers_and_fields() {
    assert_eq!(DP_TP_CTL_A, 0x64040);
    assert_eq!(DP_TP_CTL_B, 0x64140);
    assert_eq!(DP_TP_STATUS_A, 0x64044);
    assert_eq!(DP_TP_STATUS_B, 0x64144);
    assert_eq!(dp_tp_ctl(Ddi::A), 0x64040);
    assert_eq!(dp_tp_ctl(Ddi::B), 0x64140);
    assert_eq!(dp_tp_status(Ddi::A), 0x64044);
    assert_eq!(dp_tp_status(Ddi::B), 0x64144);
    assert_eq!(DP_TP_CTL_ENABLE, 0x8000_0000);
    assert_eq!(DP_TP_CTL_MODE_MASK, 0x0800_0000);
    assert_eq!(DP_TP_CTL_MODE_MST, 0x0800_0000);
    assert_eq!(DP_TP_CTL_ENHANCED_FRAME_ENABLE, 0x0004_0000);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_MASK, 0x700);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_PAT2, 0x100);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_PAT3, 0x400);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_PAT4, 0x500);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_IDLE, 0x200);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_NORMAL, 0x300);
    assert_eq!(DP_TP_STATUS_IDLE_DONE, 0x0200_0000);
    // zero-valued members (PORT_CONVENTIONS 3a.2): pinned by behaviour — distinct from every sibling and selecting the SST mode.
    assert_eq!(DP_TP_CTL_MODE_SST & DP_TP_CTL_MODE_MASK, 0);
    assert_ne!(DP_TP_CTL_MODE_SST, DP_TP_CTL_MODE_MST);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_PAT1 & DP_TP_CTL_LINK_TRAIN_MASK, 0);
    for other in [
        DP_TP_CTL_LINK_TRAIN_PAT2, DP_TP_CTL_LINK_TRAIN_PAT3, DP_TP_CTL_LINK_TRAIN_PAT4, DP_TP_CTL_LINK_TRAIN_IDLE,
        DP_TP_CTL_LINK_TRAIN_NORMAL,
    ] {
        assert_ne!(DP_TP_CTL_LINK_TRAIN_PAT1, other);
    }
}

#[test]
fn buf_trans_balance_leg_and_rawclk() {
    assert_eq!(DDI_BUF_TRANS_A, 0x64e00);
    assert_eq!(DDI_BUF_TRANS_B, 0x64e60);
    assert_eq!(ddi_buf_trans_lo(Ddi::A, 0), 0x64e00);
    assert_eq!(ddi_buf_trans_lo(Ddi::A, 9), 0x64e48);
    assert_eq!(ddi_buf_trans_lo(Ddi::B, 0), 0x64e60);
    assert_eq!(ddi_buf_trans_lo(Ddi::B, 9), 0x64ea8);
    assert_eq!(ddi_buf_trans_hi(Ddi::A, 0), 0x64e04);
    assert_eq!(ddi_buf_trans_hi(Ddi::A, 9), 0x64e4c);
    assert_eq!(ddi_buf_trans_hi(Ddi::B, 5), 0x64e8c);
    assert_eq!(DDI_BUF_BALANCE_LEG_ENABLE, 0x8000_0000);
    assert_eq!(DISPIO_CR_TX_BMU_CR0, 0x6c00c);
    assert_eq!((balance_leg_shift(Ddi::A), balance_leg_shift(Ddi::B), balance_leg_shift(Ddi::E)), (8, 11, 20));
    assert_eq!((balance_leg_mask(Ddi::A), balance_leg_mask(Ddi::B), balance_leg_mask(Ddi::E)), (0x700, 0x3800, 0x70_0000));
    assert_eq!((balance_leg_disable(Ddi::A), balance_leg_disable(Ddi::B), balance_leg_disable(Ddi::E)), (0x80_0000, 0x100_0000, 0x800_0000));
    assert_eq!((ddi_buf_trans_select(0), ddi_buf_trans_select(9), ddi_buf_trans_select(15)), (0, 0x0900_0000, 0x0f00_0000));
    assert_eq!((ddi_port_width(1), ddi_port_width(2), ddi_port_width(3), ddi_port_width(4)), (0, 2, 8, 6));
    assert_eq!(PCH_RAWCLK_FREQ, 0xc6204);
    assert_eq!(RAWCLK_FREQ_MASK, 0x3ff);
}
