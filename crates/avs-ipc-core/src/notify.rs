// SPDX-License-Identifier: GPL-2.0-only
//! Replies and notifications: how an incoming 64-bit header is read (`union avs_reply_msg`, `union avs_notify_msg`,
//! messages.h:166-235) and what `avs_dsp_receive_rx` / `avs_dsp_process_notification` (ipc.c:185-285) decide from it.
use crate::error::IpcError;
use crate::header::*;
use crate::regs::*;

/// A reply to a request (`avs_reply_msg`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reply {
    pub header: u64,
}

impl Reply {
    pub const fn new(header: u64) -> Self {
        Self { header }
    }
    /// 0 is success; anything else is the firmware's error code (`AVS_IPC_RET` maps a positive one to `-EREMOTEIO`).
    pub const fn status(&self) -> u32 {
        REPLY_STATUS.get(self.header)
    }
    /// The message-type field (global or module numbering, by [`Reply::target`]).
    pub const fn msg_type(&self) -> u32 {
        MSG_TYPE.get(self.header)
    }
    pub const fn target(&self) -> u32 {
        MSG_TARGET.get(self.header)
    }
    /// `reply.rsp.ext.get_ppl_state.state` (messages.c:133).
    pub const fn ppl_state(&self) -> u32 {
        REPLY_PPL_STATE.get(self.header)
    }
    /// `ext.load_multi_mods.err_mod_id`: the module a failed LOAD_MULTIPLE_MODULES blames.
    pub const fn err_mod_id(&self) -> u32 {
        REPLY_LOAD_ERR_MOD_ID.get(self.header)
    }
    pub const fn large_data_off_size(&self) -> u32 {
        REPLY_LARGE_DATA_OFF_SIZE.get(self.header)
    }
    pub const fn large_final_block(&self) -> bool {
        REPLY_LARGE_FINAL_BLOCK.get(self.header) != 0
    }
    pub const fn large_init_block(&self) -> bool {
        REPLY_LARGE_INIT_BLOCK.get(self.header) != 0
    }
    /// True for the one status Linux does not log as an error (`AVS_IPC_NOT_SUPPORTED`, ipc.c:425).
    pub const fn is_not_supported(&self) -> bool {
        self.status() == AVS_IPC_NOT_SUPPORTED
    }
    /// How many payload bytes `avs_dsp_receive_rx` copies out of the uplink window (ipc.c:185-207), given the `expected`
    /// size the sender reserved. A failed request carries no payload (`if (!msg.status)`); a LARGE_CONFIG_GET reply states its
    /// own size, capped at the mailbox.
    pub const fn payload_len(&self, expected: usize) -> usize {
        if self.status() != 0 {
            return 0;
        }
        if self.target() == AVS_MOD_MSG && self.msg_type() == AVS_MOD_LARGE_CONFIG_GET {
            let n = self.large_data_off_size() as usize;
            return if n < AVS_MAILBOX_SIZE { n } else { AVS_MAILBOX_SIZE };
        }
        expected
    }
}

/// `enum avs_notify_msg_type` (messages.h:200-207).
pub const AVS_NOTIFY_PHRASE_DETECTED: u32 = 4; // messages.h:201
pub const AVS_NOTIFY_RESOURCE_EVENT: u32 = 5; // messages.h:202
pub const AVS_NOTIFY_LOG_BUFFER_STATUS: u32 = 6; // messages.h:203
pub const AVS_NOTIFY_FW_READY: u32 = 8; // messages.h:204
pub const AVS_NOTIFY_EXCEPTION_CAUGHT: u32 = 10; // messages.h:205
pub const AVS_NOTIFY_MODULE_EVENT: u32 = 12; // messages.h:206

/// Fixed notification payload sizes: `sizeof(struct avs_notify_voice_data)` (messages.h:274), `avs_notify_res_data`
/// (messages.h:283), and the 12-byte `avs_notify_mod_data` header (messages.h:291).
pub const NOTIFY_VOICE_DATA_SIZE: usize = 4; // messages.h:274
pub const NOTIFY_RES_DATA_SIZE: usize = 40; // messages.h:283
pub const NOTIFY_MOD_DATA_SIZE: usize = 12; // messages.h:291

/// What a notification is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyKind {
    FwReady,
    PhraseDetected,
    ResourceEvent,
    LogBufferStatus,
    ExceptionCaught,
    ModuleEvent,
    /// Linux: `dev_info("unknown notification: 0x%08x")`, no payload, no action (ipc.c:244-247).
    Unknown(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notify {
    pub header: u64,
}

impl Notify {
    pub const fn new(header: u64) -> Self {
        Self { header }
    }
    pub const fn type_raw(&self) -> u32 {
        NOTIFY_TYPE.get(self.header)
    }
    pub const fn kind(&self) -> NotifyKind {
        match self.type_raw() {
            AVS_NOTIFY_FW_READY => NotifyKind::FwReady,
            AVS_NOTIFY_PHRASE_DETECTED => NotifyKind::PhraseDetected,
            AVS_NOTIFY_RESOURCE_EVENT => NotifyKind::ResourceEvent,
            AVS_NOTIFY_LOG_BUFFER_STATUS => NotifyKind::LogBufferStatus,
            AVS_NOTIFY_EXCEPTION_CAUGHT => NotifyKind::ExceptionCaught,
            AVS_NOTIFY_MODULE_EVENT => NotifyKind::ModuleEvent,
            t => NotifyKind::Unknown(t),
        }
    }
    /// The DSP core a LOG_BUFFER_STATUS notification names (`msg.log.core`).
    pub const fn log_core(&self) -> u32 {
        NOTIFY_LOG_CORE.get(self.header)
    }
    /// `msg.ext.coredump.core_id` and `stack_dump_size` of an EXCEPTION_CAUGHT.
    pub const fn coredump_core_id(&self) -> u32 {
        NOTIFY_COREDUMP_CORE_ID.get(self.header)
    }
    pub const fn coredump_stack_size(&self) -> u32 {
        NOTIFY_COREDUMP_STACK_SIZE.get(self.header)
    }

    /// The payload size `avs_dsp_process_notification` reads from the uplink window (ipc.c:228-247). For MODULE_EVENT the total
    /// is `12 + data_size` where `data_size` is the third dword of the payload header (`module_data_size`), which the caller
    /// reads first. Refused where Linux allocates whatever the firmware says: a total beyond the mailbox.
    pub fn payload_size(&self, module_data_size: u32) -> Result<usize, IpcError> {
        let n = match self.kind() {
            NotifyKind::PhraseDetected => NOTIFY_VOICE_DATA_SIZE,
            NotifyKind::ResourceEvent => NOTIFY_RES_DATA_SIZE,
            NotifyKind::ModuleEvent => NOTIFY_MOD_DATA_SIZE + module_data_size as usize,
            NotifyKind::FwReady | NotifyKind::LogBufferStatus | NotifyKind::ExceptionCaught | NotifyKind::Unknown(_) => 0,
        };
        if n > AVS_MAILBOX_SIZE {
            return Err(IpcError::NotifyPayloadTooBig { size: n, max: AVS_MAILBOX_SIZE });
        }
        Ok(n)
    }
}
