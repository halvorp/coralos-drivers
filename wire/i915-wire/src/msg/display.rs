//! Display ownership on the wire (ABI 1.5, D17): the display the device ADOPTED from the firmware, the one mode it offers, and the
//! presentation path - two scanout images the display authority renders into, and the flips between them.
//!
//! WHO MAY CALL WHAT. `DISPLAY-QUERY` describes and changes nothing: it is answered to any frame, with or without a session (the
//! parked device answers it too). Every other call here needs a session the device holds as the DISPLAY AUTHORITY, bound by a
//! capability the system gave the compositor - never by a flag in a request (a `SESSION-OPEN` with any caps gains no display
//! privilege). Any other session is answered `-EACCES` (`DISPLAY-NOT-AUTHORIZED` on the device's log).
//!
//! THE BUFFER LIFECYCLE the device enforces (SCANOUT-BUFFER-LIFECYCLE): a scanout image is FREE, RENDERING (the authority may draw
//! into it), COMMITTED (a flip to it is armed and waits for the GPU to finish it and the hardware to latch it) or SCANNING_OUT.
//! Drawing into a COMMITTED or SCANNING_OUT image is refused (`-EBUSY`, `SCANOUT-REUSE-VIOLATION`); an image becomes FREE again only
//! after a NEWER one is proven latched. The firmware's own framebuffer is kept until the first replacement is proven latched.
//!
//! No addresses cross the wire: an image is a GEM handle of the authority's own session; the GGTT placement is the device's.
use super::{msg, Pad32};

/// `DisplayQueryRep::adopted`.
pub mod state {
    pub const NOT_ADOPTED: u32 = 0;
    pub const ADOPTED: u32 = 1;
}

/// Why there is no adopted display (`DisplayQueryRep::refusal`). The numbers are ABI; the names are the device's log tokens.
pub mod refusal {
    pub const NONE: u32 = 0;
    pub const NO_ACTIVE_PIPE: u32 = 1;
    pub const MULTIPLE_ACTIVE_PIPES: u32 = 2;
    pub const PLANE_UNREADABLE: u32 = 3;
    pub const PLANE_DISABLED: u32 = 4;
    pub const FORMAT_NOT_RGB: u32 = 5;
    pub const COMPRESSED: u32 = 6;
    pub const ROTATED: u32 = 7;
    pub const OFFSET_NONZERO: u32 = 8;
    pub const STRIDE_UNKNOWN: u32 = 9;
    pub const PLANE_NOT_PIPE_SOURCE: u32 = 10;
    pub const PLANE_SCALER: u32 = 11;
    pub const UNSCALED_SIZE_MISMATCH: u32 = 12;
    pub const INTERLACED: u32 = 13;
    pub const EXTENT_OVERFLOW: u32 = 14;
    /// The device could not read the display at all (a refused register read during the readout).
    pub const READOUT_FAILED: u32 = 15;

    /// Every code, in order (a client that logs a code it does not know prints `UNKNOWN`).
    pub const ALL: [u32; 16] = [
        NONE, NO_ACTIVE_PIPE, MULTIPLE_ACTIVE_PIPES, PLANE_UNREADABLE, PLANE_DISABLED, FORMAT_NOT_RGB, COMPRESSED, ROTATED, OFFSET_NONZERO,
        STRIDE_UNKNOWN, PLANE_NOT_PIPE_SOURCE, PLANE_SCALER, UNSCALED_SIZE_MISMATCH, INTERLACED, EXTENT_OVERFLOW, READOUT_FAILED,
    ];

    pub const fn name(code: u32) -> &'static str {
        match code {
            NONE => "NONE",
            NO_ACTIVE_PIPE => "ADOPT-NO-ACTIVE-PIPE",
            MULTIPLE_ACTIVE_PIPES => "ADOPT-MULTIPLE-ACTIVE-PIPES",
            PLANE_UNREADABLE => "ADOPT-PLANE-UNREADABLE",
            PLANE_DISABLED => "ADOPT-PLANE-DISABLED",
            FORMAT_NOT_RGB => "ADOPT-FORMAT-NOT-RGB",
            COMPRESSED => "ADOPT-COMPRESSED",
            ROTATED => "ADOPT-ROTATED",
            OFFSET_NONZERO => "ADOPT-OFFSET-NONZERO",
            STRIDE_UNKNOWN => "ADOPT-STRIDE-UNKNOWN",
            PLANE_NOT_PIPE_SOURCE => "ADOPT-PLANE-NOT-PIPE-SOURCE",
            PLANE_SCALER => "ADOPT-PLANE-SCALER",
            UNSCALED_SIZE_MISMATCH => "ADOPT-UNSCALED-SIZE-MISMATCH",
            INTERLACED => "ADOPT-INTERLACED",
            EXTENT_OVERFLOW => "ADOPT-EXTENT-OVERFLOW",
            READOUT_FAILED => "ADOPT-READOUT-FAILED",
            _ => "UNKNOWN",
        }
    }
}

