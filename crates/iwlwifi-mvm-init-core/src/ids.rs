// SPDX-License-Identifier: GPL-2.0-only
//! Command ids, constants and sizes the INIT flow uses. Every value carries the Linux FILE:LINE it
//! was read from (Linux v7.0-rc6, `drivers/net/wireless/intel/iwlwifi/`).

/// `UCODE_ALIVE_NTFY` — the ALIVE notification (LEGACY_GROUP).
pub const UCODE_ALIVE_NTFY: u8 = 0x1; // fw/api/commands.h:65
/// `INIT_COMPLETE_NOTIF`.
pub const INIT_COMPLETE_NOTIF: u8 = 0x4; // fw/api/commands.h:80
/// `PHY_CONFIGURATION_CMD`.
pub const PHY_CONFIGURATION_CMD: u8 = 0x6a; // fw/api/commands.h:314
/// `CALIB_RES_NOTIF_PHY_DB`.
pub const CALIB_RES_NOTIF_PHY_DB: u8 = 0x6b; // fw/api/commands.h:319
/// `NVM_ACCESS_CMD`.
pub const NVM_ACCESS_CMD: u8 = 0x88; // fw/api/commands.h:351
/// `TX_ANT_CONFIGURATION_CMD`.
pub const TX_ANT_CONFIGURATION_CMD: u8 = 0x98; // fw/api/commands.h:368

/// `MVM_UCODE_ALIVE_TIMEOUT` is `2 * HZ`: 2000 ms.
pub const MVM_UCODE_ALIVE_TIMEOUT_MS: u32 = 2000; // mvm/fw.c:28
/// `MVM_UCODE_CALIB_TIMEOUT` is `2 * HZ`: 2000 ms.
pub const MVM_UCODE_CALIB_TIMEOUT_MS: u32 = 2000; // mvm/fw.c:29

/// `IWL_NVM_DEFAULT_CHUNK_SIZE` is `2 * 1024`.
pub const IWL_NVM_DEFAULT_CHUNK_SIZE: u16 = 2048; // mvm/nvm.c:18
/// `NVM_WRITE_OPCODE`.
pub const NVM_WRITE_OPCODE: u8 = 1; // mvm/nvm.c:20
/// `NVM_READ_OPCODE`.
pub const NVM_READ_OPCODE: u8 = 0; // mvm/nvm.c:21
/// `READ_NVM_CHUNK_SUCCEED`.
pub const READ_NVM_CHUNK_SUCCEED: u16 = 0; // mvm/nvm.c:25
/// `READ_NVM_CHUNK_NOT_VALID_ADDRESS`.
pub const READ_NVM_CHUNK_NOT_VALID_ADDRESS: u16 = 1; // mvm/nvm.c:26

/// `NVM_ACCESS_TARGET_CACHE` — the target Linux's designated initialiser leaves at 0 (mvm/nvm.c:75-80 sets no .target).
pub const NVM_ACCESS_TARGET_CACHE: u8 = 0; // fw/api/nvm-reg.h:72
/// `NVM_ACCESS_TARGET_OTP`.
pub const NVM_ACCESS_TARGET_OTP: u8 = 1; // fw/api/nvm-reg.h:73
/// `NVM_ACCESS_TARGET_EEPROM`.
pub const NVM_ACCESS_TARGET_EEPROM: u8 = 2; // fw/api/nvm-reg.h:74

