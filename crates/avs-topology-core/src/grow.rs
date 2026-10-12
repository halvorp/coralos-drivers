// SPDX-License-Identifier: GPL-2.0-only
//! Fallible growth of the topology's tables (DECISIONS.md D10, doctrine RUNTIME-TABLES-HAVE-NO-CAP). A topology FILE decides how
//! many libraries, formats, module configs, pipelines, modules, routes... exist, so none of them has a compile-time capacity: they
//! are `Vec`s grown with `try_reserve`, and running out of memory is the NAMED refusal [`TplgError::OutOfMemory`] raised before the
//! entry exists.
use crate::error::TplgError;
use alloc::vec::Vec;

/// Append `item`, growing fallibly. Returns its index.
pub fn try_push<T>(v: &mut Vec<T>, what: &'static str, item: T) -> Result<usize, TplgError> {
    v.try_reserve(1).map_err(|_| TplgError::OutOfMemory { what })?;
    v.push(item);
    Ok(v.len() - 1)
}

/// Replace the table by `count` default entries (a dictionary of known size), reserving exactly once.
pub fn try_fill_default<T: Default + Clone>(v: &mut Vec<T>, what: &'static str, count: u32) -> Result<(), TplgError> {
    v.clear();
    v.try_reserve_exact(count as usize).map_err(|_| TplgError::OutOfMemory { what })?;
    v.resize(count as usize, T::default()); // within the reservation: cannot allocate
    Ok(())
}
