// SPDX-License-Identifier: GPL-2.0-only
//! The scalar and capability TLVs of an iwlwifi firmware file, ported from the matching `case` arms of
//! Linux `drivers/net/wireless/intel/iwlwifi/iwl-drv.c` (`iwl_parse_tlv_firmware`, :788-1456), with
//! the TLV type numbers from `drivers/net/wireless/intel/iwlwifi/fw/file.h:47-124` and the
//! debug-address layouts from `drivers/net/wireless/intel/iwlwifi/fw/api/alive.h:41-66`.
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.
//!
//! FAMILY BRANCH, made visible: on IWL_DEVICE_FAMILY_9000 (the Wireless-AC 9260) the
//! UMAC/LMAC_DEBUG_ADDRS TLVs are length-checked and then IGNORED (iwl-drv.c:1336-1340, :1352-1356);
//! the error-table pointers come from the ALIVE notification instead. [`FwTlvInfo::tlv_error_tables`]
//! takes the family test as an argument so the caller states which branch it is on.

use crate::capa::{ApiSet, CapaSet, IndexPastBitmap};
use crate::container::{tlvs, ContainerError, Tlv};

pub const IWL_UCODE_TLV_PROBE_MAX_LEN: u32 = 6; // file.h:54
pub const IWL_UCODE_TLV_RUNT_EVTLOG_PTR: u32 = 8; // file.h:57
pub const IWL_UCODE_TLV_RUNT_EVTLOG_SIZE: u32 = 9; // file.h:58
pub const IWL_UCODE_TLV_RUNT_ERRLOG_PTR: u32 = 10; // file.h:59
pub const IWL_UCODE_TLV_INIT_EVTLOG_PTR: u32 = 11; // file.h:60
pub const IWL_UCODE_TLV_INIT_EVTLOG_SIZE: u32 = 12; // file.h:61
pub const IWL_UCODE_TLV_INIT_ERRLOG_PTR: u32 = 13; // file.h:62
pub const IWL_UCODE_TLV_FLAGS: u32 = 18; // file.h:67
pub const IWL_UCODE_TLV_PHY_SKU: u32 = 23; // file.h:72
pub const IWL_UCODE_TLV_NUM_OF_CPU: u32 = 27; // file.h:76
pub const IWL_UCODE_TLV_API_CHANGES_SET: u32 = 29; // file.h:78
pub const IWL_UCODE_TLV_ENABLED_CAPABILITIES: u32 = 30; // file.h:79
pub const IWL_UCODE_TLV_N_SCAN_CHANNELS: u32 = 31; // file.h:80
pub const IWL_UCODE_TLV_PAGING: u32 = 32; // file.h:81
pub const IWL_UCODE_TLV_FW_VERSION: u32 = 36; // file.h:84
pub const IWL_UCODE_TLV_UMAC_DEBUG_ADDRS: u32 = 54; // file.h:92
pub const IWL_UCODE_TLV_LMAC_DEBUG_ADDRS: u32 = 55; // file.h:93

/// Defaults when the TLV is absent. // iwl-drv.c:1616, :1619
pub const IWL_DEFAULT_MAX_PROBE_LENGTH: u32 = 200; // file.h:574
pub const IWL_DEFAULT_SCAN_CHANNELS: u32 = 40; // iwl-drv.c:97

/// The PAGING TLV bounds. // img.h:93-110
pub const FW_PAGING_SIZE: u32 = 1 << 12; // img.h:94
pub const PAGING_BLOCK_SIZE: u32 = (1 << 3) * FW_PAGING_SIZE; // img.h:99
pub const MAX_PAGING_IMAGE_SIZE: u32 = (1 << 5) * PAGING_BLOCK_SIZE; // img.h:110

