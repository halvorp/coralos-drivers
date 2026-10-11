// SPDX-License-Identifier: GPL-2.0-only
//! The firmware's LMAC and UMAC error-event tables and the assert-name lookup, ported from Linux
//! `drivers/net/wireless/intel/iwlwifi/fw/dump.c:17-230` (`struct iwl_error_event_table`,
//! `struct iwl_umac_error_event_table`, `iwl_fwrt_dump_umac_error_log`,
//! `iwl_fwrt_dump_lmac_error_log`), `drivers/net/wireless/intel/iwlwifi/fw/img.c:55-95`
//! (`iwl_fw_lookup_assert_desc`), `drivers/net/wireless/intel/iwlwifi/fw/img.h:282-283` and
//! `drivers/net/wireless/intel/iwlwifi/iwl-trans.h:1169-1172`.
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2014 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH; Copyright (C) 2019-2021, 2024
//! Intel Corporation (img.c).
//!
//! Both tables are read from device SRAM as u32s (dump.c:17-22), so they are decoded here from the
//! dword array the caller's `iwl_trans_read_mem_bytes` produced, by field INDEX.

/// `struct iwl_error_event_table` has 38 u32 fields (LOG_ERROR_TABLE_API_S_VER_3). // dump.c:23-71
pub const LMAC_ERROR_TABLE_DWORDS: usize = 38;
/// `struct iwl_umac_error_event_table` has 15 u32 fields. // dump.c:78-94
pub const UMAC_ERROR_TABLE_DWORDS: usize = 15;

// Field indices of struct iwl_error_event_table. // dump.c:24-68
pub const LMAC_VALID: usize = 0; // dump.c:24
pub const LMAC_ERROR_ID: usize = 1; // dump.c:25
pub const LMAC_TRM_HW_STATUS0: usize = 2; // dump.c:26
pub const LMAC_TRM_HW_STATUS1: usize = 3; // dump.c:27
pub const LMAC_BLINK2: usize = 4; // dump.c:28
pub const LMAC_ILINK1: usize = 5; // dump.c:29
pub const LMAC_ILINK2: usize = 6; // dump.c:30
pub const LMAC_DATA1: usize = 7; // dump.c:31
pub const LMAC_DATA2: usize = 8; // dump.c:32
pub const LMAC_DATA3: usize = 9; // dump.c:33
pub const LMAC_BCON_TIME: usize = 10; // dump.c:34
pub const LMAC_TSF_LOW: usize = 11; // dump.c:35
pub const LMAC_TSF_HI: usize = 12; // dump.c:36
pub const LMAC_GP1: usize = 13; // dump.c:37
pub const LMAC_GP2: usize = 14; // dump.c:38
pub const LMAC_FW_REV_TYPE: usize = 15; // dump.c:39
pub const LMAC_MAJOR: usize = 16; // dump.c:40
pub const LMAC_MINOR: usize = 17; // dump.c:41
pub const LMAC_HW_VER: usize = 18; // dump.c:42
pub const LMAC_BRD_VER: usize = 19; // dump.c:43
pub const LMAC_LOG_PC: usize = 20; // dump.c:44
pub const LMAC_FRAME_PTR: usize = 21; // dump.c:45
pub const LMAC_STACK_PTR: usize = 22; // dump.c:46
pub const LMAC_HCMD: usize = 23; // dump.c:47
pub const LMAC_ISR0: usize = 24; // dump.c:48
pub const LMAC_ISR1: usize = 25; // dump.c:50
pub const LMAC_ISR2: usize = 26; // dump.c:52
pub const LMAC_ISR3: usize = 27; // dump.c:54
pub const LMAC_ISR4: usize = 28; // dump.c:56
pub const LMAC_LAST_CMD_ID: usize = 29; // dump.c:58
pub const LMAC_WAIT_EVENT: usize = 30; // dump.c:59
pub const LMAC_L2P_CONTROL: usize = 31; // dump.c:60
pub const LMAC_L2P_DURATION: usize = 32; // dump.c:61
pub const LMAC_L2P_MHVALID: usize = 33; // dump.c:62
pub const LMAC_L2P_ADDR_MATCH: usize = 34; // dump.c:63
pub const LMAC_LMPM_PMG_SEL: usize = 35; // dump.c:64
pub const LMAC_U_TIMESTAMP: usize = 36; // dump.c:66
pub const LMAC_FLOW_HANDLER: usize = 37; // dump.c:68

