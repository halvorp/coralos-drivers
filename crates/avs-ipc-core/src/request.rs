// SPDX-License-Identifier: GPL-2.0-only
//! Every request builder of `messages.c` (and the pure part of `avs_dsp_send_tx`): the 64-bit header, the payload that goes
//! to the downlink mailbox, and the wait budget Linux gives it. Builders refuse a value that does not fit its bit-field (Linux
//! truncates) and a payload larger than the mailbox (Linux copies it anyway).
use crate::error::IpcError;
use crate::header::*;
use crate::regs::*;

/// The bytes that follow the header into the downlink window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payload<'a> {
    None,
    /// Up to 8 bytes held by the request itself (`avs_dxstate_info`, one module id).
    Inline { bytes: [u8; 8], len: u8 },
    Borrowed(&'a [u8]),
}

impl<'a> Payload<'a> {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Payload::None => &[],
            Payload::Inline { bytes, len } => &bytes[..*len as usize],
            Payload::Borrowed(b) => b,
        }
    }
}

/// How the request reaches the DSP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// `avs_dsp_send_msg*`: wait for the reply (`busy_completion`), needs the channel ready.
    Msg,
    /// `avs_dsp_send_rom_msg*` (ipc.c:498): sent while the core is stalled, then the core is un-stalled and the wait is
    /// for the DSP's ACK (`done_completion`); needs no FW_READY (there is no firmware yet). Never used on the Kaby Lake CLDMA
    /// path; used by Apollo Lake's host-DMA boot.
    Rom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub header: u64,
    pub payload: Payload<'a>,
    pub timeout_ms: u32,
    pub flow: Flow,
    /// The name Linux logs on failure ("set boot config", ...).
    pub name: &'static str,
}

impl<'a> Request<'a> {
    /// What `avs_dsp_send_tx` puts on the wire: the header with the request-BUSY bit set (ipc.c:377).
    /// The busy mask is in the LOW dword (`SKL_ADSP_HIPCI_BUSY`, bit 31 of the primary).
    pub const fn wire_header(&self) -> u64 {
        self.header | SKL_REQ_BUSY_MASK as u64
    }

    fn check(self) -> Result<Self, IpcError> {
        let n = self.payload.as_bytes().len();
        if n > AVS_MAILBOX_SIZE {
            return Err(IpcError::PayloadTooBig { len: n, max: AVS_MAILBOX_SIZE });
        }
        Ok(self)
    }
}

fn msg<'a>(header: u64, payload: Payload<'a>, timeout_ms: u32, flow: Flow, name: &'static str) -> Request<'a> {
    Request { header, payload, timeout_ms, flow, name }
}

/// `avs_ipc_set_boot_config` (messages.c:15): a ROM control message naming the host DMA stream and whether to purge.
pub fn set_boot_config(dma_id: u32, purge: bool) -> Result<Request<'static>, IpcError> {
    let mut h = global_request(AVS_GLB_ROM_CONTROL)?;
    BOOT_ROM_CTRL_MSG_TYPE.put(&mut h, AVS_ROM_SET_BOOT_CONFIG as u64)?;
    BOOT_DMA_ID.put(&mut h, dma_id as u64)?;
    BOOT_PURGE_REQUEST.put(&mut h, purge as u64)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Rom, "set boot config"))
}

fn mod_ids_payload<'a>(ids: &[u16], buf: &'a mut [u8]) -> Result<Payload<'a>, IpcError> {
    let need = ids.len() * 2;
    if buf.len() < need {
        return Err(IpcError::PayloadTooBig { len: need, max: buf.len() });
    }
    for (i, id) in ids.iter().enumerate() {
        buf[2 * i..2 * i + 2].copy_from_slice(&id.to_le_bytes());
    }
    Ok(Payload::Borrowed(&buf[..need]))
}

