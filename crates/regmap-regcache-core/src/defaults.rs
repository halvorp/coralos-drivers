// SPDX-License-Identifier: GPL-2.0-only
//! Register-default sorting, lookup, and source precedence.
//!
//! Ported from Linux `drivers/base/regmap/regcache.c:25-43,95-123,188-207,724-747` and
//! `drivers/base/regmap/regcache-flat.c:69-99`.
//!
//! Copyright 2011, 2012 Wolfson Microelectronics plc.
//! Original authors: Dimitris Papastamos and Mark Brown.

use core::fmt;

/// Linux `struct reg_default` (include/linux/regmap.h:78-89).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegDefault {
    pub reg: u32,
    pub def: u32,
}

/// Which source supplied a power-on default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultSource {
    RegDefaults,
    DefaultsRaw,
    DefaultRegFallback,
}

/// A value paired with the source that won the precedence decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultValue {
    pub value: u32,
    pub source: DefaultSource,
}

/// Literal source names in lookup order. The table wins, then raw defaults, then fallback
/// (regcache.c:95-123,188-207; regcache-flat.c:74-98).
pub const DEFAULT_SOURCE_NAMES: [&str; 3] =
    ["reg_defaults", "defaults_raw", "default_reg_fallback"];

/// Named default lookup refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultError {
    ZeroStride,
    RegisterNotStrideAligned { reg: u32, stride: u32 },
    RawIndexOutOfRange { index: usize, count: usize },
}

impl fmt::Display for DefaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::ZeroStride => f.write_str(
                "register-default lookup refused stride 0: stride must be nonzero",
            ),
            Self::RegisterNotStrideAligned { reg, stride } => write!(
                f,
                "register-default lookup refused register {reg:#x}: it is not aligned to stride {stride}"
            ),
            Self::RawIndexOutOfRange { index, count } => write!(
                f,
                "raw register-default lookup refused index {index}: defaults_raw contains {count} values"
            ),
        }
    }
}

/// Sort defaults by ascending register, matching `regcache_sort_defaults`
/// (regcache.c:25-43).
pub fn sort_defaults(defaults: &mut [RegDefault]) {
    defaults.sort_unstable_by_key(|entry| entry.reg);
}

/// Binary-search a sorted `reg_defaults` table (regcache.c:724-747).
pub fn lookup_reg_default(defaults: &[RegDefault], reg: u32) -> Option<usize> {
    defaults.binary_search_by_key(&reg, |entry| entry.reg).ok()
}

/// Resolve a register default in strict source order: sorted `reg_defaults`, then the raw default
/// at `reg / stride`, then `default_reg` fallback.
///
/// Linux copies/constructs `reg_defaults` first (regcache.c:188-207); FLAT population installs that
/// table before calling `reg_default_cb` only for still-invalid entries
/// (regcache-flat.c:69-99). This pure form makes the same precedence explicit and also accepts the
/// raw source from which Linux constructs table entries (regcache.c:95-123).
pub fn lookup_default(
    reg: u32,
    stride: u32,
    reg_defaults: &[RegDefault],
    defaults_raw: Option<&[u32]>,
    default_reg_fallback: Option<u32>,
) -> Result<Option<DefaultValue>, DefaultError> {
    if let Some(index) = lookup_reg_default(reg_defaults, reg) {
        return Ok(Some(DefaultValue {
            value: reg_defaults[index].def,
            source: DefaultSource::RegDefaults,
        }));
    }
    if let Some(raw) = defaults_raw {
        if stride == 0 {
            return Err(DefaultError::ZeroStride);
        }
        if reg % stride != 0 {
            return Err(DefaultError::RegisterNotStrideAligned { reg, stride });
        }
        let index = (reg / stride) as usize;
        let value = raw
            .get(index)
            .copied()
            .ok_or(DefaultError::RawIndexOutOfRange {
                index,
                count: raw.len(),
            })?;
        return Ok(Some(DefaultValue {
            value,
            source: DefaultSource::DefaultsRaw,
        }));
    }
    Ok(default_reg_fallback.map(|value| DefaultValue {
        value,
        source: DefaultSource::DefaultRegFallback,
    }))
}
