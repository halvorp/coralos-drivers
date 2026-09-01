// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux `drivers/base/regmap/regmap.c` formatting.
//!
//! Copyright 2011 Wolfson Microelectronics plc. Original author: Mark Brown.

use regmap_cache_core::format::{
    format_word, layout, pack_combined, pack_separate, packed_format, parse_word,
    resolve_reg_endian, resolve_val_endian, width, Endian, FormatError, PackedFormat, Width,
    ENDIAN_NAMES, PACKED_FORMAT_NAMES, WIDTH_NAMES,
};

/// regmap.c:873,883,893,896,906,916. Literal names are not generated from production data.
#[test]
fn all_six_combined_formats_are_pinned_by_count_and_name() {
    let expected = ["2/6", "4/12", "7/9", "7/17", "10/14", "12/20"];
    assert_eq!(PACKED_FORMAT_NAMES.len(), 6);
    assert_eq!(PACKED_FORMAT_NAMES, expected);
    assert_eq!(packed_format(2, 6, 0), Ok(PackedFormat::Reg2Val6));
    assert_eq!(packed_format(4, 12, 0), Ok(PackedFormat::Reg4Val12));
    assert_eq!(packed_format(7, 9, 0), Ok(PackedFormat::Reg7Val9));
    assert_eq!(packed_format(7, 17, 0), Ok(PackedFormat::Reg7Val17));
    assert_eq!(packed_format(10, 14, 0), Ok(PackedFormat::Reg10Val14));
    assert_eq!(packed_format(12, 20, 0), Ok(PackedFormat::Reg12Val20));
    // regmap.c:774,869: only the pad remainder contributes to this selector.
    assert_eq!(packed_format(1, 6, 9), Ok(PackedFormat::Reg2Val6));
    assert!(matches!(packed_format(3, 6, 0), Err(FormatError::UnsupportedPackedFormat { reg_bits_with_pad: 3, val_bits: 6 })));
}

/// regmap.c:923-1031: separately formatted words have exactly four widths.
#[test]
fn all_four_separate_widths_are_pinned_by_count_and_name() {
    let expected = ["8", "16", "24", "32"];
    assert_eq!(WIDTH_NAMES.len(), 4);
    assert_eq!(WIDTH_NAMES, expected);
    assert_eq!(width(8), Ok(Width::Bits8));
    assert_eq!(width(16), Ok(Width::Bits16));
    assert_eq!(width(24), Ok(Width::Bits24));
    assert_eq!(width(32), Ok(Width::Bits32));
    assert_eq!(width(12), Err(FormatError::UnsupportedWidth { bits: 12 }));
}

/// regmap.c:773-780: `% 8`, `/ 8`, and three `BITS_TO_BYTES` expressions.
#[test]
fn layout_preserves_whole_pad_bytes_and_pad_remainder() {
    assert_eq!(
        layout(12, 20, 13),
        Ok(regmap_cache_core::format::Layout {
            reg_shift: 5,
            pad_bytes: 1,
            reg_bytes: 2,
            val_bytes: 3,
            buf_size: 6,
        })
    );
    assert_eq!(layout(255, 1, 0).unwrap().buf_size, 32, "BITS_TO_BYTES accepts a 256-bit total");
}

/// regmap.c:615-635,650-675: all four endian choices are pinned by literal count and names.
#[test]
fn all_four_endian_names_are_pinned() {
    let expected = ["default", "big", "little", "native"];
    assert_eq!(ENDIAN_NAMES.len(), 4);
    assert_eq!(ENDIAN_NAMES, expected);
}

/// regmap.c:611-635: config wins, then bus, then literal BIG fallback.
#[test]
fn register_endian_precedence_matches_linux() {
    assert_eq!(resolve_reg_endian(Endian::Little, Endian::Native), Endian::Little);
    assert_eq!(resolve_reg_endian(Endian::Default, Endian::Native), Endian::Native);
    assert_eq!(resolve_reg_endian(Endian::Default, Endian::Default), Endian::Big);
}

/// regmap.c:646-675: config; firmware big, little, native; bus; BIG fallback.
#[test]
fn value_endian_precedence_matches_linux() {
    assert_eq!(resolve_val_endian(Endian::Little, true, false, false, Endian::Native), Endian::Little);
    assert_eq!(resolve_val_endian(Endian::Default, true, true, true, Endian::Little), Endian::Big);
    assert_eq!(resolve_val_endian(Endian::Default, false, true, true, Endian::Big), Endian::Little);
    assert_eq!(resolve_val_endian(Endian::Default, false, false, true, Endian::Big), Endian::Native);
    assert_eq!(resolve_val_endian(Endian::Default, false, false, false, Endian::Little), Endian::Little);
    assert_eq!(resolve_val_endian(Endian::Default, false, false, false, Endian::Default), Endian::Big);
}

