// SPDX-License-Identifier: GPL-2.0-only
//! Why a syncobj operation was refused. Every variant has a stable name and the Linux errno (positive) it maps to.

/// `ENOENT` (errno-base.h:6), `ENOMEM` (errno-base.h:16), `EINVAL` (errno-base.h:26), `ETIME` (errno.h:45).
pub const ENOENT: i32 = 2; // errno-base.h:6
pub const ENOMEM: i32 = 12; // errno-base.h:16
pub const EINVAL: i32 = 22; // errno-base.h:26
pub const ETIME: i32 = 62; // errno.h:45

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncError {
    /// `drm_syncobj_find` found nothing in the caller's table (drm_syncobj.c:248-262, `-ENOENT` at drm_syncobj.c:1292-1295).
    NoSuchSyncobj { handle: u32 },
    /// `drm_syncobj_destroy` of a handle that is not there is `-EINVAL`, not `-ENOENT` (drm_syncobj.c:633-635).
    DestroyMissing { handle: u32 },
    /// A sync_file token that names nothing in this session (`sync_file_get_fence` returned NULL, drm_syncobj.c:726-731).
    NoSuchSyncFile { token: u32 },
    /// An export token that names nothing (or was released, or its owner died).
    NoSuchExport { token: u64 },
    /// Flags outside the defined set (drm_syncobj.c:1332-1333, :1384-1385, :1668-1669, `drm_syncobj_create_ioctl`).
    BadFlags { flags: u32 },
    /// A zero handle count where Linux requires one (drm_syncobj.c:1523-1524, :1556-1557, :1592-1593, :1672-1673).
    CountZero,
    /// A wait without WAIT_FOR_SUBMIT on a syncobj that has no fence, or whose timeline has not reached the point yet (drm_syncobj.c:1072-1080).
    NoFence { handle: u32, point: u64 },
    /// The wait ran out (`-ETIME`, drm_syncobj.c:1181-1183).
    Timeout,
    /// A table could not grow (`SYNC-NOMEM`), named before the thing exists.
    NoMem { table: &'static str },
    /// The session is not open in this table.
    NoSuchSession { session: u64 },
    /// The array arguments do not agree (`points` shorter than `handles`).
    ArrayMismatch { handles: usize, points: usize },
}

impl SyncError {
    pub const fn errno(&self) -> i32 {
        match self {
            SyncError::NoSuchSyncobj { .. } | SyncError::NoSuchSession { .. } => ENOENT,
            SyncError::NoMem { .. } => ENOMEM,
            SyncError::Timeout => ETIME,
            SyncError::DestroyMissing { .. }
            | SyncError::NoSuchSyncFile { .. }
            | SyncError::NoSuchExport { .. }
            | SyncError::BadFlags { .. }
            | SyncError::CountZero
            | SyncError::NoFence { .. }
            | SyncError::ArrayMismatch { .. } => EINVAL,
        }
    }
    pub const fn name(&self) -> &'static str {
        match self {
            SyncError::NoSuchSyncobj { .. } => "SYNCOBJ-NO-SUCH-HANDLE",
            SyncError::DestroyMissing { .. } => "SYNCOBJ-DESTROY-MISSING",
            SyncError::NoSuchSyncFile { .. } => "SYNC-FILE-NO-SUCH-TOKEN",
            SyncError::NoSuchExport { .. } => "SYNCOBJ-NO-SUCH-EXPORT",
            SyncError::BadFlags { .. } => "SYNCOBJ-BAD-FLAGS",
            SyncError::CountZero => "SYNCOBJ-COUNT-ZERO",
            SyncError::NoFence { .. } => "SYNCOBJ-NO-FENCE",
            SyncError::Timeout => "SYNCOBJ-TIMEOUT",
            SyncError::NoMem { .. } => "SYNC-NOMEM",
            SyncError::NoSuchSession { .. } => "SYNC-NO-SUCH-SESSION",
            SyncError::ArrayMismatch { .. } => "SYNCOBJ-ARRAY-MISMATCH",
        }
    }
}
