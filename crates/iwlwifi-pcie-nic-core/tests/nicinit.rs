// SPDX-License-Identifier: GPL-2.0-only
//! The radio/MAC configuration word and the shadow-register enable. Linux `mvm/ops.c:80-137`, `trans.c:513-517`.
mod common;
use common::{Op, Rec};
use iwlwifi_pcie_nic_core::nicinit::*;

#[test]
fn the_word_for_the_9260s_phy_sku_and_hw_rev() {
    // phy_config 0x00330018: radio type = 0x18 & 3 = 0, step = (0x18 >> 2) & 3 = 2, dash = (0x18 >> 4) & 3 = 1 (mvm/ops.c:87-92)
    // hw_rev 0x321: MAC step/dash = 0x321 & 0xF = 1 (iwl-csr.h:309)
    let (mask, val) = nic_config_word(0x321, 0x0033_0018);
    assert_eq!(val, 1 | (0 << 10) | (2 << 14) | (1 << 12)); // mvm/ops.c:101-106: STEP_DASH | type << 10 | step << 14 | dash << 12
    assert_eq!(val, 0x9001);
    // mask = MAC_STEP_DASH 0xF | PHY_TYPE 0xC00 | PHY_STEP 0xC000 | PHY_DASH 0x3000 | RADIO_SI 0x200 | MAC_SI 0x100 | D3_DEBUG 0x200 (ops.c:125-131)
    assert_eq!(mask, 0xF | 0xC00 | 0xC000 | 0x3000 | 0x200 | 0x100);
    assert_eq!(mask, 0xFF0F);
}

#[test]
fn each_radio_field_lands_in_its_own_bits() {
    let w = |phy: u32| nic_config_word(0, phy).1;
    assert_eq!(w(0x3), 3 << 10, "type");
    assert_eq!(w(0xC), 3 << 14, "step");
    assert_eq!(w(0x30), 3 << 12, "dash");
    assert_eq!(w(0x0030_0000), 0, "chains do not enter the word");
    assert_eq!(nic_config_word(0x2d, 0).1, 0xd, "only the low nibble of hw_rev");
}

#[test]
fn nic_config_is_a_read_modify_write_that_keeps_the_bits_outside_the_mask() {
    let mut io = Rec::default();
    io.csr.insert(0x000, 0xFFFF_0000 | 0x00F0 | 0xFF0F); // bits outside the mask: 0xFFFF_00F0; inside: all set
    let (mask, val) = nic_config(&mut io, 0x321, 0x0033_0018);
    assert_eq!(io.ops, [Op::CsrW(0x000, (0xFFFF_FFFF & !mask) | val)]);
    assert_eq!(io.csr[&0x000] & 0xFFFF_00F0, 0xFFFF_00F0);
}

#[test]
fn the_shadow_registers_are_enabled_with_the_linux_value() {
    let mut io = Rec::default();
    io.csr.insert(0x0a8, 0x1);
    enable_shadow_regs(&mut io);
    assert_eq!(io.ops, [Op::CsrW(0x0a8, 0x800F_FFFF | 1)]); // iwl_set_bit(CSR_MAC_SHADOW_REG_CTRL, 0x800FFFFF), trans.c:515
}
