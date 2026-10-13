// SPDX-License-Identifier: GPL-2.0-only
//! The i915.device wire protocol, version 1, as far as this skeleton speaks it: every operation is answered, and every
//! answer but HELLO's is an explicit UNSUPPORTED. A client therefore learns "this device cannot do that yet" from the
//! device, in a frame it can match on, instead of from a timeout.
//!
//!   request  `[ REQ_MARKER 'I', version, op, payload… ]`
//!   reply    `[ REP_MARKER 'i', version, op, status ]`      (the op is echoed so replies match requests)
//!
//! [`SERVICE_EP`] is the CoralOS loader's registry entry `EP_I915_REQ` (65; the 13 / 14 of the first draft were `EP_LAUNCH` /
//! `EP_DRM_SURFACE`, already taken). Since 2026-10-13 it is a LISTEN endpoint: a client `connect()`s and the kernel mints it a private
//! (request, event) pair from the dynamic pool, so replies are routed per client. The old static reply endpoint 66 (`EP_I915_REPLY`) is
//! no longer used by the device (the registry row for 65 is RECV only; 66 can be freed). Nothing but [`crate::service`] reads this
//! constant, so a renumbering is a one-line change.

/// The LISTEN endpoint (the device RECVs connect announces here): the loader's `EP_I915_REQ`.
pub const SERVICE_EP: u16 = 65;

pub const REQ_MARKER: u8 = b'I';
pub const REP_MARKER: u8 = b'i';
pub const VERSION: u8 = 1;
pub const REPLY_LEN: usize = 4;

/// Operations. Everything past HELLO is unimplemented in this skeleton and says so.
pub mod op {
    pub const HELLO: u8 = 0;
    pub const GET_READOUT: u8 = 1;
    pub const SET_MODE: u8 = 2;
    pub const PAGE_FLIP: u8 = 3;
    pub const ALLOC_SCANOUT: u8 = 4;
    pub const SUBMIT_BATCH: u8 = 5;
    /// The op byte of a reply to a frame too broken to have one.
    pub const NONE: u8 = 0xff;
}

/// Reply status.
pub mod status {
    pub const OK: u8 = 0;
    /// A well-formed request for something this device does not do (yet).
    pub const UNSUPPORTED: u8 = 1;
    /// Wrong length or lead byte.
    pub const MALFORMED: u8 = 2;
    /// A version this device does not speak.
    pub const BAD_VERSION: u8 = 3;
}

/// Answer one request frame.
pub fn answer(req: &[u8]) -> [u8; REPLY_LEN] {
    use status::*;
    if req.len() < 3 || req[0] != REQ_MARKER {
        return [REP_MARKER, VERSION, op::NONE, MALFORMED];
    }
    let (version, o) = (req[1], req[2]);
    if version != VERSION {
        return [REP_MARKER, VERSION, o, BAD_VERSION];
    }
    let st = if o == op::HELLO { OK } else { UNSUPPORTED };
    [REP_MARKER, VERSION, o, st]
}

/// One turn of the parked loop: what to do with the byte count `recv` returned for the 64-byte buffer `buf`.
/// `None` = nothing usable arrived (zero bytes, or an error word larger than the buffer): pace and retry; `Some` = the
/// reply to send.
pub fn handle(received: u64, buf: &[u8]) -> Option<[u8; REPLY_LEN]> {
    if received == 0 || received > buf.len() as u64 {
        return None;
    }
    Some(answer(&buf[..received as usize]))
}

/// The task's exit codes, so a supervisor can tell why it died (the log line before the exit names the step).
pub mod exit_code {
    /// A window the device needs (config page or BAR0) is not endowed or too short.
    pub const NO_WINDOW: u64 = 1;
    /// Startup refused the function or the readout failed.
    pub const STARTUP_FAILED: u64 = 2;
    /// A panic.
    pub const PANIC: u64 = 3;
    /// The forcewake proof of life failed (the `[I915-FORCEWAKE FAIL ...]` line names why); no further register was written.
    pub const PROOF_FAILED: u64 = 4;
    /// The uAPI service could not be assembled or the live loop could not start (feature `p3-exec`; the log line names why).
    pub const UAPI_FAILED: u64 = 5;
}

/// Bytes of first-generation request [`handle`] reads (the old 64-byte buffer; the parked service now receives whole messages).
pub const REQUEST_BUF: usize = 64;