/// `avs_ipc_load_modules` (messages.c:28): LOAD_MULTIPLE_MODULES with the module ids as the payload, 5 s budget.
pub fn load_modules<'a>(ids: &[u16], buf: &'a mut [u8]) -> Result<Request<'a>, IpcError> {
    let mut h = global_request(AVS_GLB_LOAD_MULTIPLE_MODULES)?;
    LOAD_MULTI_MOD_CNT.put(&mut h, ids.len() as u64)?;
    let p = mod_ids_payload(ids, buf)?;
    msg(h, p, AVS_CL_TIMEOUT_MS, Flow::Msg, "load multiple modules").check()
}

/// `avs_ipc_load_modules(adev, &module_id, 1)` as `avs_cldma_load_module` calls it (loader.c:238): one module, no buffer needed.
pub fn load_module(module_id: u16) -> Result<Request<'static>, IpcError> {
    let mut h = global_request(AVS_GLB_LOAD_MULTIPLE_MODULES)?;
    LOAD_MULTI_MOD_CNT.put(&mut h, 1)?;
    let mut bytes = [0u8; 8];
    bytes[..2].copy_from_slice(&module_id.to_le_bytes());
    Ok(msg(h, Payload::Inline { bytes, len: 2 }, AVS_CL_TIMEOUT_MS, Flow::Msg, "load multiple modules"))
}

/// `avs_ipc_unload_modules` (messages.c:42): the same shape with the default budget.
pub fn unload_modules<'a>(ids: &[u16], buf: &'a mut [u8]) -> Result<Request<'a>, IpcError> {
    let mut h = global_request(AVS_GLB_UNLOAD_MULTIPLE_MODULES)?;
    LOAD_MULTI_MOD_CNT.put(&mut h, ids.len() as u64)?;
    let p = mod_ids_payload(ids, buf)?;
    msg(h, p, AVS_IPC_TIMEOUT_MS, Flow::Msg, "unload multiple modules").check()
}

/// `avs_ipc_load_library` (messages.c:55): LOAD_LIBRARY, 5 s budget.
pub fn load_library(dma_id: u32, lib_id: u32) -> Result<Request<'static>, IpcError> {
    let mut h = global_request(AVS_GLB_LOAD_LIBRARY)?;
    LOAD_LIB_DMA_ID.put(&mut h, dma_id as u64)?;
    LOAD_LIB_ID.put(&mut h, lib_id as u64)?;
    Ok(msg(h, Payload::None, AVS_CL_TIMEOUT_MS, Flow::Msg, "load library"))
}

/// `avs_ipc_create_pipeline` (messages.c:67).
pub fn create_pipeline(req_size: u16, priority: u8, instance_id: u8, lp: bool, attributes: u16) -> Result<Request<'static>, IpcError> {
    let mut h = global_request(AVS_GLB_CREATE_PIPELINE)?;
    CREATE_PPL_MEM_SIZE.put(&mut h, req_size as u64)?;
    CREATE_PPL_PRIORITY.put(&mut h, priority as u64)?;
    CREATE_PPL_INSTANCE_ID.put(&mut h, instance_id as u64)?;
    CREATE_PPL_LP.put(&mut h, lp as u64)?;
    CREATE_PPL_ATTRIBUTES.put(&mut h, attributes as u64)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Msg, "create pipeline"))
}

/// `avs_ipc_delete_pipeline` (messages.c:83).
pub fn delete_pipeline(instance_id: u8) -> Result<Request<'static>, IpcError> {
    let mut h = global_request(AVS_GLB_DELETE_PIPELINE)?;
    PPL_INSTANCE_ID.put(&mut h, instance_id as u64)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Msg, "delete pipeline"))
}

/// `avs_ipc_set_pipeline_state` (messages.c:94). `state` is an `AVS_PPL_STATE_*`.
pub fn set_pipeline_state(instance_id: u8, state: u32) -> Result<Request<'static>, IpcError> {
    let mut h = global_request(AVS_GLB_SET_PIPELINE_STATE)?;
    SET_PPL_ID.put(&mut h, instance_id as u64)?;
    SET_PPL_STATE.put(&mut h, state as u64)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Msg, "set pipeline state"))
}

