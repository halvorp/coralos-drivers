//! PRIME (dma-buf) and the implicit-sync bridge. A PRIME export token is a cross-session grant of a GEM object: the exporter's
//! shim hands the token to the importer's shim; PRIME-FD-TO-HANDLE gives the importing session its OWN handle on the same object.
//! The token holds the object alive until FD-CLOSE (kind `PRIME`) or until its exporting session dies; handles already imported
//! stay valid after that.
use super::{msg, Pad32};

msg!(
    /// `flags` is `DRM_CLOEXEC | DRM_RDWR` (ignored by the device; kept for the shim's use).
    PrimeHandleToFdReq { handle: u32, flags: u32 }
);
msg!(PrimeTokenRep { token: u64 });
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
