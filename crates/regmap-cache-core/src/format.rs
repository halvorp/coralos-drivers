// SPDX-License-Identifier: GPL-2.0-only
//! Register/value endianness and `reg_bits`/`val_bits`/`pad_bits` packing.
//!
//! Ported from Linux `drivers/base/regmap/regmap.c`: formatters/parsers at lines 212-376,
//! endian selection at lines 611-675, and format initialization at lines 773-1045.
//!
//! Copyright 2011 Wolfson Microelectronics plc. Original author: Mark Brown.

use core::fmt;

/// Endianness choices from Linux's `enum regmap_endian` uses (regmap.c:611-675).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Default,
    Big,
    Little,
    Native,
}

/// Frozen names for all four endian choices used by Linux (regmap.c:615-635,650-675).
pub const ENDIAN_NAMES: [&str; 4] = ["default", "big", "little", "native"];

/// The six combined register/value formats selected by Linux (regmap.c:869-922).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackedFormat {
    Reg2Val6,
    Reg4Val12,
    Reg7Val9,
    Reg7Val17,
    Reg10Val14,
    Reg12Val20,
}

/// Frozen names for every combined format Linux defines (regmap.c:873,883,893,896,906,916).
pub const PACKED_FORMAT_NAMES: [&str; 6] = [
    "2/6",
    "4/12",
    "7/9",
    "7/17",
    "10/14",
    "12/20",
];

/// Valid separately formatted widths selected by Linux (regmap.c:923-1031).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Bits8,
    Bits16,
    Bits24,
    Bits32,
}

/// Frozen names for every separate register/value width Linux defines (regmap.c:923-1031).
pub const WIDTH_NAMES: [&str; 4] = ["8", "16", "24", "32"];

/// Errors name the rejected format value and its Linux-supported bound/set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    UnsupportedPackedFormat { reg_bits_with_pad: u16, val_bits: u8 },
    UnsupportedWidth { bits: u8 },
    UnsupportedRegisterWidth { bits: u16 },
    UnsupportedEndian { bits: u8, endian: Endian },
    BufferTooSmall { supplied: usize, required: usize },
    ValueOutOfRange { field: &'static str, value: u32, bits: u8 },
    ShiftOutOfRange { shift: u8, width: u8 },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::UnsupportedPackedFormat { reg_bits_with_pad, val_bits } => write!(
                f,
                "packed regmap format refused reg_bits + pad remainder {reg_bits_with_pad} and val_bits {val_bits}: Linux supports only 2/6, 4/12, 7/9, 7/17, 10/14, and 12/20"
            ),
            Self::UnsupportedWidth { bits } => write!(
                f,
                "regmap width refused {bits} bits: Linux supports only 8, 16, 24, and 32 bits"
            ),
            Self::UnsupportedRegisterWidth { bits } => write!(
                f,
                "regmap register width plus pad remainder refused {bits} bits: Linux supports only 8, 16, 24, and 32 bits"
            ),
            Self::UnsupportedEndian { bits, endian } => write!(
                f,
                "regmap formatter refused {endian:?} endian for {bits} bits"
            ),
            Self::BufferTooSmall { supplied, required } => write!(
                f,
                "regmap byte buffer refused {supplied} bytes: at least {required} bytes are required"
            ),
            Self::ValueOutOfRange { field, value, bits } => write!(
                f,
                "regmap {field} value {value:#x} refused: maximum for {bits} bits is {:#x}",
                mask(bits)
            ),
            Self::ShiftOutOfRange { shift, width } => write!(
                f,
                "regmap shift {shift} refused: it must be smaller than the {width}-bit word"
            ),
        }
    }
}

/// Derived buffer layout from Linux initialization (regmap.c:773-780).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub reg_shift: u8,
    pub pad_bytes: usize,
    pub reg_bytes: usize,
    pub val_bytes: usize,
    pub buf_size: usize,
}

const fn bits_to_bytes(bits: u16) -> usize {
    ((bits + 7) / 8) as usize
}

const fn mask(bits: u8) -> u32 {
    if bits >= 32 { u32::MAX } else { (1u32 << bits) - 1 }
}

/// Calculate Linux's format layout: pad remainder shifts the register and whole pad bytes separate
/// the register bytes from value bytes (regmap.c:773-780).
pub fn layout(reg_bits: u8, val_bits: u8, pad_bits: u8) -> Result<Layout, FormatError> {
    let total = reg_bits as u16 + val_bits as u16 + pad_bits as u16;
    Ok(Layout {
        reg_shift: pad_bits % 8, // regmap.c:774
        pad_bytes: (pad_bits / 8) as usize, // regmap.c:776
        reg_bytes: bits_to_bytes(reg_bits as u16), // regmap.c:778
        val_bytes: bits_to_bytes(val_bits as u16), // regmap.c:779
        buf_size: bits_to_bytes(total), // regmap.c:780
    })
}

