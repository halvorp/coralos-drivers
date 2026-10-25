// SPDX-License-Identifier: GPL-2.0-only
//! The NVM (OTP) data of an extended-NVM part (IWL_NVM_EXT: the Wireless-AC 9260) decoded: version,
//! radio configuration and antenna masks, SKU capabilities, the hardware address and the channel list.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/iwl-nvm-parse.c` (`iwl_parse_nvm_data`,
//! `iwl_get_nvm_version`, `iwl_get_radio_cfg`, `iwl_set_radio_cfg`, `iwl_get_sku`, `iwl_get_n_hw_addrs`,
//! `iwl_flip_hw_address`, `iwl_set_hw_address_from_csr`, `iwl_init_channel_map`),
//! `iwl-nvm-parse.h` (`enum iwl_nvm_channel_flags`), `iwl-drv.h` (`EXT_NVM_RF_CFG_*`), `iwl-csr.h`
//! (`CSR_MAC_ADDR*`), `cfg/9000.c` (`mac_addr_from_csr`). Linux v7.0-rc6.
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! Left out, by name: the non-EXT NVM layouts (a 9260 reads an extended NVM), the SDP regulatory
//! section, the 6 GHz band (`uhb_supported` is not set for this part), HT/VHT/HE capability tables
//! (`iwl_init_ht_hw_capab` and friends: they describe what the host's MLME may negotiate; the
//! supplicant is the host's and uses none of them yet), `iwl_nvm_fixups`, the CSME-owned address arm.
//! The channel list is returned as the NVM states it; turning a channel's flags into cfg80211 regulatory
//! flags (`iwl_get_channel_flags`) is not done — with LAR the firmware owns the regulatory state
//! (`channel->flags = 0`, nvm-parse.c:422-423) and the host asks it with MCC_UPDATE.

#![no_std]
#![forbid(unsafe_code)]

/// `mac_addr_from_csr` of the 9000 family: where the address straps live in the CSR block.
/// // cfg/9000.c:43
pub const MAC_ADDR_FROM_CSR: u32 = 0x380; // cfg/9000.c:43
pub const CSR_MAC_ADDR0_OTP: u32 = MAC_ADDR_FROM_CSR; // iwl-csr.h:655
pub const CSR_MAC_ADDR1_OTP: u32 = MAC_ADDR_FROM_CSR + 0x04; // iwl-csr.h:656
pub const CSR_MAC_ADDR0_STRAP: u32 = MAC_ADDR_FROM_CSR + 0x08; // iwl-csr.h:657
pub const CSR_MAC_ADDR1_STRAP: u32 = MAC_ADDR_FROM_CSR + 0x0c; // iwl-csr.h:658

// ── NVM offsets, in 16-bit words ── iwl-nvm-parse.c
pub const NVM_VERSION_EXT_NVM: usize = 0; // iwl-nvm-parse.c:52
pub const N_HW_ADDRS_FAMILY_8000: usize = 3; // iwl-nvm-parse.c:53
pub const RADIO_CFG_FAMILY_EXT_NVM: usize = 0; // iwl-nvm-parse.c:56
pub const SKU_FAMILY_8000: usize = 2; // iwl-nvm-parse.c:57
pub const NVM_CHANNELS_EXTENDED: usize = 0; // iwl-nvm-parse.c:60
pub const NVM_LAR_OFFSET_OLD: usize = 0x4C7; // iwl-nvm-parse.c:61
pub const NVM_LAR_OFFSET: usize = 0x507; // iwl-nvm-parse.c:62
pub const NVM_LAR_ENABLED: u16 = 0x7; // iwl-nvm-parse.c:63
/// An NVM older than this keeps its LAR word at the OLD offset. // iwl-nvm-parse.c:1489
pub const NVM_LAR_NEW_VERSION: u32 = 0xE39; // iwl-nvm-parse.c:1489
pub const MAC_ADDRESS_OVERRIDE_EXT_NVM: usize = 1; // iwl-nvm-parse.c:49
pub const N_HW_ADDR_MASK: u32 = 0xF; // iwl-nvm-parse.c:117

// ── SKU bits ── iwl-nvm-parse.c:68-72
pub const NVM_SKU_CAP_BAND_24GHZ: u32 = 1 << 0; // iwl-nvm-parse.c:68
pub const NVM_SKU_CAP_BAND_52GHZ: u32 = 1 << 1; // iwl-nvm-parse.c:69
pub const NVM_SKU_CAP_11N_ENABLE: u32 = 1 << 2; // iwl-nvm-parse.c:70
pub const NVM_SKU_CAP_11AC_ENABLE: u32 = 1 << 3; // iwl-nvm-parse.c:71
pub const NVM_SKU_CAP_MIMO_DISABLE: u32 = 1 << 5; // iwl-nvm-parse.c:72

