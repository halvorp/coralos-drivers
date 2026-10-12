// SPDX-License-Identifier: GPL-2.0-only
//! The buffer descriptor list of a host stream: `setup_bdle` and `snd_hdac_stream_setup_bdle` (stream.c:451-567) as a pure function of
//! the physical extents behind the buffer. One entry per physically contiguous chunk of a period; the entry that ends a period carries
//! IOC when the period raises an interrupt. AVS sets `bdl_pos_adj = 0` (avs/core.c:391), so the position-adjustment first entry
//! (stream.c:521-545) is not planned; `align_bdle_4k` is a VIA/CTHDA quirk and off.
//!
//! The entry count is fixed by HARDWARE: the list lives in one 4 KiB page of 16-byte entries (hda_register.h:144-145,
//! [`crate::regs::BDL_SIZE`]); `LVI` is an 8-bit index. Linux returns -EINVAL at the 257th entry (stream.c:462).
use crate::error::HdaError;
use crate::consts::AZX_MAX_BDL_ENTRIES;
use alloc::vec::Vec;
use avs_fw_core::bdl::{BdlEntry, Extent};

/// The list and the numbers the controller is told about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bdl {
    pub entries: Vec<BdlEntry>,
    pub buffer_size: u32,
    pub period_bytes: u32,
}

impl Bdl {
    /// `SD_LVI` = `frags - 1` (stream.c:287).
    pub fn lvi(&self) -> u32 {
        self.entries.len() as u32 - 1
    }
    /// The 16-byte entries as the controller reads them from the BDL page (address low/high, length, IOC).
    pub fn write_to(&self, out: &mut [u8]) -> Result<usize, HdaError> {
        let need = self.entries.len() * BDL_ENTRY_BYTES;
        if out.len() < need {
            return Err(HdaError::BdlOverflow { entries: self.entries.len() as u32, max: (out.len() / BDL_ENTRY_BYTES) as u32 });
        }
        for (i, e) in self.entries.iter().enumerate() {
            out[i * BDL_ENTRY_BYTES..(i + 1) * BDL_ENTRY_BYTES].copy_from_slice(&e.to_le_bytes());
        }
        Ok(need)
    }
}

/// `sizeof` of one BDL entry (hda_register.h:145: `BDL_SIZE / 16`).
pub const BDL_ENTRY_BYTES: usize = 16;

/// The physically contiguous run starting at logical `offset`, merging touching extents (`snd_sgbuf_get_chunk_size`).
fn contiguous_from(extents: &[Extent], offset: u64) -> Option<(u64, u64)> {
    let mut start = 0u64;
    for (i, e) in extents.iter().enumerate() {
        let end = start + e.len as u64;
        if offset < end {
            let into = offset - start;
            let addr = e.addr + into;
            let mut run = e.len as u64 - into;
            let mut prev_end = e.addr + e.len as u64;
            for next in &extents[i + 1..] {
                if next.addr != prev_end {
                    break;
                }
                run += next.len as u64;
                prev_end += next.len as u64;
            }
            return Some((addr, run));
        }
        start = end;
    }
    None
}

/// Plan the BDL of a `buffer_size`-byte cyclic buffer made of `period_bytes` periods. `wakeups` is `!no_period_wakeup`: with it every period
/// ends in an IOC entry; without it none does (stream.c:483).
pub fn plan_bdl(extents: &[Extent], buffer_size: u32, period_bytes: u32, wakeups: bool) -> Result<Bdl, HdaError> {
    if buffer_size == 0 || period_bytes == 0 {
        return Err(HdaError::EmptyBuffer);
    }
    if buffer_size % period_bytes != 0 {
        return Err(HdaError::BufferNotWholePeriods { buffer: buffer_size, period: period_bytes });
    }
    let have: u64 = extents.iter().map(|e| e.len as u64).sum();
    if have < buffer_size as u64 {
        return Err(HdaError::ExtentsShort { have, need: buffer_size as u64 });
    }
    let periods = buffer_size / period_bytes; // stream.c:514
    let mut entries: Vec<BdlEntry> = Vec::new();
    let mut ofs = 0u64;
    for _ in 0..periods {
        let mut size = period_bytes as u64;
        while size > 0 {
            if entries.len() as u32 >= AZX_MAX_BDL_ENTRIES {
                return Err(HdaError::BdlOverflow { entries: entries.len() as u32 + 1, max: AZX_MAX_BDL_ENTRIES }); // stream.c:462
            }
            let (addr, run) = contiguous_from(extents, ofs).ok_or(HdaError::ExtentsShort { have, need: buffer_size as u64 })?;
            let chunk = run.min(size);
            size -= chunk;
            entries.try_reserve(1).map_err(|_| HdaError::OutOfMemory { what: "buffer descriptor list" })?;
            // stream.c:483: IOC only when the whole fragment is processed
            entries.push(BdlEntry { addr, len: chunk as u32, ioc: size == 0 && wakeups });
            ofs += chunk;
        }
    }
    Ok(Bdl { entries, buffer_size, period_bytes })
}
