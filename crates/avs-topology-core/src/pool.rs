// SPDX-License-Identifier: GPL-2.0-only
//! A fixed-capacity array with a name: pushing past the capacity is [`TplgError::TooMany`], never a panic or a silent drop.
use crate::error::TplgError;

#[derive(Clone, Copy, Debug)]
pub struct Pool<T: Copy, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T: Copy + Default, const N: usize> Default for Pool<T, N> {
    fn default() -> Self {
        Self { items: [T::default(); N], len: 0 }
    }
}

impl<T: Copy + Default, const N: usize> Pool<T, N> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn as_slice(&self) -> &[T] {
        &self.items[..self.len]
    }
    pub fn get(&self, i: usize) -> Option<&T> {
        self.as_slice().get(i)
    }
    pub fn push(&mut self, what: &'static str, v: T) -> Result<usize, TplgError> {
        if self.len >= N {
            return Err(TplgError::TooMany { what, count: self.len as u32 + 1, max: N as u32 });
        }
        self.items[self.len] = v;
        self.len += 1;
        Ok(self.len - 1)
    }
    /// Reserve `count` default entries at once (a dictionary of known size), refusing more than fit.
    pub fn reserve_default(&mut self, what: &'static str, count: u32) -> Result<(), TplgError> {
        if count as usize > N {
            return Err(TplgError::TooMany { what, count, max: N as u32 });
        }
        self.items = [T::default(); N];
        self.len = count as usize;
        Ok(())
    }
    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        if i < self.len {
            Some(&mut self.items[i])
        } else {
            None
        }
    }
}
