// SPDX-License-Identifier: GPL-2.0-only
//! The radio/MAC configuration word and the shadow registers that `iwl_pcie_nic_init` sets up around
//! the RX and TX rings. Ported from Linux `mvm/ops.c:80-137` (`iwl_mvm_nic_config`),
//! `pcie/gen1_2/trans.c:485-520` (`iwl_pcie_nic_init`), `pcie/gen1_2/trans.c:2622-2630` +
//! `_iwl_trans_set_bits_mask`, `cfg/9000.c:37` (`shadow_reg_enable`), `iwl-csr.h:162-177`.
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH.
//!
//! Family branches taken: < AX210 (the SKU word is written, mvm/ops.c:97-98), >= 8000 (no RADIO_SI,
//! mvm/ops.c:119-120), no D3 debug, `apmg_not_supported` (the APMG early-power-off write at
//! mvm/ops.c:131-136 is skipped, and `iwl_pcie_set_pwr` is a no-op, trans.c:187-188).

use crate::bus::NicIo;
use crate::regs::*;
use iwlwifi_pcie_trans_core::io::set_bit;
use iwlwifi_pcie_trans_core::regs::CSR_HW_IF_CONFIG_REG;

pub const CSR_HW_IF_CONFIG_REG_BIT_MAC_SI: u32 = 0x0000_0100; // iwl-csr.h:165
pub const CSR_HW_IF_CONFIG_REG_BIT_RADIO_SI: u32 = 0x0000_0200; // iwl-csr.h:166
pub const CSR_HW_IF_CONFIG_REG_D3_DEBUG: u32 = 0x0000_0200; // iwl-csr.h:167
/// `FW_PHY_CFG_RADIO_TYPE` and friends, as `iwl_mvm_nic_config` reads them. // fw/file.h:604-608
pub const FW_PHY_CFG_RADIO_TYPE: u32 = 0x3; // fw/file.h:604
pub const FW_PHY_CFG_RADIO_STEP: u32 = 0x3 << 2; // fw/file.h:606
pub const FW_PHY_CFG_RADIO_DASH: u32 = 0x3 << 4; // fw/file.h:608
pub const FW_PHY_CFG_RADIO_TYPE_POS: u32 = 0; // fw/file.h:603
pub const FW_PHY_CFG_RADIO_STEP_POS: u32 = 2; // fw/file.h:605
pub const FW_PHY_CFG_RADIO_DASH_POS: u32 = 4; // fw/file.h:607
/// `CSR_MAC_SHADOW_REG_CTRL ← 0x800FFFFF`, `shadow_reg_enable`. // trans.c:514-517
pub const SHADOW_REG_ENABLE_ALL: u32 = 0x800F_FFFF; // trans.c:515

/// The (`mask`, `value`) `iwl_mvm_nic_config` passes to `iwl_trans_set_bits_mask` for a 9000-family
/// part: the MAC step/dash from CSR_HW_REV, the radio type/step/dash from PHY_SKU. // mvm/ops.c:100-130
pub fn nic_config_word(hw_rev: u32, phy_config: u32) -> (u32, u32) {
    let ty = (phy_config & FW_PHY_CFG_RADIO_TYPE) >> FW_PHY_CFG_RADIO_TYPE_POS; // ops.c:87-88
    let step = (phy_config & FW_PHY_CFG_RADIO_STEP) >> FW_PHY_CFG_RADIO_STEP_POS; // ops.c:89-90
    let dash = (phy_config & FW_PHY_CFG_RADIO_DASH) >> FW_PHY_CFG_RADIO_DASH_POS; // ops.c:91-92
    let mut val = hw_rev & CSR_HW_IF_CONFIG_REG_MSK_MAC_STEP_DASH; // ops.c:101 CSR_HW_REV_STEP_DASH
    val |= ty << CSR_HW_IF_CONFIG_REG_POS_PHY_TYPE; // ops.c:104
    val |= step << CSR_HW_IF_CONFIG_REG_POS_PHY_STEP; // ops.c:105
    val |= dash << CSR_HW_IF_CONFIG_REG_POS_PHY_DASH; // ops.c:106
    let mask = CSR_HW_IF_CONFIG_REG_MSK_MAC_STEP_DASH
        | CSR_HW_IF_CONFIG_REG_MSK_PHY_TYPE
        | CSR_HW_IF_CONFIG_REG_MSK_PHY_STEP
        | CSR_HW_IF_CONFIG_REG_MSK_PHY_DASH
        | CSR_HW_IF_CONFIG_REG_BIT_RADIO_SI
        | CSR_HW_IF_CONFIG_REG_BIT_MAC_SI
        | CSR_HW_IF_CONFIG_REG_D3_DEBUG; // ops.c:125-131
    (mask, val)
}

/// `iwl_mvm_nic_config`'s register write. // mvm/ops.c:125-132; _iwl_trans_set_bits_mask
pub fn nic_config<I: NicIo>(io: &mut I, hw_rev: u32, phy_config: u32) -> (u32, u32) {
    let (mask, value) = nic_config_word(hw_rev, phy_config);
    let cur = io.csr_read(CSR_HW_IF_CONFIG_REG);
    io.csr_write(CSR_HW_IF_CONFIG_REG, (cur & !mask) | value);
    (mask, value)
}

/// The shadow-register enable of `iwl_pcie_nic_init`. // trans.c:513-517
pub fn enable_shadow_regs<I: NicIo>(io: &mut I) {
    set_bit(io, CSR_MAC_SHADOW_REG_CTRL, SHADOW_REG_ENABLE_ALL);
}