// ── channel flags ── iwl-nvm-parse.h:73-88
pub const NVM_CHANNEL_VALID: u16 = 1 << 0; // iwl-nvm-parse.h:74
pub const NVM_CHANNEL_IBSS: u16 = 1 << 1; // iwl-nvm-parse.h:75
pub const NVM_CHANNEL_ALLOW_20MHZ_ACTIVITY: u16 = 1 << 2; // iwl-nvm-parse.h:76
pub const NVM_CHANNEL_ACTIVE: u16 = 1 << 3; // iwl-nvm-parse.h:77
pub const NVM_CHANNEL_RADAR: u16 = 1 << 4; // iwl-nvm-parse.h:78
pub const NVM_CHANNEL_INDOOR_ONLY: u16 = 1 << 5; // iwl-nvm-parse.h:79
pub const NVM_CHANNEL_40MHZ: u16 = 1 << 9; // iwl-nvm-parse.h:83
pub const NVM_CHANNEL_80MHZ: u16 = 1 << 10; // iwl-nvm-parse.h:84
pub const NVM_CHANNEL_160MHZ: u16 = 1 << 11; // iwl-nvm-parse.h:85

/// The channel numbers in the order the extended NVM stores their flags. // iwl-nvm-parse.c:87-95
pub const EXT_NVM_CHANNELS: [u16; 51] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, // 2.4 GHz
    36, 40, 44, 48, 52, 56, 60, 64, 68, 72, 76, 80, 84, 88, 92, 96, 100, 104, 108, 112, 116, 120, 124, 128, 132, 136, 140, 144, 149, 153, 157, 161, 165, 169, 173, 177, 181, // 5 GHz
];
/// `NUM_2GHZ_CHANNELS`: the first this many entries are 2.4 GHz. // iwl-nvm-parse.c:113
pub const NUM_2GHZ_CHANNELS: usize = 14; // iwl-nvm-parse.c:113

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmError {
    /// A section shorter than the words the parse reads.
    Short { section: &'static str, need: usize, have: usize },
    /// "no valid mac address was found". // iwl-nvm-parse.c:1316-1319
    NoValidMac,
}

fn w16(b: &[u8], word: usize, section: &'static str) -> Result<u16, NvmError> {
    let o = word * 2;
    if b.len() < o + 2 {
        return Err(NvmError::Short { section, need: o + 2, have: b.len() });
    }
    Ok(u16::from_le_bytes([b[o], b[o + 1]]))
}

fn w32(b: &[u8], word: usize, section: &'static str) -> Result<u32, NvmError> {
    let o = word * 2;
    if b.len() < o + 4 {
        return Err(NvmError::Short { section, need: o + 4, have: b.len() });
    }
    Ok(u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]))
}

/// `EXT_NVM_RF_CFG_*`: the extended radio configuration word. // iwl-drv.h:22-25 (+ antennas, iwl-drv.h:26-27)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RadioCfg {
    pub flavor: u8,
    pub dash: u8,
    pub step: u8,
    pub ty: u16,
    pub valid_tx_ant: u8,
    pub valid_rx_ant: u8,
}

impl RadioCfg {
    /// `iwl_set_radio_cfg`, the family-8000 arm. // iwl-nvm-parse.c:1207-1213
    pub fn from_word(w: u32) -> RadioCfg {
        RadioCfg {
            flavor: (w & 0xF) as u8,
            dash: ((w >> 4) & 0xF) as u8,
            step: ((w >> 8) & 0xF) as u8,
            ty: ((w >> 12) & 0xFFF) as u16,
            valid_tx_ant: ((w >> 24) & 0xF) as u8, // EXT_NVM_RF_CFG_TX_ANT_MSK
            valid_rx_ant: ((w >> 28) & 0xF) as u8, // EXT_NVM_RF_CFG_RX_ANT_MSK
        }
    }
}

/// One channel as the NVM states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmChannel {
    pub number: u16,
    pub flags: u16,
    pub band_5ghz: bool,
}

/// What `iwl_parse_nvm_data` produces for an extended NVM (without the capability tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmData {
    pub nvm_version: u32,
    pub radio: RadioCfg,
    pub sku: u32,
    pub n_hw_addrs: u32,
    pub lar_enabled: bool,
}

