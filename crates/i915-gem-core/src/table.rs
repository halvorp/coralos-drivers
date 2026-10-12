// SPDX-License-Identifier: GPL-2.0-only
//! Growable tables with FALLIBLE growth. Doctrine RUNTIME-TABLES-HAVE-NO-CAP (owner ruling, 2026-10-12): a table that tracks a
//! runtime-growing population grows with memory; exhaustion is a NAMED refusal ([`GemError::NoMem`], marker `GEM-NOMEM`) raised
//! BEFORE the thing exists, and a compile-time constant is never its size.
//!
//! [`Quota`] is the one place growth is charged: an optional budget of NEW table entries (a per-device memory quota the executor can
//! set, and the allocation-failure injector the tests use). A real `try_reserve` failure maps to the same refusal. Reusing a freed
//! slot never charges: only growth does.
use crate::error::GemError;
use alloc::collections::TryReserveError;
use alloc::vec::Vec;

/// Budget of new table entries. `None` = bounded only by memory.
#[derive(Clone, Copy, Debug, Default)]
pub struct Quota {
    left: Option<usize>,
}

impl Quota {
    pub const fn unlimited() -> Quota {
        Quota { left: None }
    }
    pub fn set(&mut self, budget: Option<usize>) {
        self.left = budget;
    }
    pub fn remaining(&self) -> Option<usize> {
        self.left
    }
    /// Charge `n` new entries of `table`, or refuse by name.
    pub(crate) fn charge(&mut self, table: &'static str, n: usize) -> Result<(), GemError> {
        match &mut self.left {
            None => Ok(()),
            Some(l) if *l >= n => {
                *l -= n;
                Ok(())
            }
            Some(_) => Err(GemError::NoMem { table }),
        }
    }
    pub(crate) fn refund(&mut self, n: usize) {
        if let Some(l) = &mut self.left {
            *l += n;
        }
    }
}

fn nomem(table: &'static str) -> impl FnOnce(TryReserveError) -> GemError {
    move |_| GemError::NoMem { table }
}

/// Append to a vector, charging the quota and growing fallibly. On refusal nothing changed.
pub(crate) fn push<T>(v: &mut Vec<T>, x: T, q: &mut Quota, table: &'static str) -> Result<usize, GemError> {
    q.charge(table, 1)?;
    if let Err(e) = v.try_reserve(1) {
        q.refund(1);
        return Err(nomem(table)(e));
    }
    v.push(x);
    Ok(v.len() - 1)
}

/// A vector of `n` clones, built fallibly (per-request object lists).
pub(crate) fn with_capacity<T>(n: usize, table: &'static str) -> Result<Vec<T>, GemError> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(nomem(table))?;
    Ok(v)
}

#[derive(Clone, Debug)]
struct Slot<T> {
    gen: u32,
    dead: bool,
    val: Option<T>,
}

/// A slab with generation-tagged slots and a free list. `insert` reuses a free slot (no growth) or appends one (charged). A slot
/// whose generation reaches `max_gen` is retired for good, so a stale id can never alias a later occupant.
#[derive(Clone, Debug)]
pub(crate) struct Slab<T> {
    slots: Vec<Slot<T>>,
    /// Indices of free, non-dead slots. Capacity is reserved when a slot is created, so `remove` can never fail.
    free: Vec<u32>,
    max_gen: u32,
    live: usize,
}

impl<T> Slab<T> {
    pub(crate) const fn new(max_gen: u32) -> Slab<T> {
        Slab { slots: Vec::new(), free: Vec::new(), max_gen, live: 0 }
    }
    pub(crate) fn len(&self) -> usize {
        self.live
    }
    pub(crate) fn slot_count(&self) -> usize {
        self.slots.len()
    }
    /// Insert, returning `(index, generation)`. `index_limit` is an ABI width, not a table size (handle encodings).
    pub(crate) fn insert(&mut self, v: T, q: &mut Quota, table: &'static str, index_limit: usize) -> Result<(u32, u32), GemError> {
        if let Some(i) = self.free.pop() {
            let s = &mut self.slots[i as usize];
            s.val = Some(v);
            self.live += 1;
            return Ok((i, s.gen));
        }
        if self.slots.len() >= index_limit {
            return Err(GemError::HandleSpaceExhausted { table });
        }
        q.charge(table, 1)?;
        if self.slots.try_reserve(1).is_err() || self.free.try_reserve(self.slots.len() + 1 - self.free.len()).is_err() {
            q.refund(1);
            return Err(GemError::NoMem { table });
        }
        self.slots.push(Slot { gen: 1, dead: false, val: Some(v) });
        self.live += 1;
        Ok(((self.slots.len() - 1) as u32, 1))
    }
    pub(crate) fn get(&self, idx: u32, gen: u32) -> Option<&T> {
        let s = self.slots.get(idx as usize)?;
        if s.gen == gen {
            s.val.as_ref()
        } else {
            None
        }
    }
    pub(crate) fn get_mut(&mut self, idx: u32, gen: u32) -> Option<&mut T> {
        let s = self.slots.get_mut(idx as usize)?;
        if s.gen == gen {
            s.val.as_mut()
        } else {
            None
        }
    }
    /// The slot's current generation (what an id minted now would carry), and whether it is retired.
    pub(crate) fn gen_of(&self, idx: u32) -> Option<(u32, bool)> {
        self.slots.get(idx as usize).map(|s| (s.gen, s.dead))
    }
    pub(crate) fn remove(&mut self, idx: u32) -> Option<T> {
        let s = self.slots.get_mut(idx as usize)?;
        let v = s.val.take()?;
        self.live -= 1;
        if s.gen >= self.max_gen {
            s.dead = true;
        } else {
            s.gen += 1;
            self.free.push(idx); // capacity reserved at insert
        }
        Some(v)
    }
    /// The live value at `idx` with its generation (index-order scans never allocate).
    pub(crate) fn at(&self, idx: u32) -> Option<(u32, &T)> {
        let s = self.slots.get(idx as usize)?;
        s.val.as_ref().map(|v| (s.gen, v))
    }
}
