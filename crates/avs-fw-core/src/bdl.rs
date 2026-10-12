// SPDX-License-Identifier: GPL-2.0-only
//! The buffer descriptor list of the code-loader DMA: `cldma_setup_bdle` (cldma.c:205-232) as a pure function of the physical
//! extents that back the data buffer. The BDL is two periods (`bdle_size = buffer_size / 2`), the last entry carries IOC.
use crate::error::FwError;
use crate::regs::*;

/// One physically contiguous run of the CLDMA data buffer (a scatter-gather page run, `SNDRV_DMA_TYPE_DEV_SG`, cldma.c
/// `hda_cldma_init`). Logical order is the slice order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub addr: u64,
    pub len: u32,
}

/// One BDL entry: address low, address high, length, IOC - four little-endian dwords (cldma.c `cldma_setup_bdle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BdlEntry {
    pub addr: u64,
    pub len: u32,
    pub ioc: bool,
}

impl BdlEntry {
    pub const ZERO: BdlEntry = BdlEntry { addr: 0, len: 0, ioc: false };
    /// The 16 bytes the controller reads from the BDL page.
    pub fn to_le_bytes(&self) -> [u8; BDL_ENTRY_SIZE as usize] {
        let mut b = [0u8; BDL_ENTRY_SIZE as usize];
        b[0..4].copy_from_slice(&(self.addr as u32).to_le_bytes()); // lower_32_bits (cldma.c:219-220)
        b[4..8].copy_from_slice(&((self.addr >> 32) as u32).to_le_bytes()); // upper_32_bits
        b[8..12].copy_from_slice(&self.len.to_le_bytes());
        b[12..16].copy_from_slice(&(self.ioc as u32).to_le_bytes()); // `cpu_to_le32(0x01)` on the last entry only
        b
    }
}

/// Where the code loader's DMA lives and how the controller is told about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClDmaLayout {
    entries: [BdlEntry; AZX_MAX_BDL_ENTRIES as usize],
    count: u32,
    pub buffer_size: u32,
    /// Bus address of the BDL page (`cl->dmab_bdl.addr`).
    pub bdl_addr: u64,
    /// `cl->stream_tag` = `AVS_CL_STREAM_INDEX + 1` (cldma.c:54-55).
    pub stream_tag: u32,
}

/// Bus address and length of the physically contiguous run starting at logical `offset`, merging touching extents the way
/// `snd_sgbuf_get_chunk_size` walks pages.
fn contiguous_from(extents: &[Extent], offset: u64) -> Option<(u64, u64)> {
    let mut start = 0u64;
    let mut i = 0;
    while i < extents.len() {
        let end = start + extents[i].len as u64;
        if offset < end {
            let into = offset - start;
            let addr = extents[i].addr + into;
            let mut run = extents[i].len as u64 - into;
            let mut j = i + 1;
            let mut tail = extents[i].addr + extents[i].len as u64;
            while j < extents.len() && extents[j].addr == tail {
                run += extents[j].len as u64;
                tail += extents[j].len as u64;
                j += 1;
            }
            return Some((addr, run));
        }
        start = end;
        i += 1;
    }
    None
}

impl ClDmaLayout {
    /// `cldma_setup_bdle(cl, cl->buffer_size / 2)`.
    ///
    /// Refusals Linux lacks: a buffer of fewer than 2 bytes (its `bdle_size` is 0 and the loop never ends), more entries than
    /// the 4 KiB BDL page holds, extents that do not cover the buffer, and a stream tag wider than the 4-bit `SD_CTL` field.
    /// The final chunk is clamped to the bytes remaining (Linux lets an odd buffer overshoot by up to `bdle_size - 1`).
    pub fn plan(extents: &[Extent], bdl_addr: u64, buffer_size: u32, stream_tag: u32) -> Result<Self, FwError> {
        if stream_tag > (AZX_SD_CTL_STRM_MASK >> AZX_SD_CTL_STRM_SHIFT) {
            return Err(FwError::StreamTagWidth { tag: stream_tag });
        }
        let bdle_size = buffer_size / 2;
        if bdle_size == 0 {
            return Err(FwError::CldmaBufferTooSmall { buffer_size });
        }
        let mut entries = [BdlEntry::ZERO; AZX_MAX_BDL_ENTRIES as usize];
        let mut count = 0u32;
        let mut remaining = buffer_size as u64;
        let mut offset = 0u64;
        while remaining > 0 {
            if count >= AZX_MAX_BDL_ENTRIES {
                return Err(FwError::BdlOverflow { entries: count + 1, max: AZX_MAX_BDL_ENTRIES });
            }
            let Some((addr, run)) = contiguous_from(extents, offset) else {
                return Err(FwError::ExtentsShort { covered: offset, buffer_size });
            };
            let chunk = run.min(bdle_size as u64).min(remaining);
            remaining -= chunk;
            entries[count as usize] = BdlEntry { addr, len: chunk as u32, ioc: remaining == 0 }; // IOC only on the last entry
            count += 1;
            offset += chunk;
        }
        Ok(Self { entries, count, buffer_size, bdl_addr, stream_tag })
    }

    /// `cl->num_periods`.
    pub const fn num_periods(&self) -> u32 {
        self.count
    }
    pub fn entries(&self) -> &[BdlEntry] {
        &self.entries[..self.count as usize]
    }
    /// What `SD_LVI` is told: the index of the last entry (cldma.c `snd_hdac_stream_writeb(cl, SD_LVI, cl->num_periods - 1)`).
    pub const fn lvi(&self) -> u32 {
        self.count - 1
    }
}
