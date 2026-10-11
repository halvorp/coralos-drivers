// SPDX-License-Identifier: GPL-2.0-only
//! Every constant pinned by VALUE against the Linux literal (expected side written by hand).
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/` v7.0-rc6; Copyright Intel Corporation.

use iwlwifi_mvm_init_core::ids::*;

#[test]
fn command_ids_are_commands_h() {
    assert_eq!(UCODE_ALIVE_NTFY, 0x1); // fw/api/commands.h:65
    assert_eq!(INIT_COMPLETE_NOTIF, 0x4); // fw/api/commands.h:80
    assert_eq!(PHY_CONFIGURATION_CMD, 0x6a); // fw/api/commands.h:314
    assert_eq!(CALIB_RES_NOTIF_PHY_DB, 0x6b); // fw/api/commands.h:319
    assert_eq!(NVM_ACCESS_CMD, 0x88); // fw/api/commands.h:351
    assert_eq!(TX_ANT_CONFIGURATION_CMD, 0x98); // fw/api/commands.h:368
    // The previous attempt's invented values must not be these.
    assert_ne!(PHY_CONFIGURATION_CMD, 0x6c);
    assert_ne!(NVM_ACCESS_CMD, 0x45);
    assert_ne!(TX_ANT_CONFIGURATION_CMD, 0x8);
}

#[test]
fn timeouts_are_two_seconds() {
    assert_eq!(MVM_UCODE_ALIVE_TIMEOUT_MS, 2000); // mvm/fw.c:28 (2 * HZ)
    assert_eq!(MVM_UCODE_CALIB_TIMEOUT_MS, 2000); // mvm/fw.c:29 (2 * HZ)
}

#[test]
fn nvm_access_constants_are_nvm_c_and_nvm_reg_h() {
    assert_eq!(IWL_NVM_DEFAULT_CHUNK_SIZE, 2 * 1024); // mvm/nvm.c:18
    assert_eq!(NVM_WRITE_OPCODE, 1); // mvm/nvm.c:20
    assert_eq!(NVM_READ_OPCODE, 0); // mvm/nvm.c:21
    assert_eq!(READ_NVM_CHUNK_SUCCEED, 0); // mvm/nvm.c:25
    assert_eq!(READ_NVM_CHUNK_NOT_VALID_ADDRESS, 1); // mvm/nvm.c:26
    assert_ne!(NVM_READ_OPCODE, NVM_WRITE_OPCODE); // zero-valued: pinned by distinctness too
    assert_ne!(READ_NVM_CHUNK_SUCCEED, READ_NVM_CHUNK_NOT_VALID_ADDRESS);
    assert_eq!(NVM_ACCESS_TARGET_CACHE, 0); // fw/api/nvm-reg.h:72
    assert_eq!(NVM_ACCESS_TARGET_OTP, 1); // fw/api/nvm-reg.h:73
    assert_eq!(NVM_ACCESS_TARGET_EEPROM, 2); // fw/api/nvm-reg.h:74
}

#[test]
fn nvm_section_types_are_nvm_reg_h() {
    assert_eq!(NVM_SECTION_TYPE_SW, 1); // fw/api/nvm-reg.h:89
    assert_eq!(NVM_SECTION_TYPE_REGULATORY, 3); // fw/api/nvm-reg.h:90
    assert_eq!(NVM_SECTION_TYPE_CALIBRATION, 4); // fw/api/nvm-reg.h:91
    assert_eq!(NVM_SECTION_TYPE_PRODUCTION, 5); // fw/api/nvm-reg.h:92
    assert_eq!(NVM_SECTION_TYPE_REGULATORY_SDP, 8); // fw/api/nvm-reg.h:93
    assert_eq!(NVM_SECTION_TYPE_MAC_OVERRIDE, 11); // fw/api/nvm-reg.h:94
    assert_eq!(NVM_SECTION_TYPE_PHY_SKU, 12); // fw/api/nvm-reg.h:95
    assert_eq!(NVM_MAX_NUM_SECTIONS, 13); // fw/api/nvm-reg.h:96
}

#[test]
fn fw_phy_cfg_chain_fields_are_file_h() {
    assert_eq!(FW_PHY_CFG_TX_CHAIN_POS, 16); // fw/file.h:609
    assert_eq!(FW_PHY_CFG_TX_CHAIN, 0x000f_0000); // fw/file.h:610
    assert_eq!(FW_PHY_CFG_RX_CHAIN_POS, 20); // fw/file.h:611
    assert_eq!(FW_PHY_CFG_RX_CHAIN, 0x00f0_0000); // fw/file.h:612
    // composite: the OR of the named components, and each component by value above
    assert_eq!(FW_PHY_CFG_TX_CHAIN, 0xf << FW_PHY_CFG_TX_CHAIN_POS);
    assert_eq!(FW_PHY_CFG_RX_CHAIN, 0xf << FW_PHY_CFG_RX_CHAIN_POS);
}

#[test]
fn phy_db_section_types_are_iwl_phy_db_c() {
    assert_eq!(IWL_PHY_DB_CFG, 1); // iwl-phy-db.c:44
    assert_eq!(IWL_PHY_DB_CALIB_NCH, 2); // iwl-phy-db.c:45
    assert_eq!(IWL_PHY_DB_UNUSED, 3); // iwl-phy-db.c:46
    assert_eq!(IWL_PHY_DB_CALIB_CHG_PAPD, 4); // iwl-phy-db.c:47
    assert_eq!(IWL_PHY_DB_CALIB_CHG_TXP, 5); // iwl-phy-db.c:48
    assert_eq!(IWL_PHY_DB_MAX, 6); // iwl-phy-db.c:49
}

#[test]
fn packed_struct_sizes() {
    assert_eq!(TX_ANT_CFG_CMD_LEN, 4); // fw/api/config.h:22-24: one __le32
    assert_eq!(PHY_CFG_CMD_V1_LEN, 12); // fw/api/config.h:84-87: 3 x __le32
    assert_eq!(NVM_ACCESS_CMD_LEN, 8); // fw/api/nvm-reg.h:108-115: u8,u8,le16 x3
    assert_eq!(NVM_ACCESS_RESP_HDR_LEN, 8); // fw/api/nvm-reg.h:125-130: le16 x4
    assert_eq!(CALIB_RES_PHY_DB_HDR_LEN, 4); // fw/api/cmdhdr.h:130-134: le16 x2
}
