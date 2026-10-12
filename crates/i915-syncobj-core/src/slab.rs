// SPDX-License-Identifier: GPL-2.0-only
//! A generation-tagged slab with FALLIBLE growth and a charged budget (RUNTIME-TABLES-HAVE-NO-CAP): the table grows with memory, a
//! refusal is the named `SYNC-NOMEM` before the thing exists, and a reused slot never charges. The budget is the per-device quota
//! the executor can set and the allocation-failure injector the tests use.
use crate::error::SyncError;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Default)]
pub struct Budget {
    left: Option<usize>,
}

impl Budget {
    pub const fn unlimited() -> Budget {
        Budget { left: None }
    }
    pub fn set(&mut self, entries: Option<usize>) {
        self.left = entries;
    }
    pub fn remaining(&self) -> Option<usize> {
        self.left
    }
    pub(crate) fn charge(&mut self, table: &'static str) -> Result<(), SyncError> {
        match &mut self.left {
            None => Ok(()),
            Some(l) if *l > 0 => {
                *l -= 1;
                Ok(())
            }
            Some(_) => Err(SyncError::NoMem { table }),
        }
    }
    pub(crate) fn refund(&mut self) {
        if let Some(l) = &mut self.left {
            *l += 1;
        }
    }
}

/// Append to a vector, charging the budget and growing fallibly. On refusal nothing changed.
pub(crate) fn push<T>(v: &mut Vec<T>, x: T, b: &mut Budget, table: &'static str) -> Result<usize, SyncError> {
    b.charge(table)?;
    if v.try_reserve(1).is_err() {
        b.refund();
        return Err(SyncError::NoMem { table });
    }
    v.push(x);
    Ok(v.len() - 1)
}

#[derive(Debug)]
struct Slot<T> {
    gen: u32,
    val: Option<T>,
}

#[derive(Debug)]
pub(crate) struct Slab<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    live: usize,
}

impl<T> Slab<T> {
    pub(crate) const fn new() -> Slab<T> {
        Slab { slots: Vec::new(), free: Vec::new(), live: 0 }
    }
    pub(crate) fn len(&self) -> usize {
        self.live
    }
    /// `(index, generation)`. Reuses a free slot (no charge) or appends one (charged, fallible, and with room reserved for its
    /// later `remove` so a removal can never fail).
    pub(crate) fn insert(&mut self, v: T, b: &mut Budget, table: &'static str) -> Result<(u32, u32), SyncError> {
        if let Some(i) = self.free.pop() {
            let s = &mut self.slots[i as usize];
            s.val = Some(v);
            self.live += 1;
            return Ok((i, s.gen));
        }
        b.charge(table)?;
        if self.slots.try_reserve(1).is_err() || self.free.try_reserve(self.slots.len() + 1 - self.free.len()).is_err() {
            b.refund();
            return Err(SyncError::NoMem { table });
        }
        self.slots.push(Slot { gen: 1, val: Some(v) });
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
    pub(crate) fn remove(&mut self, idx: u32, gen: u32) -> Option<T> {
        let s = self.slots.get_mut(idx as usize)?;
        if s.gen != gen {
            return None;
        }
        let v = s.val.take()?;
        self.live -= 1;
        s.gen = s.gen.wrapping_add(1).max(1);
        self.free.push(idx); // capacity reserved at insert
        Some(v)
    }
    pub(crate) fn live_ids(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.slots.iter().enumerate().filter(|(_, s)| s.val.is_some()).map(|(i, s)| (i as u32, s.gen))
    }
}
