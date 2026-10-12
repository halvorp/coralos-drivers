// SPDX-License-Identifier: GPL-2.0-only
//! The structures the base firmware returns through LARGE_CONFIG_GET: the TLV walk (`struct avs_tlv`, messages.h:98-103), the
//! firmware configuration (`avs_ipc_get_fw_config`, messages.c:391-511), the hardware configuration
//! (`avs_ipc_get_hw_config`, messages.c:551-642) and the modules-info table (`avs_ipc_get_modules_info`, messages.c:644-661,
//! `struct avs_module_entry`, messages.h:574-589).
//!
//! Linux walks a TLV list with `offset += sizeof(*tlv) + tlv->length` and reads `tlv->value[0]` without checking that either
//! lies inside the payload the firmware returned; here every step is bounds-checked and a cut-short payload is a named
//! [`IpcError::Truncated`].
use crate::error::IpcError;

pub const TLV_HEADER_SIZE: usize = 8; // messages.h:103: sizeof(struct avs_tlv) == 8

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tlv<'a> {
    pub ty: u32,
    pub value: &'a [u8],
}

impl<'a> Tlv<'a> {
    /// `tlv->value[i]`, refusing a TLV too short to hold it.
    pub fn dword(&self, i: usize) -> Result<u32, IpcError> {
        let at = i * 4;
        if self.value.len() < at + 4 {
            return Err(IpcError::TlvValueTooShort { tlv_type: self.ty, need: at + 4, have: self.value.len() });
        }
        Ok(u32::from_le_bytes([self.value[at], self.value[at + 1], self.value[at + 2], self.value[at + 3]]))
    }
}

pub struct TlvIter<'a> {
    b: &'a [u8],
    off: usize,
    done: bool,
}

pub fn tlvs(payload: &[u8]) -> TlvIter<'_> {
    TlvIter { b: payload, off: 0, done: false }
}

impl<'a> Iterator for TlvIter<'a> {
    type Item = Result<Tlv<'a>, IpcError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.off >= self.b.len() {
            return None;
        }
        let have = self.b.len() - self.off;
        if have < TLV_HEADER_SIZE {
            self.done = true;
            return Some(Err(IpcError::Truncated { what: "tlv header", need: TLV_HEADER_SIZE, have }));
        }
        let o = self.off;
        let ty = u32::from_le_bytes([self.b[o], self.b[o + 1], self.b[o + 2], self.b[o + 3]]);
        let len = u32::from_le_bytes([self.b[o + 4], self.b[o + 5], self.b[o + 6], self.b[o + 7]]) as usize;
        if len > have - TLV_HEADER_SIZE {
            self.done = true;
            return Some(Err(IpcError::Truncated { what: "tlv value", need: len, have: have - TLV_HEADER_SIZE }));
        }
        self.off = o + TLV_HEADER_SIZE + len;
        Some(Ok(Tlv { ty, value: &self.b[o + TLV_HEADER_SIZE..o + TLV_HEADER_SIZE + len] }))
    }
}

