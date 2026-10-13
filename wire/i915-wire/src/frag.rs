// SPDX-License-Identifier: MIT OR Apache-2.0
//! FRAMING over a message-bounded transport: the ONE definition of how an i915-wire frame travels as consecutive messages.
//!
//! WHY. The CoralOS kernel's IPC message is [`KERNEL_EP_CAP`] (4096) bytes and `send` TRUNCATES anything longer, so a frame longer than
//! one message (a header plus an EXECBUFFER2's object table is routinely 8 KiB) cannot travel as one. A ring holds
//! [`KERNEL_EP_DEPTH`] (16) messages and a full ring drops the NEWEST, so a frame must never need all 16: at most [`MAX_MESSAGES`] (15).
//!
//! THE RULE (no new bytes on the wire, no sequence numbers): a frame is cut into consecutive messages of exactly `msg_cap` bytes, the
//! last one holding the remainder, and it ENDS where its own header says it does (`HEADER_BYTES + payload_bytes`). The first message
//! carries the header, so the receiver knows the total from byte 0. Because every message but the last is a full `msg_cap`, a SHORT
//! message in the middle of a sequence is a violation the receiver can name, and so is a message that runs past the declared end.
//!
//! The sender side is [`split`]; the receiver side is [`Joiner`], which owns no memory: it tells the caller what to do with each message
//! ([`Feed`]) and the caller keeps the bytes in whatever buffer it has (a fixed one in a client, a growable one per session in the
//! device). Both sides compile this file, so they cannot disagree about where a frame ends.
//!
//! BOUND. A frame that fragments is at most `max_msgs * msg_cap` = 61440 bytes (also never past [`crate::MAX_FRAME_BYTES`]). This is a
//! VALIDATION limit of the transport, not a storage size: a call that needs more travels as a payload region ([`crate::region`]).
use crate::{Header, WireError, HEADER_BYTES, MAX_FRAME_BYTES};

/// The kernel's per-message byte cap (`EP_CAP`, kernel/src/arch/x86_64/endpoint.rs): `send` truncates a longer message.
pub const KERNEL_EP_CAP: usize = 4096;
/// The kernel's ring depth (`EP_DEPTH`, same file): a full ring DROPS the newest message.
pub const KERNEL_EP_DEPTH: usize = 16;
/// Messages one frame may use: one ring slot is always left free, so a frame never needs the whole ring.
pub const MAX_MESSAGES: usize = KERNEL_EP_DEPTH - 1;

/// The transport's message geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Bytes in a full message.
    pub msg_cap: usize,
    /// Messages one frame may span.
    pub max_msgs: usize,
}

impl Limits {
    /// The CoralOS kernel's endpoints.
    pub const KERNEL: Limits = Limits { msg_cap: KERNEL_EP_CAP, max_msgs: MAX_MESSAGES };

    /// The largest whole frame that fragments under these limits.
    pub const fn max_frame_bytes(&self) -> usize {
        let v = self.msg_cap.saturating_mul(self.max_msgs);
        if v < MAX_FRAME_BYTES {
            v
        } else {
            MAX_FRAME_BYTES
        }
    }

    /// Messages a frame of `total` bytes occupies (at least one).
    pub const fn messages_for(&self, total: usize) -> usize {
        let cap = if self.msg_cap == 0 { 1 } else { self.msg_cap };
        if total == 0 {
            1
        } else {
            total.div_ceil(cap)
        }
    }
}

/// Why a frame could not be cut, or a sequence of messages is not a frame. Every variant names the values and the bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FragError {
    /// The first message of a frame is shorter than a header, so the frame's length cannot be read.
    FirstShort { got: usize },
    /// The frame's header (or its declared length, when cutting) is refused by the wire.
    Header(WireError),
    /// The frame needs more messages than the ring allows a frame to use.
    TooManyMessages { needed: usize, max: usize },
    /// A message in the middle of a sequence is shorter than a full message (the sequence stopped early, or something else was sent).
    ShortFragment { index: usize, got: usize, cap: usize },
    /// A message in the middle of a sequence is a NEW frame's header: two frames interleaved.
    Interleaved { index: usize },
    /// A message runs past the end the first one declared.
    Overlong { declared: usize, got: usize },
}

impl FragError {
    pub const fn name(&self) -> &'static str {
        match self {
            FragError::FirstShort { .. } => "FRAG-FIRST-SHORT",
            FragError::Header(e) => e.name(),
            FragError::TooManyMessages { .. } => "FRAG-TOO-MANY-MESSAGES",
            FragError::ShortFragment { .. } => "FRAG-SHORT-FRAGMENT",
            FragError::Interleaved { .. } => "FRAG-INTERLEAVED",
            FragError::Overlong { .. } => "FRAG-OVERLONG",
        }
    }
    /// The wire error to answer a refusal with when a reply must name one (the framing violations are malformed frames).
    pub fn wire(&self) -> WireError {
        match *self {
            FragError::Header(e) => e,
            FragError::FirstShort { got } => WireError::ShortBuffer { need: HEADER_BYTES, have: got },
            FragError::TooManyMessages { needed, max } => WireError::LimitExceeded { what: "frame.messages", got: needed as u64, max: max as u64 },
            FragError::ShortFragment { got, cap, .. } => WireError::ShortBuffer { need: cap, have: got },
            FragError::Interleaved { index } => WireError::BadValue { what: "frame.interleaved", got: index as u64 },
            FragError::Overlong { declared, got } => WireError::PayloadLengthMismatch { declared: declared.saturating_sub(HEADER_BYTES) as u32, actual: got.saturating_sub(HEADER_BYTES) },
        }
    }
}