/// Resolve register endian: config, then bus, then big endian (regmap.c:611-635).
pub fn resolve_reg_endian(config: Endian, bus: Endian) -> Endian {
    if config != Endian::Default { config } else if bus != Endian::Default { bus } else { Endian::Big }
}

/// Resolve value endian: config, firmware big/little/native (in that order), bus, then big
/// (regmap.c:646-675).
pub fn resolve_val_endian(
    config: Endian,
    firmware_big: bool,
    firmware_little: bool,
    firmware_native: bool,
    bus: Endian,
) -> Endian {
    if config != Endian::Default {
        config
    } else if firmware_big {
        Endian::Big
    } else if firmware_little {
        Endian::Little
    } else if firmware_native {
        Endian::Native
    } else if bus != Endian::Default {
        bus
    } else {
        Endian::Big
    }
}

/// Select one of Linux's six combined packed formats (regmap.c:869-922).
pub fn packed_format(reg_bits: u8, val_bits: u8, pad_bits: u8) -> Result<PackedFormat, FormatError> {
    let shifted = reg_bits as u16 + (pad_bits % 8) as u16; // regmap.c:774,869
    match (shifted, val_bits) {
        (2, 6) => Ok(PackedFormat::Reg2Val6), // regmap.c:873
        (4, 12) => Ok(PackedFormat::Reg4Val12), // regmap.c:883
        (7, 9) => Ok(PackedFormat::Reg7Val9), // regmap.c:893
        (7, 17) => Ok(PackedFormat::Reg7Val17), // regmap.c:896
        (10, 14) => Ok(PackedFormat::Reg10Val14), // regmap.c:906
        (12, 20) => Ok(PackedFormat::Reg12Val20), // regmap.c:916
        _ => Err(FormatError::UnsupportedPackedFormat { reg_bits_with_pad: shifted, val_bits }),
    }
}

fn checked_field(field: &'static str, value: u32, bits: u8) -> Result<(), FormatError> {
    if bits < 32 && value > mask(bits) {
        Err(FormatError::ValueOutOfRange { field, value, bits })
    } else {
        Ok(())
    }
}

/// Pack a register/value pair exactly as Linux's six combined formatters do (regmap.c:212-263).
pub fn pack_combined(
    format: PackedFormat,
    reg: u32,
    val: u32,
    out: &mut [u8],
) -> Result<usize, FormatError> {
    let (reg_bits, val_bits, required) = match format {
        PackedFormat::Reg2Val6 => (2, 6, 1),
        PackedFormat::Reg4Val12 => (4, 12, 2),
        PackedFormat::Reg7Val9 => (7, 9, 2),
        PackedFormat::Reg7Val17 => (7, 17, 3),
        PackedFormat::Reg10Val14 => (10, 14, 3),
        PackedFormat::Reg12Val20 => (12, 20, 4),
    };
    checked_field("register", reg, reg_bits)?;
    checked_field("value", val, val_bits)?;
    if out.len() < required {
        return Err(FormatError::BufferTooSmall { supplied: out.len(), required });
    }
    match format {
        PackedFormat::Reg2Val6 => out[0] = ((reg << 6) | val) as u8, // regmap.c:229
        PackedFormat::Reg4Val12 => out[..2].copy_from_slice(&((reg << 12) | val).to_be_bytes()[2..]), // regmap.c:236
        PackedFormat::Reg7Val9 => out[..2].copy_from_slice(&((reg << 9) | val).to_be_bytes()[2..]), // regmap.c:243
        PackedFormat::Reg7Val17 => { // regmap.c:251-253
            out[2] = val as u8; out[1] = (val >> 8) as u8; out[0] = ((val >> 16) | (reg << 1)) as u8;
        }
        PackedFormat::Reg10Val14 => { // regmap.c:261-263
            out[2] = val as u8; out[1] = ((val >> 8) | (reg << 6)) as u8; out[0] = (reg >> 2) as u8;
        }
        PackedFormat::Reg12Val20 => { // regmap.c:218-221
            out[0] = (reg >> 4) as u8; out[1] = ((reg << 4) | (val >> 16)) as u8; out[2] = (val >> 8) as u8; out[3] = val as u8;
        }
    }
    Ok(required)
}

/// Convert a bit count to Linux's separately formatted width set (regmap.c:923-1031).
pub fn width(bits: u8) -> Result<Width, FormatError> {
    match bits {
        8 => Ok(Width::Bits8),
        16 => Ok(Width::Bits16),
        24 => Ok(Width::Bits24),
        32 => Ok(Width::Bits32),
        _ => Err(FormatError::UnsupportedWidth { bits }),
    }
}

fn validate_endian(width: Width, endian: Endian) -> Result<(), FormatError> {
    let bits = match width { Width::Bits8 => 8, Width::Bits16 => 16, Width::Bits24 => 24, Width::Bits32 => 32 };
    let valid = match width {
        Width::Bits8 => true,
        Width::Bits16 | Width::Bits32 => matches!(endian, Endian::Big | Endian::Little | Endian::Native),
        Width::Bits24 => endian == Endian::Big,
    };
    if valid { Ok(()) } else { Err(FormatError::UnsupportedEndian { bits, endian }) }
}

