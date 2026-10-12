// SPDX-License-Identifier: GPL-2.0-only
//! KEPT FIXED (audited for RUNTIME-TABLES-HAVE-NO-CAP, D10): every `FixedVec` in this crate holds a HARDWARE-defined or Linux-table-defined
//! sequence - the engines a part fuses (<= 5), a workaround list (a static Linux table per platform), an interrupt-bank reset/postinstall
//! write sequence, a clock-gating row set - whose maximum length is known when the plan is written. None holds a population that
//! clients create. A population that clients drive must NOT use this type: use an `alloc` collection with fallible growth.
//!
//! A tiny fixed-capacity vector: the crate allocates nothing, so every list is bounded by construction and a push past the
//! bound is a NAMED refusal, never a silent clamp.
use crate::error::GtError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedVec<T: Copy, const N: usize> {
    buf: [T; N],
    len: usize,
    name: &'static str,
}

impl<T: Copy, const N: usize> FixedVec<T, N> {
    pub const fn new(name: &'static str, fill: T) -> Self {
        FixedVec { buf: [fill; N], len: 0, name }
    }
    pub fn push(&mut self, v: T) -> Result<(), GtError> {
        if self.len == N {
            return Err(GtError::ListFull { list: self.name, capacity: N as u32 });
        }
        self.buf[self.len] = v;
        self.len += 1;
        Ok(())
    }
    pub fn as_slice(&self) -> &[T] {
        &self.buf[..self.len]
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub const fn capacity(&self) -> usize {
        N
    }
    /// Insert at `idx`, shifting the tail up (the sorted insert of `_wa_add`).
    pub(crate) fn insert(&mut self, idx: usize, v: T) -> Result<(), GtError> {
        if self.len == N {
            return Err(GtError::ListFull { list: self.name, capacity: N as u32 });
        }
        let mut i = self.len;
        while i > idx {
            self.buf[i] = self.buf[i - 1];
            i -= 1;
        }
        self.buf[idx] = v;
        self.len += 1;
        Ok(())
    }
    pub(crate) fn get_mut(&mut self, idx: usize) -> &mut T {
        &mut self.buf[idx]
    }
}
