//! The uAPI messages of ABI 1.1: one request payload and one reply payload per opcode.
//!
//! LAYOUT RULES (all of them tested): little-endian; every field naturally aligned; padding is an explicit `_pad` field that
//! MUST be zero (a non-zero pad is [`WireError::FieldReservedNonZero`], never ignored); a payload is decoded to the last byte
//! (short = [`WireError::Truncated`], long = [`WireError::TrailingBytes`]) so a decode is TRANSACTIONAL: it returns a whole value
//! or nothing. No pointers, no fd numbers: a mapping is a lease the reply names, a descriptor is a token the device issued.
//!
//! BOUNDS ARE VALIDATION LIMITS, NOT STORAGE: every `MAX_*` here is the number a request may not exceed (Linux's own limit where
//! Linux has one, else a named bound that keeps one payload inside [`crate::MAX_PAYLOAD`]). Nothing in this crate sizes a table.
//!
//! VERSIONING: [`crate::Opcode::since_minor`] says which ABI minor introduced each opcode; a body is only ever EXTENDED by a new
//! opcode, so a body layout is fixed for the life of its opcode.
use crate::codec::{Reader, Writer};
use crate::{Status, WireError};

pub mod ctx;
pub mod device;
pub mod display;
pub mod exec;
pub mod gem;
pub mod prime;
pub mod session;
pub mod sync;

/// A fixed-width payload field.
pub trait Field: Sized + Copy + Default {
    const SIZE: usize;
    fn put(&self, w: &mut Writer<'_>) -> Result<(), WireError>;
    fn get(r: &mut Reader<'_>, what: &'static str) -> Result<Self, WireError>;
}

macro_rules! int_field {
    ($t:ty, $size:expr, $put:ident, $get:ident) => {
        impl Field for $t {
            const SIZE: usize = $size;
            fn put(&self, w: &mut Writer<'_>) -> Result<(), WireError> {
                w.$put(*self)
            }
            fn get(r: &mut Reader<'_>, what: &'static str) -> Result<Self, WireError> {
                r.$get(what)
            }
        }
    };
}
int_field!(u32, 4, u32, u32);
int_field!(i32, 4, i32, i32);
int_field!(u64, 8, u64, u64);
int_field!(i64, 8, i64, i64);

/// A zero padding word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pad32;
/// A zero padding doubleword.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pad64;

impl Field for Pad32 {
    const SIZE: usize = 4;
    fn put(&self, w: &mut Writer<'_>) -> Result<(), WireError> {
        w.u32(0)
    }
    fn get(r: &mut Reader<'_>, what: &'static str) -> Result<Self, WireError> {
        r.zero32(what).map(|()| Pad32)
    }
}
impl Field for Pad64 {
    const SIZE: usize = 8;
    fn put(&self, w: &mut Writer<'_>) -> Result<(), WireError> {
        w.u64(0)
    }
    fn get(r: &mut Reader<'_>, what: &'static str) -> Result<Self, WireError> {
        match r.u64(what)? {
            0 => Ok(Pad64),
            got => Err(WireError::FieldReservedNonZero { what, got }),
        }
    }
}

/// A payload of fixed layout.
pub trait Body: Sized {
    /// Bytes on the wire.
    const SIZE: usize;
    fn encode(&self, w: &mut Writer<'_>) -> Result<(), WireError>;
    fn decode(r: &mut Reader<'_>) -> Result<Self, WireError>;
    /// Encode as a whole payload; returns its length.
    fn to_payload(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        self.encode(&mut w)?;
        Ok(w.pos())
    }
    /// Decode a whole payload: exactly [`Body::SIZE`] bytes, transactionally.
    fn from_payload(p: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let v = Self::decode(&mut r)?;
        r.finish()?;
        Ok(v)
    }
}

/// Define a fixed-layout message: `msg!(Name { field: ty, ... })`. Fields are written in order with no implicit padding; a
/// `_pad*: Pad32` / `Pad64` field is padding the decode requires to be zero (construct with `..Default::default()`).
macro_rules! msg {
    ($(#[$m:meta])* $name:ident { $($(#[$fm:meta])* $f:ident : $t:ty),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct $name { $($(#[$fm])* pub $f: $t,)* }
        impl $crate::msg::Body for $name {
            const SIZE: usize = 0 $(+ <$t as $crate::msg::Field>::SIZE)*;
            #[allow(unused_variables)]
            fn encode(&self, w: &mut $crate::codec::Writer<'_>) -> Result<(), $crate::WireError> {
                $(<$t as $crate::msg::Field>::put(&self.$f, w)?;)*
                Ok(())
            }
            #[allow(unused_variables)]
            fn decode(r: &mut $crate::codec::Reader<'_>) -> Result<Self, $crate::WireError> {
                Ok($name { $($f: <$t as $crate::msg::Field>::get(r, concat!(stringify!($name), ".", stringify!($f)))?,)* })
            }
        }
    };
}
pub(crate) use msg;

msg!(
    /// A body with no fields.
    Empty {}
);

/// `status: u32` then `errno: i32`.
pub const REPLY_HEAD_BYTES: usize = 8;

/// Why an `Unsupported` reply is unsupported.
pub mod unsupported {
    /// The skeleton never implemented this operation (the first generation's answer).
    pub const NOT_IMPLEMENTED: u32 = 1;
    /// The call exists but the increment that serves it (`p3-exec`) is not compiled into this device.
    pub const INCREMENT_NOT_ENABLED: u32 = 2;
    pub const fn name(reason: u32) -> &'static str {
        match reason {
            NOT_IMPLEMENTED => "NOT-IMPLEMENTED",
            INCREMENT_NOT_ENABLED => "INCREMENT-NOT-ENABLED",
            _ => "UNKNOWN",
        }
    }
}

msg!(
    /// The body of a `Status::Unsupported` reply.
    UnsupportedBody { reason: u32 }
);

/// A decoded reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reply<B> {
    pub status: Status,
    /// `0`, or `-errno` exactly as the ioctl returned it.
    pub errno: i32,
    /// All zeroes when `errno != 0` or the status is not `Ok`.
    pub body: B,
}

/// Encode a reply payload: `status`, `errno` (`-errno` or 0), then `body` (zeroes when `None`).
pub fn encode_reply<B: Body>(out: &mut [u8], status: Status, errno: i32, body: Option<&B>) -> Result<usize, WireError> {
    let mut w = Writer::new(out);
    w.u32(status as u32)?;
    w.i32(errno)?;
    match body {
        Some(b) => b.encode(&mut w)?,
        None => w.zeros(B::SIZE)?,
    }
    Ok(w.pos())
}

/// Encode only the head (for the variable replies, whose body the caller appends).
pub fn encode_reply_head(w: &mut Writer<'_>, status: Status, errno: i32) -> Result<(), WireError> {
    w.u32(status as u32)?;
    w.i32(errno)
}

/// Read a reply's head.
pub fn decode_reply_head(r: &mut Reader<'_>) -> Result<(Status, i32), WireError> {
    let s = r.u32("reply.status")?;
    let status = Status::from_u32(s).ok_or(WireError::BadValue { what: "reply.status", got: s as u64 })?;
    let errno = r.i32("reply.errno")?;
    if errno > 0 {
        return Err(WireError::BadValue { what: "reply.errno", got: errno as u64 });
    }
    Ok((status, errno))
}

/// Decode a fixed-body reply payload, transactionally.
pub fn decode_reply<B: Body>(payload: &[u8]) -> Result<Reply<B>, WireError> {
    let mut r = Reader::new(payload);
    let (status, errno) = decode_reply_head(&mut r)?;
    let body = B::decode(&mut r)?;
    r.finish()?;
    Ok(Reply { status, errno, body })
}

/// Encode the reply to a call the device does not serve: status `Unsupported`, `-EOPNOTSUPP`, and the reason.
pub fn encode_unsupported(out: &mut [u8], reason: u32) -> Result<usize, WireError> {
    encode_reply(out, Status::Unsupported, -crate::errno::EOPNOTSUPP, Some(&UnsupportedBody { reason }))
}

/// Round `n` up to a multiple of 8 (variable tables are padded so the next record stays 8-aligned).
pub const fn pad8(n: usize) -> usize {
    n.div_ceil(8) * 8
}

/// A borrowed table of little-endian `u32`s, bounds-checked at construction.
#[derive(Clone, Copy, Debug)]
pub struct U32s<'a>(&'a [u8]);

impl<'a> U32s<'a> {
    /// `count` words from the reader, followed by the padding that keeps the next record 8-aligned (which must be zero).
    pub fn read(r: &mut Reader<'a>, what: &'static str, count: usize) -> Result<U32s<'a>, WireError> {
        let bytes = count.checked_mul(4).ok_or(WireError::LimitExceeded { what, got: count as u64, max: u32::MAX as u64 })?;
        let b = r.take(what, bytes)?;
        let pad = pad8(bytes) - bytes;
        if pad != 0 {
            let p = r.take(what, pad)?;
            if p.iter().any(|x| *x != 0) {
                return Err(WireError::FieldReservedNonZero { what, got: 1 });
            }
        }
        Ok(U32s(b))
    }
    pub fn len(&self) -> usize {
        self.0.len() / 4
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, i: usize) -> Option<u32> {
        let b = self.0.get(i.checked_mul(4)?..)?.get(..4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.len()).filter_map(move |i| self.get(i))
    }
}