/// `avs_ipc_get_pipeline_state` (messages.c:107): the state comes back in the reply's extension ([`crate::txn::Completed::ppl_state`]).
pub fn get_pipeline_state(instance_id: u8) -> Result<Request<'static>, IpcError> {
    let mut h = global_request(AVS_GLB_GET_PIPELINE_STATE)?;
    GET_PPL_ID.put(&mut h, instance_id as u64)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Msg, "get pipeline state"))
}

/// `avs_ipc_init_instance` (messages.c:142). "Firmware expects size provided in dwords": `param_block_size` is
/// `DIV_ROUND_UP(param_size, 4)`. `param` is the module-type specific configuration.
pub fn init_instance<'a>(module_id: u16, instance_id: u8, ppl_id: u8, core_id: u8, domain: u8, param: &'a [u8]) -> Result<Request<'a>, IpcError> {
    let mut h = module_request(AVS_MOD_INIT_INSTANCE)?;
    MOD_MODULE_ID.put(&mut h, module_id as u64)?;
    MOD_INSTANCE_ID.put(&mut h, instance_id as u64)?;
    INIT_PARAM_BLOCK_SIZE.put(&mut h, param.len().div_ceil(4) as u64)?;
    INIT_PPL_INSTANCE_ID.put(&mut h, ppl_id as u64)?;
    INIT_CORE_ID.put(&mut h, core_id as u64)?;
    INIT_PROC_DOMAIN.put(&mut h, domain as u64)?;
    msg(h, Payload::Borrowed(param), AVS_IPC_TIMEOUT_MS, Flow::Msg, "init instance").check()
}

/// `avs_ipc_delete_instance` (messages.c:178).
pub fn delete_instance(module_id: u16, instance_id: u8) -> Result<Request<'static>, IpcError> {
    let mut h = module_request(AVS_MOD_DELETE_INSTANCE)?;
    MOD_MODULE_ID.put(&mut h, module_id as u64)?;
    MOD_INSTANCE_ID.put(&mut h, instance_id as u64)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Msg, "delete instance"))
}

fn bind_header(msg_type: u32, module_id: u16, instance_id: u8, dst_module_id: u16, dst_instance_id: u8, dst_queue: u8, src_queue: u8) -> Result<u64, IpcError> {
    let mut h = module_request(msg_type)?;
    MOD_MODULE_ID.put(&mut h, module_id as u64)?;
    MOD_INSTANCE_ID.put(&mut h, instance_id as u64)?;
    BIND_DST_MODULE_ID.put(&mut h, dst_module_id as u64)?;
    BIND_DST_INSTANCE_ID.put(&mut h, dst_instance_id as u64)?;
    BIND_DST_QUEUE.put(&mut h, dst_queue as u64)?;
    BIND_SRC_QUEUE.put(&mut h, src_queue as u64)?;
    Ok(h)
}

/// `avs_ipc_bind` (messages.c:201): bind source pin `src_queue` to sink pin `dst_queue`.
pub fn bind(module_id: u16, instance_id: u8, dst_module_id: u16, dst_instance_id: u8, dst_queue: u8, src_queue: u8) -> Result<Request<'static>, IpcError> {
    let h = bind_header(AVS_MOD_BIND, module_id, instance_id, dst_module_id, dst_instance_id, dst_queue, src_queue)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Msg, "bind modules"))
}

/// `avs_ipc_unbind` (messages.c:230).
pub fn unbind(module_id: u16, instance_id: u8, dst_module_id: u16, dst_instance_id: u8, dst_queue: u8, src_queue: u8) -> Result<Request<'static>, IpcError> {
    let h = bind_header(AVS_MOD_UNBIND, module_id, instance_id, dst_module_id, dst_instance_id, dst_queue, src_queue)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Msg, "unbind modules"))
}