// ---- firmware configuration ---------------------------------------------------------------------------------------------------
pub const AVS_FW_CFG_FW_VERSION: u32 = 0; // messages.h:432
pub const AVS_FW_CFG_MEMORY_RECLAIMED: u32 = 1; // messages.h:433
pub const AVS_FW_CFG_SLOW_CLOCK_FREQ_HZ: u32 = 2; // messages.h:434
pub const AVS_FW_CFG_FAST_CLOCK_FREQ_HZ: u32 = 3; // messages.h:435
pub const AVS_FW_CFG_DMA_BUFFER_CONFIG: u32 = 4; // messages.h:436
pub const AVS_FW_CFG_ALH_SUPPORT_LEVEL: u32 = 5; // messages.h:437
pub const AVS_FW_CFG_IPC_DL_MAILBOX_BYTES: u32 = 6; // messages.h:438
pub const AVS_FW_CFG_IPC_UL_MAILBOX_BYTES: u32 = 7; // messages.h:439
pub const AVS_FW_CFG_TRACE_LOG_BYTES: u32 = 8; // messages.h:440
pub const AVS_FW_CFG_MAX_PPL_COUNT: u32 = 9; // messages.h:441
pub const AVS_FW_CFG_MAX_ASTATE_COUNT: u32 = 10; // messages.h:442
pub const AVS_FW_CFG_MAX_MODULE_PIN_COUNT: u32 = 11; // messages.h:443
pub const AVS_FW_CFG_MODULES_COUNT: u32 = 12; // messages.h:444
pub const AVS_FW_CFG_MAX_MOD_INST_COUNT: u32 = 13; // messages.h:445
pub const AVS_FW_CFG_MAX_LL_TASKS_PER_PRI_COUNT: u32 = 14; // messages.h:446
pub const AVS_FW_CFG_LL_PRI_COUNT: u32 = 15; // messages.h:447
pub const AVS_FW_CFG_MAX_DP_TASKS_COUNT: u32 = 16; // messages.h:448
pub const AVS_FW_CFG_MAX_LIBS_COUNT: u32 = 17; // messages.h:449
pub const AVS_FW_CFG_SCHEDULER_CONFIG: u32 = 18; // messages.h:450
pub const AVS_FW_CFG_XTAL_FREQ_HZ: u32 = 19; // messages.h:451
pub const AVS_FW_CFG_CLOCKS_CONFIG: u32 = 20; // messages.h:452
pub const AVS_FW_CFG_RESERVED: u32 = 21; // messages.h:453
pub const AVS_FW_CFG_POWER_GATING_POLICY: u32 = 22; // messages.h:454
pub const AVS_FW_CFG_ASSERT_MODE: u32 = 23; // messages.h:455
pub const AVS_FW_CFG_RESERVED2: u32 = 24; // messages.h:456
pub const AVS_FW_CFG_BUS_HARDWARE_ID: u32 = 25; // messages.h:457

/// `struct avs_fw_version` (messages.h:424-429).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FwVersion {
    pub major: u16,
    pub minor: u16,
    pub hotfix: u16,
    pub build: u16,
}

/// `struct avs_fw_cfg` (messages.h:460-479). A field the firmware did not report stays 0, as in Linux.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FwCfg {
    pub fw_version: FwVersion,
    pub memory_reclaimed: u32,
    pub slow_clock_freq_hz: u32,
    pub fast_clock_freq_hz: u32,
    pub alh_support: u32,
    pub ipc_dl_mailbox_bytes: u32,
    pub ipc_ul_mailbox_bytes: u32,
    pub trace_log_bytes: u32,
    pub max_ppl_count: u32,
    pub max_astate_count: u32,
    pub max_module_pin_count: u32,
    pub modules_count: u32,
    pub max_mod_inst_count: u32,
    pub max_ll_tasks_per_pri_count: u32,
    pub ll_pri_count: u32,
    pub max_dp_tasks_count: u32,
    pub max_libs_count: u32,
    pub xtal_freq_hz: u32,
    pub power_gating_policy: u32,
    /// TLVs of a type this port does not know ("Unrecognized fw param", messages.c:487-489).
    pub unrecognized: u32,
}

