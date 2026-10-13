// SPDX-License-Identifier: MIT OR Apache-2.0
//! The avs.device wire protocol header, ABI major 1.
//!
//! avs.device is the Intel cAVS (Sunrise Point-LP / Kaby Lake) audio DSP and the Dialog DA7219 headset codec of the Pixelbook Go (ATLAS), as a
//! ring-3 `.device`. Its one client is CoralOS's Aura.engine: Aura keeps the master volume, the mute and the mixing; this protocol carries the
//! device's STATUS (with the separated causes of silence), the playback stream, the jack, and the volume Aura asks for.
//!
//! A pointer-free, little-endian, explicitly decoded frame: a fixed 32-byte header followed by exactly `payload_bytes` bytes.
//!
//! ```text
//!  off  size  field
//!   0    4    magic            b"AVSD" read as a little-endian u32
//!   4    2    major            ABI major; a frame of another major is refused
//!   6    2    minor            additive revision; any minor is accepted by a reader of the same major
//!   8    4    opcode           see [`Opcode`]
//!  12    4    flags            bit 0 = REPLY, bit 1 = EVENT (a frame the device sends unasked); every other bit must be zero
//!  16    8    request_id       chosen by the requester, echoed by the reply; an event carries 0
//!  24    4    payload_bytes    length of what follows, <= [`MAX_PAYLOAD`]
//!  28    4    reserved         must be zero
//! ```
//! A REPLY has the request's opcode and request_id, and its payload begins with a `u32` [`Status`].
//!
//! SPEAKERS ARE NOT IN THIS PROTOCOL. The device plays to the headphone jack only; the speaker amplifiers are never addressed (the Astra audio
//! ruling of 2026-10-12, headphones first: recorded in this repository's DECISIONS.md, D11 and D13). [`Cause::SpeakerProtectionUnavailable`] says so, always, in every STATUS.
//!
//! PCM travels inline in [`msg::PcmWrite`] frames (at most [`MAX_PCM_BYTES`] each) until the kernel can map a client region; the region form
//! ([`Opcode::StreamOpenRing`]) is DEFINED so Aura can be written against it, and is refused by name ([`Status::RingRegionUnavailable`]).
//! This crate is MIT OR Apache-2.0 because the closed CoralOS tree compiles against it; it holds no Linux-derived code.

#![no_std]
#![forbid(unsafe_code)]

pub mod board;
pub mod codec;
pub mod fw;
pub mod msg;

/// `b"AVSD"` as a little-endian u32.
pub const MAGIC: u32 = u32::from_le_bytes(*b"AVSD");
pub const ABI_MAJOR: u16 = 1;
pub const ABI_MINOR: u16 = 0;
pub const HEADER_BYTES: usize = 32;
/// The largest payload a frame may declare: one inline PCM write of 16 KiB plus its small header, rounded to a power of two. A VALIDATION
/// limit of the wire, not a buffer the device sizes its queues by.
pub const MAX_PAYLOAD: u32 = 32 * 1024;
/// Bytes of the leading status word of every reply payload.
pub const STATUS_BYTES: usize = 4;
/// The largest frame: header and a full payload. A receive buffer is this long, never less.
pub const MAX_FRAME_BYTES: usize = HEADER_BYTES + MAX_PAYLOAD as usize;
/// The most PCM bytes one [`msg::PcmWrite`] may carry.
pub const MAX_PCM_BYTES: u32 = 16 * 1024;
/// A firmware file name on the firmware channel is at most this long.
pub const MAX_FW_NAME: usize = 96;

pub const FLAG_REPLY: u32 = 1 << 0;
pub const FLAG_EVENT: u32 = 1 << 1;
pub const FLAGS_KNOWN: u32 = FLAG_REPLY | FLAG_EVENT;

