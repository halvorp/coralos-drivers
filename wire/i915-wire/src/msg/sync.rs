//! Syncobj, timeline and sync_file calls. A syncobj handle is session-scoped. A DESCRIPTOR (what Linux calls an fd) is a `token`:
//! SYNC-FILE tokens (`u32`) are session-scoped like fds of one process; EXPORT tokens (`u64`) are the cross-session form of a
//! syncobj or PRIME grant: the exporter's shim hands the token to the importer's shim, the importer presents it, and it dies with its
//! owner session or its FD-CLOSE.
use super::{msg, put_u32s, put_u64s, Pad32, U32s, U64s};
use crate::codec::{Reader, Writer};
use crate::WireError;

/// Handles per array call (wait, signal, reset, timeline signal, query): a VALIDATION limit; Linux allocates `count_handles` words
/// with `kmalloc_array` and fails with `-ENOMEM` when it cannot.
pub const MAX_HANDLES: u32 = 4096;

/// `DRM_SYNCOBJ_*` flag bits (drm.h), the same numbers the ioctls use.
pub mod flags {
    pub const CREATE_SIGNALED: u32 = 1 << 0;
    pub const WAIT_ALL: u32 = 1 << 0;
    pub const WAIT_FOR_SUBMIT: u32 = 1 << 1;
    pub const WAIT_AVAILABLE: u32 = 1 << 2;
    pub const WAIT_DEADLINE: u32 = 1 << 3;
    pub const QUERY_LAST_SUBMITTED: u32 = 1 << 0;
    pub const HANDLE_TO_FD_EXPORT_SYNC_FILE: u32 = 1 << 0;
    pub const HANDLE_TO_FD_TIMELINE: u32 = 1 << 1;
    pub const FD_TO_HANDLE_IMPORT_SYNC_FILE: u32 = 1 << 0;
    pub const FD_TO_HANDLE_TIMELINE: u32 = 1 << 1;
}

/// What an FD-CLOSE token names.
pub mod fd_kind {
    /// A syncobj export token (SYNCOBJ-HANDLE-TO-FD without EXPORT_SYNC_FILE).
    pub const SYNCOBJ_EXPORT: u32 = 0;
    /// A sync_file token.
    pub const SYNC_FILE: u32 = 1;
    /// A PRIME export token.
    pub const PRIME: u32 = 2;
}

msg!(SyncobjCreateReq { flags: u32, _pad: Pad32 });
msg!(SyncobjHandleRep { handle: u32, _pad: Pad32 });
msg!(SyncobjDestroyReq { handle: u32, pad: u32 });

msg!(
    /// SYNCOBJ-WAIT / SYNCOBJ-TIMELINE-WAIT head. `timeout_ns` is RELATIVE (the shim converts Linux's absolute `CLOCK_MONOTONIC`
    /// deadline): zero polls, negative waits forever. `count` handles follow (zero-padded to 8 bytes), then, for the timeline form,
    /// `count` points. A wait that has to block is answered LATER by the same request id.
    SyncobjWaitHead { count: u32, flags: u32, timeout_ns: i64 }
);
msg!(SyncobjWaitRep { first_signaled: u32, _pad: Pad32 });

msg!(
    /// SIGNAL / RESET / TIMELINE-SIGNAL / QUERY head: `count` handles follow (padded to 8), then for TIMELINE-SIGNAL `count` points.
    SyncobjArrayHead { count: u32, flags: u32 }
);
msg!(SyncobjQueryRepHead { count: u32, _pad: Pad32 });

msg!(
    /// The fence of `(src_handle, src_point)` becomes `(dst_handle, dst_point)`'s (binary when `dst_point == 0`).
    SyncobjTransferReq { src_handle: u32, dst_handle: u32, src_point: u64, dst_point: u64, flags: u32, _pad: Pad32 }
);

msg!(
    /// `drm_syncobj_handle` for HANDLE_TO_FD. With EXPORT_SYNC_FILE the reply names a sync_file token (in the low 32 bits); without
    /// it, a syncobj EXPORT token.
    SyncobjHandleToFdReq { handle: u32, flags: u32, point: u64 }
);
msg!(SyncobjTokenRep { token: u64 });
msg!(
    /// `drm_syncobj_handle` for FD_TO_HANDLE. With IMPORT_SYNC_FILE, `token` is a sync_file token of this session and `handle` the
    /// EXISTING syncobj to import into (`point` under the TIMELINE flag); without it, `token` is a syncobj export token and the
    /// reply carries a NEW handle.
    SyncobjFdToHandleReq { token: u64, handle: u32, flags: u32, point: u64 }
);

msg!(FdCloseReq { token: u64, kind: u32, _pad: Pad32 });