/// `avs_ipc_set_dx` (messages.c:356): the payload is `struct avs_dxstate_info { core_mask; dx_mask }` and the request is
/// sent through the power-management path (wakes the DSP from D0ix first; a no-op on cAVS 1.5).
pub fn set_dx(core_mask: u32, powerup: bool) -> Result<Request<'static>, IpcError> {
    let h = module_request(AVS_MOD_SET_DX)?;
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&core_mask.to_le_bytes());
    bytes[4..].copy_from_slice(&(if powerup { core_mask } else { 0 }).to_le_bytes());
    Ok(msg(h, Payload::Inline { bytes, len: 8 }, AVS_IPC_TIMEOUT_MS, Flow::Msg, "set dx"))
}

/// `avs_ipc_set_d0ix` (messages.c:377). Unsupported by cAVS 1.5 (skl.c:159-163 makes the op a no-op); the message exists
/// for the later families.
pub fn set_d0ix(enable_pg: bool, streaming: bool) -> Result<Request<'static>, IpcError> {
    let mut h = module_request(AVS_MOD_SET_D0IX)?;
    D0IX_WAKE.put(&mut h, enable_pg as u64)?;
    D0IX_STREAMING.put(&mut h, streaming as u64)?;
    D0IX_PREVENT_PG.put(&mut h, !enable_pg as u64)?;
    Ok(msg(h, Payload::None, AVS_IPC_TIMEOUT_MS, Flow::Msg, "set d0ix"))
}

// ---- large config -------------------------------------------------------------------------------------------------------------

/// Header of a LARGE_CONFIG_SET block (`__avs_ipc_set_large_config`, messages.c:248).
pub fn large_config_set_block(module_id: u16, instance_id: u8, param_id: u8, init_block: bool, final_block: bool, off_size: usize) -> Result<u64, IpcError> {
    let mut h = module_request(AVS_MOD_LARGE_CONFIG_SET)?;
    MOD_MODULE_ID.put(&mut h, module_id as u64)?;
    MOD_INSTANCE_ID.put(&mut h, instance_id as u64)?;
    LARGE_DATA_OFF_SIZE.put(&mut h, off_size as u64)?;
    LARGE_PARAM_ID.put(&mut h, param_id as u64)?;
    LARGE_FINAL_BLOCK.put(&mut h, final_block as u64)?;
    LARGE_INIT_BLOCK.put(&mut h, init_block as u64)?;
    Ok(h)
}

/// One block of a large-config SET.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LargeBlock {
    pub header: u64,
    /// Where this block's payload starts in the caller's buffer and how long it is.
    pub offset: usize,
    pub len: usize,
}

/// `avs_ipc_set_large_config` (messages.c:269) as an iterator of blocks. The first block states the TOTAL size in
/// `data_off_size` and has `init_block`; each later block states its OFFSET; `final_block` is set on the block that ends the
/// payload. A payload of `AVS_MAILBOX_SIZE` bytes or less is exactly one block, an empty one is one empty block.
///
/// Refused, where Linux truncates: a total that does not fit the 20-bit size field.
#[derive(Clone, Copy, Debug)]
pub struct LargeSet {
    module_id: u16,
    instance_id: u8,
    param_id: u8,
    total: usize,
    sent: usize,
    started: bool,
}

impl LargeSet {
    pub fn new(module_id: u16, instance_id: u8, param_id: u8, total: usize) -> Result<Self, IpcError> {
        if total as u64 > LARGE_DATA_OFF_SIZE.max() {
            return Err(IpcError::LargeConfigTooBig { total });
        }
        Ok(Self { module_id, instance_id, param_id, total, sent: 0, started: false })
    }
}

