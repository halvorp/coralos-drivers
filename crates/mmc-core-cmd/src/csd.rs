// SPDX-License-Identifier: GPL-2.0-only
//! R2 bit extraction and MMC CSD decoding.
//!
//! Ported from Linux `drivers/mmc/core/mmc.c` and `mmc_ops.h`.
//! Copyright (C) 2003-2004 Russell King; 2005-2007 Pierre Ossman; MMCv4
//! support Copyright (C) 2006 Philip Langdale.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldDef { pub name: &'static str, pub start: u8, pub size: u8 }
pub const CSD_FIELDS: [FieldDef; 21] = [
    FieldDef { name: "STRUCTURE", start: 126, size: 2 }, // mmc.c:161
    FieldDef { name: "MMCA_VSN", start: 122, size: 4 }, // mmc.c:168
    FieldDef { name: "TAAC_MANT", start: 115, size: 4 }, // mmc.c:169
    FieldDef { name: "TAAC_EXP", start: 112, size: 3 }, // mmc.c:170
    FieldDef { name: "TAAC_CLKS", start: 104, size: 8 }, // mmc.c:172
    FieldDef { name: "TRAN_MANT", start: 99, size: 4 }, // mmc.c:174
    FieldDef { name: "TRAN_EXP", start: 96, size: 3 }, // mmc.c:175
    FieldDef { name: "CMDCLASS", start: 84, size: 12 }, // mmc.c:177
    FieldDef { name: "READ_BLKBITS", start: 80, size: 4 }, // mmc.c:183
    FieldDef { name: "READ_PARTIAL", start: 79, size: 1 }, // mmc.c:184
    FieldDef { name: "WRITE_MISALIGN", start: 78, size: 1 }, // mmc.c:185
    FieldDef { name: "READ_MISALIGN", start: 77, size: 1 }, // mmc.c:186
    FieldDef { name: "DSR_IMP", start: 76, size: 1 }, // mmc.c:187
    FieldDef { name: "C_SIZE", start: 62, size: 12 }, // mmc.c:180
    FieldDef { name: "C_SIZE_MULT", start: 47, size: 3 }, // mmc.c:179
    FieldDef { name: "ERASE_GRP_SIZE", start: 42, size: 5 }, // mmc.c:193
    FieldDef { name: "ERASE_GRP_MULT", start: 37, size: 5 }, // mmc.c:194
    FieldDef { name: "WP_GRP_SIZE", start: 32, size: 5 }, // mmc.c:197
    FieldDef { name: "R2W_FACTOR", start: 26, size: 3 }, // mmc.c:188
    FieldDef { name: "WRITE_BLKBITS", start: 22, size: 4 }, // mmc.c:189
    FieldDef { name: "WRITE_PARTIAL", start: 21, size: 1 }, // mmc.c:190
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractError { StartOutOfRange { start: u8, maximum: u8 }, SizeOutOfRange { size: u8, maximum: u8 }, FieldPastResponse { start: u8, size: u8, response_bits: u16 } }

pub fn extract_bits(resp: &[u32; 4], start: u8, size: u8) -> Result<u32, ExtractError> {
    if start > 127 { return Err(ExtractError::StartOutOfRange { start, maximum: 127 }); }
    if size == 0 || size > 32 { return Err(ExtractError::SizeOutOfRange { size, maximum: 32 }); }
    if u16::from(start) + u16::from(size) > 128 { return Err(ExtractError::FieldPastResponse { start, size, response_bits: 128 }); }
    let off = 3usize - usize::from(start / 32);
    let shift = u32::from(start & 31);
    let mut value = resp[off] >> shift;
    if u32::from(size) + shift > 32 { value |= resp[off - 1] << ((32 - shift) % 32); }
    let mask = if size == 32 { u32::MAX } else { (1u32 << size) - 1 };
    Ok(value & mask)
}

const TRAN_EXP: [u32; 8] = [10_000, 100_000, 1_000_000, 10_000_000, 0, 0, 0, 0]; // mmc.c:43-46
const TRAN_MANT: [u32; 16] = [0, 10, 12, 13, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 70, 80]; // mmc.c:48-51
const TAAC_EXP: [u32; 8] = [1, 10, 100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000]; // mmc.c:53-55
const TAAC_MANT: [u32; 16] = [0, 10, 12, 13, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 70, 80]; // mmc.c:57-60

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Csd { pub structure: u8, pub mmca_vsn: u8, pub taac_ns: u32, pub taac_clks: u32, pub max_dtr: u32, pub cmdclass: u16, pub capacity: u64, pub read_blkbits: u8, pub read_partial: bool, pub write_misalign: bool, pub read_misalign: bool, pub dsr_imp: bool, pub r2w_factor: u8, pub write_blkbits: u8, pub write_partial: bool, pub erase_size: u32, pub wp_grp_size: u8 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsdError { Extract(ExtractError), UnrecognisedStructure { value: u8, forbidden: u8 } }
impl From<ExtractError> for CsdError { fn from(value: ExtractError) -> Self { Self::Extract(value) } }

pub fn decode(resp: &[u32; 4]) -> Result<Csd, CsdError> {
    let structure = extract_bits(resp, 126, 2)? as u8;
    if structure == 0 { return Err(CsdError::UnrecognisedStructure { value: structure, forbidden: 0 }); }
    let taac_m = extract_bits(resp, 115, 4)? as usize; let taac_e = extract_bits(resp, 112, 3)? as usize;
    let tran_m = extract_bits(resp, 99, 4)? as usize; let tran_e = extract_bits(resp, 96, 3)? as usize;
    let c_mult = extract_bits(resp, 47, 3)?; let c_size = extract_bits(resp, 62, 12)?;
    let write_blkbits = extract_bits(resp, 22, 4)? as u8;
    let (erase_size, wp_grp_size) = if write_blkbits >= 9 {
        let mut erase = (extract_bits(resp, 42, 5)? + 1) * (extract_bits(resp, 37, 5)? + 1);
        erase <<= write_blkbits - 9; (erase, extract_bits(resp, 32, 5)? as u8)
    } else { (0, 0) };
    Ok(Csd { structure, mmca_vsn: extract_bits(resp, 122, 4)? as u8,
        taac_ns: (TAAC_EXP[taac_e] * TAAC_MANT[taac_m] + 9) / 10,
        taac_clks: extract_bits(resp, 104, 8)? * 100, max_dtr: TRAN_EXP[tran_e] * TRAN_MANT[tran_m],
        cmdclass: extract_bits(resp, 84, 12)? as u16, capacity: u64::from((1 + c_size) << (c_mult + 2)),
        read_blkbits: extract_bits(resp, 80, 4)? as u8, read_partial: extract_bits(resp, 79, 1)? != 0,
        write_misalign: extract_bits(resp, 78, 1)? != 0, read_misalign: extract_bits(resp, 77, 1)? != 0,
        dsr_imp: extract_bits(resp, 76, 1)? != 0, r2w_factor: extract_bits(resp, 26, 3)? as u8,
        write_blkbits, write_partial: extract_bits(resp, 21, 1)? != 0, erase_size, wp_grp_size })
}