msg!(SyncFileMergeReq { a: u32, b: u32 });
msg!(SyncFileTokenRep { token: u32, _pad: Pad32 });
msg!(
    /// `poll(2)` on a sync_file: zero `timeout_ns` reports now, otherwise the reply may come later (`-ETIME` on timeout).
    SyncFileWaitReq { token: u32, _pad: Pad32, timeout_ns: i64 }
);
msg!(
    /// `fence_error` is the fence's error word (`0`, or `-errno` if the work behind it failed).
    SyncFileWaitRep { fence_error: i32, _pad: Pad32 }
);

fn check_count(what: &'static str, count: u32) -> Result<(), WireError> {
    if count > MAX_HANDLES {
        return Err(WireError::LimitExceeded { what, got: count as u64, max: MAX_HANDLES as u64 });
    }
    Ok(())
}

/// A decoded SYNCOBJ-WAIT / TIMELINE-WAIT request.
#[derive(Clone, Copy, Debug)]
pub struct WaitReq<'a> {
    pub head: SyncobjWaitHead,
    pub handles: U32s<'a>,
    /// Present for the timeline form only.
    pub points: Option<U64s<'a>>,
}

impl<'a> WaitReq<'a> {
    pub fn decode(payload: &'a [u8], timeline: bool) -> Result<WaitReq<'a>, WireError> {
        let mut r = Reader::new(payload);
        let head = <SyncobjWaitHead as super::Body>::decode(&mut r)?;
        check_count("SyncobjWait.count", head.count)?;
        let handles = U32s::read(&mut r, "SyncobjWait.handles", head.count as usize)?;
        let points = if timeline { Some(U64s::read(&mut r, "SyncobjWait.points", head.count as usize)?) } else { None };
        r.finish()?;
        Ok(WaitReq { head, handles, points })
    }
    pub fn encode(out: &mut [u8], flags: u32, timeout_ns: i64, handles: &[u32], points: Option<&[u64]>) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        super::Body::encode(&SyncobjWaitHead { count: handles.len() as u32, flags, timeout_ns }, &mut w)?;
        put_u32s(&mut w, handles)?;
        if let Some(p) = points {
            if p.len() != handles.len() {
                return Err(WireError::CountMismatch { what: "SyncobjWait.points", declared: handles.len() as u64, actual: p.len() as u64 });
            }
            put_u64s(&mut w, p)?;
        }
        Ok(w.pos())
    }
}

/// A decoded SIGNAL / RESET / TIMELINE-SIGNAL / QUERY request.
#[derive(Clone, Copy, Debug)]
pub struct ArrayReq<'a> {
    pub head: SyncobjArrayHead,
    pub handles: U32s<'a>,
    pub points: Option<U64s<'a>>,
}

impl<'a> ArrayReq<'a> {
    pub fn decode(payload: &'a [u8], with_points: bool) -> Result<ArrayReq<'a>, WireError> {
        let mut r = Reader::new(payload);
        let head = <SyncobjArrayHead as super::Body>::decode(&mut r)?;
        check_count("SyncobjArray.count", head.count)?;
        let handles = U32s::read(&mut r, "SyncobjArray.handles", head.count as usize)?;
        let points = if with_points { Some(U64s::read(&mut r, "SyncobjArray.points", head.count as usize)?) } else { None };
        r.finish()?;
        Ok(ArrayReq { head, handles, points })
    }
    pub fn encode(out: &mut [u8], flags: u32, handles: &[u32], points: Option<&[u64]>) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        super::Body::encode(&SyncobjArrayHead { count: handles.len() as u32, flags }, &mut w)?;
        put_u32s(&mut w, handles)?;
        if let Some(p) = points {
            if p.len() != handles.len() {
                return Err(WireError::CountMismatch { what: "SyncobjArray.points", declared: handles.len() as u64, actual: p.len() as u64 });
            }
            put_u64s(&mut w, p)?;
        }
        Ok(w.pos())
    }
}

/// The SYNCOBJ-QUERY reply body after the head: `count` points.
pub fn encode_query_reply(out: &mut [u8], errno: i32, points: &[u64]) -> Result<usize, WireError> {
    let mut w = Writer::new(out);
    super::encode_reply_head(&mut w, crate::Status::Ok, errno)?;
    super::Body::encode(&SyncobjQueryRepHead { count: points.len() as u32, _pad: Pad32 }, &mut w)?;
    put_u64s(&mut w, points)?;
    Ok(w.pos())
}

pub fn decode_query_reply(payload: &[u8]) -> Result<(i32, U64s<'_>), WireError> {
    let mut r = Reader::new(payload);
    let (_s, errno) = super::decode_reply_head(&mut r)?;
    let head = <SyncobjQueryRepHead as super::Body>::decode(&mut r)?;
    check_count("SyncobjQueryRep.count", head.count)?;
    let pts = U64s::read(&mut r, "SyncobjQueryRep.points", head.count as usize)?;
    r.finish()?;
    Ok((errno, pts))
}
