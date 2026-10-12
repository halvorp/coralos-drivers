// SPDX-License-Identifier: GPL-2.0-only
//! Every refusal of this crate has a NAME (PORT_CONVENTIONS.md section 5): a bare `false` or a clamp is never an answer.

/// What the image parsers, layout planners and library table refuse, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwError {
    /// The image is shorter than the fixed header the next step must read (`loader.c:75-76`, `:116-117`).
    ImageTooShort { have: usize, need: usize },
    /// The extended manifest's `len` is larger than the image. Linux does `fw->size -= man->len` unchecked (loader.c:79-82).
    ExtManifestPastImage { ext_len: u32, image_len: usize },
    /// The extended manifest's `len` is smaller than the 16-byte extended-manifest header itself, so stripping it would leave
    /// part of the header in the "image". Linux strips it anyway (loader.c:79-82).
    ExtManifestShorterThanHeader { ext_len: u32 },
    /// The first dword of the stripped image is neither the Skylake nor the Apollo Lake manifest magic (loader.c:92-98).
    UnknownManifestMagic { magic: u32 },
    /// The manifest magic is a real one but belongs to a family this driver was not asked for (Linux does not check; an
    /// Apollo Lake image handed to the Kaby Lake loader would be DMA'd to a ROM that cannot parse it).
    ManifestFamilyMismatch { found: ManifestFamily, expected: ManifestFamily },
    /// Base firmware older (or of another release train) than the row's minimum (loader.c:121-133).
    FwVersionMismatch { found: FwVersion, min: FwVersion },
    /// `id + num_libs >= max_libs_count` (loader.c:498-499).
    LibraryTableFull { next_id: u32, wanted: u32, max_libs: u32 },
    /// The firmware reports more library slots than the 4-bit `lib_id` of LOAD_LIBRARY can address (messages.h `load_lib`).
    LibraryIdWidth { max_libs: u32 },
    /// A library name's file-system form does not fit the caller's buffer.
    NameBufferTooSmall { need: usize, have: usize },
    /// A CLDMA buffer of fewer than 2 bytes has a zero-length half; `cldma_setup_bdle(cl, buffer_size / 2)` would loop forever
    /// (cldma.c `while (remaining > 0)` with `chunk = 0`).
    CldmaBufferTooSmall { buffer_size: u32 },
    /// The data buffer needs more BDL entries than the one 4 KiB page `hda_cldma_init` allocates holds (BDL_SIZE, 256 entries).
    /// Linux writes past the page.
    BdlOverflow { entries: u32, max: u32 },
    /// The extents the caller says back the CLDMA data buffer do not cover `buffer_size`.
    ExtentsShort { covered: u64, buffer_size: u32 },
    /// The stream tag does not fit the 4-bit `SD_CTL` stream field (cldma.c:16-17).
    StreamTagWidth { tag: u32 },
    /// The SPIB register read back a position beyond the buffer; `ss = buffer_size - offset` would wrap (cldma.c `hda_cldma_fill`).
    SpibBeyondBuffer { spib: u32, buffer_size: u32 },
    /// A program builder produced more steps than its fixed storage (a bug in this crate, caught by tests, refused at runtime).
    ProgramOverflow,
}

/// Which manifest family a firmware or library image belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestFamily {
    /// `SKL_MANIFEST_MAGIC`, adsp firmware at +0x284 (Skylake, Kaby Lake).
    Skl,
    /// `APL_MANIFEST_MAGIC`, adsp firmware at +0x2000 (Apollo Lake and later). Never loaded by this crate.
    Apl,
}

/// `struct avs_fw_version` (messages.h:424-429).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FwVersion {
    pub major: u16,
    pub minor: u16,
    pub hotfix: u16,
    pub build: u16,
}

impl FwError {
    /// The stable one-word name of the refusal, for logs and for tests that must not depend on the payload.
    pub const fn name(&self) -> &'static str {
        match self {
            FwError::ImageTooShort { .. } => "image-too-short",
            FwError::ExtManifestPastImage { .. } => "ext-manifest-past-image",
            FwError::ExtManifestShorterThanHeader { .. } => "ext-manifest-shorter-than-header",
            FwError::UnknownManifestMagic { .. } => "unknown-manifest-magic",
            FwError::ManifestFamilyMismatch { .. } => "manifest-family-mismatch",
            FwError::FwVersionMismatch { .. } => "fw-version-mismatch",
            FwError::LibraryTableFull { .. } => "library-table-full",
            FwError::LibraryIdWidth { .. } => "library-id-width",
            FwError::NameBufferTooSmall { .. } => "name-buffer-too-small",
            FwError::CldmaBufferTooSmall { .. } => "cldma-buffer-too-small",
            FwError::BdlOverflow { .. } => "bdl-overflow",
            FwError::ExtentsShort { .. } => "extents-short",
            FwError::StreamTagWidth { .. } => "stream-tag-width",
            FwError::SpibBeyondBuffer { .. } => "spib-beyond-buffer",
            FwError::ProgramOverflow => "program-overflow",
        }
    }
}
