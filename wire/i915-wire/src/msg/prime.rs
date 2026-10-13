//! PRIME (dma-buf) and the implicit-sync bridge. A PRIME export token is a cross-session grant of a GEM object: the exporter's
//! shim hands the token to the importer's shim; PRIME-FD-TO-HANDLE gives the importing session its OWN handle on the same object.
//! The token holds the object alive until FD-CLOSE (kind `PRIME`) or until its exporting session dies; handles already imported
//! stay valid after that.
//!
//! WHO MAY REDEEM (ABI 1.4, D15). A token is a NUMBER, so it cannot be the authority: a token minted by `PRIME-HANDLE-TO-FD` is
//! redeemable only by the session that minted it (its own re-import; the shim's in-process hand-over to another session asks for a
//! grant to that session instead); a token minted by [`PrimeGrantReq`] is addressed to ONE session incarnation (`recipient`) and is
//! redeemable only by a `PRIME-FD-TO-HANDLE` that ARRIVES on that session's own endpoint, which the kernel lets only that client
//! use. A stolen or copied token is useless to anyone else (`EINVAL`, named `PRIME-NOT-RECIPIENT` on the device's log - the errno does not say whether a token exists), and the recipient's handle is
//! independent of the exporter's: it survives the exporter's death.
use super::{msg, Pad32};

msg!(
    /// `flags` is `DRM_CLOEXEC | DRM_RDWR` (ignored by the device; kept for the shim's use).
    PrimeHandleToFdReq { handle: u32, flags: u32 }
);
msg!(PrimeTokenRep { token: u64 });
msg!(
    /// `PRIME-GRANT` (ABI 1.4): export `handle` as a grant addressed to the session whose incarnation is `recipient` (non-zero, a live
    /// session of this device; the recipient learned it from its own SESSION-OPEN reply and sent it to the exporter over whatever
    /// channel the two processes share). The answer is the token to hand over. `flags` as `PRIME-HANDLE-TO-FD`.
    PrimeGrantReq { handle: u32, flags: u32, recipient: u64 }
);
msg!(PrimeFdToHandleReq { token: u64 });
msg!(PrimeHandleRep { handle: u32, _pad: Pad32 });

/// `DMA_BUF_SYNC_READ` / `WRITE` (linux/dma-buf.h).
pub mod dma_buf_sync {
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const RW: u32 = READ | WRITE;
}

msg!(
    /// `DMA_BUF_IOCTL_EXPORT_SYNC_FILE`: a sync_file that signals when the object's pending work (per `flags`) has.
    DmabufExportSyncFileReq { handle: u32, flags: u32 }
);
msg!(DmabufSyncFileRep { token: u32, _pad: Pad32 });
msg!(
    /// `DMA_BUF_IOCTL_IMPORT_SYNC_FILE`: later work on the object waits for the sync_file (`flags` as above).
    DmabufImportSyncFileReq { handle: u32, flags: u32, token: u32, _pad: Pad32 }
);
