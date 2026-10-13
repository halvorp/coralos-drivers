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
//! ABI 1.2 adds the PAYLOAD REGION ([`region`]): a frame whose payload is too large to travel inline (`payload_bytes` <= [`MAX_PAYLOAD`]) names bytes
//! in a region the client granted, instead (`FLAG_PAYLOAD_REGION`, a [`region::RegionRef`] as the whole inline payload).
//!
//! ABI 1.1 adds the uAPI messages ([`msg`]): one opcode per i915 call, a request payload and a reply payload for each, every field
//! fixed-endian and explicitly decoded, every bound a VALIDATION limit (Linux's own) and never a storage size. A reply payload is
//! `status: u32`, `errno: i32` (`0` or `-errno`, exactly what the ioctl returned), then a body of fixed size per opcode (zeroes when
//! `errno != 0`), except the variable replies the message modules name.
//!
//! FRAMING ([`frag`]): the kernel's IPC message is 4096 bytes, so a longer frame travels as consecutive messages that end where its own header
//! says; `frag::split` and `frag::Joiner` are the one definition both ends compile.
//!
//! This crate is MIT OR Apache-2.0 because the closed CoralOS shim compiles against it; it holds no Linux-derived code.

#![no_std]
#![forbid(unsafe_code)]

pub mod codec;
pub mod errno;
pub mod frag;
pub mod msg;
pub mod region;

/// `b"I915"` as a little-endian u32.
pub const MAGIC: u32 = u32::from_le_bytes(*b"I915");
pub const ABI_MAJOR: u16 = 1;
/// 1: the uAPI messages (opcodes `0x100..`). Every opcode records the minor that introduced it ([`Opcode::since_minor`]).
/// 2: the payload region ([`FLAG_PAYLOAD_REGION`], [`region`]); additive - no opcode changed.
/// 3: a region call may name a separate reply region ([`region::REPLY_ELSEWHERE`]) and `GEM-MMAP-OPEN`/`GEM-MMAP-LEGACY`'s reply carries the kernel
/// lease handle and the offset to map ([`msg::gem::GemMmapRep`]); additive - no opcode changed.
/// 4: grants addressed to a NAMED session, for sharing across processes: `PRIME-GRANT`, `SYNC-FILE-GRANT`, `SYNC-FILE-REDEEM` (D15); additive.
pub const ABI_MINOR: u16 = 4;
pub const HEADER_BYTES: usize = 40;
/// The largest payload a frame may declare (64 KiB): a submit carries object and dependency tables, never GPU data.
pub const MAX_PAYLOAD: u32 = 64 * 1024;

/// The largest whole frame that travels inline: the header and a full [`MAX_PAYLOAD`]. A receive buffer is sized to this, never less.
pub const MAX_FRAME_BYTES: usize = HEADER_BYTES + MAX_PAYLOAD as usize;

pub const FLAG_REPLY: u32 = 1 << 0;
/// The payload is a [`region::RegionRef`], not the call's own payload (ABI minor 2, [`region::REGION_SINCE_MINOR`]).
pub const FLAG_PAYLOAD_REGION: u32 = 1 << 1;
/// Every flag bit this ABI defines.
pub const FLAGS_KNOWN: u32 = FLAG_REPLY | FLAG_PAYLOAD_REGION;

