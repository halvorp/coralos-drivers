// SPDX-License-Identifier: GPL-2.0-only
//! Cache-type matching and initialization validation.
//!
//! Ported from Linux `drivers/base/regmap/regcache.c:18-23,133-186` and
//! `include/linux/regmap.h:65-75`.
//!
//! Copyright 2011, 2012 Wolfson Microelectronics plc.
//! Original authors: Dimitris Papastamos and Mark Brown.

use core::fmt;

/// Every `enum regcache_type` member (include/linux/regmap.h:69-75).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheType {
    None = 0,       // include/linux/regmap.h:70
    Rbtree = 1,     // include/linux/regmap.h:71
    Flat = 2,       // include/linux/regmap.h:72
    Maple = 3,      // include/linux/regmap.h:73
    FlatSparse = 4, // include/linux/regmap.h:74
}

/// Literal names for all five public cache-type values (include/linux/regmap.h:70-74).
pub const CACHE_TYPE_NAMES: [&str; 5] = ["none", "rbtree", "flat", "maple", "flat-sparse"];

/// Cache implementations in Linux's actual matching order (regcache.c:18-23).
pub const SELECTABLE_CACHE_NAMES: [&str; 4] = ["flat-sparse", "rbtree", "maple", "flat"];

/// The selected cache implementation. `None` has no implementation because Linux bypasses it
/// before searching `cache_types[]` (regcache.c:139-146,164-181).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheBackend {
    FlatSparse,
    Rbtree,
    Maple,
    Flat,
}

/// Named initialization refusals corresponding to Linux's `-EINVAL` exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionError {
    DefaultsWithoutCount,                                // regcache.c:148-152
    DefaultsCountWithoutTable,                           // regcache.c:154-158
    ZeroRegisterStride,                                  // safe prerequisite for regcache.c:161
    DefaultRegisterMisaligned { reg: u32, stride: u32 }, // regcache.c:160-162
    CacheTypeUnmatched { cache_type: u8 },               // regcache.c:168-171
}

impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::DefaultsWithoutCount => f.write_str(
                "regcache initialization refused: register defaults are set without their count",
            ),
            Self::DefaultsCountWithoutTable => f.write_str(
                "regcache initialization refused: register-default count is set without a defaults table",
            ),
            Self::ZeroRegisterStride => {
                f.write_str("regcache initialization refused register stride 0: stride must be nonzero")
            }
            Self::DefaultRegisterMisaligned { reg, stride } => write!(
                f,
                "regcache initialization refused default register {reg:#x}: it is not aligned to stride {stride}"
            ),
            Self::CacheTypeUnmatched { cache_type } => write!(
                f,
                "regcache initialization refused cache type {cache_type}: Linux has no matching cache implementation"
            ),
        }
    }
}

/// Match a raw cache type using Linux's `cache_types[]` order (regcache.c:18-23,164-181).
pub fn select_cache_type(cache_type: u8) -> Result<Option<CacheBackend>, SelectionError> {
    if cache_type == CacheType::None as u8 {
        return Ok(None); // regcache.c:139-146
    }
    let backend = if cache_type == CacheType::FlatSparse as u8 {
        CacheBackend::FlatSparse // regcache.c:19
    } else if cache_type == CacheType::Rbtree as u8 {
        CacheBackend::Rbtree // regcache.c:20
    } else if cache_type == CacheType::Maple as u8 {
        CacheBackend::Maple // regcache.c:21
    } else if cache_type == CacheType::Flat as u8 {
        CacheBackend::Flat // regcache.c:22
    } else {
        return Err(SelectionError::CacheTypeUnmatched { cache_type });
    };
    Ok(Some(backend))
}

/// Validate the defaults pair, alignment, and cache type in Linux's order (regcache.c:139-181).
///
/// `default_regs` stands for `config->reg_defaults`; a nonempty slice is a present table.
pub fn validate_selection(
    cache_type: u8,
    reg_stride: u32,
    default_regs: &[u32],
    num_reg_defaults: usize,
) -> Result<Option<CacheBackend>, SelectionError> {
    if cache_type == CacheType::None as u8 {
        return Ok(None); // regcache.c:139-146: defaults are warned about, not refused
    }
    if !default_regs.is_empty() && num_reg_defaults == 0 {
        return Err(SelectionError::DefaultsWithoutCount); // regcache.c:148-152
    }
    if num_reg_defaults != 0 && default_regs.is_empty() {
        return Err(SelectionError::DefaultsCountWithoutTable); // regcache.c:154-158
    }
    if reg_stride == 0 {
        return Err(SelectionError::ZeroRegisterStride);
    }
    for &reg in default_regs.iter().take(num_reg_defaults) {
        if reg % reg_stride != 0 {
            return Err(SelectionError::DefaultRegisterMisaligned {
                reg,
                stride: reg_stride,
            });
        }
    }
    select_cache_type(cache_type)
}
