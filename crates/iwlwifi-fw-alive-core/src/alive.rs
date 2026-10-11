// SPDX-License-Identifier: GPL-2.0-only
//! The UCODE_ALIVE notification in its v3 layout, keeping every debug pointer, ported from Linux
//! `drivers/net/wireless/intel/iwlwifi/fw/api/alive.h:36-83`,
//! `drivers/net/wireless/intel/iwlwifi/fw/api/commands.h:39, :65` and
//! `drivers/net/wireless/intel/iwlwifi/mvm/fw.c:100-238` (`iwl_alive_fn`).
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.
//!
//! Layout selection (mvm/fw.c:112-203): the blob's CMD_VERSIONS notif_ver for (LEGACY_GROUP,
//! UCODE_ALIVE_NTFY); `>= 6` is the v7/v8 layout, otherwise a payload of EXACTLY sizeof(v3) = 68
//! bytes is v3, and anything else is "unsupported alive notification". The 9260's -46 firmware states
//! notif_ver 3, so only v3 is ported; every other case is refused by name.

use crate::packet::RxPacket;

pub const LEGACY_GROUP: u8 = 0x0; // commands.h:39
pub const UCODE_ALIVE_NTFY: u8 = 0x1; // commands.h:65
pub const IWL_ALIVE_STATUS_ERR: u16 = 0xDEAD; // alive.h:36
pub const IWL_ALIVE_STATUS_OK: u16 = 0xCAFE; // alive.h:37
pub const IWL_ALIVE_FLG_RFKILL: u16 = 1 << 0; // alive.h:39
/// The cache-control bits above a UMAC SRAM address. // img.h:121
pub const FW_ADDR_CACHE_CONTROL: u32 = 0xC000_0000; // img.h:121

/// sizeof(struct iwl_lmac_debug_addrs): 8 le32. // alive.h:41-50
pub const LMAC_DEBUG_ADDRS_LEN: usize = 8 * 4;
/// sizeof(struct iwl_lmac_alive): 2 le32 + 4 u8 + le32 + the debug addrs. // alive.h:52-61
pub const LMAC_ALIVE_LEN: usize = 4 + 4 + 4 + 4 + LMAC_DEBUG_ADDRS_LEN;
/// sizeof(struct iwl_umac_alive): 2 le32 + 2 le32 debug addrs. // alive.h:63-72
pub const UMAC_ALIVE_LEN: usize = 4 + 4 + 4 + 4;
/// sizeof(struct iwl_alive_ntf_v3): le16 status + le16 flags + lmac + umac. // alive.h:78-83
pub const ALIVE_NTF_V3_LEN: usize = 2 + 2 + LMAC_ALIVE_LEN + UMAC_ALIVE_LEN;
/// mvm/fw.c:117: notif_ver >= 6 selects the v7/v8 layout.
pub const ALIVE_V7_MIN_NOTIF_VER: u8 = 6;

/// `struct iwl_lmac_debug_addrs`. // alive.h:41-50
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LmacDebugAddrs {
    pub error_event_table_ptr: u32,
    pub log_event_table_ptr: u32,
    pub cpu_register_ptr: u32,
    pub dbgm_config_ptr: u32,
    pub alive_counter_ptr: u32,
    pub scd_base_ptr: u32,
    pub st_fwrd_addr: u32,
    pub st_fwrd_size: u32,
}

/// `struct iwl_lmac_alive`. // alive.h:52-61
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LmacAlive {
    pub ucode_major: u32,
    pub ucode_minor: u32,
    pub ver_subtype: u8,
    pub ver_type: u8,
    pub mac: u8,
    pub opt: u8,
    pub timestamp: u32,
    pub dbg_ptrs: LmacDebugAddrs,
}

/// `struct iwl_umac_alive` with its `struct iwl_umac_debug_addrs`. // alive.h:63-72
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UmacAlive {
    pub umac_major: u32,
    pub umac_minor: u32,
    pub error_info_addr: u32,
    pub dbg_print_buff_addr: u32,
}