/// Format one 8/16/24/32-bit word, shifting before conversion like Linux (regmap.c:266-313).
pub fn format_word(
    width: Width,
    endian: Endian,
    value: u32,
    shift: u8,
    out: &mut [u8],
) -> Result<usize, FormatError> {
    validate_endian(width, endian)?;
    let bits: u8 = match width { Width::Bits8 => 8, Width::Bits16 => 16, Width::Bits24 => 24, Width::Bits32 => 32 };
    if shift >= bits {
        return Err(FormatError::ShiftOutOfRange { shift, width: bits });
    }
    checked_field("value", value, bits - shift)?;
    let required = bits as usize / 8;
    if out.len() < required { return Err(FormatError::BufferTooSmall { supplied: out.len(), required }); }
    let shifted = value << shift;
    match width {
        Width::Bits8 => out[0] = shifted as u8, // regmap.c:270
        Width::Bits16 => {
            let bytes = match endian { Endian::Big => (shifted as u16).to_be_bytes(), Endian::Little => (shifted as u16).to_le_bytes(), Endian::Native => (shifted as u16).to_ne_bytes(), Endian::Default => unreachable!() };
            out[..2].copy_from_slice(&bytes);
        }
        Width::Bits24 => out[..3].copy_from_slice(&shifted.to_be_bytes()[1..]), // regmap.c:293
        Width::Bits32 => {
            let bytes = match endian { Endian::Big => shifted.to_be_bytes(), Endian::Little => shifted.to_le_bytes(), Endian::Native => shifted.to_ne_bytes(), Endian::Default => unreachable!() };
            out[..4].copy_from_slice(&bytes);
        }
    }
    Ok(required)
}

/// Pack a separately formatted register, zero pad bytes, and value into Linux's transaction layout
/// (regmap.c:773-780, 869-1031, 1720-1731). The pad remainder shifts the register; whole pad bytes
/// remain zero between register and value because Linux allocates the work buffer zeroed (:1045).
pub fn pack_separate(
    reg_bits: u8,
    val_bits: u8,
    pad_bits: u8,
    reg_endian: Endian,
    val_endian: Endian,
    reg: u32,
    val: u32,
    out: &mut [u8],
) -> Result<usize, FormatError> {
    let layout = layout(reg_bits, val_bits, pad_bits)?;
    if out.len() < layout.buf_size {
        return Err(FormatError::BufferTooSmall { supplied: out.len(), required: layout.buf_size });
    }
    let effective_reg_bits = reg_bits as u16 + layout.reg_shift as u16; // regmap.c:869
    let effective_reg_bits_u8 = u8::try_from(effective_reg_bits)
        .map_err(|_| FormatError::UnsupportedRegisterWidth { bits: effective_reg_bits })?;
    let reg_width = width(effective_reg_bits_u8)
        .map_err(|_| FormatError::UnsupportedRegisterWidth { bits: effective_reg_bits })?;
    let val_width = width(val_bits)?;

    out[..layout.buf_size].fill(0);
    format_word(reg_width, reg_endian, reg, layout.reg_shift, &mut out[..layout.reg_bytes])?;
    let val_start = layout.reg_bytes + layout.pad_bytes; // regmap.c:1645-1646,1926-1927
    format_word(val_width, val_endian, val, 0, &mut out[val_start..layout.buf_size])?;
    Ok(layout.buf_size)
}

/// Parse one 8/16/24/32-bit word as Linux's parsers do (regmap.c:320-376).
pub fn parse_word(width: Width, endian: Endian, input: &[u8]) -> Result<u32, FormatError> {
    validate_endian(width, endian)?;
    let required = match width { Width::Bits8 => 1, Width::Bits16 => 2, Width::Bits24 => 3, Width::Bits32 => 4 };
    if input.len() < required { return Err(FormatError::BufferTooSmall { supplied: input.len(), required }); }
    Ok(match width {
        Width::Bits8 => input[0] as u32, // regmap.c:324
        Width::Bits16 => match endian { Endian::Big => u16::from_be_bytes([input[0], input[1]]) as u32, Endian::Little => u16::from_le_bytes([input[0], input[1]]) as u32, Endian::Native => u16::from_ne_bytes([input[0], input[1]]) as u32, Endian::Default => unreachable!() },
        Width::Bits24 => u32::from_be_bytes([0, input[0], input[1], input[2]]), // regmap.c:355
        Width::Bits32 => match endian { Endian::Big => u32::from_be_bytes([input[0], input[1], input[2], input[3]]), Endian::Little => u32::from_le_bytes([input[0], input[1], input[2], input[3]]), Endian::Native => u32::from_ne_bytes([input[0], input[1], input[2], input[3]]), Endian::Default => unreachable!() },
    })
}
