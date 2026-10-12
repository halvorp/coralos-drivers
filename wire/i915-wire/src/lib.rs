// SPDX-License-Identifier: MIT OR Apache-2.0
//! The i915.device wire protocol header, ABI major 1.
//!
//! A pointer-free, little-endian, explicitly decoded frame: a fixed 40-byte header followed by exactly `payload_bytes` bytes.
//! There are no user pointers, no fd numbers and no extension chains on the wire (they are process-local; the client side flattens
//! them into checked offsets inside session grants before sending). GPU data stays in shared grants and is never copied into a frame.
//!
//! ```text
//!  off  size  field
//!   0    4    magic              b"I915" read as a little-endian u32 = 0x3531_3949
//!   4    2    major              ABI major; a frame of another major is refused
//!   6    2    minor              additive revision; any minor is accepted by a reader of the same major
//!   8    4    opcode             see [`Opcode`]
//!  12    4    flags              bit 0 = REPLY; every other bit must be zero
//!  16    8    request_id         chosen by the requester, echoed by the reply
//!  24    8    session_incarnation  the device's incarnation of the session; a stale value is refused by the device
//!  32    4    payload_bytes      length of what follows, <= [`MAX_PAYLOAD`]
//!  36    4    reserved           must be zero
//! ```
//! A REPLY has the request's opcode, request_id and incarnation, and its payload begins with a `u32` [`Status`].
//!
//! This crate is MIT OR Apache-2.0 because the closed CoralOS shim compiles against it; it holds no Linux-derived code.

#![no_std]
#![forbid(unsafe_code)]

/// `b"I915"` as a little-endian u32.
pub const MAGIC: u32 = u32::from_le_bytes(*b"I915");
pub const ABI_MAJOR: u16 = 1;
pub const ABI_MINOR: u16 = 0;
pub const HEADER_BYTES: usize = 40;
/// The largest payload a frame may declare (64 KiB): a submit carries object and dependency tables, never GPU data.
pub const MAX_PAYLOAD: u32 = 64 * 1024;

pub const FLAG_REPLY: u32 = 1 << 0;
/// Every flag bit this ABI defines.
pub const FLAGS_KNOWN: u32 = FLAG_REPLY;

/// Operations, as the first-generation skeleton named them (`devices/i915` wire v1). Everything past `Hello` is answered
/// `Unsupported` until the increment that implements it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Opcode {
    Hello = 0,
    GetReadout = 1,
    SetMode = 2,
    PageFlip = 3,
    AllocScanout = 4,
    SubmitBatch = 5,
}

impl Opcode {
    pub const fn from_u32(v: u32) -> Result<Opcode, WireError> {
        match v {
            0 => Ok(Opcode::Hello),
            1 => Ok(Opcode::GetReadout),
            2 => Ok(Opcode::SetMode),
            3 => Ok(Opcode::PageFlip),
            4 => Ok(Opcode::AllocScanout),
            5 => Ok(Opcode::SubmitBatch),
            _ => Err(WireError::UnknownOpcode { got: v }),
        }
    }
}

/// The status word that opens a reply payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Status {
    Ok = 0,
    /// A well-formed request for something this device does not do (yet).
    Unsupported = 1,
    /// The frame did not decode.
    Malformed = 2,
    /// A major this device does not speak.
    BadVersion = 3,
    /// The session incarnation is not the current one.
    StaleSession = 4,
}

impl Status {
    pub const fn from_u32(v: u32) -> Option<Status> {
        match v {
            0 => Some(Status::Ok),
            1 => Some(Status::Unsupported),
            2 => Some(Status::Malformed),
            3 => Some(Status::BadVersion),
            4 => Some(Status::StaleSession),
            _ => None,
        }
    }
}

/// Why a frame was refused. Every variant names the value and the bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    ShortBuffer { need: usize, have: usize },
    BadMagic { got: u32 },
    UnsupportedMajor { got: u16 },
    UnknownOpcode { got: u32 },
    UnknownFlags { got: u32 },
    ReservedNonZero { got: u32 },
    PayloadTooLarge { got: u32, max: u32 },
    /// The bytes after the header are not exactly `payload_bytes` long.
    PayloadLengthMismatch { declared: u32, actual: usize },
}

