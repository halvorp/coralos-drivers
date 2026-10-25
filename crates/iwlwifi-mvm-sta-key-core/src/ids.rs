// SPDX-License-Identifier: GPL-2.0-only
//! Command id, key flags and sizes, each with the Linux FILE:LINE it was read from.

/// `ADD_STA_KEY` (LEGACY_GROUP).
pub const ADD_STA_KEY: u8 = 0x17; // fw/api/commands.h:134

/// `STA_KEY_FLG_CCM`: CCMP encryption algorithm.
pub const STA_KEY_FLG_CCM: u16 = 2 << 0; // fw/api/sta.h:124
/// `STA_KEY_FLG_EN_MSK`: mask for the encryption algorithm value.
pub const STA_KEY_FLG_EN_MSK: u16 = 7 << 0; // fw/api/sta.h:130
/// `STA_KEY_FLG_WEP_KEY_MAP`: set on every key `iwl_mvm_send_sta_key` sends (sta.c:3550).
pub const STA_KEY_FLG_WEP_KEY_MAP: u16 = 1 << 3; // fw/api/sta.h:132
/// `STA_KEY_FLG_KEYID_POS`.
pub const STA_KEY_FLG_KEYID_POS: u16 = 8; // fw/api/sta.h:134
/// `STA_KEY_FLG_KEYID_MSK`.
pub const STA_KEY_FLG_KEYID_MSK: u16 = 3 << STA_KEY_FLG_KEYID_POS; // fw/api/sta.h:135
/// `STA_KEY_NOT_VALID`: the key is invalid (used to remove one).
pub const STA_KEY_NOT_VALID: u16 = 1 << 11; // fw/api/sta.h:136
/// `STA_KEY_MULTICAST`: set for a group key.
pub const STA_KEY_MULTICAST: u16 = 1 << 14; // fw/api/sta.h:139

/// `STA_KEY_MAX_NUM`: firmware key slots.
pub const STA_KEY_MAX_NUM: usize = 16; // fw/api/sta.h:190
/// `STA_KEY_IDX_INVALID`.
pub const STA_KEY_IDX_INVALID: u8 = 0xff; // fw/api/sta.h:191

/// `ADD_STA_SUCCESS`: the only status `iwl_mvm_send_sta_key` accepts.
pub const ADD_STA_SUCCESS: u32 = 0x1; // fw/api/sta.h:413

/// `sizeof(struct iwl_mvm_add_sta_key_common)`: u8, u8, le16, key[32], rx_secur_seq_cnt[16].
pub const COMMON_LEN: usize = 52; // fw/api/sta.h:364
/// `sizeof(struct iwl_mvm_add_sta_key_cmd_v1)`: common + u8 + u8 + le16[5].
pub const CMD_V1_LEN: usize = 64; // fw/api/sta.h:379
/// `sizeof(struct iwl_mvm_add_sta_key_cmd)` (versions 2 and 3): common + 3 x le64.
pub const CMD_V2_LEN: usize = 76; // fw/api/sta.h:397