/// `NVM_SECTION_TYPE_SW`.
pub const NVM_SECTION_TYPE_SW: u16 = 1; // fw/api/nvm-reg.h:89
/// `NVM_SECTION_TYPE_REGULATORY`.
pub const NVM_SECTION_TYPE_REGULATORY: u16 = 3; // fw/api/nvm-reg.h:90
/// `NVM_SECTION_TYPE_CALIBRATION`.
pub const NVM_SECTION_TYPE_CALIBRATION: u16 = 4; // fw/api/nvm-reg.h:91
/// `NVM_SECTION_TYPE_PRODUCTION`.
pub const NVM_SECTION_TYPE_PRODUCTION: u16 = 5; // fw/api/nvm-reg.h:92
/// `NVM_SECTION_TYPE_REGULATORY_SDP`.
pub const NVM_SECTION_TYPE_REGULATORY_SDP: u16 = 8; // fw/api/nvm-reg.h:93
/// `NVM_SECTION_TYPE_MAC_OVERRIDE`.
pub const NVM_SECTION_TYPE_MAC_OVERRIDE: u16 = 11; // fw/api/nvm-reg.h:94
/// `NVM_SECTION_TYPE_PHY_SKU`.
pub const NVM_SECTION_TYPE_PHY_SKU: u16 = 12; // fw/api/nvm-reg.h:95
/// `NVM_MAX_NUM_SECTIONS` — `iwl_nvm_init` walks sections `0..13` (nvm.c:323).
pub const NVM_MAX_NUM_SECTIONS: u16 = 13; // fw/api/nvm-reg.h:96

/// `FW_PHY_CFG_TX_CHAIN_POS`.
pub const FW_PHY_CFG_TX_CHAIN_POS: u32 = 16; // fw/file.h:609
/// `FW_PHY_CFG_TX_CHAIN` (`0xf << FW_PHY_CFG_TX_CHAIN_POS`).
pub const FW_PHY_CFG_TX_CHAIN: u32 = 0xf << FW_PHY_CFG_TX_CHAIN_POS; // fw/file.h:610
/// `FW_PHY_CFG_RX_CHAIN_POS`.
pub const FW_PHY_CFG_RX_CHAIN_POS: u32 = 20; // fw/file.h:611
/// `FW_PHY_CFG_RX_CHAIN` (`0xf << FW_PHY_CFG_RX_CHAIN_POS`).
pub const FW_PHY_CFG_RX_CHAIN: u32 = 0xf << FW_PHY_CFG_RX_CHAIN_POS; // fw/file.h:612

/// `IWL_PHY_DB_CFG`.
pub const IWL_PHY_DB_CFG: u16 = 1; // iwl-phy-db.c:44
/// `IWL_PHY_DB_CALIB_NCH`.
pub const IWL_PHY_DB_CALIB_NCH: u16 = 2; // iwl-phy-db.c:45
/// `IWL_PHY_DB_UNUSED` — `iwl_phy_db_get_section` returns NULL for it (iwl-phy-db.c:102).
pub const IWL_PHY_DB_UNUSED: u16 = 3; // iwl-phy-db.c:46
/// `IWL_PHY_DB_CALIB_CHG_PAPD`.
pub const IWL_PHY_DB_CALIB_CHG_PAPD: u16 = 4; // iwl-phy-db.c:47
/// `IWL_PHY_DB_CALIB_CHG_TXP`.
pub const IWL_PHY_DB_CALIB_CHG_TXP: u16 = 5; // iwl-phy-db.c:48
/// `IWL_PHY_DB_MAX` — types `>=` this are refused (iwl-phy-db.c:86).
pub const IWL_PHY_DB_MAX: u16 = 6; // iwl-phy-db.c:49

/// `sizeof(struct iwl_tx_ant_cfg_cmd)`: one `__le32` (fw/api/config.h:22-24).
pub const TX_ANT_CFG_CMD_LEN: usize = 4;
/// `sizeof(struct iwl_phy_cfg_cmd_v1)`: `phy_cfg` + `struct iwl_calib_ctrl` (two `__le32`)
/// (fw/api/config.h:34-37, :84-87).
pub const PHY_CFG_CMD_V1_LEN: usize = 12;
/// `sizeof(struct iwl_nvm_access_cmd)` without `data[]`: `u8,u8,le16,le16,le16`
/// (fw/api/nvm-reg.h:108-115).
pub const NVM_ACCESS_CMD_LEN: usize = 8;
/// `sizeof(struct iwl_nvm_access_resp)` without `data[]`: four `__le16` (fw/api/nvm-reg.h:125-130).
pub const NVM_ACCESS_RESP_HDR_LEN: usize = 8;
/// `sizeof(struct iwl_calib_res_notif_phy_db)` without `data[]`: two `__le16`
/// (fw/api/cmdhdr.h:130-134).
pub const CALIB_RES_PHY_DB_HDR_LEN: usize = 4;
