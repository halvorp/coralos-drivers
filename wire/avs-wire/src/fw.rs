// SPDX-License-Identifier: MIT OR Apache-2.0
//! The firmware channel: how the device asks CoralOS for the files it needs and never embeds - the base firmware, the library images, the
//! topology and the ACPI NHLT table.
//!
//! The device is the CLIENT here. It sends FW-STAT and FW-READ requests on its firmware request endpoint and receives the replies on its
//! firmware reply endpoint; the broker on the CoralOS side (Kickstart placed the files under `CoralOS:System/Firmware/avs/`) resolves a
//! NAME to a file, VERIFIES it against the digest the build baked, and serves it in slices. A file the broker could not verify does not
//! exist as far as this channel is concerned ([`crate::Status::FwNotFound`]); the device never sees an unverified byte, and it parses every
//! file it gets again (manifest, topology and NHLT checks) before it trusts it.
//!
//! Names are relative paths below the avs firmware directory, `/`-separated, printable ASCII (a subset of UTF-8), no `..`, at most [`crate::MAX_FW_NAME`] bytes (the
//! device checks, and a broker must too). Examples: `skl/dsp_basefw.bin`, `skl/dsp_mod_<UUID>.bin`, `da7219-tplg.bin`, `nhlt.bin`.
use crate::codec::{Reader, Writer};
use crate::{frame, Opcode, Status, WireError, FLAG_REPLY, MAX_FW_NAME, MAX_PAYLOAD, STATUS_BYTES};

/// The most bytes one FW-READ reply carries: the largest inline slice (the payload also holds the 4-byte status).
pub const MAX_FW_SLICE: u32 = MAX_PAYLOAD - STATUS_BYTES as u32;
/// What the device asks for per request. Smaller than the maximum so a reply fits a single endpoint message with room to spare.
pub const FW_SLICE: u32 = 16 * 1024;

/// A file name that is safe to hand to a broker: relative, ASCII, `/`-separated components, none empty or `..`, no control bytes.
pub fn check_name(name: &[u8]) -> Result<(), WireError> {
    if name.is_empty() || name.len() > MAX_FW_NAME {
        return Err(WireError::LimitExceeded { what: "fw.name", got: name.len() as u64, max: MAX_FW_NAME as u64 });
    }
    if name[0] == b'/' || name[name.len() - 1] == b'/' {
        return Err(WireError::BadValue { what: "fw.name", got: name[0] as u64 });
    }
    for comp in name.split(|b| *b == b'/') {
        if comp.is_empty() || comp == b".." || comp == b"." {
            return Err(WireError::BadValue { what: "fw.name.component", got: comp.len() as u64 });
        }
    }
    if name.iter().any(|b| !(0x21..=0x7e).contains(b)) {
        return Err(WireError::BadValue { what: "fw.name.byte", got: 0 });
    }
    Ok(())
}

/// FW-STAT request: the file's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FwStat<'a> {
    pub name: &'a [u8],
}

impl<'a> FwStat<'a> {
    pub fn decode(p: &'a [u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let n = r.u16("fw.name_len")? as usize;
        let name = r.bytes("fw.name", n)?;
        r.finish()?;
        check_name(name)?;
        Ok(Self { name })
    }
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        check_name(self.name)?;
        let mut w = Writer::new(out);
        w.u16("fw.name_len", self.name.len() as u16)?;
        w.bytes("fw.name", self.name)?;
        Ok(w.pos())
    }
}

/// FW-STAT reply body: the verified file's size in bytes.
pub fn stat_reply_body(size: u64, out: &mut [u8]) -> Result<usize, WireError> {
    let mut w = Writer::new(out);
    w.u64("fw.size", size)?;
    Ok(w.pos())
}

pub fn decode_stat_reply(body: &[u8]) -> Result<u64, WireError> {
    let mut r = Reader::new(body);
    let v = r.u64("fw.size")?;
    r.finish()?;
    Ok(v)
}

/// FW-READ request: a slice of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FwRead<'a> {
    pub name: &'a [u8],
    pub offset: u64,
    pub len: u32,
}

impl<'a> FwRead<'a> {
    pub fn decode(p: &'a [u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let offset = r.u64("fw.offset")?;
        let len = r.u32("fw.len")?;
        let n = r.u16("fw.name_len")? as usize;
        let name = r.bytes("fw.name", n)?;
        r.finish()?;
        check_name(name)?;
        if len == 0 || len > MAX_FW_SLICE {
            return Err(WireError::LimitExceeded { what: "fw.len", got: len as u64, max: MAX_FW_SLICE as u64 });
        }
        if offset.checked_add(len as u64).is_none() {
            return Err(WireError::BadValue { what: "fw.offset+len", got: offset });
        }
        Ok(Self { name, offset, len })
    }
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        check_name(self.name)?;
        if self.len == 0 || self.len > MAX_FW_SLICE {
            return Err(WireError::LimitExceeded { what: "fw.len", got: self.len as u64, max: MAX_FW_SLICE as u64 });
        }
        let mut w = Writer::new(out);
        w.u64("fw.offset", self.offset)?;
        w.u32("fw.len", self.len)?;
        w.u16("fw.name_len", self.name.len() as u16)?;
        w.bytes("fw.name", self.name)?;
        Ok(w.pos())
    }
}

/// A whole FW-READ reply frame: the status, then the slice (empty unless the status is OK).
pub fn read_reply(request_id: u64, status: Status, data: &[u8], out: &mut [u8]) -> Result<usize, WireError> {
    if data.len() > MAX_FW_SLICE as usize {
        return Err(WireError::LimitExceeded { what: "fw.data", got: data.len() as u64, max: MAX_FW_SLICE as u64 });
    }
    if out.len() < crate::HEADER_BYTES + STATUS_BYTES + data.len() {
        return Err(WireError::NoRoom { what: "fw.reply", need: crate::HEADER_BYTES + STATUS_BYTES + data.len(), have: out.len() });
    }
    let at = crate::HEADER_BYTES;
    out[at..at + STATUS_BYTES].copy_from_slice(&(status as u32).to_le_bytes());
    out[at + STATUS_BYTES..at + STATUS_BYTES + data.len()].copy_from_slice(data);
    // header written last, over the same buffer, by building the payload in place
    crate::Header::encode(Opcode::FwRead, FLAG_REPLY, request_id, (STATUS_BYTES + data.len()) as u32, out)?;
    Ok(at + STATUS_BYTES + data.len())
}

/// A whole FW-STAT reply frame.
pub fn stat_reply(request_id: u64, status: Status, size: u64, out: &mut [u8]) -> Result<usize, WireError> {
    let mut body = [0u8; 8];
    let n = if status == Status::Ok { stat_reply_body(size, &mut body)? } else { 0 };
    let mut p = [0u8; STATUS_BYTES + 8];
    p[..STATUS_BYTES].copy_from_slice(&(status as u32).to_le_bytes());
    p[STATUS_BYTES..STATUS_BYTES + n].copy_from_slice(&body[..n]);
    frame(Opcode::FwStat, FLAG_REPLY, request_id, &p[..STATUS_BYTES + n], out)
}
