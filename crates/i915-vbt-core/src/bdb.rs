// SPDX-License-Identifier: GPL-2.0-only
//! VBT header validation, BDB header and the block walk: `intel_bios_is_valid_vbt()` (intel_bios.c:3064),
//! `find_raw_section()` / `_get_blocksize()` (intel_bios.c:83-131), struct vbt_header / bdb_header
//! (intel_vbt_defs.h:97-123).

use crate::error::VbtError;

/// sizeof(struct vbt_header): signature[20], version, header_size, vbt_size u16, checksum u8, reserved0 u8,
/// bdb_offset u32, aim_offset[4] — intel_vbt_defs.h:97-108 (= 48; real VBTs say bdb_offset 0x30).
pub const VBT_HEADER_SIZE: usize = 48;
/// sizeof(struct bdb_header): signature[16], version, header_size, bdb_size u16 — intel_vbt_defs.h:115-121.
pub const BDB_HEADER_SIZE: usize = 22;
/// `vbt_signature[] = "$VBT"` — intel_bios.c:3053.
pub const VBT_SIGNATURE: &[u8; 4] = b"$VBT";
/// The BDB signature "BIOS_DATA_BLOCK" (intel_vbt_defs.h:107 doc); Linux never checks it, so we only report it.
pub const BDB_SIGNATURE: &[u8; 15] = b"BIOS_DATA_BLOCK";

/// `enum bdb_block_id` — intel_vbt_defs.h:141-198 (the ones this crate reads, plus the rest for the census).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockId {
    GeneralFeatures = 1,
    GeneralDefinitions = 2,
    /// BDB_PSR = 9 (165+) — intel_vbt_defs.h:153 (same value as BDB_DOT_CLOCK_OVERRIDE_ALM).
    Psr = 9,
    DriverFeatures = 12,
    Edp = 27,
    LfpOptions = 40,
    LfpDataPtrs = 41,
    LfpData = 42,
    LfpBacklight = 43,
    LfpPower = 44,
    MipiSequence = 53,
    GenericDtd = 58,
}

/// One block: its id and the data AFTER the 3-byte block header. Reads past the end return zero, mirroring
/// Linux's copy into a zero-padded `max(min_size, block_size)` buffer (init_bdb_block, intel_bios.c:456-512).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block<'a> {
    pub id: u8,
    pub data: &'a [u8],
    /// Offset of `data` from the start of the BDB (`raw_block_offset()`, intel_bios.c:133).
    pub bdb_offset: u32,
}

impl<'a> Block<'a> {
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn u8_at(&self, off: usize) -> u8 {
        self.data.get(off).copied().unwrap_or(0)
    }
    pub fn u16_at(&self, off: usize) -> u16 {
        u16::from(self.u8_at(off)) | (u16::from(self.u8_at(off + 1)) << 8)
    }
    pub fn u32_at(&self, off: usize) -> u32 {
        u32::from(self.u16_at(off)) | (u32::from(self.u16_at(off + 2)) << 16)
    }
    pub fn u64_at(&self, off: usize) -> u64 {
        u64::from(self.u32_at(off)) | (u64::from(self.u32_at(off + 4)) << 32)
    }
    /// Fail with `BlockTooSmall` when the block is shorter than `need` (a parser's required minimum).
    pub fn require(&self, need: usize) -> Result<(), VbtError> {
        if self.data.len() < need {
            Err(VbtError::BlockTooSmall { id: self.id, size: self.data.len(), need })
        } else {
            Ok(())
        }
    }
}

/// A validated VBT: header fields + a view of the BDB.
#[derive(Debug, Clone, Copy)]
pub struct Vbt<'a> {
    bdb: &'a [u8],
    /// vbt_header.version
    pub vbt_version: u16,
    /// vbt_header.vbt_size
    pub vbt_size: u16,
    /// bdb_header.version == `display->vbt.version` (intel_bios.c:3256).
    pub version: u16,
    pub bdb_header_size: u16,
    pub bdb_size: u16,
    /// 8-bit sum of the first vbt_size bytes is zero (Linux does not enforce it; reported only).
    pub checksum_ok: bool,
    /// bdb_header.signature starts with "BIOS_DATA_BLOCK" (Linux does not enforce it; reported only).
    pub bdb_signature_ok: bool,
}

fn le16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn le32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

