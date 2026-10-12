// SPDX-License-Identifier: GPL-2.0-only
//! Named refusals. Each says what refused and the offending value / bound.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VbtError {
    /// "VBT header incomplete" — intel_bios.c:3074.
    HeaderIncomplete { have: usize, need: usize },
    /// "VBT invalid signature" — intel_bios.c:3079.
    BadSignature,
    /// "VBT incomplete (vbt_size overflows)" — intel_bios.c:3085.
    VbtSizeOverflows { vbt_size: usize, buffer: usize },
    /// "BDB header incomplete" — intel_bios.c:3095 (bdb_offset + sizeof(bdb_header) > vbt_size).
    BdbHeaderIncomplete { bdb_offset: usize, vbt_size: usize },
    /// "BDB incomplete" — intel_bios.c:3101 (bdb_offset + bdb_size > vbt_size).
    BdbIncomplete { bdb_offset: usize, bdb_size: usize, vbt_size: usize },
    /// bdb_header.header_size smaller than the header itself or bigger than the BDB: Linux walks garbage here.
    BadBdbHeaderSize { header_size: usize, bdb_size: usize },
    /// find_raw_section(): `index + current_size > total` — intel_bios.c:117.
    BlockOverflows { id: u8, size: usize, offset: usize, bdb_size: usize },
    /// MIPI sequence block v3's separate size field does not fit in the BDB — intel_bios.c:85.
    MipiSizeFieldTruncated { offset: usize },
    /// A block shorter than the minimum its parser reads (`bdb_blocks[].min_size`, intel_bios.c:164).
    BlockTooSmall { id: u8, size: usize, need: usize },
    /// "General definitions block too small" — intel_bios.c:2906.
    GeneralDefinitionsTooSmall { size: usize },
    /// "Child device config size N is too small" — intel_bios.c:2878 (< LEGACY_CHILD_DEVICE_CONFIG_SIZE 33, intel_vbt_defs.h:384).
    ChildDeviceSizeTooSmall { size: usize },
    /// child_dev_size 0 would divide by zero in `(block_size - sizeof(*defs)) / defs->child_dev_size`.
    ChildDeviceSizeZero,
    /// panel_type outside the 16-entry tables (Linux indexes `[panel_type]` unchecked; we refuse).
    PanelTypeOutOfRange { panel_type: u8 },
    /// No panel type could be selected (all of OpRegion/VBT/PNPID absent and no fallback).
    NoPanelType,
    /// Block required by this parser is absent.
    BlockMissing { id: u8 },
    /// Modern VBT without block 41: Linux synthesises the pointers (generate_lfp_data_ptrs, intel_bios.c:366);
    /// not ported.
    LfpDataPtrsMissing,
    /// "VBT has malformed LFP data table pointers" — intel_bios.c:507; names the failed check.
    MalformedLfpDataPtrs { check: &'static str },
    /// "Unsupported backlight data entry size" — intel_bios.c:1029.
    UnsupportedBacklightEntrySize { size: u8 },
}
