// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bounds-checked little-endian reading and writing. Every read and write names the field it was for.
use crate::WireError;

pub struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub const fn new(b: &'a [u8]) -> Self {
        Self { b, at: 0 }
    }
    fn take(&mut self, what: &'static str, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.at.checked_add(n).ok_or(WireError::Short { what, need: usize::MAX, have: self.b.len() })?;
        if end > self.b.len() {
            return Err(WireError::Short { what, need: end, have: self.b.len() });
        }
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    pub fn u8(&mut self, what: &'static str) -> Result<u8, WireError> {
        Ok(self.take(what, 1)?[0])
    }
    pub fn u16(&mut self, what: &'static str) -> Result<u16, WireError> {
        let s = self.take(what, 2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    pub fn u32(&mut self, what: &'static str) -> Result<u32, WireError> {
        let s = self.take(what, 4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    pub fn u64(&mut self, what: &'static str) -> Result<u64, WireError> {
        let s = self.take(what, 8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(u64::from_le_bytes(a))
    }
    pub fn bytes(&mut self, what: &'static str, n: usize) -> Result<&'a [u8], WireError> {
        self.take(what, n)
    }
    /// Everything not read yet.
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.at..];
        self.at = self.b.len();
        s
    }
    pub fn left(&self) -> usize {
        self.b.len() - self.at
    }
    /// The whole payload must have been consumed: trailing bytes are a malformed message, not slack.
    pub fn finish(&self) -> Result<(), WireError> {
        if self.at != self.b.len() {
            return Err(WireError::Trailing { left: self.b.len() - self.at });
        }
        Ok(())
    }
}

pub struct Writer<'a> {
    b: &'a mut [u8],
    at: usize,
}

impl<'a> Writer<'a> {
    pub fn new(b: &'a mut [u8]) -> Self {
        Self { b, at: 0 }
    }
    fn put(&mut self, what: &'static str, s: &[u8]) -> Result<(), WireError> {
        let end = self.at + s.len();
        if end > self.b.len() {
            return Err(WireError::NoRoom { what, need: end, have: self.b.len() });
        }
        self.b[self.at..end].copy_from_slice(s);
        self.at = end;
        Ok(())
    }
    pub fn u8(&mut self, what: &'static str, v: u8) -> Result<(), WireError> {
        self.put(what, &[v])
    }
    pub fn u16(&mut self, what: &'static str, v: u16) -> Result<(), WireError> {
        self.put(what, &v.to_le_bytes())
    }
    pub fn u32(&mut self, what: &'static str, v: u32) -> Result<(), WireError> {
        self.put(what, &v.to_le_bytes())
    }
    pub fn u64(&mut self, what: &'static str, v: u64) -> Result<(), WireError> {
        self.put(what, &v.to_le_bytes())
    }
    pub fn bytes(&mut self, what: &'static str, s: &[u8]) -> Result<(), WireError> {
        self.put(what, s)
    }
    pub const fn pos(&self) -> usize {
        self.at
    }
}