/// `struct iwl_alive_ntf_v3`. // alive.h:78-83
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AliveV3 {
    pub status: u16,
    pub flags: u16,
    pub lmac: LmacAlive,
    pub umac: UmacAlive,
}

/// The error-table pointers the driver keeps from an ALIVE — on the 9000 family the ONLY source of
/// them (the TLV ones are ignored below 22000). // mvm/fw.c:204-218
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorTablePtrs {
    /// `lmac_error_event_table[0]`, unmasked. // mvm/fw.c:204-206
    pub lmac: u32,
    /// `umac_error_event_table`, cache-control stripped; `None` when zero (not recorded). // :212-218
    pub umac: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliveError {
    /// Some other notification or response.
    NotAlive { group_id: u8, cmd: u8 },
    /// notif_ver >= 6 asks for the v7/v8 layout, which this port does not carry. // mvm/fw.c:117-186
    LayoutNotPorted { notif_ver: u8 },
    /// "unsupported alive notification (size %d)". // mvm/fw.c:198-203
    UnsupportedSize { len: usize, need: usize },
}

fn le32(p: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]])
}

/// Decode an ALIVE by the rule of mvm/fw.c:112-203.
pub fn parse_alive_v3(notif_ver: u8, pkt: &RxPacket<'_>) -> Result<AliveV3, AliveError> {
    if pkt.group_id != LEGACY_GROUP || pkt.cmd != UCODE_ALIVE_NTFY {
        return Err(AliveError::NotAlive { group_id: pkt.group_id, cmd: pkt.cmd });
    }
    if notif_ver >= ALIVE_V7_MIN_NOTIF_VER {
        return Err(AliveError::LayoutNotPorted { notif_ver });
    }
    let p = pkt.payload;
    if p.len() != ALIVE_NTF_V3_LEN {
        return Err(AliveError::UnsupportedSize { len: p.len(), need: ALIVE_NTF_V3_LEN }); // fw.c:187-188
    }
    let l = 4; // lmac_data, alive.h:81
    let d = l + 16; // dbg_ptrs, alive.h:60
    let u = l + LMAC_ALIVE_LEN; // umac_data, alive.h:82
    Ok(AliveV3 {
        status: u16::from_le_bytes([p[0], p[1]]), // alive.h:79
        flags: u16::from_le_bytes([p[2], p[3]]),  // alive.h:80
        lmac: LmacAlive {
            ucode_major: le32(p, l),
            ucode_minor: le32(p, l + 4),
            ver_subtype: p[l + 8],
            ver_type: p[l + 9],
            mac: p[l + 10],
            opt: p[l + 11],
            timestamp: le32(p, l + 12),
            dbg_ptrs: LmacDebugAddrs {
                error_event_table_ptr: le32(p, d),
                log_event_table_ptr: le32(p, d + 4),
                cpu_register_ptr: le32(p, d + 8),
                dbgm_config_ptr: le32(p, d + 12),
                alive_counter_ptr: le32(p, d + 16),
                scd_base_ptr: le32(p, d + 20),
                st_fwrd_addr: le32(p, d + 24),
                st_fwrd_size: le32(p, d + 28),
            },
        },
        umac: UmacAlive {
            umac_major: le32(p, u),
            umac_minor: le32(p, u + 4),
            error_info_addr: le32(p, u + 8),
            dbg_print_buff_addr: le32(p, u + 12),
        },
    })
}

impl AliveV3 {
    /// `alive_data->valid = status == IWL_ALIVE_STATUS_OK`. // mvm/fw.c:220
    pub fn is_ok(&self) -> bool {
        self.status == IWL_ALIVE_STATUS_OK
    }

    /// The pointers mvm/fw.c:204-218 saves for the error dump.
    pub fn error_tables(&self) -> ErrorTablePtrs {
        let umac = self.umac.error_info_addr & !FW_ADDR_CACHE_CONTROL;
        ErrorTablePtrs { lmac: self.lmac.dbg_ptrs.error_event_table_ptr, umac: (umac != 0).then_some(umac) }
    }
}
