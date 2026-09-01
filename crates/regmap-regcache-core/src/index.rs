// SPDX-License-Identifier: GPL-2.0-only
//! FLAT and RBTREE cache block index/range arithmetic.
//!
//! Ported from Linux `drivers/base/regmap/regcache-flat.c:19-22,30-38,146-155`,
//! `drivers/base/regmap/regcache-rbtree.c:39-45,244-278,465-540`, and
//! `drivers/base/regmap/internal.h:317-321`.
//!
//! Copyright 2011, 2012 Wolfson Microelectronics plc.
//! Original authors: Dimitris Papastamos and Mark Brown.

use core::fmt;

/// A half-open block-index interval `[start, end)` as used by RBTREE sync/drop
/// (regcache-rbtree.c:488-500,530-540).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRange {
    pub start: usize,
    pub end: usize,
}

/// Named arithmetic refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexError {
    StrideOrderOutOfRange { order: u32 },
    ZeroStride,
    EmptyBlock,
    RegisterBelowBlock { reg: u32, base: u32 },
    RangeReversed { min: u32, max: u32 },
    BlockTopOverflow { base: u32, len: usize, stride: u32 },
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::StrideOrderOutOfRange { order } => write!(
                f,
                "flat cache index refused stride order {order}: it must be smaller than 32"
            ),
            Self::ZeroStride => {
                f.write_str("regcache block arithmetic refused stride 0: stride must be nonzero")
            }
            Self::EmptyBlock => {
                f.write_str("regcache block arithmetic refused length 0: a block must contain a register")
            }
            Self::RegisterBelowBlock { reg, base } => write!(
                f,
                "rbtree block index refused register {reg:#x}: it is below block base {base:#x}"
            ),
            Self::RangeReversed { min, max } => write!(
                f,
                "regcache range refused {min:#x}..={max:#x}: minimum exceeds maximum"
            ),
            Self::BlockTopOverflow { base, len, stride } => write!(
                f,
                "rbtree block refused base {base:#x}, length {len}, stride {stride}: top register overflows u32"
            ),
        }
    }
}

/// `reg >> reg_stride_order` (internal.h:317-321; regcache-flat.c:19-22).
pub fn flat_index(reg: u32, reg_stride_order: u32) -> Result<usize, IndexError> {
    if reg_stride_order >= 32 {
        return Err(IndexError::StrideOrderOutOfRange {
            order: reg_stride_order,
        });
    }
    Ok((reg >> reg_stride_order) as usize)
}

/// FLAT allocation count, `index(max_register) + 1` (regcache-flat.c:30-39).
pub fn flat_entry_count(max_register: u32, reg_stride_order: u32) -> Result<usize, IndexError> {
    Ok(flat_index(max_register, reg_stride_order)? + 1)
}

/// Inclusive FLAT drop converted to bitmap's `(start, count)`
/// (regcache-flat.c:146-154).
pub fn flat_drop_span(
    min: u32,
    max: u32,
    reg_stride_order: u32,
) -> Result<(usize, usize), IndexError> {
    if min > max {
        return Err(IndexError::RangeReversed { min, max });
    }
    let bitmap_min = flat_index(min, reg_stride_order)?;
    let bitmap_max = flat_index(max, reg_stride_order)?;
    Ok((bitmap_min, bitmap_max + 1 - bitmap_min))
}

/// RBTREE block top, `base + (blklen - 1) * stride` (regcache-rbtree.c:39-45).
pub fn rbtree_block_top(base: u32, len: usize, stride: u32) -> Result<u32, IndexError> {
    if len == 0 {
        return Err(IndexError::EmptyBlock);
    }
    if stride == 0 {
        return Err(IndexError::ZeroStride);
    }
    let offset = (len - 1)
        .checked_mul(stride as usize)
        .and_then(|offset| u32::try_from(offset).ok())
        .and_then(|offset| base.checked_add(offset))
        .ok_or(IndexError::BlockTopOverflow { base, len, stride })?;
    Ok(offset)
}

/// RBTREE register slot, `(reg - base_reg) / stride`
/// (regcache-rbtree.c:244-255,276-278).
pub fn rbtree_index(reg: u32, base: u32, stride: u32) -> Result<usize, IndexError> {
    if stride == 0 {
        return Err(IndexError::ZeroStride);
    }
    let offset = reg
        .checked_sub(base)
        .ok_or(IndexError::RegisterBelowBlock { reg, base })?;
    Ok((offset / stride) as usize)
}

/// Intersect an inclusive register range with one RBTREE block and return Linux's half-open
/// block-index range (regcache-rbtree.c:465-500,510-540).
pub fn rbtree_range_indices(
    base: u32,
    len: usize,
    stride: u32,
    min: u32,
    max: u32,
) -> Result<Option<IndexRange>, IndexError> {
    if min > max {
        return Err(IndexError::RangeReversed { min, max });
    }
    let top = rbtree_block_top(base, len, stride)?;
    if base > max || top < min {
        return Ok(None); // regcache-rbtree.c:483-486,525-528
    }
    let start = if min > base {
        ((min - base) / stride) as usize
    } else {
        0
    };
    let end = if max < top {
        ((max - base) / stride) as usize + 1
    } else {
        len
    };
    Ok(Some(IndexRange { start, end }))
}