msg!(
    /// `DISPLAY-QUERY` reply (request: an empty payload). When `adopted == state::ADOPTED`: the pipe (0 = A) and plane (0 = the
    /// primary) the firmware lit, the framebuffer the clients draw (`width` x `height`, `stride` bytes, DRM `fourcc` and `modifier`),
    /// the panel's active size, the refresh in millihertz (0 = unknown), whether the pipe's panel fitter scales the source onto the
    /// panel, and how many modes are offered (exactly 1: the adopted one). Otherwise every field but `refusal` is zero.
    DisplayQueryRep {
        adopted: u32,
        refusal: u32,
        pipe: u32,
        plane: u32,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        modifier: u64,
        active_width: u32,
        active_height: u32,
        refresh_millihz: u32,
        scaled: u32,
        mode_count: u32,
        _pad: Pad32,
    }
);

msg!(
    /// `DISPLAY-SET-MODE` request (authority only; reply: an empty body). The adopted mode is accepted and changes nothing; any
    /// other is `-EINVAL` (`MODE-NOT-ADOPTED`): this device does not modeset.
    DisplayModeReq { width: u32, height: u32, refresh_millihz: u32, _pad: Pad32 }
);

/// The scanout pair: two images, so one can be drawn while the other is scanned out. A property of double buffering, not a table.
pub const SCANOUT_IMAGES: usize = 2;

msg!(
    /// `SCANOUT-ACQUIRE` reply (authority only; request: an empty payload): the two scanout images, as GEM handles of the
    /// authority's session, both FREE, at the adopted geometry (`width` x `height`, `stride`, `fourcc`, `modifier`), `size` bytes
    /// each. A second acquire by the same session returns the same pair; the pair dies with the session.
    ScanoutAcquireRep {
        handle0: u32,
        handle1: u32,
        width: u32,
        height: u32,
        stride: u32,
        fourcc: u32,
        modifier: u64,
        size: u64,
    }
);

msg!(
    /// `SCANOUT-BEGIN` request (authority only; reply: an empty body): the authority is about to draw into image `handle`. FREE ->
    /// RENDERING; an image COMMITTED or SCANNING_OUT is `-EBUSY` (`SCANOUT-REUSE-VIOLATION`).
    ScanoutBeginReq { handle: u32, _pad: Pad32 }
);

msg!(
    /// `SCANOUT-PRESENT` request (authority only): show image `handle` (RENDERING -> COMMITTED). The device arms the flip once the
    /// GPU work writing the image has completed, and confirms it when the hardware shows the new surface live.
    ScanoutPresentReq { handle: u32, _pad: Pad32 }
);
msg!(
    /// `SCANOUT-PRESENT` reply: the present's sequence number (1, 2, ... per pair).
    ScanoutPresentRep { seq: u64 }
);

msg!(
    /// `SCANOUT-WAIT` request (authority only): block until present `seq` is latched, at most `timeout_ns` (negative = no limit,
    /// as GEM-WAIT). `-ETIME` when the time ran out first; `-EIO` when the device's own bounded latch confirmation gave up on that
    /// present (`latch-timeout`: the image stays COMMITTED, the one before it stays SCANNING_OUT - nothing is released on a guess).
    ScanoutWaitReq { seq: u64, timeout_ns: i64 }
);
msg!(
    /// `SCANOUT-WAIT` reply: the newest latched present (0 = the firmware's framebuffer is still the one scanned out), the handle
    /// being scanned out (0 = the firmware's), a bit per image that is FREE, and how many latch confirmations have timed out.
    ScanoutStatusRep { latched_seq: u64, scanning: u32, free_mask: u32, latch_timeouts: u32, _pad: Pad32 }
);
