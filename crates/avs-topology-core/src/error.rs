// SPDX-License-Identifier: GPL-2.0-only
//! Every refusal has a NAME (PORT_CONVENTIONS.md section 5). Linux reports most of these as a bare `-EINVAL`; the name says which.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TplgError {
    /// The data ends inside a structure that its own header says is complete.
    Truncated { what: &'static str, need: usize, have: usize },
    /// `hdr->size != sizeof(*hdr)` (soc-topology.c:1948).
    BadHeaderSize { at: usize, size: u32 },
    /// `hdr->magic` is neither the little-endian magic nor its reverse (soc-topology.c:1967-1985).
    BadMagic { at: usize, magic: u32 },
    /// The file is big-endian ("ASoC" reversed): not supported (soc-topology.c:1967).
    BigEndian { at: usize },
    /// `abi` outside `SND_SOC_TPLG_ABI_VERSION_MIN..=SND_SOC_TPLG_ABI_VERSION` (soc-topology.c:1976).
    BadAbi { at: usize, abi: u32 },
    /// `payload_size == 0` (soc-topology.c:1987).
    EmptyPayload { at: usize },
    /// A block's payload runs past the end of the file. Linux compares `offset + payload` without the 36-byte header.
    BlockPastEnd { at: usize, payload: u32, file_len: usize },
    /// A block type the AVS driver has no loader for (soc-topology.c:2050 -> `soc_tplg_vendor_load`: "no vendor load callback").
    UnsupportedBlockType { ty: u32 },
    /// More than one manifest, or a block type that must appear once.
    DuplicateBlock { ty: u32 },
    /// An element's `size` field is not the size of its structure (soc-topology.c:985, :1276, :1568).
    BadElementSize { what: &'static str, size: u32, want: u32 },
    /// A control of a type AVS refuses (`avs_control_load`: only mixers; this port adds BYTES, see [`crate::control`]).
    UnsupportedControlType { ty: u32 },
    /// A vendor array has `size < 12`: Linux's lookup never advances over a zero-sized one and loops forever.
    ArrayTooSmall { at: usize, size: u32 },
    /// A vendor array is larger than the block that contains it (`-EINVAL`, topology.c:45-46, :78-79, :294-297).
    ArrayPastBlock { at: usize, size: u32, block: u32 },
    /// A vendor array states more elements than its `size` can hold.
    TupleCountPastArray { at: usize, num_elems: u32, size: u32 },
    /// An unknown tuple type ("unknown token type", topology.c:313-316).
    UnknownTupleType { ty: u32 },
    /// A string tuple without a terminating NUL in its 44 bytes.
    StringNotTerminated { token: u32 },
    /// The manifest does not begin with its header (topology.c:1973-1977).
    ManifestWithoutHeader,
    /// A dictionary the manifest must carry is not there (topology.c:1991-2109: one lookup per dictionary).
    MissingDictionary { token: u32 },
    /// A dictionary's first tuple is not its entry count (`parse_dictionary_header`, topology.c:449-455).
    BadDictionaryHeader { want: u32, got: u32 },
    /// A dictionary promises more entries than its block holds.
    DictionaryTooShort { token: u32, entry: u32 },
    /// An index token names an object that does not exist (`AVS_DEFINE_PTR_PARSER`, topology.c:340-342).
    IndexOutOfRange { what: &'static str, index: u32, count: u32 },
    /// A count in the file is larger than this port's fixed storage.
    TooMany { what: &'static str, count: u32, max: u32 },
    /// A byte/short token carries a value that does not fit. Linux truncates (topology.c:152-172).
    ValueTooWide { token: u32, value: u32, bits: u32 },
    /// A module-config-ext with generic pins states zero pins but is followed by pin formats (topology.c:983-984).
    NoPins,
    /// A pipeline header is not followed by a module (topology.c:1321-1326).
    PipelineWithoutModule,
    /// A pipeline's binding count disagrees with its block (topology.c:1346-1347).
    BindingCountMismatch { states: u32, found: u32 },
    /// A module has trailing config ids but no count (topology.c:1250-1251).
    ConfigIdsWithoutCount,
    /// A path template without a path variant (`parse_path_template`, topology.c:1544-1546).
    TemplateWithoutPath,
    /// A raw-data section (init config, NHLT config) runs past the manifest.
    RawDataPastBlock { token: u32, need: u32, have: u32 },
    /// An NHLT config entry whose SIZE token never came (topology.c:1784-1785).
    NhltConfigWithoutSize,
    /// `%d` expansion would not fit a 44-byte name.
    NameTooLong,
    /// A widget with kcontrols of a kind this port cannot size.
    WidgetControlType { ty: u32 },
}

impl TplgError {
    pub const fn name(&self) -> &'static str {
        match self {
            TplgError::Truncated { .. } => "truncated",
            TplgError::BadHeaderSize { .. } => "bad-header-size",
            TplgError::BadMagic { .. } => "bad-magic",
            TplgError::BigEndian { .. } => "big-endian",
            TplgError::BadAbi { .. } => "bad-abi",
            TplgError::EmptyPayload { .. } => "empty-payload",
            TplgError::BlockPastEnd { .. } => "block-past-end",
            TplgError::UnsupportedBlockType { .. } => "unsupported-block-type",
            TplgError::DuplicateBlock { .. } => "duplicate-block",
            TplgError::BadElementSize { .. } => "bad-element-size",
            TplgError::UnsupportedControlType { .. } => "unsupported-control-type",
            TplgError::ArrayTooSmall { .. } => "array-too-small",
            TplgError::ArrayPastBlock { .. } => "array-past-block",
            TplgError::TupleCountPastArray { .. } => "tuple-count-past-array",
            TplgError::UnknownTupleType { .. } => "unknown-tuple-type",
            TplgError::StringNotTerminated { .. } => "string-not-terminated",
            TplgError::ManifestWithoutHeader => "manifest-without-header",
            TplgError::MissingDictionary { .. } => "missing-dictionary",
            TplgError::BadDictionaryHeader { .. } => "bad-dictionary-header",
            TplgError::DictionaryTooShort { .. } => "dictionary-too-short",
            TplgError::IndexOutOfRange { .. } => "index-out-of-range",
            TplgError::TooMany { .. } => "too-many",
            TplgError::ValueTooWide { .. } => "value-too-wide",
            TplgError::NoPins => "no-pins",
            TplgError::PipelineWithoutModule => "pipeline-without-module",
            TplgError::BindingCountMismatch { .. } => "binding-count-mismatch",
            TplgError::ConfigIdsWithoutCount => "config-ids-without-count",
            TplgError::TemplateWithoutPath => "template-without-path",
            TplgError::RawDataPastBlock { .. } => "raw-data-past-block",
            TplgError::NhltConfigWithoutSize => "nhlt-config-without-size",
            TplgError::NameTooLong => "name-too-long",
            TplgError::WidgetControlType { .. } => "widget-control-type",
        }
    }
}