impl NvmData {
    pub fn band_24ghz(&self) -> bool {
        self.sku & NVM_SKU_CAP_BAND_24GHZ != 0
    }
    pub fn band_52ghz(&self) -> bool {
        self.sku & NVM_SKU_CAP_BAND_52GHZ != 0
    }
    pub fn mimo_disabled(&self) -> bool {
        self.sku & NVM_SKU_CAP_MIMO_DISABLE != 0
    }
}

/// `iwl_parse_nvm_data`, `IWL_NVM_EXT` arm: `sw` is NVM section 1, `regulatory` section 3, `phy_sku`
/// section 12. // iwl-nvm-parse.c:1429-1505
pub fn parse_ext(sw: &[u8], regulatory: &[u8], phy_sku: &[u8]) -> Result<NvmData, NvmError> {
    let nvm_version = w32(sw, NVM_VERSION_EXT_NVM, "SW")?; // iwl_get_nvm_version, :1164-1171
    let radio = RadioCfg::from_word(w32(phy_sku, RADIO_CFG_FAMILY_EXT_NVM, "PHY_SKU")?); // :1173-1181, :1445-1447
    let sku = w32(phy_sku, SKU_FAMILY_8000, "PHY_SKU")?; // iwl_get_sku, :1155-1162
    let n_hw_addrs = w32(sw, N_HW_ADDRS_FAMILY_8000, "SW")? & N_HW_ADDR_MASK; // :1183-1193
    let lar_offset = if nvm_version < NVM_LAR_NEW_VERSION { NVM_LAR_OFFSET_OLD } else { NVM_LAR_OFFSET }; // :1489-1491
    let lar_config = w16(regulatory, lar_offset, "REGULATORY")?; // :1493
    Ok(NvmData { nvm_version, radio, sku, n_hw_addrs, lar_enabled: lar_config & NVM_LAR_ENABLED != 0 })
}

/// `iwl_flip_hw_address`: the two address dwords as the CSR holds them → the six address bytes.
/// // iwl-nvm-parse.c:1216-1229
pub fn flip_hw_address(mac_addr0: u32, mac_addr1: u32) -> [u8; 6] {
    let a = mac_addr0.to_le_bytes();
    let b = mac_addr1.to_le_bytes();
    [a[3], a[2], a[1], a[0], b[1], b[0]]
}

/// `is_valid_ether_addr`: not multicast (bit 0 of byte 0), not all zero.
pub fn is_valid_ether_addr(a: &[u8; 6]) -> bool {
    a[0] & 1 == 0 && a.iter().any(|&x| x != 0)
}

/// `iwl_set_hw_address_from_csr`: the OEM-fused strap address if valid, else the OTP one.
/// Arguments are the four CSR reads, in the order of the register definitions. // iwl-nvm-parse.c:1231-1251
pub fn mac_from_csr(strap0: u32, strap1: u32, otp0: u32, otp1: u32) -> Result<[u8; 6], NvmError> {
    let strap = flip_hw_address(strap0, strap1); // :1237-1240
    if is_valid_ether_addr(&strap) {
        return Ok(strap); // :1241-1246
    }
    let otp = flip_hw_address(otp0, otp1); // :1248-1250
    if is_valid_ether_addr(&otp) {
        Ok(otp)
    } else {
        Err(NvmError::NoValidMac) // iwl_set_hw_address, :1316-1319
    }
}

/// The channels of the extended NVM's regulatory section, in NVM order, with the 5 GHz entries
/// dropped when the SKU lacks the band. Channels the NVM marks not VALID are kept only when `lar` is
/// set (they may become valid with the firmware's regulatory domain); without LAR they are dropped.
/// // iwl_init_channel_map, iwl-nvm-parse.c:336-436
pub fn channels<'a>(regulatory: &'a [u8], sku_52ghz: bool, lar: bool) -> impl Iterator<Item = NvmChannel> + 'a {
    EXT_NVM_CHANNELS.iter().enumerate().filter_map(move |(idx, &number)| {
        let band_5ghz = idx >= NUM_2GHZ_CHANNELS; // iwl_nl80211_band_from_channel_idx
        let o = (NVM_CHANNELS_EXTENDED + idx) * 2;
        let flags = u16::from_le_bytes([*regulatory.get(o)?, *regulatory.get(o + 1)?]); // :370-371
        if band_5ghz && !sku_52ghz {
            return None; // :373-375
        }
        if !lar && flags & NVM_CHANNEL_VALID == 0 {
            return None; // :388-398
        }
        Some(NvmChannel { number, flags, band_5ghz })
    })
}