impl WireError {
    pub const fn name(&self) -> &'static str {
        match self {
            WireError::ShortBuffer { .. } => "WIRE-SHORT-BUFFER",
            WireError::BadMagic { .. } => "WIRE-BAD-MAGIC",
            WireError::UnsupportedMajor { .. } => "WIRE-UNSUPPORTED-MAJOR",
            WireError::UnknownOpcode { .. } => "WIRE-UNKNOWN-OPCODE",
            WireError::UnknownFlags { .. } => "WIRE-UNKNOWN-FLAGS",
            WireError::ReservedNonZero { .. } => "WIRE-RESERVED-NONZERO",
            WireError::PayloadTooLarge { .. } => "WIRE-PAYLOAD-TOO-LARGE",
            WireError::PayloadLengthMismatch { .. } => "WIRE-PAYLOAD-LENGTH-MISMATCH",
        }
    }
    /// The reply [`Status`] a device answers this refusal with.
    pub const fn status(&self) -> Status {
        match self {
            WireError::UnsupportedMajor { .. } => Status::BadVersion,
            _ => Status::Malformed,
        }
    }
}

/// The decoded header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub major: u16,
    pub minor: u16,
    pub opcode: Opcode,
    pub flags: u32,
    pub request_id: u64,
    pub session_incarnation: u64,
    pub payload_bytes: u32,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut x = [0u8; 8];
    x.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(x)
}

impl Header {
    /// A request header of this ABI.
    pub const fn request(opcode: Opcode, request_id: u64, session_incarnation: u64, payload_bytes: u32) -> Header {
        Header { major: ABI_MAJOR, minor: ABI_MINOR, opcode, flags: 0, request_id, session_incarnation, payload_bytes }
    }

    pub const fn is_reply(&self) -> bool {
        self.flags & FLAG_REPLY != 0
    }

    /// The reply header to this request, declaring `payload_bytes` of reply payload (which must begin with a [`Status`]).
    pub const fn reply(&self, payload_bytes: u32) -> Header {
        Header { major: ABI_MAJOR, minor: ABI_MINOR, flags: self.flags | FLAG_REPLY, payload_bytes, ..*self }
    }

    /// Write the 40 header bytes. `Err(ShortBuffer)` if `out` is shorter; the declared payload is bounds-checked too.
    pub fn encode(&self, out: &mut [u8]) -> Result<(), WireError> {
        if out.len() < HEADER_BYTES {
            return Err(WireError::ShortBuffer { need: HEADER_BYTES, have: out.len() });
        }
        if self.payload_bytes > MAX_PAYLOAD {
            return Err(WireError::PayloadTooLarge { got: self.payload_bytes, max: MAX_PAYLOAD });
        }
        out[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        out[4..6].copy_from_slice(&self.major.to_le_bytes());
        out[6..8].copy_from_slice(&self.minor.to_le_bytes());
        out[8..12].copy_from_slice(&(self.opcode as u32).to_le_bytes());
        out[12..16].copy_from_slice(&self.flags.to_le_bytes());
        out[16..24].copy_from_slice(&self.request_id.to_le_bytes());
        out[24..32].copy_from_slice(&self.session_incarnation.to_le_bytes());
        out[32..36].copy_from_slice(&self.payload_bytes.to_le_bytes());
        out[36..40].copy_from_slice(&0u32.to_le_bytes());
        Ok(())
    }

    /// Decode and validate the header at the start of `buf`; does not look at what follows.
    pub fn decode_header(buf: &[u8]) -> Result<Header, WireError> {
        if buf.len() < HEADER_BYTES {
            return Err(WireError::ShortBuffer { need: HEADER_BYTES, have: buf.len() });
        }
        let magic = u32_at(buf, 0);
        if magic != MAGIC {
            return Err(WireError::BadMagic { got: magic });
        }
        let major = u16_at(buf, 4);
        if major != ABI_MAJOR {
            return Err(WireError::UnsupportedMajor { got: major });
        }
        let opcode = Opcode::from_u32(u32_at(buf, 8))?;
        let flags = u32_at(buf, 12);
        if flags & !FLAGS_KNOWN != 0 {
            return Err(WireError::UnknownFlags { got: flags });
        }
        let payload_bytes = u32_at(buf, 32);
        if payload_bytes > MAX_PAYLOAD {
            return Err(WireError::PayloadTooLarge { got: payload_bytes, max: MAX_PAYLOAD });
        }
        let reserved = u32_at(buf, 36);
        if reserved != 0 {
            return Err(WireError::ReservedNonZero { got: reserved });
        }
        Ok(Header { major, minor: u16_at(buf, 6), opcode, flags, request_id: u64_at(buf, 16), session_incarnation: u64_at(buf, 24), payload_bytes })
    }

    /// Decode a whole frame: the header, then exactly `payload_bytes` bytes (neither more nor fewer).
    pub fn decode(buf: &[u8]) -> Result<(Header, &[u8]), WireError> {
        let h = Header::decode_header(buf)?;
        let payload = &buf[HEADER_BYTES..];
        if payload.len() != h.payload_bytes as usize {
            return Err(WireError::PayloadLengthMismatch { declared: h.payload_bytes, actual: payload.len() });
        }
        Ok((h, payload))
    }
}