/// `enum iwl_fw_phy_cfg` fields of the PHY_SKU word. // file.h:602-618
pub const FW_PHY_CFG_RADIO_TYPE: u32 = 0x3; // file.h:604
pub const FW_PHY_CFG_RADIO_STEP_POS: u32 = 2; // file.h:605
pub const FW_PHY_CFG_RADIO_STEP: u32 = 0x3 << FW_PHY_CFG_RADIO_STEP_POS; // file.h:606
pub const FW_PHY_CFG_RADIO_DASH_POS: u32 = 4; // file.h:607
pub const FW_PHY_CFG_RADIO_DASH: u32 = 0x3 << FW_PHY_CFG_RADIO_DASH_POS; // file.h:608
pub const FW_PHY_CFG_TX_CHAIN_POS: u32 = 16; // file.h:609
pub const FW_PHY_CFG_TX_CHAIN: u32 = 0xf << FW_PHY_CFG_TX_CHAIN_POS; // file.h:610
pub const FW_PHY_CFG_RX_CHAIN_POS: u32 = 20; // file.h:611
pub const FW_PHY_CFG_RX_CHAIN: u32 = 0xf << FW_PHY_CFG_RX_CHAIN_POS; // file.h:612

/// The address bits above the SRAM address that select cache control. // img.h:121
pub const FW_ADDR_CACHE_CONTROL: u32 = 0xC000_0000; // img.h:121

/// sizeof(struct iwl_lmac_debug_addrs): 8 le32. // alive.h:41-50
pub const LMAC_DEBUG_ADDRS_LEN: usize = 32;
/// sizeof(struct iwl_umac_debug_addrs): 2 le32. // alive.h:63-66
pub const UMAC_DEBUG_ADDRS_LEN: usize = 8;

/// What the TLVs say, after the walk. Each field names the iwl-drv.c arm that fills it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FwTlvInfo {
    pub api: ApiSet,
    pub capa: CapaSet,
    /// PROBE_MAX_LEN. // iwl-drv.c:897-901
    pub max_probe_length: u32,
    /// N_SCAN_CHANNELS. // iwl-drv.c:1064-1069
    pub n_scan_channels: u32,
    /// FLAGS: the first word only. // iwl-drv.c:908-923
    pub flags: u32,
    /// PHY_SKU. // iwl-drv.c:1020-1030
    pub phy_config: Option<u32>,
    /// NUM_OF_CPU == 2 marks every image dual-CPU. // iwl-drv.c:1046-1063
    pub num_of_cpu: u32,
    /// PAGING, bytes; 0 when absent. // iwl-drv.c:1213-1241
    pub paging_mem_size: u32,
    /// FW_VERSION (major, minor, local_comp). // iwl-drv.c:1070-1087
    pub fw_version: Option<(u32, u32, u8)>,
    /// The legacy log pointers. // iwl-drv.c:934-969
    pub init_evtlog_ptr: u32,
    pub init_evtlog_size: u32,
    pub init_errlog_ptr: u32,
    pub inst_evtlog_ptr: u32,
    pub inst_evtlog_size: u32,
    pub inst_errlog_ptr: u32,
    /// UMAC_DEBUG_ADDRS raw: (error_info_addr, dbg_print_buff_addr). // iwl-drv.c:1332-1347
    pub umac_debug_addrs: Option<(u32, u32)>,
    /// LMAC_DEBUG_ADDRS raw, the 8 words of struct iwl_lmac_debug_addrs. // iwl-drv.c:1348-1363
    pub lmac_debug_addrs: Option<[u32; 8]>,
    /// The first API/CAPA word whose index was past its bitmap (Linux warns and drops it).
    pub dropped: Option<IndexPastBitmap>,
}

