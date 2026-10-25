// SPDX-License-Identifier: GPL-2.0-only
//! The CPU2 paging image: how the REGULAR image's paging sections are cut into DRAM blocks (`iwl_alloc_fw_paging_mem`,
//! `iwl_fill_paging_mem`) and the `FW_PAGING_BLOCK_CMD` that tells the firmware where they are (`iwl_send_paging_cmd`).
//! Ported from Linux `fw/paging.c:34-296` and `fw/img.h:93-117`, `fw/api/paging.h`.
//!
//! The image layout the code walks (fw/paging.c:118-127): CPU1 sections, `CPU1_CPU2_SEPARATOR_SECTION`, CPU2 sections
//! (not paged), `PAGING_SEPARATOR_SECTION`, then the CPU2 paging CSS (one section) and the CPU2 paging image (one
//! section, including instructions and data).

use crate::encode::{check, put32, EncodeError};
use crate::ids::*;

/// The section table entry the plan needs: the `offset` word and the data length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecInfo {
    pub offset: u32,
    pub len: usize,
}

/// The block layout of a paging image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagingLayout {
    /// `fwrt->num_of_paging_blk`: data blocks (the CSS block is one more). // fw/paging.c:58-61
    pub num_blk: u32,
    /// `fwrt->num_of_pages_in_last_blk`. // fw/paging.c:62-64
    pub pages_in_last: u32,
    /// Section index of the CSS. // fw/paging.c:131-136
    pub css_sec: usize,
    /// Section index of the paging image. // fw/paging.c:158
    pub data_sec: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagingError {
    /// "Paging: Missing CSS and/or paging sections". // fw/paging.c:134-139
    MissingSections { sec_idx: usize, num_sec: usize },
    /// "CSS block is larger than paging size". // fw/paging.c:144-148
    CssTooLarge { len: usize },
    /// "Paging: last block contains more data than expected %d". // fw/paging.c:182-189
    LastBlockMismatch { remaining: usize, expected: usize },
    /// "Paging: not enough data in other in block %d (%d)". // fw/paging.c:190-195
    NotEnoughData { block: u32, remaining: usize },
    /// More blocks than the command can carry. // fw/api/paging.h:10
    TooManyBlocks { blocks: u32 },
    /// A paging size of zero has no layout. // fw/paging.c (iwl_init_paging returns early)
    NoPaging,
}

/// `iwl_alloc_fw_paging_mem`'s counts and `iwl_fill_paging_mem`'s checks, without touching memory.
pub fn plan(secs: &[SecInfo], paging_mem_size: u32) -> Result<PagingLayout, PagingError> {
    if paging_mem_size == 0 {
        return Err(PagingError::NoPaging);
    }
    let num_of_pages = paging_mem_size / FW_PAGING_SIZE;
    let num_blk = num_of_pages.div_ceil(NUM_OF_PAGE_PER_GROUP);
    let pages_in_last = num_of_pages - NUM_OF_PAGE_PER_GROUP * (num_blk - 1);
    if num_blk as usize + 1 > NUM_OF_FW_PAGING_BLOCKS {
        return Err(PagingError::TooManyBlocks { blocks: num_blk });
    }
    let mut sec_idx = 0;
    while sec_idx < secs.len() {
        if secs[sec_idx].offset == PAGING_SEPARATOR_SECTION {
            sec_idx += 1;
            break;
        }
        sec_idx += 1;
    }
    // If paging is enabled there should be at least 2 more sections left (one for CSS and one for Paging data).
    if sec_idx + 1 >= secs.len() {
        return Err(PagingError::MissingSections { sec_idx, num_sec: secs.len() });
    }
    let css_sec = sec_idx;
    if secs[css_sec].len > FW_PAGING_SIZE as usize {
        return Err(PagingError::CssTooLarge { len: secs[css_sec].len });
    }
    let data_sec = css_sec + 1;
    let data_len = secs[data_sec].len;
    let mut offset = 0usize;
    for idx in 1..num_blk + 1 {
        let remaining = data_len.saturating_sub(offset);
        if idx == num_blk {
            let expected = (pages_in_last * FW_PAGING_SIZE) as usize;
            if data_len < offset || remaining != expected {
                return Err(PagingError::LastBlockMismatch { remaining, expected });
            }
        } else if (PAGING_BLOCK_SIZE as usize) > remaining {
            return Err(PagingError::NotEnoughData { block: idx, remaining });
        }
        offset += PAGING_BLOCK_SIZE as usize;
    }
    Ok(PagingLayout { num_blk, pages_in_last, css_sec, data_sec })
}

impl PagingLayout {
    /// DRAM block count including the CSS block 0. // fw/paging.c:67 (`num_of_paging_blk + 1`)
    pub fn dram_blocks(&self) -> usize {
        self.num_blk as usize + 1
    }

    /// The size of DRAM block `blk`: 4 KiB for the CSS, 32 KiB for the rest. // fw/paging.c:69-71
    pub fn block_size(&self, blk: usize) -> usize {
        if blk == 0 {
            FW_PAGING_SIZE as usize
        } else {
            PAGING_BLOCK_SIZE as usize
        }
    }

    /// Where data block `blk` (1-based among the paging blocks) is read from in the paging section, and how many bytes
    /// are copied: the last block copies what remains, the others a full block. // fw/paging.c:163-205
    pub fn copy_range(&self, blk: u32) -> (usize, usize) {
        let off = (blk as usize - 1) * PAGING_BLOCK_SIZE as usize;
        let len = if blk == self.num_blk { (self.pages_in_last * FW_PAGING_SIZE) as usize } else { PAGING_BLOCK_SIZE as usize };
        (off, len)
    }
}

/// `iwl_send_paging_cmd`: `struct iwl_fw_paging_cmd { le32 flags; le32 block_size; le32 block_num; le32
/// device_phy_addr[33]; }`; `flags = IS_SECURED | IS_ENABLED | pages_in_last << 0`, `block_size = BLOCK_2_EXP_SIZE`,
/// `block_num = num_blk`, each address `phys >> PAGE_2_EXP_SIZE`. `phys[0]` is the CSS block.
/// // fw/paging.c:206-240; fw/api/paging.h:19-27
pub fn encode_fw_paging_cmd(l: &PagingLayout, phys: &[u64], buf: &mut [u8]) -> Result<usize, EncodeError> {
    if phys.len() != l.dram_blocks() {
        return Err(EncodeError::TooManyPagingBlocks { n: phys.len() });
    }
    check(buf, FW_PAGING_CMD_LEN)?;
    put32(buf, 0, PAGING_CMD_IS_SECURED | PAGING_CMD_IS_ENABLED | (l.pages_in_last << PAGING_CMD_NUM_OF_PAGES_IN_LAST_GRP_POS));
    put32(buf, 4, BLOCK_2_EXP_SIZE);
    put32(buf, 8, l.num_blk);
    for (i, p) in phys.iter().enumerate() {
        put32(buf, 12 + 4 * i, (*p >> PAGE_2_EXP_SIZE) as u32);
    }
    Ok(FW_PAGING_CMD_LEN)
}