/// The messages of one frame, in order. Each is a slice of the frame itself (nothing is copied).
#[derive(Clone, Debug)]
pub struct Chunks<'a> {
    rest: &'a [u8],
    cap: usize,
    count: usize,
}

impl Chunks<'_> {
    /// How many messages there are in all (constant over the iteration's life).
    pub const fn messages(&self) -> usize {
        self.count
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<&'a [u8]> {
        if self.rest.is_empty() {
            return None;
        }
        let n = self.rest.len().min(self.cap);
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Some(head)
    }
}

/// Cut a whole frame (header and exactly `payload_bytes`) into the messages it travels as. Refused by name when the frame is not whole
/// (its header does not decode, or the bytes are not exactly what the header declares) or needs more than `limits.max_msgs` messages;
/// nothing is cut then.
pub fn split(frame: &[u8], limits: Limits) -> Result<Chunks<'_>, FragError> {
    Header::decode(frame).map_err(FragError::Header)?;
    if frame.len() > limits.max_frame_bytes() {
        return Err(FragError::TooManyMessages { needed: limits.messages_for(frame.len()), max: limits.max_msgs });
    }
    Ok(Chunks { rest: frame, cap: limits.msg_cap.max(1), count: limits.messages_for(frame.len()) })
}

/// What the caller does with the message it just fed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
    /// The message IS the whole frame: use it in place.
    Whole,
    /// The first of several: make room for `total` bytes (the whole frame's length) and keep this message as its start.
    Begin { total: usize },
    /// Keep this message after the ones before it.
    Middle,
    /// Keep this message; the frame is now whole.
    End,
}

/// The receiver's reassembly state for ONE stream of messages (one session). It holds three counters, never a byte of the frame.
#[derive(Clone, Copy, Debug)]
pub struct Joiner {
    limits: Limits,
    /// 0 = between frames; else the declared length of the frame in progress.
    total: usize,
    got: usize,
    msgs: usize,
}

impl Joiner {
    pub const fn new(limits: Limits) -> Joiner {
        Joiner { limits, total: 0, got: 0, msgs: 0 }
    }

    /// Is a multi-message frame partly received? (A kernel lifecycle message is only told from data between frames.)
    pub const fn in_progress(&self) -> bool {
        self.total != 0
    }

    /// Bytes of the frame in progress received so far (0 between frames).
    pub const fn received(&self) -> usize {
        self.got
    }

    /// The declared length of the frame in progress (0 between frames).
    pub const fn declared(&self) -> usize {
        self.total
    }

    /// Forget a frame in progress.
    pub fn reset(&mut self) {
        self.total = 0;
        self.got = 0;
        self.msgs = 0;
    }

    /// Take the next message of the stream. On `Err` the frame in progress (if any) is forgotten and the joiner is between frames, so
    /// the caller drops what it kept and the NEXT message starts a frame.
    pub fn feed(&mut self, msg: &[u8]) -> Result<Feed, FragError> {
        let r = if self.total == 0 { self.first(msg) } else { self.next(msg) };
        if r.is_err() {
            self.reset();
        }
        r
    }

    fn first(&mut self, msg: &[u8]) -> Result<Feed, FragError> {
        if msg.len() < HEADER_BYTES {
            return Err(FragError::FirstShort { got: msg.len() });
        }
        let h = Header::decode_header(msg).map_err(FragError::Header)?;
        let total = HEADER_BYTES + h.payload_bytes as usize;
        if total > self.limits.max_frame_bytes() {
            return Err(FragError::TooManyMessages { needed: self.limits.messages_for(total), max: self.limits.max_msgs });
        }
        if msg.len() > total {
            return Err(FragError::Overlong { declared: total, got: msg.len() });
        }
        if msg.len() == total {
            return Ok(Feed::Whole);
        }
        if msg.len() != self.limits.msg_cap {
            return Err(FragError::ShortFragment { index: 0, got: msg.len(), cap: self.limits.msg_cap });
        }
        self.total = total;
        self.got = msg.len();
        self.msgs = 1;
        Ok(Feed::Begin { total })
    }

    fn next(&mut self, msg: &[u8]) -> Result<Feed, FragError> {
        let remaining = self.total - self.got;
        if msg.len() > remaining {
            return Err(FragError::Overlong { declared: self.total, got: self.got + msg.len() });
        }
        if msg.len() == remaining {
            self.reset();
            return Ok(Feed::End);
        }
        if msg.len() != self.limits.msg_cap {
            if msg.len() >= HEADER_BYTES && Header::decode_header(msg).is_ok() {
                return Err(FragError::Interleaved { index: self.msgs });
            }
            return Err(FragError::ShortFragment { index: self.msgs, got: msg.len(), cap: self.limits.msg_cap });
        }
        self.got += msg.len();
        self.msgs += 1;
        Ok(Feed::Middle)
    }
}
