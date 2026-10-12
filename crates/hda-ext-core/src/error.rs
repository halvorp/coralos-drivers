// SPDX-License-Identifier: GPL-2.0-only
//! Every refusal of this crate has a name and says what refused and why.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdaError {
    /// A growable list could not grow (`try_reserve`): the named table, before anything exists.
    OutOfMemory { what: &'static str },
    /// The capability chain would not terminate within [`crate::consts::HDAC_MAX_CAPS`] entries (controller.c:466-469 logs and stops).
    TooManyCapabilities { read: u32 },
    /// A capability header read as all ones (controller.c:420-423 "Invalid capability reg read"): the window is not decoding.
    CapabilityReadAllOnes { offset: u32 },
    /// A capability id this port does not know (controller.c:456-459 logs and ends the chain, hiding every later capability).
    UnknownCapability { id: u32, offset: u32 },
    /// `LLCH` (the first capability's offset) is zero or not dword aligned.
    BadCapabilityStart { offset: u32 },
    /// A capability pointer that is not dword aligned (a header read would straddle two dwords).
    MisalignedCapability { offset: u32 },
    /// The multi-link capability states more links than fit in the BAR window that holds them.
    LinkWindowPastBar { links: u32, mlcap: u32, bar_len: u32 },
    /// A link-dependent plan was asked for without the capability it lives in.
    MissingCapability { which: &'static str },
    /// A stream index at or past the controller's stream count (GCAP).
    StreamIndexOutOfRange { index: u32, streams: u32 },
    /// A stream tag outside 1 ..= 15 (tag 0 is reserved by the HDA spec; the field is 4 bits).
    StreamTagOutOfRange { tag: u32 },
    /// The BDL would hold more entries than its 4 KiB page can (hda_register.h:144-145; setup_bdle returns -EINVAL).
    BdlOverflow { entries: u32, max: u32 },
    /// The buffer is not a whole number of periods (Linux silently drops the remainder, stream.c `periods = bufsize / period_bytes`).
    BufferNotWholePeriods { buffer: u32, period: u32 },
    /// A zero-sized buffer or period.
    EmptyBuffer,
    /// The physical extents do not cover the buffer.
    ExtentsShort { have: u64, need: u64 },
    /// SPIB beyond the buffer.
    SpibBeyondBuffer { spib: u32, buffer: u32 },
    /// A stream format Linux's `snd_hdac_stream_format` returns 0 for (stream.c: "zero if invalid").
    InvalidStreamFormat { rate: u32, channels: u32, bits: u32 },
    /// A 64-bit address where the plan needs the controller to address it (BDL/position buffer) is not 128-byte aligned
    /// (`SD_BDLPL` bits 6:0 are reserved, hda_register.h; cldma.c masks them with `AZX_SD_BDLPL_BDLPLBA_MASK`).
    UnalignedDmaAddress { addr: u64, align: u64 },
}

impl HdaError {
    /// The stable one-word name of the refusal.
    pub const fn name(&self) -> &'static str {
        match self {
            HdaError::OutOfMemory { .. } => "out-of-memory",
            HdaError::TooManyCapabilities { .. } => "too-many-capabilities",
            HdaError::CapabilityReadAllOnes { .. } => "capability-read-all-ones",
            HdaError::UnknownCapability { .. } => "unknown-capability",
            HdaError::BadCapabilityStart { .. } => "bad-capability-start",
            HdaError::MisalignedCapability { .. } => "misaligned-capability",
            HdaError::LinkWindowPastBar { .. } => "link-window-past-bar",
            HdaError::MissingCapability { .. } => "missing-capability",
            HdaError::StreamIndexOutOfRange { .. } => "stream-index-out-of-range",
            HdaError::StreamTagOutOfRange { .. } => "stream-tag-out-of-range",
            HdaError::BdlOverflow { .. } => "bdl-overflow",
            HdaError::BufferNotWholePeriods { .. } => "buffer-not-whole-periods",
            HdaError::EmptyBuffer => "empty-buffer",
            HdaError::ExtentsShort { .. } => "extents-short",
            HdaError::SpibBeyondBuffer { .. } => "spib-beyond-buffer",
            HdaError::InvalidStreamFormat { .. } => "invalid-stream-format",
            HdaError::UnalignedDmaAddress { .. } => "unaligned-dma-address",
        }
    }
}
