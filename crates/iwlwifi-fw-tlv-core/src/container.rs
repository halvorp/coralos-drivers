// SPDX-License-Identifier: GPL-2.0-only
//! The TLV ucode file header and the TLV walk, ported from Linux
//! `drivers/net/wireless/intel/iwlwifi/fw/file.h:126-156` and
//! `drivers/net/wireless/intel/iwlwifi/iwl-drv.c:807-855`.
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation, and the Linux iwlwifi authors.

/// `struct iwl_tlv_ucode_header` magic. // file.h:132
pub const IWL_TLV_UCODE_MAGIC: u32 = 0x0a4c5749; // file.h:132
pub const FW_VER_HUMAN_READABLE_SZ: usize = 64; // file.h:133
/// sizeof(struct iwl_tlv_ucode_header): zero 4 + magic 4 + human_readable 64 + ver 4 + build 4 +
/// ignore 8. // file.h:135-156
pub const TLV_UCODE_HEADER_LEN: usize = 4 + 4 + FW_VER_HUMAN_READABLE_SZ + 4 + 4 + 8;
/// sizeof(struct iwl_ucode_tlv): type 4 + length 4. // file.h:126-130
pub const TLV_HEADER_LEN: usize = 8;

/// The header fields a driver reports. // file.h:135-156; iwl-drv.c:818-821
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UcodeHeader<'a> {
    pub ver: u32,
    pub build: u32,
    pub human_readable: &'a [u8],
}

/// One TLV: its type and its UNPADDED payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tlv<'a> {
    pub ty: u32,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerError {
    /// "uCode has invalid length" — shorter than the header. // iwl-drv.c:807-810
    TooShort { len: usize, need: usize },
    /// The first word is not zero: a v1/v2 header, not the TLV format. // file.h:136-142
    NotTlvFormat { zero: u32 },
    /// "invalid uCode magic". // iwl-drv.c:812-816
    BadMagic { magic: u32 },
    /// "invalid TLV len" — a TLV claims more payload than remains. // iwl-drv.c:849-853
    TlvOverrun { offset: usize, ty: u32, tlv_len: u32, remaining: usize },
    /// The payload fits but its 4-byte padding does not. Linux's `len -= ALIGN(tlv_len, 4)`
    /// (iwl-drv.c:854) underflows a size_t here and walks on past the buffer; the port refuses.
    PaddingOverrun { offset: usize, ty: u32, tlv_len: u32, remaining: usize },
}

fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Validate the header and return it. // iwl-drv.c:807-821
pub fn parse_header(blob: &[u8]) -> Result<UcodeHeader<'_>, ContainerError> {
    if blob.len() < TLV_UCODE_HEADER_LEN {
        return Err(ContainerError::TooShort { len: blob.len(), need: TLV_UCODE_HEADER_LEN });
    }
    let zero = le32(blob, 0);
    if zero != 0 {
        return Err(ContainerError::NotTlvFormat { zero });
    }
    let magic = le32(blob, 4);
    if magic != IWL_TLV_UCODE_MAGIC {
        return Err(ContainerError::BadMagic { magic });
    }
    let hr = 8;
    Ok(UcodeHeader {
        human_readable: &blob[hr..hr + FW_VER_HUMAN_READABLE_SZ],
        ver: le32(blob, hr + FW_VER_HUMAN_READABLE_SZ),
        build: le32(blob, hr + FW_VER_HUMAN_READABLE_SZ + 4),
    })
}

/// The TLV walk of iwl-drv.c:841-855 as an iterator: it stops when fewer than 8 bytes remain (the
/// loop condition), and yields one error, then ends, on a TLV that does not fit.
pub struct Tlvs<'a> {
    blob: &'a [u8],
    off: usize,
    failed: bool,
}

/// Validate the header, then iterate the TLVs after it.
pub fn tlvs(blob: &[u8]) -> Result<Tlvs<'_>, ContainerError> {
    parse_header(blob)?;
    Ok(Tlvs { blob, off: TLV_UCODE_HEADER_LEN, failed: false })
}

impl<'a> Iterator for Tlvs<'a> {
    type Item = Result<Tlv<'a>, ContainerError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.blob.len() - self.off < TLV_HEADER_LEN {
            return None; // iwl-drv.c:841
        }
        let ty = le32(self.blob, self.off);
        let tlv_len = le32(self.blob, self.off + 4);
        let start = self.off + TLV_HEADER_LEN;
        let remaining = self.blob.len() - start; // iwl-drv.c:842
        let offset = self.off;
        if tlv_len as usize > remaining {
            self.failed = true;
            return Some(Err(ContainerError::TlvOverrun { offset, ty, tlv_len, remaining }));
        }
        let padded = (tlv_len as usize).next_multiple_of(4); // ALIGN(tlv_len, 4), iwl-drv.c:854
        if padded > remaining {
            self.failed = true;
            return Some(Err(ContainerError::PaddingOverrun { offset, ty, tlv_len, remaining }));
        }
        self.off = start + padded; // iwl-drv.c:855
        Some(Ok(Tlv { ty, data: &self.blob[start..start + tlv_len as usize] }))
    }
}