// Field indices of struct iwl_umac_error_event_table. // dump.c:79-93
pub const UMAC_VALID: usize = 0; // dump.c:79
pub const UMAC_ERROR_ID: usize = 1; // dump.c:80
pub const UMAC_BLINK1: usize = 2; // dump.c:81
pub const UMAC_BLINK2: usize = 3; // dump.c:82
pub const UMAC_ILINK1: usize = 4; // dump.c:83
pub const UMAC_ILINK2: usize = 5; // dump.c:84
pub const UMAC_DATA1: usize = 6; // dump.c:85
pub const UMAC_DATA2: usize = 7; // dump.c:86
pub const UMAC_DATA3: usize = 8; // dump.c:87
pub const UMAC_MAJOR: usize = 9; // dump.c:88
pub const UMAC_MINOR: usize = 10; // dump.c:89
pub const UMAC_FRAME_POINTER: usize = 11; // dump.c:90
pub const UMAC_STACK_POINTER: usize = 12; // dump.c:91
pub const UMAC_CMD_HEADER: usize = 13; // dump.c:92
pub const UMAC_NIC_ISR_PREF: usize = 14; // dump.c:93

/// `ERROR_START_OFFSET` / `ERROR_ELEM_SIZE`: Linux prints the dump banner when
/// `ERROR_START_OFFSET <= valid * ERROR_ELEM_SIZE`, i.e. when `valid != 0`. // dump.c:96-97
pub const ERROR_START_OFFSET: u32 = 4; // dump.c:96
pub const ERROR_ELEM_SIZE: u32 = 7 * 4; // dump.c:97

pub const FW_SYSASSERT_CPU_MASK: u32 = 0xf000_0000; // img.h:282
pub const FW_SYSASSERT_PNVM_MISSING: u32 = 0x0010_070d; // img.h:283

/// `advanced_lookup[]` without its "ADVANCED_SYSASSERT" sentinel. // img.c:55-81
pub const ADVANCED_LOOKUP: [(&str, u32); 21] = [
    ("NMI_INTERRUPT_WDG", 0x34),                 // img.c:59
    ("SYSASSERT", 0x35),                         // img.c:60
    ("UCODE_VERSION_MISMATCH", 0x37),            // img.c:61
    ("BAD_COMMAND", 0x38),                       // img.c:62
    ("BAD_COMMAND", 0x39),                       // img.c:63
    ("NMI_INTERRUPT_DATA_ACTION_PT", 0x3C),      // img.c:64
    ("FATAL_ERROR", 0x3D),                       // img.c:65
    ("NMI_TRM_HW_ERR", 0x46),                    // img.c:66
    ("NMI_INTERRUPT_TRM", 0x4C),                 // img.c:67
    ("NMI_INTERRUPT_BREAK_POINT", 0x54),         // img.c:68
    ("NMI_INTERRUPT_WDG_RXF_FULL", 0x5C),        // img.c:69
    ("NMI_INTERRUPT_WDG_NO_RBD_RXF_FULL", 0x64), // img.c:70
    ("NMI_INTERRUPT_HOST", 0x66),                // img.c:71
    ("NMI_INTERRUPT_LMAC_FATAL", 0x70),          // img.c:72
    ("NMI_INTERRUPT_UMAC_FATAL", 0x71),          // img.c:73
    ("NMI_INTERRUPT_OTHER_LMAC_FATAL", 0x73),    // img.c:74
    ("NMI_INTERRUPT_ACTION_PT", 0x7C),           // img.c:75
    ("NMI_INTERRUPT_UNKNOWN", 0x84),             // img.c:76
    ("NMI_INTERRUPT_INST_ACTION_PT", 0x86),      // img.c:77
    ("NMI_INTERRUPT_PREG", 0x88),                // img.c:78
    ("PNVM_MISSING", FW_SYSASSERT_PNVM_MISSING), // img.c:79
];
/// The name for every id the table does not list. // img.c:80, :92-93
pub const ADVANCED_SYSASSERT: &str = "ADVANCED_SYSASSERT";

