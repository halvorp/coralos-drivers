// SPDX-License-Identifier: GPL-2.0-only
//! The 64-bit message header: primary dword in bits 0..31, extension dword in bits 32..63 (`val` of `union avs_global_msg`,
//! messages.h:40-96). Linux describes the fields as GCC little-endian bit-fields, first-declared field in the lowest bits; each
//! [`Field`] below carries the line of the declaration it was counted from.
use crate::error::IpcError;

/// One bit-field of a header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    /// Bit offset in the 64-bit header (extension fields are `32 +` their offset in the extension dword).
    pub shift: u32,
    pub width: u32,
}

impl Field {
    pub const fn new(name: &'static str, shift: u32, width: u32) -> Self {
        Self { name, shift, width }
    }
    pub const fn max(&self) -> u64 {
        (1u64 << self.width) - 1
    }
    pub const fn get(&self, header: u64) -> u32 {
        ((header >> self.shift) & self.max()) as u32
    }
    /// Store `value`, refusing one that does not fit (Linux truncates).
    pub fn put(&self, header: &mut u64, value: u64) -> Result<(), IpcError> {
        if value > self.max() {
            return Err(IpcError::FieldOverflow { field: self.name, value, width: self.width });
        }
        *header = (*header & !(self.max() << self.shift)) | (value << self.shift);
        Ok(())
    }
}

const E: u32 = 32; // the extension dword

// ---- common to global, module, reply and notification headers --------------------------------------------------------------
pub const MSG_TYPE: Field = Field::new("msg_type", 24, 5); // messages.h:47, :126, :173, :217
pub const MSG_DIRECTION: Field = Field::new("msg_direction", 29, 1); // messages.h:48, :127, :174, :218
pub const MSG_TARGET: Field = Field::new("msg_target", 30, 1); // messages.h:49, :128, :175, :219

// ---- global (firmware general) messages: primary -----------------------------------------------------------------------------
pub const BOOT_ROM_CTRL_MSG_TYPE: Field = Field::new("boot_cfg.rom_ctrl_msg_type", 0, 9); // messages.h:53
pub const BOOT_DMA_ID: Field = Field::new("boot_cfg.dma_id", 9, 5); // messages.h:54
pub const BOOT_PURGE_REQUEST: Field = Field::new("boot_cfg.purge_request", 14, 1); // messages.h:55
pub const LOAD_MULTI_MOD_CNT: Field = Field::new("load_multi_mods.mod_cnt", 0, 8); // messages.h:59
pub const CREATE_PPL_MEM_SIZE: Field = Field::new("create_ppl.ppl_mem_size", 0, 11); // messages.h:63
pub const CREATE_PPL_PRIORITY: Field = Field::new("create_ppl.ppl_priority", 11, 5); // messages.h:64
pub const CREATE_PPL_INSTANCE_ID: Field = Field::new("create_ppl.instance_id", 16, 8); // messages.h:65
pub const PPL_INSTANCE_ID: Field = Field::new("ppl.instance_id", 16, 8); // messages.h:69 (after rsvd:16, :68)
pub const SET_PPL_STATE: Field = Field::new("set_ppl_state.state", 0, 16); // messages.h:72
pub const SET_PPL_ID: Field = Field::new("set_ppl_state.ppl_id", 16, 8); // messages.h:73
pub const GET_PPL_ID: Field = Field::new("get_ppl_state.ppl_id", 0, 8); // messages.h:76
pub const LOAD_LIB_DMA_ID: Field = Field::new("load_lib.dma_id", 0, 5); // messages.h:80
pub const LOAD_LIB_ID: Field = Field::new("load_lib.lib_id", 16, 4); // messages.h:82 (after rsvd:11, :81)
// global messages: extension (create pipeline only)
pub const CREATE_PPL_LP: Field = Field::new("ext.create_ppl.lp", E, 1); // messages.h:89
pub const CREATE_PPL_ATTRIBUTES: Field = Field::new("ext.create_ppl.attributes", E + 4, 16); // messages.h:91 (after rsvd:3, :90)