/// `avs_ipc_get_fw_config`'s parse (messages.c:391-511). An empty payload is `-EREMOTEIO` (messages.c:404-407).
pub fn parse_fw_config(payload: &[u8]) -> Result<FwCfg, IpcError> {
    if payload.is_empty() {
        return Err(IpcError::EmptyPayload { what: "firmware config" });
    }
    let mut c = FwCfg::default();
    for t in tlvs(payload) {
        let t = t?;
        match t.ty {
            AVS_FW_CFG_FW_VERSION => {
                if t.value.len() < 8 {
                    return Err(IpcError::TlvValueTooShort { tlv_type: t.ty, need: 8, have: t.value.len() });
                }
                let h = |i: usize| u16::from_le_bytes([t.value[i], t.value[i + 1]]);
                c.fw_version = FwVersion { major: h(0), minor: h(2), hotfix: h(4), build: h(6) };
            }
            AVS_FW_CFG_MEMORY_RECLAIMED => c.memory_reclaimed = t.dword(0)?,
            AVS_FW_CFG_SLOW_CLOCK_FREQ_HZ => c.slow_clock_freq_hz = t.dword(0)?,
            AVS_FW_CFG_FAST_CLOCK_FREQ_HZ => c.fast_clock_freq_hz = t.dword(0)?,
            AVS_FW_CFG_ALH_SUPPORT_LEVEL => c.alh_support = t.dword(0)?,
            AVS_FW_CFG_IPC_DL_MAILBOX_BYTES => c.ipc_dl_mailbox_bytes = t.dword(0)?,
            AVS_FW_CFG_IPC_UL_MAILBOX_BYTES => c.ipc_ul_mailbox_bytes = t.dword(0)?,
            AVS_FW_CFG_TRACE_LOG_BYTES => c.trace_log_bytes = t.dword(0)?,
            AVS_FW_CFG_MAX_PPL_COUNT => c.max_ppl_count = t.dword(0)?,
            AVS_FW_CFG_MAX_ASTATE_COUNT => c.max_astate_count = t.dword(0)?,
            AVS_FW_CFG_MAX_MODULE_PIN_COUNT => c.max_module_pin_count = t.dword(0)?,
            AVS_FW_CFG_MODULES_COUNT => c.modules_count = t.dword(0)?,
            AVS_FW_CFG_MAX_MOD_INST_COUNT => c.max_mod_inst_count = t.dword(0)?,
            AVS_FW_CFG_MAX_LL_TASKS_PER_PRI_COUNT => c.max_ll_tasks_per_pri_count = t.dword(0)?,
            AVS_FW_CFG_LL_PRI_COUNT => c.ll_pri_count = t.dword(0)?,
            AVS_FW_CFG_MAX_DP_TASKS_COUNT => c.max_dp_tasks_count = t.dword(0)?,
            AVS_FW_CFG_MAX_LIBS_COUNT => c.max_libs_count = t.dword(0)?,
            AVS_FW_CFG_XTAL_FREQ_HZ => c.xtal_freq_hz = t.dword(0)?,
            AVS_FW_CFG_POWER_GATING_POLICY => c.power_gating_policy = t.dword(0)?,
            // "Known but not useful to us" (messages.c:478-483)
            AVS_FW_CFG_DMA_BUFFER_CONFIG | AVS_FW_CFG_SCHEDULER_CONFIG | AVS_FW_CFG_CLOCKS_CONFIG | AVS_FW_CFG_RESERVED => {}
            _ => c.unrecognized += 1,
        }
    }
    Ok(c)
}

// ---- hardware configuration ---------------------------------------------------------------------------------------------------
pub const AVS_HW_CFG_AVS_VER: u32 = 0; // messages.h:492
pub const AVS_HW_CFG_DSP_CORES: u32 = 1; // messages.h:493
pub const AVS_HW_CFG_MEM_PAGE_BYTES: u32 = 2; // messages.h:494
pub const AVS_HW_CFG_TOTAL_PHYS_MEM_PAGES: u32 = 3; // messages.h:495
pub const AVS_HW_CFG_I2S_CAPS: u32 = 4; // messages.h:496
pub const AVS_HW_CFG_GPDMA_CAPS: u32 = 5; // messages.h:497
pub const AVS_HW_CFG_GATEWAY_COUNT: u32 = 6; // messages.h:498
pub const AVS_HW_CFG_HP_EBB_COUNT: u32 = 7; // messages.h:499
pub const AVS_HW_CFG_LP_EBB_COUNT: u32 = 8; // messages.h:500
pub const AVS_HW_CFG_EBB_SIZE_BYTES: u32 = 9; // messages.h:501

/// `enum avs_iface_version` (messages.h:504-507).
pub const AVS_AVS_VER_1_5: u32 = 0x10005; // messages.h:505
pub const AVS_AVS_VER_1_8: u32 = 0x10008; // messages.h:506
/// `enum avs_i2s_version` (messages.h:509-514).
pub const AVS_I2S_VER_15_SKYLAKE: u32 = 0x00000; // messages.h:510
pub const AVS_I2S_VER_15_BROXTON: u32 = 0x10000; // messages.h:511
pub const AVS_I2S_VER_15_BROXTON_P: u32 = 0x20000; // messages.h:512
pub const AVS_I2S_VER_18_KBL_CNL: u32 = 0x30000; // messages.h:513

