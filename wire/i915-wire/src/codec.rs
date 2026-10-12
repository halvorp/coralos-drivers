//! Little-endian, bounds-checked field readers and writers for payloads. A failed read is a NAMED [`WireError`], never a panic and
//! never a partial value.
use crate::WireError;

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
    pub fn take(&mut self, what: &'static str, n: usize) -> Result<&'a [u8], WireError> {
        if n > self.remaining() {
            return Err(WireError::Truncated { what, need: n, have: self.remaining() });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn u32(&mut self, what: &'static str) -> Result<u32, WireError> {
        let b = self.take(what, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn i32(&mut self, what: &'static str) -> Result<i32, WireError> {
        Ok(self.u32(what)? as i32)
    }
    pub fn u64(&mut self, what: &'static str) -> Result<u64, WireError> {
        let b = self.take(what, 8)?;
        let mut x = [0u8; 8];
        x.copy_from_slice(b);
        Ok(u64::from_le_bytes(x))
    }
    pub fn i64(&mut self, what: &'static str) -> Result<i64, WireError> {
        Ok(self.u64(what)? as i64)
    }
    pub fn u16(&mut self, what: &'static str) -> Result<u16, WireError> {
        let b = self.take(what, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    /// A padding word: must be zero.
    pub fn zero32(&mut self, what: &'static str) -> Result<(), WireError> {
        match self.u32(what)? {
            0 => Ok(()),
            got => Err(WireError::FieldReservedNonZero { what, got: got as u64 }),
        }
    }
    /// The payload must end here.
    pub fn finish(&self) -> Result<(), WireError> {
        match self.remaining() {
            0 => Ok(()),
            extra => Err(WireError::TrailingBytes { extra }),
        }
    }
}

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
    pub fn bytes(&mut self, b: &[u8]) -> Result<(), WireError> {
        let end = self.pos.checked_add(b.len()).filter(|e| *e <= self.buf.len());
        match end {
            Some(e) => {
                self.buf[self.pos..e].copy_from_slice(b);
                self.pos = e;
                Ok(())
            }
            None => Err(WireError::OutputTooSmall { need: self.pos.saturating_add(b.len()), have: self.buf.len() }),
        }
    }
    pub fn u16(&mut self, v: u16) -> Result<(), WireError> {
        self.bytes(&v.to_le_bytes())
    }
    pub fn u32(&mut self, v: u32) -> Result<(), WireError> {
        self.bytes(&v.to_le_bytes())
    }
    pub fn i32(&mut self, v: i32) -> Result<(), WireError> {
        self.u32(v as u32)
    }
    pub fn u64(&mut self, v: u64) -> Result<(), WireError> {
        self.bytes(&v.to_le_bytes())
    }
    pub fn i64(&mut self, v: i64) -> Result<(), WireError> {
        self.u64(v as u64)
    }
    pub fn zeros(&mut self, n: usize) -> Result<(), WireError> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.buf.len());
        match end {
            Some(e) => {
                self.buf[self.pos..e].fill(0);
                self.pos = e;
                Ok(())
            }
            None => Err(WireError::OutputTooSmall { need: self.pos.saturating_add(n), have: self.buf.len() }),
        }
    }
}