// ---- module messages ---------------------------------------------------------------------------------------------------------
pub const MOD_MODULE_ID: Field = Field::new("module_id", 0, 16); // messages.h:124
pub const MOD_INSTANCE_ID: Field = Field::new("instance_id", 16, 8); // messages.h:125
pub const INIT_PARAM_BLOCK_SIZE: Field = Field::new("ext.init_instance.param_block_size", E, 16); // messages.h:134
pub const INIT_PPL_INSTANCE_ID: Field = Field::new("ext.init_instance.ppl_instance_id", E + 16, 8); // messages.h:135
pub const INIT_CORE_ID: Field = Field::new("ext.init_instance.core_id", E + 24, 4); // messages.h:136
pub const INIT_PROC_DOMAIN: Field = Field::new("ext.init_instance.proc_domain", E + 28, 1); // messages.h:137
pub const LARGE_DATA_OFF_SIZE: Field = Field::new("ext.large_config.data_off_size", E, 20); // messages.h:140
pub const LARGE_PARAM_ID: Field = Field::new("ext.large_config.large_param_id", E + 20, 8); // messages.h:141
pub const LARGE_FINAL_BLOCK: Field = Field::new("ext.large_config.final_block", E + 28, 1); // messages.h:142
pub const LARGE_INIT_BLOCK: Field = Field::new("ext.large_config.init_block", E + 29, 1); // messages.h:143
pub const BIND_DST_MODULE_ID: Field = Field::new("ext.bind_unbind.dst_module_id", E, 16); // messages.h:146
pub const BIND_DST_INSTANCE_ID: Field = Field::new("ext.bind_unbind.dst_instance_id", E + 16, 8); // messages.h:147
pub const BIND_DST_QUEUE: Field = Field::new("ext.bind_unbind.dst_queue", E + 24, 3); // messages.h:148
pub const BIND_SRC_QUEUE: Field = Field::new("ext.bind_unbind.src_queue", E + 27, 3); // messages.h:149
pub const D0IX_WAKE: Field = Field::new("ext.set_d0ix.wake", E, 1); // messages.h:153
pub const D0IX_STREAMING: Field = Field::new("ext.set_d0ix.streaming", E + 1, 1); // messages.h:154
pub const D0IX_PREVENT_PG: Field = Field::new("ext.set_d0ix.prevent_pg", E + 2, 1); // messages.h:156
pub const D0IX_PREVENT_LOCAL_CG: Field = Field::new("ext.set_d0ix.prevent_local_cg", E + 3, 1); // messages.h:157

// ---- replies -----------------------------------------------------------------------------------------------------------------
pub const REPLY_STATUS: Field = Field::new("status", 0, 24); // messages.h:172
pub const REPLY_LOAD_ERR_MOD_ID: Field = Field::new("ext.load_multi_mods.err_mod_id", E, 16); // messages.h:182
pub const REPLY_PPL_STATE: Field = Field::new("ext.get_ppl_state.state", E, 5); // messages.h:186
pub const REPLY_LARGE_DATA_OFF_SIZE: Field = Field::new("ext.large_config.data_off_size", E, 20); // messages.h:190
pub const REPLY_LARGE_PARAM_ID: Field = Field::new("ext.large_config.large_param_id", E + 20, 8); // messages.h:191
pub const REPLY_LARGE_FINAL_BLOCK: Field = Field::new("ext.large_config.final_block", E + 28, 1); // messages.h:192
pub const REPLY_LARGE_INIT_BLOCK: Field = Field::new("ext.large_config.init_block", E + 29, 1); // messages.h:193

// ---- notifications -----------------------------------------------------------------------------------------------------------
pub const NOTIFY_TYPE: Field = Field::new("notify_msg_type", 16, 8); // messages.h:216
pub const NOTIFY_LOG_CORE: Field = Field::new("log.core", 12, 4); // messages.h:223 (u16 rsvd:12 then core:4)
pub const NOTIFY_COREDUMP_CORE_ID: Field = Field::new("ext.coredump.core_id", E, 2); // messages.h:229
pub const NOTIFY_COREDUMP_STACK_SIZE: Field = Field::new("ext.coredump.stack_dump_size", E + 2, 16); // messages.h:230

/// `enum avs_msg_target` (messages.h:18-21).
pub const AVS_FW_GEN_MSG: u32 = 0; // messages.h:19
pub const AVS_MOD_MSG: u32 = 1; // messages.h:20
/// `enum avs_msg_direction` (messages.h:23-26).
pub const AVS_MSG_REQUEST: u32 = 0; // messages.h:24
pub const AVS_MSG_REPLY: u32 = 1; // messages.h:25