impl<'a> Vbt<'a> {
    /// `intel_bios_is_valid_vbt()` (intel_bios.c:3064-3106), then the BDB header size sanity Linux omits.
    pub fn parse(buf: &'a [u8]) -> Result<Vbt<'a>, VbtError> {
        if buf.len() < VBT_HEADER_SIZE {
            return Err(VbtError::HeaderIncomplete { have: buf.len(), need: VBT_HEADER_SIZE });
        }
        if &buf[..VBT_SIGNATURE.len()] != VBT_SIGNATURE {
            return Err(VbtError::BadSignature);
        }
        // signature[20] @0, version @20, header_size @22, vbt_size @24, checksum @26, reserved0 @27, bdb_offset @28
        let vbt_size = le16(buf, 24) as usize;
        if vbt_size > buf.len() {
            return Err(VbtError::VbtSizeOverflows { vbt_size, buffer: buf.len() });
        }
        let vbt = &buf[..vbt_size];
        let bdb_offset = le32(buf, 28) as usize;
        if bdb_offset.checked_add(BDB_HEADER_SIZE).map_or(true, |e| e > vbt_size) {
            return Err(VbtError::BdbHeaderIncomplete { bdb_offset, vbt_size });
        }
        // signature[16] @0, version @16, header_size @18, bdb_size @20
        let h = &vbt[bdb_offset..];
        let bdb_size = le16(h, 20) as usize;
        if bdb_offset.checked_add(bdb_size).map_or(true, |e| e > vbt_size) {
            return Err(VbtError::BdbIncomplete { bdb_offset, bdb_size, vbt_size });
        }
        let header_size = le16(h, 18) as usize;
        if header_size < BDB_HEADER_SIZE || header_size > bdb_size {
            return Err(VbtError::BadBdbHeaderSize { header_size, bdb_size });
        }
        let sum = vbt.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        Ok(Vbt {
            bdb: &vbt[bdb_offset..bdb_offset + bdb_size],
            vbt_version: le16(buf, 20),
            vbt_size: vbt_size as u16,
            version: le16(h, 16),
            bdb_header_size: header_size as u16,
            bdb_size: bdb_size as u16,
            checksum_ok: sum == 0,
            bdb_signature_ok: h[..BDB_SIGNATURE.len()] == BDB_SIGNATURE[..],
        })
    }

    /// The BDB bytes (header included), exactly `bdb_size` long.
    pub fn bdb(&self) -> &'a [u8] {
        self.bdb
    }

    /// `_get_blocksize()` for the block header at `index` (intel_bios.c:83-91): the MIPI sequence block v3+ has a
    /// separate u32 size field; everything else a u16 at +1.
    fn block_size_at(&self, index: usize) -> Result<usize, VbtError> {
        let b = self.bdb;
        if b[index] == BlockId::MipiSequence as u8 && b.get(index + 3).copied().unwrap_or(0) >= 3 {
            if index + 8 > b.len() {
                return Err(VbtError::MipiSizeFieldTruncated { offset: index });
            }
            Ok(le32(b, index + 4) as usize)
        } else {
            Ok(le16(b, index + 1) as usize)
        }
    }

    /// Iterate every block in order: `find_raw_section()`'s loop (intel_bios.c:98-125). A block that overflows the
    /// BDB ends the walk with its named error (Linux returns NULL and so hides every later block).
    pub fn blocks(&self) -> Blocks<'a> {
        Blocks { vbt: *self, index: self.bdb_header_size as usize, done: false }
    }

    /// `find_raw_section()`: the first block with `id`, `Ok(None)` if absent.
    pub fn find_block(&self, id: u8) -> Result<Option<Block<'a>>, VbtError> {
        for b in self.blocks() {
            let b = b?;
            if b.id == id {
                return Ok(Some(b));
            }
        }
        Ok(None)
    }

    /// Like `find_block` but absent is `BlockMissing`.
    pub fn require_block(&self, id: u8) -> Result<Block<'a>, VbtError> {
        self.find_block(id)?.ok_or(VbtError::BlockMissing { id })
    }
}

/// Block iterator, see [`Vbt::blocks`].
pub struct Blocks<'a> {
    vbt: Vbt<'a>,
    index: usize,
    done: bool,
}

impl<'a> Iterator for Blocks<'a> {
    type Item = Result<Block<'a>, VbtError>;
    fn next(&mut self) -> Option<Self::Item> {
        let total = self.vbt.bdb.len();
        // `while (index + 3 < total)` — intel_bios.c:109
        if self.done || self.index + 3 >= total {
            return None;
        }
        let id = self.vbt.bdb[self.index];
        let size = match self.vbt.block_size_at(self.index) {
            Ok(s) => s,
            Err(e) => {
                self.done = true;
                return Some(Err(e));
            }
        };
        let start = self.index + 3;
        // `if (index + current_size > total) return NULL;` — intel_bios.c:117
        if start + size > total {
            self.done = true;
            return Some(Err(VbtError::BlockOverflows { id, size, offset: start, bdb_size: total }));
        }
        self.index = start + size;
        Some(Ok(Block { id, data: &self.vbt.bdb[start..start + size], bdb_offset: start as u32 }))
    }
}
