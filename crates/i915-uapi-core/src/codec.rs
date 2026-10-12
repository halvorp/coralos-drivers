// SPDX-License-Identifier: GPL-2.0-only
//! Little-endian, bounds-checked field readers and writers. Every read is checked against the slice; nothing here can index out
//! of range, and a short buffer is a NAMED `UapiError::Truncated`, never a panic and never a partial value.
use crate::error::UapiError;

/// A cursor over an immutable payload. Reads advance it; a failed read leaves the cursor where it was.
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub const fn new(buf: &'a [u8]) -> Reader<'a> {
        Reader { buf, pos: 0 }
    }
    pub const fn pos(&self) -> usize {
        self.pos
    }
    pub const fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    /// The next `n` bytes, borrowed from the payload.
    pub fn take(&mut self, what: &'static str, n: usize) -> Result<&'a [u8], UapiError> {
        if n > self.remaining() {
            return Err(UapiError::Truncated { what, need: n, have: self.remaining() });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn u32(&mut self, what: &'static str) -> Result<u32, UapiError> {
        let b = self.take(what, 4)?;
        Ok(u32_at(b, 0))
    }
    pub fn u64(&mut self, what: &'static str) -> Result<u64, UapiError> {
        let b = self.take(what, 8)?;
        Ok(u64_at(b, 0))
    }
}

/// `b[o..o+2]` as a LE u16. The caller has already length-checked `b`.
pub fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
pub fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
pub fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut x = [0u8; 8];
    x.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(x)
}

/// A cursor over a mutable output buffer: every write is bounds-checked.
#[derive(Debug)]
pub struct Writer<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> Writer<'a> {
    pub fn new(buf: &'a mut [u8]) -> Writer<'a> {
        Writer { buf, pos: 0 }
    }
    pub fn pos(&self) -> usize {
        self.pos
    }
    pub fn bytes(&mut self, b: &[u8]) -> Result<(), UapiError> {
        let end = self.pos.checked_add(b.len()).filter(|e| *e <= self.buf.len());
        match end {
            Some(e) => {
                self.buf[self.pos..e].copy_from_slice(b);
                self.pos = e;
                Ok(())
            }
            None => Err(UapiError::QueryReplyTooSmall { need: self.pos.saturating_add(b.len()), have: self.buf.len() }),
        }
    }
    pub fn u16(&mut self, v: u16) -> Result<(), UapiError> {
        self.bytes(&v.to_le_bytes())
    }
    pub fn u32(&mut self, v: u32) -> Result<(), UapiError> {
        self.bytes(&v.to_le_bytes())
    }
    pub fn u64(&mut self, v: u64) -> Result<(), UapiError> {
        self.bytes(&v.to_le_bytes())
    }
    pub fn zeros(&mut self, n: usize) -> Result<(), UapiError> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.buf.len());
        match end {
            Some(e) => {
                self.buf[self.pos..e].fill(0);
                self.pos = e;
                Ok(())
            }
            None => Err(UapiError::QueryReplyTooSmall { need: self.pos.saturating_add(n), have: self.buf.len() }),
        }
    }
}