/// `enum avs_global_msg_type` (messages.h:28-38).
pub const AVS_GLB_ROM_CONTROL: u32 = 1; // messages.h:29
pub const AVS_GLB_LOAD_MULTIPLE_MODULES: u32 = 15; // messages.h:30
pub const AVS_GLB_UNLOAD_MULTIPLE_MODULES: u32 = 16; // messages.h:31
pub const AVS_GLB_CREATE_PIPELINE: u32 = 17; // messages.h:32
pub const AVS_GLB_DELETE_PIPELINE: u32 = 18; // messages.h:33
pub const AVS_GLB_SET_PIPELINE_STATE: u32 = 19; // messages.h:34
pub const AVS_GLB_GET_PIPELINE_STATE: u32 = 20; // messages.h:35
pub const AVS_GLB_LOAD_LIBRARY: u32 = 24; // messages.h:36
pub const AVS_GLB_NOTIFICATION: u32 = 27; // messages.h:37

/// `enum avs_module_msg_type` (messages.h:107-116).
pub const AVS_MOD_INIT_INSTANCE: u32 = 0; // messages.h:108
pub const AVS_MOD_LARGE_CONFIG_GET: u32 = 3; // messages.h:109
pub const AVS_MOD_LARGE_CONFIG_SET: u32 = 4; // messages.h:110
pub const AVS_MOD_BIND: u32 = 5; // messages.h:111
pub const AVS_MOD_UNBIND: u32 = 6; // messages.h:112
pub const AVS_MOD_SET_DX: u32 = 7; // messages.h:113
pub const AVS_MOD_SET_D0IX: u32 = 8; // messages.h:114
pub const AVS_MOD_DELETE_INSTANCE: u32 = 11; // messages.h:115

/// `enum avs_rom_control_msg_type` (messages.h:294-296).
pub const AVS_ROM_SET_BOOT_CONFIG: u32 = 0; // messages.h:295

/// `enum avs_pipeline_state` (messages.h:306-312).
pub const AVS_PPL_STATE_INVALID: u32 = 0; // messages.h:307
pub const AVS_PPL_STATE_UNINITIALIZED: u32 = 1; // messages.h:308
pub const AVS_PPL_STATE_RESET: u32 = 2; // messages.h:309
pub const AVS_PPL_STATE_PAUSED: u32 = 3; // messages.h:310
pub const AVS_PPL_STATE_RUNNING: u32 = 4; // messages.h:311

/// `avs_msg_is_reply(hdr)` (messages.h:261-266): a reply is direction REPLY and not a notification. Linux reads the
/// message type through `avs_reply_msg`, whose type field is the same five bits as every other header's.
pub const fn is_reply(header: u64) -> bool {
    MSG_DIRECTION.get(header) == AVS_MSG_REPLY && MSG_TYPE.get(header) != AVS_GLB_NOTIFICATION
}

/// The header of a global request: `AVS_GLOBAL_REQUEST(msg_type)` (messages.h:239-244). Refuses a type that does not fit the
/// five-bit field.
pub fn global_request(msg_type: u32) -> Result<u64, IpcError> {
    let mut h = 0;
    MSG_TYPE.put(&mut h, msg_type as u64)?;
    MSG_DIRECTION.put(&mut h, AVS_MSG_REQUEST as u64)?;
    MSG_TARGET.put(&mut h, AVS_FW_GEN_MSG as u64)?;
    Ok(h)
}

/// The header of a module request: `AVS_MODULE_REQUEST(msg_type)` (messages.h:246-251).
pub fn module_request(msg_type: u32) -> Result<u64, IpcError> {
    let mut h = 0;
    MSG_TYPE.put(&mut h, msg_type as u64)?;
    MSG_DIRECTION.put(&mut h, AVS_MSG_REQUEST as u64)?;
    MSG_TARGET.put(&mut h, AVS_MOD_MSG as u64)?;
    Ok(h)
}

/// The header of a notification: `AVS_NOTIFICATION(msg_type)` (messages.h:253-259).
pub fn notification(notify_type: u32) -> Result<u64, IpcError> {
    let mut h = 0;
    NOTIFY_TYPE.put(&mut h, notify_type as u64)?;
    MSG_TYPE.put(&mut h, AVS_GLB_NOTIFICATION as u64)?;
    MSG_DIRECTION.put(&mut h, AVS_MSG_REPLY as u64)?;
    MSG_TARGET.put(&mut h, AVS_FW_GEN_MSG as u64)?;
    Ok(h)
}

/// Primary dword of a header (written to `req_offset`).
pub const fn primary(header: u64) -> u32 {
    header as u32
}
/// Extension dword of a header (written to `req_ext_offset`).
pub const fn ext(header: u64) -> u32 {
    (header >> 32) as u32
}
