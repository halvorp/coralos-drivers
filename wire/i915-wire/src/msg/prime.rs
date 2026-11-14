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
msg!(
    /// The image an object holds (ABI 1.6): what a display consumer needs to scan it out or sample it, in `drm_mode_fb_cmd2`'s terms
    /// (include/uapi/drm/drm_mode.h). `fourcc` is a [`fourcc`] code; `planes` the number of colour planes the format has; plane `i` starts
    /// `offsets[i]` bytes into the one granted object and its rows are `pitches[i]` bytes apart; entries past `planes` must be zero.
    /// `modifier` is a [`modifier`] value and covers every plane. The device checks it against the object at grant and at redeem
    /// (a tiling change in between is caught), and the recipient must expect exactly this image: it is never inferred.
    PrimeImage { fourcc: u32, width: u32, height: u32, planes: u32, pitches: [u32; 4], offsets: [u32; 4], modifier: u64 }
);
impl super::Field for PrimeImage {
    const SIZE: usize = <PrimeImage as super::Body>::SIZE;
    fn put(&self, w: &mut crate::codec::Writer<'_>) -> Result<(), crate::WireError> {
        super::Body::encode(self, w)
    }
    fn get(r: &mut crate::codec::Reader<'_>, _what: &'static str) -> Result<Self, crate::WireError> {
        super::Body::decode(r)
    }
}
msg!(
    /// `PRIME-GRANT-IMAGE` (ABI 1.6): [`PrimeGrantReq`] for an object that holds `image`. Refused by name when the image does not fit the
    /// object (`PRIME-IMAGE-*`, all `EINVAL`). `tag` is the recipient's binding stamp (a compositor names the video layer generation the
    /// buffer is for); the device does not interpret it, it only requires the redeem to state the same one. The answer is a
    /// [`PrimeTokenRep`]; the token is redeemable only by `PRIME-REDEEM-IMAGE`. No flags: no descriptor is made for an image grant.
    PrimeGrantImageReq { handle: u32, tag: u32, recipient: u64, image: PrimeImage }
);
msg!(
    /// `PRIME-REDEEM-IMAGE` (ABI 1.6): `PRIME-FD-TO-HANDLE` for an image grant, stating the binding `tag` and the image the recipient
    /// expects. A different tag is refused first (`PRIME-TAG-MISMATCH`), then any difference from the granted image, by name
    /// (`PRIME-FORMAT-MISMATCH`, `PRIME-SIZE-MISMATCH`, `PRIME-PITCH-MISMATCH`, `PRIME-OFFSET-MISMATCH`, `PRIME-MODIFIER-MISMATCH`, all
    /// `EINVAL`), and so is a token that carries no image (`PRIME-NO-IMAGE`). The answer is a [`PrimeHandleRep`].
    PrimeRedeemImageReq { token: u64, tag: u32, _pad: Pad32, image: PrimeImage }
);
msg!(PrimeHandleRep { handle: u32, _pad: Pad32 });

/// The `DRM_FORMAT_*` codes an image grant may name: `fourcc_code(a, b, c, d) = a | b << 8 | c << 16 | d << 24` (drm_fourcc.h).
pub mod fourcc {
    const fn code(n: &[u8; 4]) -> u32 {
        u32::from_le_bytes(*n)
    }
    pub const XRGB8888: u32 = code(b"XR24");
    pub const ARGB8888: u32 = code(b"AR24");
    pub const XBGR8888: u32 = code(b"XB24");
    pub const ABGR8888: u32 = code(b"AB24");
    /// Two planes: Y, then interleaved CbCr at half width and half height.
    pub const NV12: u32 = code(b"NV12");
}

/// The `DRM_FORMAT_MOD_*` values an image grant may name (drm_fourcc.h: `fourcc_mod_code(vendor, n) = vendor << 56 | n`, Intel = 1).
pub mod modifier {
    pub const LINEAR: u64 = 0;
    pub const I915_X_TILED: u64 = 1 << 56 | 1;
    pub const I915_Y_TILED: u64 = 1 << 56 | 2;
}

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