impl Iterator for LargeSet {
    type Item = Result<LargeBlock, IpcError>;
    fn next(&mut self) -> Option<Self::Item> {
        let remaining = self.total - self.sent;
        if self.started && remaining == 0 {
            return None;
        }
        let tx = remaining.min(AVS_MAILBOX_SIZE);
        let final_block = tx == remaining;
        let (init, off_size) = if !self.started { (true, self.total) } else { (false, self.sent) };
        let r = large_config_set_block(self.module_id, self.instance_id, self.param_id, init, final_block, off_size);
        let offset = self.sent;
        self.started = true;
        self.sent += tx;
        Some(r.map(|header| LargeBlock { header, offset, len: tx }))
    }
}

/// `avs_ipc_get_large_config` (messages.c:311): the request states how many bytes of parameters follow in
/// `data_off_size`, `init_block` set, `final_block` clear ("always 0 on request. Updated by fw on reply"). The reply may
/// carry up to `AVS_MAILBOX_SIZE` bytes.
pub fn get_large_config<'a>(module_id: u16, instance_id: u8, param_id: u8, request: &'a [u8]) -> Result<Request<'a>, IpcError> {
    let mut h = module_request(AVS_MOD_LARGE_CONFIG_GET)?;
    MOD_MODULE_ID.put(&mut h, module_id as u64)?;
    MOD_INSTANCE_ID.put(&mut h, instance_id as u64)?;
    LARGE_DATA_OFF_SIZE.put(&mut h, request.len() as u64)?;
    LARGE_PARAM_ID.put(&mut h, param_id as u64)?;
    LARGE_FINAL_BLOCK.put(&mut h, 0)?;
    LARGE_INIT_BLOCK.put(&mut h, 1)?;
    msg(h, Payload::Borrowed(request), AVS_IPC_TIMEOUT_MS, Flow::Msg, "large config get").check()
}

/// `AVS_BASEFW_MOD_ID` / `AVS_BASEFW_INST_ID` (messages.h:352-353).
pub const AVS_BASEFW_MOD_ID: u16 = 0; // messages.h:352
pub const AVS_BASEFW_INST_ID: u8 = 0; // messages.h:353
/// `enum avs_basefw_runtime_param` (messages.h:355-362).
pub const AVS_BASEFW_ENABLE_LOGS: u8 = 6; // messages.h:356
pub const AVS_BASEFW_FIRMWARE_CONFIG: u8 = 7; // messages.h:357
pub const AVS_BASEFW_HARDWARE_CONFIG: u8 = 8; // messages.h:358
pub const AVS_BASEFW_MODULES_INFO: u8 = 9; // messages.h:359
pub const AVS_BASEFW_LIBRARIES_INFO: u8 = 16; // messages.h:360
pub const AVS_BASEFW_SYSTEM_TIME: u8 = 20; // messages.h:361

/// `avs_ipc_get_fw_config` request (messages.c:391): FIRMWARE_CONFIG of the base firmware.
pub fn get_fw_config() -> Result<Request<'static>, IpcError> {
    get_large_config(AVS_BASEFW_MOD_ID, AVS_BASEFW_INST_ID, AVS_BASEFW_FIRMWARE_CONFIG, &[])
}
/// `avs_ipc_get_hw_config` request (messages.c:551): HARDWARE_CONFIG of the base firmware.
pub fn get_hw_config() -> Result<Request<'static>, IpcError> {
    get_large_config(AVS_BASEFW_MOD_ID, AVS_BASEFW_INST_ID, AVS_BASEFW_HARDWARE_CONFIG, &[])
}
/// `avs_ipc_get_modules_info` request (messages.c:644): MODULES_INFO of the base firmware.
pub fn get_modules_info() -> Result<Request<'static>, IpcError> {
    get_large_config(AVS_BASEFW_MOD_ID, AVS_BASEFW_INST_ID, AVS_BASEFW_MODULES_INFO, &[])
}

/// `avs_ipc_set_system_time` payload (messages.c:813): UTC microseconds, low dword then high dword.
pub fn system_time_payload(us: u64) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&(us as u32).to_le_bytes());
    b[4..].copy_from_slice(&((us >> 32) as u32).to_le_bytes());
    b
}