/// regmap.c:212-263. Every expected byte is a literal independent of the production formatter.
#[test]
fn every_combined_formatter_matches_linux_literals() {
    let vectors: [(PackedFormat, u32, u32, &[u8]); 6] = [
        (PackedFormat::Reg2Val6, 0x2, 0x15, &[0x95]), // regmap.c:229
        (PackedFormat::Reg4Val12, 0xa, 0x5bc, &[0xa5, 0xbc]), // regmap.c:236
        (PackedFormat::Reg7Val9, 0x35, 0x1aa, &[0x6b, 0xaa]), // regmap.c:243
        (PackedFormat::Reg7Val17, 0x35, 0x1_abcd, &[0x6b, 0xab, 0xcd]), // regmap.c:251-253
        (PackedFormat::Reg10Val14, 0x2ab, 0x2bcd, &[0xaa, 0xeb, 0xcd]), // regmap.c:261-263
        (PackedFormat::Reg12Val20, 0xabc, 0xd_ef01, &[0xab, 0xcd, 0xef, 0x01]), // regmap.c:218-221
    ];
    for (format, reg, val, expected) in vectors {
        let mut out = [0; 4];
        let used = pack_combined(format, reg, val, &mut out).unwrap();
        assert_eq!(&out[..used], expected, "{format:?}");
    }
}

/// The safe port names short buffers and over-wide values rather than reproducing C truncation.
#[test]
fn combined_formatter_refusals_name_the_value_or_bound() {
    let mut none = [];
    assert_eq!(pack_combined(PackedFormat::Reg2Val6, 0, 0, &mut none), Err(FormatError::BufferTooSmall { supplied: 0, required: 1 }));
    let mut out = [0; 1];
    assert_eq!(pack_combined(PackedFormat::Reg2Val6, 4, 0, &mut out), Err(FormatError::ValueOutOfRange { field: "register", value: 4, bits: 2 }));
    assert_eq!(pack_combined(PackedFormat::Reg2Val6, 0, 64, &mut out), Err(FormatError::ValueOutOfRange { field: "value", value: 64, bits: 6 }));
}

/// regmap.c:266-313. Literals pin shift-before-endian-conversion and both byte orders.
#[test]
fn separate_word_formatters_match_linux_literals() {
    let mut out = [0; 4];
    assert_eq!(format_word(Width::Bits8, Endian::Big, 0x35, 1, &mut out).unwrap(), 1);
    assert_eq!(&out[..1], &[0x6a]); // regmap.c:270
    format_word(Width::Bits16, Endian::Big, 0x1234, 0, &mut out).unwrap();
    assert_eq!(&out[..2], &[0x12, 0x34]); // regmap.c:275
    format_word(Width::Bits16, Endian::Little, 0x1234, 0, &mut out).unwrap();
    assert_eq!(&out[..2], &[0x34, 0x12]); // regmap.c:280
    format_word(Width::Bits24, Endian::Big, 0x12_3456, 0, &mut out).unwrap();
    assert_eq!(&out[..3], &[0x12, 0x34, 0x56]); // regmap.c:293
    format_word(Width::Bits32, Endian::Big, 0x1234_5678, 0, &mut out).unwrap();
    assert_eq!(out, [0x12, 0x34, 0x56, 0x78]); // regmap.c:298
    format_word(Width::Bits32, Endian::Little, 0x1234_5678, 0, &mut out).unwrap();
    assert_eq!(out, [0x78, 0x56, 0x34, 0x12]); // regmap.c:303
    let native = 0x1234u16.to_ne_bytes();
    format_word(Width::Bits16, Endian::Native, 0x1234, 0, &mut out).unwrap();
    assert_eq!(&out[..2], &native); // regmap.c:285-288
    assert!(matches!(format_word(Width::Bits24, Endian::Little, 0, 0, &mut out), Err(FormatError::UnsupportedEndian { bits: 24, endian: Endian::Little })));
    assert_eq!(format_word(Width::Bits8, Endian::Big, 0, 8, &mut out), Err(FormatError::ShiftOutOfRange { shift: 8, width: 8 }));
}

/// regmap.c:773-780,869-1031,1720-1731. This pins the complete reg/pad/value layout: a 16-bit
/// big-endian register shifted by one pad remainder bit, one whole zero pad byte, then a little-
/// endian 16-bit value. Expected bytes are literal, not assembled from production helpers.
#[test]
fn separate_register_pad_and_value_are_packed_in_linux_order() {
    let mut out = [0xff; 5];
    assert_eq!(pack_separate(15, 16, 9, Endian::Big, Endian::Little, 0x1234, 0xabcd, &mut out), Ok(5));
    assert_eq!(out, [0x24, 0x68, 0x00, 0xcd, 0xab]);
    assert_eq!(pack_separate(12, 16, 0, Endian::Big, Endian::Big, 0, 0, &mut out), Err(FormatError::UnsupportedRegisterWidth { bits: 12 }));
}

/// regmap.c:320-376. Parse expected values are written literally, not obtained by formatting first.
#[test]
fn every_parser_matches_linux_literals() {
    assert_eq!(parse_word(Width::Bits8, Endian::Big, &[0xa5]), Ok(0xa5));
    assert_eq!(parse_word(Width::Bits16, Endian::Big, &[0x12, 0x34]), Ok(0x1234));
    assert_eq!(parse_word(Width::Bits16, Endian::Little, &[0x34, 0x12]), Ok(0x1234));
    assert_eq!(parse_word(Width::Bits24, Endian::Big, &[0x12, 0x34, 0x56]), Ok(0x12_3456));
    assert_eq!(parse_word(Width::Bits32, Endian::Big, &[0x12, 0x34, 0x56, 0x78]), Ok(0x1234_5678));
    assert_eq!(parse_word(Width::Bits32, Endian::Little, &[0x78, 0x56, 0x34, 0x12]), Ok(0x1234_5678));
    assert_eq!(parse_word(Width::Bits32, Endian::Big, &[0x12]), Err(FormatError::BufferTooSmall { supplied: 1, required: 4 }));
}
