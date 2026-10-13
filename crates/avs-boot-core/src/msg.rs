// SPDX-License-Identifier: GPL-2.0-only
//! An IPC message the orchestrator asks the `.device` to run: an OWNED copy of an [`avs_ipc_core::request::Request`].
use alloc::vec::Vec;
use avs_ipc_core::error::IpcError;
use avs_ipc_core::regs::AVS_MAILBOX_SIZE;
use avs_ipc_core::request::{Flow, Payload, Request};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpcMsg {
    /// The request header without the BUSY bit ([`Request::header`]).
    pub header: u64,
    pub payload: Vec<u8>,
    pub timeout_ms: u32,
    pub flow: Flow,
    /// The name Linux logs on failure ("create pipeline", ...): also the step name of a failure.
    pub name: &'static str,
    /// How many reply-payload bytes to reserve (0 for none; the mailbox size for a LARGE_CONFIG_GET).
    pub expected_reply: usize,
}

impl IpcMsg {
    pub fn from_request(r: &Request<'_>, expected_reply: usize) -> Result<Self, IpcError> {
        let bytes = r.payload.as_bytes();
        let mut payload = Vec::new();
        payload.try_reserve_exact(bytes.len()).map_err(|_| IpcError::OutOfMemory { what: "ipc payload copy" })?;
        payload.extend_from_slice(bytes);
        Ok(Self { header: r.header, payload, timeout_ms: r.timeout_ms, flow: r.flow, name: r.name, expected_reply })
    }

    /// The request to hand to [`avs_ipc_core::txn::Ipc::begin`].
    pub fn as_request(&self) -> Request<'_> {
        let payload = if self.payload.is_empty() { Payload::None } else { Payload::Borrowed(&self.payload) };
        Request { header: self.header, payload, timeout_ms: self.timeout_ms, flow: self.flow, name: self.name }
    }

    /// A reply payload bound for the mailbox window.
    pub const fn reply_limit() -> usize {
        AVS_MAILBOX_SIZE
    }
}
