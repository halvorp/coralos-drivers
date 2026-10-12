// SPDX-License-Identifier: GPL-2.0-only
//! Every refusal has a NAME (PORT_CONVENTIONS.md section 5).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcError {
    /// A value does not fit its bit-field. Linux truncates it silently (a C bit-field assignment).
    FieldOverflow { field: &'static str, value: u64, width: u32 },
    /// A request payload is larger than the 4 KiB mailbox window (`AVS_MAILBOX_SIZE`, messages.h:16). Linux copies it anyway.
    PayloadTooBig { len: usize, max: usize },
    /// The IPC channel is not ready: no FW_READY yet, or the DSP raised an exception (`-EPERM`, ipc.c:327, :397).
    NotReady,
    /// Another transaction is in flight. Linux sleeps on `msg_mutex`; a planner cannot, so the second sender is refused.
    TransactionInFlight,
    /// The caller answered with something the last question did not ask for, or asked for a step out of order.
    Protocol,
    /// No answer within the budget (ipc.c:332, :352, `-ETIMEDOUT`). `exception` is true when Linux treats it as a DSP crash
    /// and starts recovery (ipc.c:409-413).
    Timeout { exception: bool },
    /// A TLV or table is cut short: the payload ends inside what its own header says it holds.
    Truncated { what: &'static str, need: usize, have: usize },
    /// The firmware returned a zero-length payload for a request whose reply must have one (`-EREMOTEIO`, messages.c:366, :544).
    EmptyPayload { what: &'static str },
    /// A TLV value is shorter than the scalar it carries.
    TlvValueTooShort { tlv_type: u32, need: usize, have: usize },
    /// A table could not grow (DECISIONS.md D10); raised before anything is half-built.
    OutOfMemory { what: &'static str },
    /// A large-config total does not fit the 20-bit `data_off_size`.
    LargeConfigTooBig { total: usize },
    /// The ROM-message path could not un-stall the main core (ipc.c:515).
    UnstallFailed,
    /// A notification's payload would not fit the mailbox.
    NotifyPayloadTooBig { size: usize, max: usize },
}

impl IpcError {
    pub const fn name(&self) -> &'static str {
        match self {
            IpcError::FieldOverflow { .. } => "field-overflow",
            IpcError::PayloadTooBig { .. } => "payload-too-big",
            IpcError::NotReady => "not-ready",
            IpcError::TransactionInFlight => "transaction-in-flight",
            IpcError::Protocol => "protocol",
            IpcError::Timeout { .. } => "timeout",
            IpcError::Truncated { .. } => "truncated",
            IpcError::EmptyPayload { .. } => "empty-payload",
            IpcError::TlvValueTooShort { .. } => "tlv-value-too-short",
            IpcError::OutOfMemory { .. } => "out-of-memory",
            IpcError::LargeConfigTooBig { .. } => "large-config-too-big",
            IpcError::UnstallFailed => "unstall-failed",
            IpcError::NotifyPayloadTooBig { .. } => "notify-payload-too-big",
        }
    }
}
