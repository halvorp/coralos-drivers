// SPDX-License-Identifier: GPL-2.0-only
//! Access-table and readable/writeable/volatile/precious predicate rules.
//!
//! Ported from Linux `drivers/base/regmap/regmap.c` lines 56-198.
//!
//! Copyright 2011 Wolfson Microelectronics plc. Original author: Mark Brown.

/// Inclusive Linux `struct regmap_range` equivalent (regmap.c:56-66).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub min: u32,
    pub max: u32,
}

/// Caller-supplied answer for an optional callback/table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Override {
    Absent,
    Answer(bool),
}

/// Inputs shared by readable and writeable policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessInput {
    pub reg: u32,
    pub max_register: Option<u32>,
    pub callback: Override,
    pub table: Override,
}

/// Additional inputs to Linux's readable predicate (regmap.c:127-144).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadableInput {
    pub access: AccessInput,
    pub has_reg_read: bool,
    pub has_combined_format_write: bool,
}

/// Inputs to volatile/precious after the caller computes readability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecialInput {
    pub readable: bool,
    pub has_combined_format_write: bool,
    pub callback: Override,
    pub table: Override,
    pub has_cache_ops: bool,
}

/// Whether `reg` lies in any inclusive range (regmap.c:56-66).
pub fn reg_in_ranges(reg: u32, ranges: &[Range]) -> bool {
    ranges.iter().any(|range| reg >= range.min && reg <= range.max)
}

/// Linux access table rule: deny ranges first; no allow ranges means allow all (regmap.c:69-87).
pub fn check_range_table(reg: u32, yes_ranges: &[Range], no_ranges: &[Range]) -> bool {
    if reg_in_ranges(reg, no_ranges) { return false; }
    yes_ranges.is_empty() || reg_in_ranges(reg, yes_ranges)
}

/// Linux writeable rule: maximum, callback, table, then permissive default (regmap.c:90-103).
pub fn writeable(input: AccessInput) -> bool {
    if input.max_register.is_some_and(|max| input.reg > max) { return false; }
    match input.callback { Override::Answer(answer) => answer, Override::Absent => match input.table { Override::Answer(answer) => answer, Override::Absent => true } }
}

/// Linux readable rule: read operation, maximum, combined-write exclusion, callback/table/default
/// (regmap.c:127-144).
pub fn readable(input: ReadableInput) -> bool {
    if !input.has_reg_read || input.access.max_register.is_some_and(|max| input.access.reg > max) || input.has_combined_format_write { return false; }
    match input.access.callback { Override::Answer(answer) => answer, Override::Absent => match input.access.table { Override::Answer(answer) => answer, Override::Absent => true } }
}

/// Linux volatile rule (regmap.c:147-161). With cache ops, unspecified readable registers are
/// nonvolatile/cacheable; without cache ops they default volatile. Tests pin both directions.
pub fn volatile(input: SpecialInput) -> bool {
    if !input.has_combined_format_write && !input.readable { return false; }
    match input.callback {
        Override::Answer(answer) => answer,
        Override::Absent => match input.table { Override::Answer(answer) => answer, Override::Absent => !input.has_cache_ops },
    }
}

/// Linux precious rule: unreadable is never precious, then callback/table, default false
/// (regmap.c:164-176).
pub fn precious(input: SpecialInput) -> bool {
    if !input.readable { return false; }
    match input.callback { Override::Answer(answer) => answer, Override::Absent => match input.table { Override::Answer(answer) => answer, Override::Absent => false } }
}

/// Linux no-increment predicates share callback/table/default-true ordering (regmap.c:178-198).
pub fn noinc_allowed(callback: Override, table: Override) -> bool {
    match callback { Override::Answer(answer) => answer, Override::Absent => match table { Override::Answer(answer) => answer, Override::Absent => true } }
}

/// Cache membership excludes volatile registers in both read and write paths (regcache.c:280-321).
pub fn cacheable(has_cache: bool, is_volatile: bool) -> bool {
    has_cache && !is_volatile
}

/// A bulk range is volatile only when every element is volatile (regmap.c:200-209).
pub fn volatile_range(flags: &[bool]) -> bool {
    flags.iter().all(|flag| *flag)
}