impl Default for FwTlvInfo {
    fn default() -> Self {
        FwTlvInfo {
            api: ApiSet::default(),
            capa: CapaSet::default(),
            max_probe_length: IWL_DEFAULT_MAX_PROBE_LENGTH,
            n_scan_channels: IWL_DEFAULT_SCAN_CHANNELS,
            flags: 0,
            phy_config: None,
            num_of_cpu: 1,
            paging_mem_size: 0,
            fw_version: None,
            init_evtlog_ptr: 0,
            init_evtlog_size: 0,
            init_errlog_ptr: 0,
            inst_evtlog_ptr: 0,
            inst_evtlog_size: 0,
            inst_errlog_ptr: 0,
            umac_debug_addrs: None,
            lmac_debug_addrs: None,
            dropped: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamsError {
    Container(ContainerError),
    /// "TLV %d has invalid size: %u" — the whole file is refused. // iwl-drv.c:1450-1455
    InvalidTlvLen { ty: u32, len: usize },
    /// "Driver support up to 2 CPUs". // iwl-drv.c:1059-1062
    NumOfCpu { n: u32 },
    /// "Paging: driver supports up to %lu bytes". // iwl-drv.c:1222-1227
    PagingTooLarge { size: u32, max: u32 },
    /// "Paging: image isn't multiple %lu". // iwl-drv.c:1229-1234
    PagingNotPageMultiple { size: u32, page: u32 },
}

/// TX / RX chain masks out of a PHY_SKU word. // iwl-drv.c:1024-1029
pub fn phy_valid_ant(phy_config: u32) -> (u8, u8) {
    (
        ((phy_config & FW_PHY_CFG_TX_CHAIN) >> FW_PHY_CFG_TX_CHAIN_POS) as u8,
        ((phy_config & FW_PHY_CFG_RX_CHAIN) >> FW_PHY_CFG_RX_CHAIN_POS) as u8,
    )
}

/// Radio (type, step, dash) out of a PHY_SKU word, as mvm/ops.c:87-92 reads them.
pub fn phy_radio_cfg(phy_config: u32) -> (u8, u8, u8) {
    (
        (phy_config & FW_PHY_CFG_RADIO_TYPE) as u8,
        ((phy_config & FW_PHY_CFG_RADIO_STEP) >> FW_PHY_CFG_RADIO_STEP_POS) as u8,
        ((phy_config & FW_PHY_CFG_RADIO_DASH) >> FW_PHY_CFG_RADIO_DASH_POS) as u8,
    )
}

/// Error-table pointers the DEBUG_ADDRS TLVs contribute after the family gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TlvErrorTables {
    pub lmac: Option<u32>,
    pub umac: Option<u32>,
}

impl FwTlvInfo {
    /// `fw_has_api`. // img.h:62-67
    pub fn has_api(&self, bit: u32) -> bool {
        self.api.has(bit)
    }

    /// `fw_has_capa`. // img.h:69-74
    pub fn has_capa(&self, bit: u32) -> bool {
        self.capa.has(bit)
    }

    /// `is_dual_cpus`. // iwl-drv.c:1052-1058
    pub fn is_dual_cpus(&self) -> bool {
        self.num_of_cpu == 2
    }

    /// Below IWL_DEVICE_FAMILY_22000 nothing (iwl-drv.c:1338-1340, :1354-1356); otherwise each
    /// address with FW_ADDR_CACHE_CONTROL stripped (:1341-1343, :1357-1359).
    pub fn tlv_error_tables(&self, family_ge_22000: bool) -> TlvErrorTables {
        if !family_ge_22000 {
            return TlvErrorTables::default();
        }
        TlvErrorTables {
            lmac: self.lmac_debug_addrs.map(|a| a[0] & !FW_ADDR_CACHE_CONTROL),
            umac: self.umac_debug_addrs.map(|(e, _)| e & !FW_ADDR_CACHE_CONTROL),
        }
    }

    fn take(&mut self, t: Tlv<'_>) -> Result<(), ParamsError> {
        let d = t.data;
        let bad = ParamsError::InvalidTlvLen { ty: t.ty, len: d.len() };
        let w = |i: usize| u32::from_le_bytes([d[4 * i], d[4 * i + 1], d[4 * i + 2], d[4 * i + 3]]);
        let one = || if d.len() == 4 { Ok(w(0)) } else { Err(bad) };
        match t.ty {
            IWL_UCODE_TLV_PROBE_MAX_LEN => self.max_probe_length = one()?,
            IWL_UCODE_TLV_FLAGS => {
                if d.len() < 4 || d.len() % 4 != 0 {
                    return Err(bad); // iwl-drv.c:910-914
                }
                self.flags = w(0);
            }
            IWL_UCODE_TLV_API_CHANGES_SET | IWL_UCODE_TLV_ENABLED_CAPABILITIES => {
                if d.len() != 8 {
                    return Err(bad); // iwl-drv.c:925-926, :930-931 (sizeof struct iwl_ucode_api/capa)
                }
                let r = if t.ty == IWL_UCODE_TLV_API_CHANGES_SET {
                    self.api.set_word(w(0), w(1))
                } else {
                    self.capa.set_word(w(0), w(1))
                };
                if let Err(e) = r {
                    self.dropped.get_or_insert(e);
                }
            }
            IWL_UCODE_TLV_INIT_EVTLOG_PTR => self.init_evtlog_ptr = one()?,
            IWL_UCODE_TLV_INIT_EVTLOG_SIZE => self.init_evtlog_size = one()?,
            IWL_UCODE_TLV_INIT_ERRLOG_PTR => self.init_errlog_ptr = one()?,
            IWL_UCODE_TLV_RUNT_EVTLOG_PTR => self.inst_evtlog_ptr = one()?,
            IWL_UCODE_TLV_RUNT_EVTLOG_SIZE => self.inst_evtlog_size = one()?,
            IWL_UCODE_TLV_RUNT_ERRLOG_PTR => self.inst_errlog_ptr = one()?,
            IWL_UCODE_TLV_PHY_SKU => self.phy_config = Some(one()?),
            IWL_UCODE_TLV_NUM_OF_CPU => {
                let n = one()?;
                if !(1..=2).contains(&n) {
                    return Err(ParamsError::NumOfCpu { n });
                }
                self.num_of_cpu = n;
            }
            IWL_UCODE_TLV_N_SCAN_CHANNELS => self.n_scan_channels = one()?,
            IWL_UCODE_TLV_PAGING => {
                let size = one()?;
                if size > MAX_PAGING_IMAGE_SIZE {
                    return Err(ParamsError::PagingTooLarge { size, max: MAX_PAGING_IMAGE_SIZE });
                }
                if size & (FW_PAGING_SIZE - 1) != 0 {
                    return Err(ParamsError::PagingNotPageMultiple { size, page: FW_PAGING_SIZE });
                }
                self.paging_mem_size = size;
            }
            IWL_UCODE_TLV_FW_VERSION => {
                if d.len() != 12 {
                    return Err(bad); // iwl-drv.c:1075-1076
                }
                self.fw_version = Some((w(0), w(1), w(2) as u8)); // local_comp is a u8, :1073,1080
            }
            IWL_UCODE_TLV_UMAC_DEBUG_ADDRS => {
                if d.len() != UMAC_DEBUG_ADDRS_LEN {
                    return Err(bad); // iwl-drv.c:1336-1337, BEFORE the family gate
                }
                self.umac_debug_addrs = Some((w(0), w(1)));
            }
            IWL_UCODE_TLV_LMAC_DEBUG_ADDRS => {
                if d.len() != LMAC_DEBUG_ADDRS_LEN {
                    return Err(bad); // iwl-drv.c:1352-1353
                }
                self.lmac_debug_addrs = Some([w(0), w(1), w(2), w(3), w(4), w(5), w(6), w(7)]);
            }
            _ => {}
        }
        Ok(())
    }
}

/// Walk the whole file and collect the capability/parameter TLVs. Repeated API/CAPA words OR in; a
/// repeated scalar keeps the last, as the Linux loop does.
pub fn parse_fw_tlvs(blob: &[u8]) -> Result<FwTlvInfo, ParamsError> {
    let mut info = FwTlvInfo::default();
    for t in tlvs(blob).map_err(ParamsError::Container)? {
        info.take(t.map_err(ParamsError::Container)?)?;
    }
    Ok(info)
}