/// Why a frame or payload did not decode (or encode), by field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Short { what: &'static str, need: usize, have: usize },
    NoRoom { what: &'static str, need: usize, have: usize },
    Trailing { left: usize },
    BadMagic { got: u32 },
    BadMajor { got: u16 },
    UnknownOpcode { got: u32 },
    UnknownFlags { got: u32 },
    ReservedNonZero { what: &'static str, got: u64 },
    PayloadTooBig { got: u32, max: u32 },
    PayloadLength { declared: u32, have: usize },
    BadValue { what: &'static str, got: u64 },
    LimitExceeded { what: &'static str, got: u64, max: u64 },
    /// A reply or event used where a request belongs, or the reverse.
    WrongDirection { opcode: u32 },
}

impl WireError {
    pub const fn name(&self) -> &'static str {
        match self {
            WireError::Short { .. } => "short",
            WireError::NoRoom { .. } => "no-room",
            WireError::Trailing { .. } => "trailing-bytes",
            WireError::BadMagic { .. } => "bad-magic",
            WireError::BadMajor { .. } => "bad-major",
            WireError::UnknownOpcode { .. } => "unknown-opcode",
            WireError::UnknownFlags { .. } => "unknown-flags",
            WireError::ReservedNonZero { .. } => "reserved-non-zero",
            WireError::PayloadTooBig { .. } => "payload-too-big",
            WireError::PayloadLength { .. } => "payload-length",
            WireError::BadValue { .. } => "bad-value",
            WireError::LimitExceeded { .. } => "limit-exceeded",
            WireError::WrongDirection { .. } => "wrong-direction",
        }
    }
}

macro_rules! opcodes {
    ($($(#[$m:meta])* $name:ident = $val:expr, $text:expr;)*) => {
        /// Operations (requests, and the events the device sends unasked).
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u32)]
        pub enum Opcode { $($(#[$m])* $name = $val,)* }
        impl Opcode {
            pub const ALL: &'static [Opcode] = &[$(Opcode::$name,)*];
            pub const fn from_u32(v: u32) -> Result<Opcode, WireError> {
                match v { $($val => Ok(Opcode::$name),)* _ => Err(WireError::UnknownOpcode { got: v }) }
            }
            pub const fn name(self) -> &'static str {
                match self { $(Opcode::$name => $text,)* }
            }
        }
    };
}

opcodes! {
    Hello = 0, "HELLO";
    Status = 1, "STATUS";
    Subscribe = 2, "SUBSCRIBE";
    StreamCaps = 3, "STREAM-CAPS";
    StreamOpen = 4, "STREAM-OPEN";
    /// DEFINED, REFUSED: PCM through a client region instead of inline frames; needs the kernel's region-map call.
    StreamOpenRing = 5, "STREAM-OPEN-RING";
    StreamStart = 6, "STREAM-START";
    StreamStop = 7, "STREAM-STOP";
    StreamClose = 8, "STREAM-CLOSE";
    PcmWrite = 9, "PCM-WRITE";
    SetVolume = 10, "SET-VOLUME";
    /// Event: the STATUS changed.
    EvStatus = 0x80, "EV-STATUS";
    /// Event: the jack changed.
    EvJack = 0x81, "EV-JACK";
    /// Event: a period was consumed (credit for more PCM) - the audio clock.
    EvPeriod = 0x82, "EV-PERIOD";
    /// Firmware channel (device -> firmware broker): the size of a file.
    FwStat = 0x100, "FW-STAT";
    /// Firmware channel: a slice of a file.
    FwRead = 0x101, "FW-READ";
}

impl Opcode {
    pub const fn is_event(self) -> bool {
        (self as u32) & 0x80 != 0 && (self as u32) < 0x100
    }
    pub const fn is_firmware(self) -> bool {
        (self as u32) >= 0x100
    }
}

/// The decoded 32-byte header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub minor: u16,
    pub opcode: Opcode,
    pub flags: u32,
    pub request_id: u64,
    pub payload_bytes: u32,
}

impl Header {
    pub const fn is_reply(&self) -> bool {
        self.flags & FLAG_REPLY != 0
    }
    pub const fn is_event(&self) -> bool {
        self.flags & FLAG_EVENT != 0
    }

    /// Decode and validate the header of `frame`; the payload slice is the `payload_bytes` that follow, no more and no fewer.
    pub fn decode(frame: &[u8]) -> Result<(Header, &[u8]), WireError> {
        use codec::Reader;
        let mut r = Reader::new(frame);
        let magic = r.u32("header.magic")?;
        if magic != MAGIC {
            return Err(WireError::BadMagic { got: magic });
        }
        let major = r.u16("header.major")?;
        if major != ABI_MAJOR {
            return Err(WireError::BadMajor { got: major });
        }
        let minor = r.u16("header.minor")?;
        let opcode = Opcode::from_u32(r.u32("header.opcode")?)?;
        let flags = r.u32("header.flags")?;
        if flags & !FLAGS_KNOWN != 0 {
            return Err(WireError::UnknownFlags { got: flags });
        }
        let request_id = r.u64("header.request_id")?;
        let payload_bytes = r.u32("header.payload_bytes")?;
        let reserved = r.u32("header.reserved")?;
        if reserved != 0 {
            return Err(WireError::ReservedNonZero { what: "header.reserved", got: reserved as u64 });
        }
        if payload_bytes > MAX_PAYLOAD {
            return Err(WireError::PayloadTooBig { got: payload_bytes, max: MAX_PAYLOAD });
        }
        let payload = r.rest();
        if payload.len() != payload_bytes as usize {
            return Err(WireError::PayloadLength { declared: payload_bytes, have: payload.len() });
        }
        Ok((Header { minor, opcode, flags, request_id, payload_bytes }, payload))
    }

    /// Write the header for a payload of `payload_bytes` into the first [`HEADER_BYTES`] of `out`.
    pub fn encode(opcode: Opcode, flags: u32, request_id: u64, payload_bytes: u32, out: &mut [u8]) -> Result<(), WireError> {
        use codec::Writer;
        if payload_bytes > MAX_PAYLOAD {
            return Err(WireError::PayloadTooBig { got: payload_bytes, max: MAX_PAYLOAD });
        }
        if flags & !FLAGS_KNOWN != 0 {
            return Err(WireError::UnknownFlags { got: flags });
        }
        let mut w = Writer::new(out);
        w.u32("header.magic", MAGIC)?;
        w.u16("header.major", ABI_MAJOR)?;
        w.u16("header.minor", ABI_MINOR)?;
        w.u32("header.opcode", opcode as u32)?;
        w.u32("header.flags", flags)?;
        w.u64("header.request_id", request_id)?;
        w.u32("header.payload_bytes", payload_bytes)?;
        w.u32("header.reserved", 0)?;
        Ok(())
    }
}

macro_rules! coded {
    ($(#[$m:meta])* $name:ident : $ty:ty { $($(#[$vm:meta])* $var:ident = $val:expr, $text:expr;)* }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr($ty)]
        pub enum $name { $($(#[$vm])* $var = $val,)* }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$var,)*];
            pub const fn from_raw(v: $ty) -> Option<$name> {
                match v { $($val => Some($name::$var),)* _ => None }
            }
            pub const fn name(self) -> &'static str {
                match self { $($name::$var => $text,)* }
            }
        }
    };
}

coded! {
    /// The first `u32` of every reply payload. The values from 0x10 up are the SEPARATED CAUSES of Astra's audio ruling, so Aura can tell the
    /// desktop what is wrong without asking the owner to plug in hardware the OS has not learned to drive.
    Status : u32 {
        Ok = 0, "OK";
        /// A well-formed request for something this device does not do.
        Unsupported = 1, "UNSUPPORTED";
        Malformed = 2, "MALFORMED";
        BadVersion = 3, "BAD-VERSION";
        UnknownStream = 4, "UNKNOWN-STREAM";
        /// A stream is already open: one stream at a time, refused by name, never serialised behind.
        StreamBusy = 5, "STREAM-BUSY";
        /// The request does not fit the stream's state (start before open, write while stopped).
        BadState = 6, "BAD-STATE";
        /// The format is not one the topology offers (S16 / 48 kHz / stereo).
        FormatRefused = 7, "FORMAT-REFUSED";
        VolumeRefused = 8, "VOLUME-REFUSED";
        RingRegionUnavailable = 9, "RING-REGION-UNAVAILABLE";
        TooLarge = 10, "TOO-LARGE";
        NoMemory = 11, "NO-MEMORY";
        FwNotFound = 12, "FW-NOT-FOUND";
        FwRange = 13, "FW-RANGE";
        /// The device's PCI function, windows or controller are not usable.
        BackendUnavailable = 0x10, "BACKEND-UNAVAILABLE";
        /// A firmware, topology or NHLT file is missing or refused.
        FirmwareUnavailable = 0x11, "FIRMWARE-UNAVAILABLE";
        /// A bring-up step failed; the STATUS names the step.
        InitFailed = 0x12, "INIT-FAILED";
        HeadphoneDisconnected = 0x13, "HEADPHONE-DISCONNECTED";
        SpeakerProtectionUnavailable = 0x14, "SPEAKER-PROTECTION-UNAVAILABLE";
    }
}

coded! {
    /// Why nothing can play right now. At most one is primary; [`Cause::SpeakerProtectionUnavailable`] is reported ONLY when it is the
    /// only thing left (the speakers are refused in every state - [`StatusBody::speaker`] says so separately).
    Cause : u8 {
        None = 0, "none";
        BackendUnavailable = 1, "backend-unavailable";
        FirmwareUnavailable = 2, "firmware-unavailable";
        InitFailed = 3, "init-failed";
        HeadphoneDisconnected = 4, "headphone-disconnected";
        SpeakerProtectionUnavailable = 5, "speaker-protection-unavailable";
    }
}

/// A cause's reply status is `CAUSE_STATUS_BASE + cause`: the arithmetic below is the one home of the mapping, and the compile-time asserts pin
/// each [`Status`] cause variant to it, so the two enums cannot drift.
pub const CAUSE_STATUS_BASE: u32 = 0x0F;

impl Cause {
    /// The reply status a refusal because of this cause carries.
    pub const fn status(self) -> Status {
        match self {
            Cause::None => Status::Ok,
            Cause::BackendUnavailable => Status::BackendUnavailable,
            Cause::FirmwareUnavailable => Status::FirmwareUnavailable,
            Cause::InitFailed => Status::InitFailed,
            Cause::HeadphoneDisconnected => Status::HeadphoneDisconnected,
            Cause::SpeakerProtectionUnavailable => Status::SpeakerProtectionUnavailable,
        }
    }
}

const _: () = {
    assert!(Status::BackendUnavailable as u32 == CAUSE_STATUS_BASE + Cause::BackendUnavailable as u32);
    assert!(Status::FirmwareUnavailable as u32 == CAUSE_STATUS_BASE + Cause::FirmwareUnavailable as u32);
    assert!(Status::InitFailed as u32 == CAUSE_STATUS_BASE + Cause::InitFailed as u32);
    assert!(Status::HeadphoneDisconnected as u32 == CAUSE_STATUS_BASE + Cause::HeadphoneDisconnected as u32);
    assert!(Status::SpeakerProtectionUnavailable as u32 == CAUSE_STATUS_BASE + Cause::SpeakerProtectionUnavailable as u32);
};

coded! {
    /// The device as a whole.
    State : u8 {
        /// Not started, or the backend is unusable.
        Down = 0, "down";
        Starting = 1, "starting";
        /// Everything verified; no stream running. The DSP is off.
        Idle = 2, "idle";
        Running = 3, "running";
        /// A stream is open but cannot run (jack out): paused, codec and DSP in their safe state.
        Suspended = 4, "suspended";
        /// A fault: the safe state was entered (see [`StatusBody::safe_verified`]).
        Faulted = 5, "faulted";
    }
}

coded! {
    Jack : u8 {
        Absent = 0, "absent";
        Headphones = 1, "headphones";
        Headset = 2, "headset";
    }
}

coded! {
    StreamPhase : u8 {
        None = 0, "none";
        Open = 1, "open";
        Running = 2, "running";
        Suspended = 3, "suspended";
    }
}

coded! {
    /// Where a stream asks to be played. Only the headphones are served; the speakers are refused by name.
    Route : u8 {
        Headphones = 0, "headphones";
        Speakers = 1, "speakers";
    }
}

coded! {
    /// The speakers. There is one state: refused.
    Speaker : u8 {
        Refused = 0, "refused";
    }
}

coded! {
    /// The speaker amplifiers' reset line (SPKR_RST_L), which keeps them silent whatever else happens.
    SpeakerHold : u8 {
        /// No grant for the pad: the hold is a named GAP.
        NotGranted = 0, "not-granted";
        /// The pad was driven low and read back low.
        Held = 1, "held";
        /// The pad was driven but could not be read back.
        Unverified = 2, "unverified";
        /// The pad write was refused or read back high.
        Failed = 3, "failed";
    }
}

/// `StatusBody::step` is this many bytes of ASCII, NUL padded: the name of the step an init failure stopped at.
pub const STEP_BYTES: usize = 32;
pub const STATUS_BODY_BYTES: usize = 12 + 4 + 4 + 4 + STEP_BYTES;

/// The STATUS: what the device is, why it cannot play, and the facts a UI shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusBody {
    pub state: State,
    pub cause: Cause,
    pub jack: Jack,
    pub stream: StreamPhase,
    /// The codec's safe state was proven by readback the last time it was entered.
    pub safe_verified: bool,
    pub speaker: Speaker,
    pub speaker_hold: SpeakerHold,
    /// Bumped on every change, so a UI discards a stale event.
    pub generation: u32,
    /// The volume Aura last asked for, 0..=[`VOLUME_MAX`].
    pub volume_level: u16,
    /// The firmware's or the bus's own code for the failure (an IPC status, a NAK), 0 when none.
    pub detail: u32,
    /// The step an [`Cause::InitFailed`] stopped at, ASCII, NUL padded.
    pub step: [u8; STEP_BYTES],
}

/// The volume scale of THIS protocol: 0 is silence, 1000 the device's ceiling. It has one home, here; Aura's adapter maps its own scale onto
/// it (today both are 0..=1000), and the device maps it onto codec gain codes below its hardware ceiling.
pub const VOLUME_MAX: u16 = 1000;

impl StatusBody {
    pub const fn new() -> Self {
        Self {
            state: State::Down,
            cause: Cause::BackendUnavailable,
            jack: Jack::Absent,
            stream: StreamPhase::None,
            safe_verified: false,
            speaker: Speaker::Refused,
            speaker_hold: SpeakerHold::NotGranted,
            generation: 0,
            volume_level: 0,
            detail: 0,
            step: [0; STEP_BYTES],
        }
    }
    /// Store a step name, cut at 31 bytes so a NUL always ends it.
    pub fn set_step(&mut self, name: &str) {
        self.step = [0; STEP_BYTES];
        let n = name.len().min(STEP_BYTES - 1);
        self.step[..n].copy_from_slice(&name.as_bytes()[..n]);
    }
    pub fn step_str(&self) -> &str {
        let n = self.step.iter().position(|b| *b == 0).unwrap_or(STEP_BYTES);
        core::str::from_utf8(&self.step[..n]).unwrap_or("")
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        use codec::Writer;
        let mut w = Writer::new(out);
        w.u8("status.state", self.state as u8)?;
        w.u8("status.cause", self.cause as u8)?;
        w.u8("status.jack", self.jack as u8)?;
        w.u8("status.stream", self.stream as u8)?;
        w.u8("status.safe_verified", self.safe_verified as u8)?;
        w.u8("status.speaker", self.speaker as u8)?;
        w.u8("status.speaker_hold", self.speaker_hold as u8)?;
        w.u8("status.reserved", 0)?;
        w.u32("status.generation", self.generation)?;
        w.u16("status.volume_level", self.volume_level)?;
        w.u16("status.reserved2", 0)?;
        w.u32("status.detail", self.detail)?;
        w.u32("status.reserved3", 0)?;
        w.bytes("status.step", &self.step)?;
        Ok(w.pos())
    }

    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        use codec::Reader;
        let mut r = Reader::new(b);
        let bad = |what: &'static str, v: u8| WireError::BadValue { what, got: v as u64 };
        let state = r.u8("status.state")?;
        let cause = r.u8("status.cause")?;
        let jack = r.u8("status.jack")?;
        let stream = r.u8("status.stream")?;
        let safe = r.u8("status.safe_verified")?;
        let speaker = r.u8("status.speaker")?;
        let hold = r.u8("status.speaker_hold")?;
        let rsv = r.u8("status.reserved")?;
        let generation = r.u32("status.generation")?;
        let volume_level = r.u16("status.volume_level")?;
        let rsv2 = r.u16("status.reserved2")?;
        let detail = r.u32("status.detail")?;
        let rsv3 = r.u32("status.reserved3")?;
        let mut step = [0u8; STEP_BYTES];
        step.copy_from_slice(r.bytes("status.step", STEP_BYTES)?);
        r.finish()?;
        if rsv != 0 || rsv2 != 0 || rsv3 != 0 {
            return Err(WireError::ReservedNonZero { what: "status.reserved", got: (rsv as u64) | (rsv2 as u64) << 8 | (rsv3 as u64) << 24 });
        }
        if safe > 1 {
            return Err(bad("status.safe_verified", safe));
        }
        if volume_level > VOLUME_MAX {
            return Err(WireError::LimitExceeded { what: "status.volume_level", got: volume_level as u64, max: VOLUME_MAX as u64 });
        }
        Ok(Self {
            state: State::from_raw(state).ok_or(bad("status.state", state))?,
            cause: Cause::from_raw(cause).ok_or(bad("status.cause", cause))?,
            jack: Jack::from_raw(jack).ok_or(bad("status.jack", jack))?,
            stream: StreamPhase::from_raw(stream).ok_or(bad("status.stream", stream))?,
            safe_verified: safe == 1,
            speaker: Speaker::from_raw(speaker).ok_or(bad("status.speaker", speaker))?,
            speaker_hold: SpeakerHold::from_raw(hold).ok_or(bad("status.speaker_hold", hold))?,
            generation,
            volume_level,
            detail,
            step,
        })
    }
}

impl Default for StatusBody {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a complete frame (header then payload) in `out`; returns its length.
pub fn frame(opcode: Opcode, flags: u32, request_id: u64, payload: &[u8], out: &mut [u8]) -> Result<usize, WireError> {
    if payload.len() > MAX_PAYLOAD as usize {
        return Err(WireError::PayloadTooBig { got: payload.len() as u32, max: MAX_PAYLOAD });
    }
    let total = HEADER_BYTES + payload.len();
    if out.len() < total {
        return Err(WireError::NoRoom { what: "frame", need: total, have: out.len() });
    }
    Header::encode(opcode, flags, request_id, payload.len() as u32, out)?;
    out[HEADER_BYTES..total].copy_from_slice(payload);
    Ok(total)
}