/// The most I2S controllers a configuration may list. Linux `devm_kmemdup`s however many the firmware states; this port has
/// fixed storage and refuses more (cAVS has at most six SSP ports).
pub const MAX_I2S_CTRL: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct I2sCaps {
    pub i2s_version: u32,
    pub ctrl_count: u32,
    /// `ctrl_base_addr[0..ctrl_count]`.
    pub ctrl_base_addr: [u32; MAX_I2S_CTRL],
}

/// `struct avs_hw_cfg` (messages.h:522-532).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HwCfg {
    pub avs_version: u32,
    pub dsp_cores: u32,
    pub mem_page_bytes: u32,
    pub total_phys_mem_pages: u32,
    pub i2s_caps: I2sCaps,
    pub gateway_count: u32,
    pub hp_ebb_count: u32,
    pub lp_ebb_count: u32,
    pub ebb_size_bytes: u32,
    pub unrecognized: u32,
}

/// `avs_ipc_get_hw_config`'s parse (messages.c:551-642).
pub fn parse_hw_config(payload: &[u8]) -> Result<HwCfg, IpcError> {
    if payload.is_empty() {
        return Err(IpcError::EmptyPayload { what: "hardware config" });
    }
    let mut c = HwCfg::default();
    for t in tlvs(payload) {
        let t = t?;
        match t.ty {
            AVS_HW_CFG_AVS_VER => c.avs_version = t.dword(0)?,
            AVS_HW_CFG_DSP_CORES => c.dsp_cores = t.dword(0)?,
            AVS_HW_CFG_MEM_PAGE_BYTES => c.mem_page_bytes = t.dword(0)?,
            AVS_HW_CFG_TOTAL_PHYS_MEM_PAGES => c.total_phys_mem_pages = t.dword(0)?,
            AVS_HW_CFG_I2S_CAPS => {
                c.i2s_caps.i2s_version = t.dword(0)?;
                let n = t.dword(1)? as usize;
                c.i2s_caps.ctrl_count = n as u32;
                if n > MAX_I2S_CTRL {
                    return Err(IpcError::TooMany { what: "i2s controllers", count: n, max: MAX_I2S_CTRL });
                }
                for i in 0..n {
                    c.i2s_caps.ctrl_base_addr[i] = t.dword(2 + i)?; // refused when the TLV is shorter than it claims
                }
            }
            AVS_HW_CFG_GATEWAY_COUNT => c.gateway_count = t.dword(0)?,
            AVS_HW_CFG_HP_EBB_COUNT => c.hp_ebb_count = t.dword(0)?,
            AVS_HW_CFG_LP_EBB_COUNT => c.lp_ebb_count = t.dword(0)?,
            AVS_HW_CFG_EBB_SIZE_BYTES => c.ebb_size_bytes = t.dword(0)?,
            AVS_HW_CFG_GPDMA_CAPS => {}
            _ => c.unrecognized += 1,
        }
    }
    Ok(c)
}

// ---- modules info -------------------------------------------------------------------------------------------------------------
/// `sizeof(struct avs_module_entry)` (messages.h:589).
pub const MODULE_ENTRY_SIZE: usize = 116; // messages.h:589
pub const AVS_MODULE_LOAD_TYPE_BUILTIN: u32 = 0; // messages.h:536
pub const AVS_MODULE_LOAD_TYPE_LOADABLE: u32 = 1; // messages.h:537
pub const AVS_MODULE_STATE_LOADED: u16 = 0x1; // messages.h:538

/// `union avs_segment_flags` (messages.h:550-565) + `struct avs_segment_desc` (messages.h:567-572).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SegmentDesc {
    pub flags: u32,
    pub v_base_addr: u32,
    pub file_offset: u32,
}

impl SegmentDesc {
    pub const fn contents(&self) -> bool {
        self.flags & 1 != 0
    }
    pub const fn alloc(&self) -> bool {
        self.flags & (1 << 1) != 0
    }
    pub const fn load(&self) -> bool {
        self.flags & (1 << 2) != 0
    }
    pub const fn readonly(&self) -> bool {
        self.flags & (1 << 3) != 0
    }
    pub const fn code(&self) -> bool {
        self.flags & (1 << 4) != 0
    }
    pub const fn data(&self) -> bool {
        self.flags & (1 << 5) != 0
    }
    pub const fn seg_type(&self) -> u32 {
        (self.flags >> 8) & 0xF
    }
    pub const fn length(&self) -> u32 {
        self.flags >> 16
    }
}