macro_rules! opcodes {
    ($($(#[$m:meta])* $name:ident = $val:expr, $since:expr, $text:expr;)*) => {
        /// Operations. The first six are the skeleton's (minor 0; everything past `Hello` was `Unsupported` until the increment that
        /// implements it landed); the rest are the uAPI messages of minor 1 (see [`msg`]).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u32)]
        pub enum Opcode { $($(#[$m])* $name = $val,)* }

        impl Opcode {
            /// Every opcode, in numeric order.
            pub const ALL: &'static [Opcode] = &[$(Opcode::$name,)*];
            pub const fn from_u32(v: u32) -> Result<Opcode, WireError> {
                match v { $($val => Ok(Opcode::$name),)* _ => Err(WireError::UnknownOpcode { got: v }) }
            }
            /// The ABI minor that introduced this opcode: a frame that uses it under an older minor is refused.
            pub const fn since_minor(self) -> u16 {
                match self { $(Opcode::$name => $since,)* }
            }
            pub const fn name(self) -> &'static str {
                match self { $(Opcode::$name => $text,)* }
            }
        }
    };
}

opcodes! {
    Hello = 0, 0, "HELLO";
    GetReadout = 1, 0, "GET-READOUT";
    SetMode = 2, 0, "SET-MODE";
    PageFlip = 3, 0, "PAGE-FLIP";
    AllocScanout = 4, 0, "ALLOC-SCANOUT";
    SubmitBatch = 5, 0, "SUBMIT-BATCH";
    // ---- sessions (msg::session)
    SessionOpen = 0x100, 1, "SESSION-OPEN";
    SessionClose = 0x101, 1, "SESSION-CLOSE";
    // ---- device queries (msg::device)
    GetParam = 0x110, 1, "GETPARAM";
    Query = 0x111, 1, "QUERY";
    RegRead = 0x112, 1, "REG-READ";
    // ---- GEM objects (msg::gem)
    GemCreate = 0x120, 1, "GEM-CREATE";
    GemCreateExt = 0x121, 1, "GEM-CREATE-EXT";
    GemClose = 0x122, 1, "GEM-CLOSE";
    GemMmapOffset = 0x123, 1, "GEM-MMAP-OFFSET";
    GemMmapOpen = 0x124, 1, "GEM-MMAP-OPEN";
    GemMmapLegacy = 0x125, 1, "GEM-MMAP-LEGACY";
    GemMmapClose = 0x126, 1, "GEM-MMAP-CLOSE";
    GemSetDomain = 0x127, 1, "GEM-SET-DOMAIN";
    GemSetTiling = 0x128, 1, "GEM-SET-TILING";
    GemGetTiling = 0x129, 1, "GEM-GET-TILING";
    GemBusy = 0x12a, 1, "GEM-BUSY";
    GemWait = 0x12b, 1, "GEM-WAIT";
    GemMadvise = 0x12c, 1, "GEM-MADVISE";
    GemSetCaching = 0x12d, 1, "GEM-SET-CACHING";
    GemGetCaching = 0x12e, 1, "GEM-GET-CACHING";
    // ---- contexts and address spaces (msg::ctx)
    ContextCreate = 0x130, 1, "CONTEXT-CREATE";
    ContextCreateExt = 0x131, 1, "CONTEXT-CREATE-EXT";
    ContextDestroy = 0x132, 1, "CONTEXT-DESTROY";
    ContextGetParam = 0x133, 1, "CONTEXT-GETPARAM";
    ContextSetParam = 0x134, 1, "CONTEXT-SETPARAM";
    VmCreate = 0x135, 1, "VM-CREATE";
    VmDestroy = 0x136, 1, "VM-DESTROY";
    GetResetStats = 0x137, 1, "GET-RESET-STATS";
    // ---- execution (msg::exec)
    Execbuffer2 = 0x140, 1, "EXECBUFFER2";
    // ---- syncobj, timeline, sync_file (msg::sync)
    SyncobjCreate = 0x150, 1, "SYNCOBJ-CREATE";
    SyncobjDestroy = 0x151, 1, "SYNCOBJ-DESTROY";
    SyncobjWait = 0x152, 1, "SYNCOBJ-WAIT";
    SyncobjTimelineWait = 0x153, 1, "SYNCOBJ-TIMELINE-WAIT";
    SyncobjSignal = 0x154, 1, "SYNCOBJ-SIGNAL";
    SyncobjReset = 0x155, 1, "SYNCOBJ-RESET";
    SyncobjTimelineSignal = 0x156, 1, "SYNCOBJ-TIMELINE-SIGNAL";
    SyncobjQuery = 0x157, 1, "SYNCOBJ-QUERY";
    SyncobjTransfer = 0x158, 1, "SYNCOBJ-TRANSFER";
    SyncobjHandleToFd = 0x159, 1, "SYNCOBJ-HANDLE-TO-FD";
    SyncobjFdToHandle = 0x15a, 1, "SYNCOBJ-FD-TO-HANDLE";
    FdClose = 0x15b, 1, "FD-CLOSE";
    SyncFileMerge = 0x15c, 1, "SYNC-FILE-MERGE";
    SyncFileWait = 0x15d, 1, "SYNC-FILE-WAIT";
    // ---- PRIME and dma-buf sync (msg::prime)
    PrimeHandleToFd = 0x160, 1, "PRIME-HANDLE-TO-FD";
    PrimeFdToHandle = 0x161, 1, "PRIME-FD-TO-HANDLE";
    DmabufExportSyncFile = 0x162, 1, "DMABUF-EXPORT-SYNC-FILE";
    DmabufImportSyncFile = 0x163, 1, "DMABUF-IMPORT-SYNC-FILE";
    // ---- grants to a named session (msg::prime, msg::sync; ABI 1.4)
    PrimeGrant = 0x164, 4, "PRIME-GRANT";
    SyncFileGrant = 0x165, 4, "SYNC-FILE-GRANT";
    SyncFileRedeem = 0x166, 4, "SYNC-FILE-REDEEM";
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
    /// The opcode exists, but a later minor than the frame declares introduced it.
    OpcodeNewerThanMinor { opcode: u32, since: u16, got: u16 },
    /// A flag exists, but a later minor than the frame declares introduced it.
    FlagNewerThanMinor { flag: u32, since: u16, got: u16 },
    // ---- payload decode (the msg modules)
    /// A payload field ran past the end of the payload.
    Truncated { what: &'static str, need: usize, have: usize },
    /// Bytes remain after the last field.
    TrailingBytes { extra: usize },
    /// A padding / reserved field is not zero.
    FieldReservedNonZero { what: &'static str, got: u64 },
    /// A declared count disagrees with what the payload carries.
    CountMismatch { what: &'static str, declared: u64, actual: u64 },
    /// A VALIDATION limit (Linux's own; never a storage size).
    LimitExceeded { what: &'static str, got: u64, max: u64 },
    /// A field holds a value the message does not define.
    BadValue { what: &'static str, got: u64 },
    /// The output buffer cannot hold the encoding.
    OutputTooSmall { need: usize, have: usize },
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
            WireError::OpcodeNewerThanMinor { .. } => "WIRE-OPCODE-NEWER-THAN-MINOR",
            WireError::FlagNewerThanMinor { .. } => "WIRE-FLAG-NEWER-THAN-MINOR",
            WireError::Truncated { .. } => "WIRE-TRUNCATED",
            WireError::TrailingBytes { .. } => "WIRE-TRAILING-BYTES",
            WireError::FieldReservedNonZero { .. } => "WIRE-FIELD-RESERVED-NONZERO",
            WireError::CountMismatch { .. } => "WIRE-COUNT-MISMATCH",
            WireError::LimitExceeded { .. } => "WIRE-LIMIT-EXCEEDED",
            WireError::BadValue { .. } => "WIRE-BAD-VALUE",
            WireError::OutputTooSmall { .. } => "WIRE-OUTPUT-TOO-SMALL",
        }
    }
    /// The Linux errno (positive) the reply carries for this refusal: what the ioctl would have returned for the same malformed
    /// argument. A frame too short or too long for its fields is `EFAULT` (the `copy_from_user` that would have faulted); a bad
    /// count or value `EINVAL`; a limit `E2BIG`/`ENOMEM` as the call names it (`ENOMEM`, like Linux's allocation of an oversize array).
    pub const fn errno(&self) -> i32 {
        match self {
            WireError::ShortBuffer { .. } | WireError::PayloadLengthMismatch { .. } | WireError::Truncated { .. } | WireError::OutputTooSmall { .. } => errno::EFAULT,
            WireError::LimitExceeded { .. } | WireError::PayloadTooLarge { .. } => errno::ENOMEM,
            _ => errno::EINVAL,
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

    /// Does the payload name a region ([`FLAG_PAYLOAD_REGION`]) instead of carrying the call's bytes?
    pub const fn is_region(&self) -> bool {
        self.flags & FLAG_PAYLOAD_REGION != 0
    }

    pub const fn is_reply(&self) -> bool {
        self.flags & FLAG_REPLY != 0
    }

    /// The reply header to this request, declaring `payload_bytes` of reply payload (which must begin with a [`Status`]).
    pub const fn reply(&self, payload_bytes: u32) -> Header {
        // the reply travels inline unless the device says otherwise ([`Header::reply_in_region`]): a request's region flag is not inherited
        Header { major: ABI_MAJOR, minor: ABI_MINOR, flags: (self.flags & !FLAG_PAYLOAD_REGION) | FLAG_REPLY, payload_bytes, ..*self }
    }

    /// The reply header whose payload is a [`region::RegionRef`] to reply bytes the device placed in the client's reply area.
    pub const fn reply_in_region(&self) -> Header {
        Header { flags: self.flags | FLAG_PAYLOAD_REGION | FLAG_REPLY, payload_bytes: region::REGION_REF_BYTES as u32, major: ABI_MAJOR, minor: ABI_MINOR, ..*self }
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
        let minor = u16_at(buf, 6);
        if minor < opcode.since_minor() {
            return Err(WireError::OpcodeNewerThanMinor { opcode: opcode as u32, since: opcode.since_minor(), got: minor });
        }
        let flags = u32_at(buf, 12);
        if flags & !FLAGS_KNOWN != 0 {
            return Err(WireError::UnknownFlags { got: flags });
        }
        if flags & FLAG_PAYLOAD_REGION != 0 && minor < region::REGION_SINCE_MINOR {
            return Err(WireError::FlagNewerThanMinor { flag: FLAG_PAYLOAD_REGION, since: region::REGION_SINCE_MINOR, got: minor });
        }
        let payload_bytes = u32_at(buf, 32);
        if payload_bytes > MAX_PAYLOAD {
            return Err(WireError::PayloadTooLarge { got: payload_bytes, max: MAX_PAYLOAD });
        }
        let reserved = u32_at(buf, 36);
        if reserved != 0 {
            return Err(WireError::ReservedNonZero { got: reserved });
        }
        Ok(Header { major, minor, opcode, flags, request_id: u64_at(buf, 16), session_incarnation: u64_at(buf, 24), payload_bytes })
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

/// Write a whole frame: the header (its `payload_bytes` is set from `payload`) then the payload. Returns the frame length.
pub fn encode_frame(mut h: Header, payload: &[u8], out: &mut [u8]) -> Result<usize, WireError> {
    if payload.len() > MAX_PAYLOAD as usize {
        return Err(WireError::PayloadTooLarge { got: payload.len().min(u32::MAX as usize) as u32, max: MAX_PAYLOAD });
    }
    h.payload_bytes = payload.len() as u32;
    let total = HEADER_BYTES + payload.len();
    if out.len() < total {
        return Err(WireError::OutputTooSmall { need: total, have: out.len() });
    }
    h.encode(out)?;
    out[HEADER_BYTES..total].copy_from_slice(payload);
    Ok(total)
}