/// A borrowed table of little-endian `u64`s.
#[derive(Clone, Copy, Debug)]
pub struct U64s<'a>(&'a [u8]);

impl<'a> U64s<'a> {
    pub fn read(r: &mut Reader<'a>, what: &'static str, count: usize) -> Result<U64s<'a>, WireError> {
        let bytes = count.checked_mul(8).ok_or(WireError::LimitExceeded { what, got: count as u64, max: u32::MAX as u64 })?;
        Ok(U64s(r.take(what, bytes)?))
    }
    pub fn len(&self) -> usize {
        self.0.len() / 8
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, i: usize) -> Option<u64> {
        let b = self.0.get(i.checked_mul(8)?..)?.get(..8)?;
        let mut x = [0u8; 8];
        x.copy_from_slice(b);
        Some(u64::from_le_bytes(x))
    }
    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        (0..self.len()).filter_map(move |i| self.get(i))
    }
}

/// Write `vals` as `u32`s plus the zero padding to the next 8 bytes.
pub fn put_u32s(w: &mut Writer<'_>, vals: &[u32]) -> Result<(), WireError> {
    for v in vals {
        w.u32(*v)?;
    }
    w.zeros(pad8(vals.len() * 4) - vals.len() * 4)
}

/// Write `vals` as `u64`s.
pub fn put_u64s(w: &mut Writer<'_>, vals: &[u64]) -> Result<(), WireError> {
    for v in vals {
        w.u64(*v)?;
    }
    Ok(())
}