/// `struct avs_module_entry` (messages.h:574-589), 116 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleEntry {
    pub module_id: u16,
    pub state_flags: u16,
    pub name: [u8; 8],
    /// `guid_t` bytes as the firmware sent them.
    pub uuid: [u8; 16],
    /// `struct avs_module_type` (messages.h:540-547) as one dword.
    pub type_bits: u32,
    pub hash: [u8; 32],
    pub entry_point: u32,
    pub cfg_offset: u16,
    pub cfg_count: u16,
    pub affinity_mask: u32,
    pub instance_max_count: u16,
    pub instance_bss_size: u16,
    pub segments: [SegmentDesc; 3],
}

impl ModuleEntry {
    pub const fn load_type(&self) -> u32 {
        self.type_bits & 0xF
    }
    pub const fn auto_start(&self) -> bool {
        self.type_bits & (1 << 4) != 0
    }
    pub const fn domain_ll(&self) -> bool {
        self.type_bits & (1 << 5) != 0
    }
    pub const fn domain_dp(&self) -> bool {
        self.type_bits & (1 << 6) != 0
    }
    pub const fn lib_code(&self) -> bool {
        self.type_bits & (1 << 7) != 0
    }
    /// `avs_module_entry_is_loaded` (messages.h:594-601): built into the base firmware, or marked loaded.
    pub const fn is_loaded(&self) -> bool {
        self.load_type() == AVS_MODULE_LOAD_TYPE_BUILTIN || self.state_flags & AVS_MODULE_STATE_LOADED != 0
    }
}

fn rd16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// The table `avs_ipc_get_modules_info` returns: a count, then `count` fixed-size entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModulesInfo<'a> {
    count: u32,
    entries: &'a [u8],
}

/// Validate the table's own claim against the payload (Linux trusts `count`; `avs_get_module_entry` then indexes past the
/// buffer if the firmware lies).
pub fn parse_modules_info(payload: &[u8]) -> Result<ModulesInfo<'_>, IpcError> {
    if payload.is_empty() {
        return Err(IpcError::EmptyPayload { what: "modules info" });
    }
    if payload.len() < 4 {
        return Err(IpcError::Truncated { what: "modules info count", need: 4, have: payload.len() });
    }
    let count = rd32(payload, 0);
    let need = (count as usize).saturating_mul(MODULE_ENTRY_SIZE);
    let have = payload.len() - 4;
    if need > have {
        return Err(IpcError::Truncated { what: "modules info entries", need, have });
    }
    Ok(ModulesInfo { count, entries: &payload[4..4 + need] })
}

impl<'a> ModulesInfo<'a> {
    pub const fn count(&self) -> u32 {
        self.count
    }
    pub fn entry(&self, i: usize) -> Option<ModuleEntry> {
        if i >= self.count as usize {
            return None;
        }
        let e = &self.entries[i * MODULE_ENTRY_SIZE..(i + 1) * MODULE_ENTRY_SIZE];
        let mut name = [0u8; 8];
        name.copy_from_slice(&e[4..12]);
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&e[12..28]);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&e[32..64]);
        let seg = |o: usize| SegmentDesc { flags: rd32(e, o), v_base_addr: rd32(e, o + 4), file_offset: rd32(e, o + 8) };
        Some(ModuleEntry {
            module_id: rd16(e, 0),
            state_flags: rd16(e, 2),
            name,
            uuid,
            type_bits: rd32(e, 28),
            hash,
            entry_point: rd32(e, 64),
            cfg_offset: rd16(e, 68),
            cfg_count: rd16(e, 70),
            affinity_mask: rd32(e, 72),
            instance_max_count: rd16(e, 76),
            instance_bss_size: rd16(e, 78),
            segments: [seg(80), seg(92), seg(104)],
        })
    }
    /// Find a module by its type UUID (`avs_get_module_entry`, utils.c).
    pub fn find_uuid(&self, uuid: &[u8; 16]) -> Option<ModuleEntry> {
        (0..self.count as usize).filter_map(|i| self.entry(i)).find(|e| &e.uuid == uuid)
    }
}