/// `iwl_fw_lookup_assert_desc`: matched with the CPU nibble masked off. // img.c:84-94
pub fn lookup_assert_desc(num: u32) -> &'static str {
    let key = num & !FW_SYSASSERT_CPU_MASK;
    ADVANCED_LOOKUP.iter().find(|(_, n)| *n == key).map_or(ADVANCED_SYSASSERT, |(name, _)| name)
}

/// `iwl_trans_is_hw_error_value`: the word reads as a bus error, so Linux SW-resets and reactivates
/// the NIC before reading the table. // iwl-trans.h:1169-1172; dump.c:169-182
pub fn is_hw_error_value(val: u32) -> bool {
    (val & !0xF) == 0xA5A5_A5A0 || (val & !0xF) == 0x5A5A_5A50
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableError {
    /// The caller read fewer dwords than the struct has.
    Short { got: usize, need: usize },
    /// "Not valid error log pointer 0x%08X for %s uCode". // dump.c:160-166
    NoLmacPointer { init_image: bool },
}

/// The LMAC table base `iwl_fwrt_dump_lmac_error_log` reads: the ALIVE pointer, else the image's
/// TLV ERRLOG pointer, else refused. // dump.c:146-164
pub fn lmac_table_base(alive_ptr: u32, init_image: bool, init_errlog_ptr: u32, inst_errlog_ptr: u32) -> Result<u32, TableError> {
    let base = match (alive_ptr, init_image) {
        (0, true) => init_errlog_ptr,  // dump.c:152-154
        (0, false) => inst_errlog_ptr, // dump.c:156-157
        (p, _) => p,
    };
    if base == 0 {
        return Err(TableError::NoLmacPointer { init_image });
    }
    Ok(base)
}

/// `struct iwl_error_event_table`, as read. // dump.c:23-71
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LmacErrorTable(pub [u32; LMAC_ERROR_TABLE_DWORDS]);

/// `struct iwl_umac_error_event_table`, as read. // dump.c:78-94
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UmacErrorTable(pub [u32; UMAC_ERROR_TABLE_DWORDS]);

fn take<const N: usize>(dwords: &[u32]) -> Result<[u32; N], TableError> {
    dwords.get(..N).and_then(|s| s.try_into().ok()).ok_or(TableError::Short { got: dwords.len(), need: N })
}

impl LmacErrorTable {
    pub fn from_dwords(dwords: &[u32]) -> Result<Self, TableError> {
        take(dwords).map(LmacErrorTable)
    }
    pub fn get(&self, field: usize) -> u32 {
        self.0[field]
    }
    /// Non-zero `valid`: the log holds an entry (dump.c:24) and Linux records `error_id` (:188-189).
    pub fn is_valid(&self) -> bool {
        ERROR_START_OFFSET as u64 <= self.0[LMAC_VALID] as u64 * ERROR_ELEM_SIZE as u64 // dump.c:191 (size_t arithmetic: no wrap)
    }
}

impl UmacErrorTable {
    pub fn from_dwords(dwords: &[u32]) -> Result<Self, TableError> {
        take(dwords).map(UmacErrorTable)
    }
    pub fn get(&self, field: usize) -> u32 {
        self.0[field]
    }
    pub fn is_valid(&self) -> bool {
        ERROR_START_OFFSET as u64 <= self.0[UMAC_VALID] as u64 * ERROR_ELEM_SIZE as u64 // dump.c:116
    }
}
