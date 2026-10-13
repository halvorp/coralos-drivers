// SPDX-License-Identifier: MIT OR Apache-2.0
//! The payload of every message. A request payload decodes with `decode`, a reply BODY (what follows the leading `u32` status) encodes with
//! `encode`; [`reply`] puts the status in front and the frame header on top. Every decode is total: a short payload, trailing bytes, a reserved
//! field that is not zero, or a value outside its range is a named [`WireError`], never a guess.
use crate::codec::{Reader, Writer};
use crate::{frame, Cause, Jack, Opcode, Route, Status, StreamPhase, WireError, FLAG_EVENT, FLAG_REPLY, MAX_PCM_BYTES, MAX_PAYLOAD};

/// Build a reply frame: the request's opcode and id, the status, then the body.
pub fn reply(opcode: Opcode, request_id: u64, status: Status, body: &[u8], out: &mut [u8]) -> Result<usize, WireError> {
    let mut p = [0u8; 4 + 128];
    if body.len() > 128 {
        return Err(WireError::PayloadTooBig { got: body.len() as u32, max: 128 });
    }
    p[..4].copy_from_slice(&(status as u32).to_le_bytes());
    p[4..4 + body.len()].copy_from_slice(body);
    frame(opcode, FLAG_REPLY, request_id, &p[..4 + body.len()], out)
}

/// Build an event frame (device to client, unasked).
pub fn event(opcode: Opcode, body: &[u8], out: &mut [u8]) -> Result<usize, WireError> {
    frame(opcode, FLAG_EVENT, 0, body, out)
}

/// A reply payload split into its status and its body.
pub fn split_reply(payload: &[u8]) -> Result<(Status, &[u8]), WireError> {
    let mut r = Reader::new(payload);
    let raw = r.u32("reply.status")?;
    let status = Status::from_raw(raw).ok_or(WireError::BadValue { what: "reply.status", got: raw as u64 })?;
    Ok((status, r.rest()))
}

/// What a client may subscribe to.
pub const SUB_STATUS: u32 = 1 << 0;
pub const SUB_JACK: u32 = 1 << 1;
pub const SUB_PERIOD: u32 = 1 << 2;
pub const SUB_ALL: u32 = SUB_STATUS | SUB_JACK | SUB_PERIOD;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subscribe {
    pub mask: u32,
}

impl Subscribe {
    pub fn decode(p: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let mask = r.u32("subscribe.mask")?;
        r.finish()?;
        if mask & !SUB_ALL != 0 {
            return Err(WireError::BadValue { what: "subscribe.mask", got: mask as u64 });
        }
        Ok(Self { mask })
    }
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32("subscribe.mask", self.mask)?;
        Ok(w.pos())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelloReply {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub max_pcm_bytes: u32,
}

impl HelloReply {
    pub fn current() -> Self {
        Self { abi_major: crate::ABI_MAJOR, abi_minor: crate::ABI_MINOR, max_pcm_bytes: MAX_PCM_BYTES }
    }
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u16("hello.major", self.abi_major)?;
        w.u16("hello.minor", self.abi_minor)?;
        w.u32("hello.max_pcm_bytes", self.max_pcm_bytes)?;
        Ok(w.pos())
    }
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(b);
        let s = Self { abi_major: r.u16("hello.major")?, abi_minor: r.u16("hello.minor")?, max_pcm_bytes: r.u32("hello.max_pcm_bytes")? };
        r.finish()?;
        Ok(s)
    }
}

/// One PCM format. Only formats the topology offers are ever accepted; Aura converts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub rate: u32,
    pub channels: u8,
    /// The container: 16 or 32.
    pub container_bits: u8,
    /// The valid bits within the container.
    pub valid_bits: u8,
}

impl Format {
    /// The one format Atlas' headset topology offers (da7219-tplg.bin `ssp1p_fe`: 2 channels, 48 kHz, 16-bit container).
    pub const S16_48K_STEREO: Format = Format { rate: 48_000, channels: 2, container_bits: 16, valid_bits: 16 };
    pub const BYTES: usize = 8;
    pub const fn frame_bytes(&self) -> u32 {
        self.channels as u32 * (self.container_bits as u32 / 8)
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), WireError> {
        w.u32("format.rate", self.rate)?;
        w.u8("format.channels", self.channels)?;
        w.u8("format.container_bits", self.container_bits)?;
        w.u8("format.valid_bits", self.valid_bits)?;
        w.u8("format.reserved", 0)
    }
    fn read(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let rate = r.u32("format.rate")?;
        let channels = r.u8("format.channels")?;
        let container_bits = r.u8("format.container_bits")?;
        let valid_bits = r.u8("format.valid_bits")?;
        let rsv = r.u8("format.reserved")?;
        if rsv != 0 {
            return Err(WireError::ReservedNonZero { what: "format.reserved", got: rsv as u64 });
        }
        Ok(Self { rate, channels, container_bits, valid_bits })
    }
}

/// The most formats one STREAM-CAPS reply lists: a limit of the WIRE layout (a reply is a fixed-size frame), not of the device's tables.
pub const MAX_CAPS_FORMATS: usize = 4;
/// `latency_frames` when the host-DMA-to-pin delay has not been measured. Never a guess.
pub const LATENCY_UNMEASURED: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    pub formats: [Format; MAX_CAPS_FORMATS],
    pub n_formats: u8,
    pub period_frames: u32,
    pub periods: u32,
    /// Frames between the host DMA reading a sample and it leaving the jack, or [`LATENCY_UNMEASURED`].
    pub latency_frames: u32,
    /// The device-side queue PCM writes land in, in bytes.
    pub fifo_bytes: u32,
}

impl Caps {
    pub fn formats(&self) -> &[Format] {
        &self.formats[..self.n_formats as usize]
    }
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        if self.n_formats as usize > MAX_CAPS_FORMATS {
            return Err(WireError::LimitExceeded { what: "caps.n_formats", got: self.n_formats as u64, max: MAX_CAPS_FORMATS as u64 });
        }
        let mut w = Writer::new(out);
        w.u8("caps.n_formats", self.n_formats)?;
        w.bytes("caps.reserved", &[0; 3])?;
        for f in &self.formats {
            f.write(&mut w)?;
        }
        w.u32("caps.period_frames", self.period_frames)?;
        w.u32("caps.periods", self.periods)?;
        w.u32("caps.latency_frames", self.latency_frames)?;
        w.u32("caps.fifo_bytes", self.fifo_bytes)?;
        Ok(w.pos())
    }
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(b);
        let n = r.u8("caps.n_formats")?;
        if n as usize > MAX_CAPS_FORMATS {
            return Err(WireError::LimitExceeded { what: "caps.n_formats", got: n as u64, max: MAX_CAPS_FORMATS as u64 });
        }
        if r.bytes("caps.reserved", 3)? != [0; 3] {
            return Err(WireError::ReservedNonZero { what: "caps.reserved", got: 1 });
        }
        let mut formats = [Format { rate: 0, channels: 0, container_bits: 0, valid_bits: 0 }; MAX_CAPS_FORMATS];
        for f in formats.iter_mut() {
            *f = Format::read(&mut r)?;
        }
        let s = Self { formats, n_formats: n, period_frames: r.u32("caps.period_frames")?, periods: r.u32("caps.periods")?, latency_frames: r.u32("caps.latency_frames")?, fifo_bytes: r.u32("caps.fifo_bytes")? };
        r.finish()?;
        Ok(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open {
    pub format: Format,
    /// Only [`Route::Headphones`] is served; [`Route::Speakers`] is answered [`Status::SpeakerProtectionUnavailable`] and touches nothing.
    pub route: Route,
    /// 0: the device's default.
    pub period_frames: u32,
}

impl Open {
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        self.format.write(&mut w)?;
        w.u8("open.route", self.route as u8)?;
        w.bytes("open.reserved", &[0; 3])?;
        w.u32("open.period_frames", self.period_frames)?;
        Ok(w.pos())
    }
    pub fn decode(p: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let format = Format::read(&mut r)?;
        let route = r.u8("open.route")?;
        if r.bytes("open.reserved", 3)? != [0; 3] {
            return Err(WireError::ReservedNonZero { what: "open.reserved", got: 1 });
        }
        let s = Self {
            format,
            route: Route::from_raw(route).ok_or(WireError::BadValue { what: "open.route", got: route as u64 })?,
            period_frames: r.u32("open.period_frames")?,
        };
        r.finish()?;
        Ok(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenReply {
    pub stream: u32,
    pub period_frames: u32,
    pub periods: u32,
    pub fifo_bytes: u32,
}

impl OpenReply {
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32("open.stream", self.stream)?;
        w.u32("open.period_frames", self.period_frames)?;
        w.u32("open.periods", self.periods)?;
        w.u32("open.fifo_bytes", self.fifo_bytes)?;
        Ok(w.pos())
    }
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(b);
        let s = Self { stream: r.u32("open.stream")?, period_frames: r.u32("open.period_frames")?, periods: r.u32("open.periods")?, fifo_bytes: r.u32("open.fifo_bytes")? };
        r.finish()?;
        Ok(s)
    }
}

/// STREAM-OPEN-RING: the region form of PCM, defined now and refused until the kernel can map a client region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenRing {
    pub stream: u32,
    pub region: u32,
    pub layout: u32,
}

impl OpenRing {
    pub fn decode(p: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let s = Self { stream: r.u32("ring.stream")?, region: r.u32("ring.region")?, layout: r.u32("ring.layout")? };
        r.finish()?;
        Ok(s)
    }
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32("ring.stream", self.stream)?;
        w.u32("ring.region", self.region)?;
        w.u32("ring.layout", self.layout)?;
        Ok(w.pos())
    }
}

/// START, STOP and CLOSE name a stream and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamRef {
    pub stream: u32,
}

impl StreamRef {
    pub fn decode(p: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let s = Self { stream: r.u32("stream.id")? };
        r.finish()?;
        Ok(s)
    }
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32("stream.id", self.stream)?;
        Ok(w.pos())
    }
}

/// The reply body of START / STOP / CLOSE: the stream's phase afterwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseReply {
    pub phase: StreamPhase,
    /// The cause, when the stream could not run (a start with the jack out is accepted and SUSPENDED, with this cause).
    pub cause: Cause,
}

impl PhaseReply {
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u8("phase.phase", self.phase as u8)?;
        w.u8("phase.cause", self.cause as u8)?;
        w.u16("phase.reserved", 0)?;
        Ok(w.pos())
    }
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(b);
        let (p, c) = (r.u8("phase.phase")?, r.u8("phase.cause")?);
        if r.u16("phase.reserved")? != 0 {
            return Err(WireError::ReservedNonZero { what: "phase.reserved", got: 1 });
        }
        r.finish()?;
        Ok(Self {
            phase: StreamPhase::from_raw(p).ok_or(WireError::BadValue { what: "phase.phase", got: p as u64 })?,
            cause: Cause::from_raw(c).ok_or(WireError::BadValue { what: "phase.cause", got: c as u64 })?,
        })
    }
}

/// PCM-WRITE: the stream and the bytes. A whole number of frames is the CLIENT's duty; the device accepts what fits (a short write) and
/// says how much, never blocking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmWrite<'a> {
    pub stream: u32,
    pub data: &'a [u8],
}

impl<'a> PcmWrite<'a> {
    pub fn decode(p: &'a [u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let stream = r.u32("pcm.stream")?;
        let len = r.u32("pcm.len")?;
        if len > MAX_PCM_BYTES {
            return Err(WireError::LimitExceeded { what: "pcm.len", got: len as u64, max: MAX_PCM_BYTES as u64 });
        }
        let data = r.bytes("pcm.data", len as usize)?;
        r.finish()?;
        Ok(Self { stream, data })
    }
    /// The payload; the caller frames it.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        if self.data.len() > MAX_PCM_BYTES as usize {
            return Err(WireError::LimitExceeded { what: "pcm.len", got: self.data.len() as u64, max: MAX_PCM_BYTES as u64 });
        }
        let mut w = Writer::new(out);
        w.u32("pcm.stream", self.stream)?;
        w.u32("pcm.len", self.data.len() as u32)?;
        w.bytes("pcm.data", self.data)?;
        Ok(w.pos())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmReply {
    /// Bytes taken (a multiple of the frame size).
    pub accepted: u32,
    /// Bytes of room left in the queue.
    pub free: u32,
    /// Bytes waiting for the hardware.
    pub queued: u32,
}

impl PcmReply {
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32("pcm.accepted", self.accepted)?;
        w.u32("pcm.free", self.free)?;
        w.u32("pcm.queued", self.queued)?;
        Ok(w.pos())
    }
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(b);
        let s = Self { accepted: r.u32("pcm.accepted")?, free: r.u32("pcm.free")?, queued: r.u32("pcm.queued")? };
        r.finish()?;
        Ok(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetVolume {
    /// Aura's 0..=[`crate::VOLUME_MAX`] level; the device maps it to codec gain codes BELOW its own ceiling.
    pub level: u16,
    pub mute: bool,
}

impl SetVolume {
    pub fn decode(p: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(p);
        let level = r.u16("volume.level")?;
        let mute = r.u8("volume.mute")?;
        let rsv = r.u8("volume.reserved")?;
        r.finish()?;
        if mute > 1 || rsv != 0 {
            return Err(WireError::BadValue { what: "volume.mute", got: ((mute as u64) << 8) | rsv as u64 });
        }
        if level > crate::VOLUME_MAX {
            return Err(WireError::LimitExceeded { what: "volume.level", got: level as u64, max: crate::VOLUME_MAX as u64 });
        }
        Ok(Self { level, mute: mute == 1 })
    }
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u16("volume.level", self.level)?;
        w.u8("volume.mute", self.mute as u8)?;
        w.u8("volume.reserved", 0)?;
        Ok(w.pos())
    }
}

/// What the device applied: the codec gain codes (0x6F = 0 dB on the DAC) and whether the output is muted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeReply {
    pub dac_code: u8,
    pub hp_code: u8,
    pub muted: bool,
}

impl VolumeReply {
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u8("volume.dac_code", self.dac_code)?;
        w.u8("volume.hp_code", self.hp_code)?;
        w.u8("volume.muted", self.muted as u8)?;
        w.u8("volume.reserved", 0)?;
        Ok(w.pos())
    }
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(b);
        let s = Self { dac_code: r.u8("volume.dac_code")?, hp_code: r.u8("volume.hp_code")?, muted: r.u8("volume.muted")? == 1 };
        r.u8("volume.reserved")?;
        r.finish()?;
        Ok(s)
    }
}

/// EV-JACK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JackEvent {
    pub jack: Jack,
    /// Bit n set: headset button n is down (A = bit 0).
    pub buttons: u8,
}

impl JackEvent {
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u8("jack.jack", self.jack as u8)?;
        w.u8("jack.buttons", self.buttons)?;
        w.u16("jack.reserved", 0)?;
        Ok(w.pos())
    }
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(b);
        let (j, buttons) = (r.u8("jack.jack")?, r.u8("jack.buttons")?);
        if r.u16("jack.reserved")? != 0 {
            return Err(WireError::ReservedNonZero { what: "jack.reserved", got: 1 });
        }
        r.finish()?;
        Ok(Self { jack: Jack::from_raw(j).ok_or(WireError::BadValue { what: "jack.jack", got: j as u64 })?, buttons })
    }
}

/// EV-PERIOD: the audio clock and the credit. `played_frames` is what the HARDWARE has read from the stream's buffer (the host-DMA position),
/// never what was submitted and never an interrupt count; the pipeline latency to the pin is [`Caps::latency_frames`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeriodEvent {
    pub stream: u32,
    /// Bytes of the client's writes the device has moved into the DMA buffer.
    pub consumed_bytes: u64,
    pub played_frames: u64,
    /// Periods the device had to play as silence because no PCM was queued.
    pub underruns: u32,
    pub free_bytes: u32,
}

impl PeriodEvent {
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32("period.stream", self.stream)?;
        w.u32("period.underruns", self.underruns)?;
        w.u64("period.consumed_bytes", self.consumed_bytes)?;
        w.u64("period.played_frames", self.played_frames)?;
        w.u32("period.free_bytes", self.free_bytes)?;
        w.u32("period.reserved", 0)?;
        Ok(w.pos())
    }
    pub fn decode(b: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(b);
        let stream = r.u32("period.stream")?;
        let underruns = r.u32("period.underruns")?;
        let consumed_bytes = r.u64("period.consumed_bytes")?;
        let played_frames = r.u64("period.played_frames")?;
        let free_bytes = r.u32("period.free_bytes")?;
        if r.u32("period.reserved")? != 0 {
            return Err(WireError::ReservedNonZero { what: "period.reserved", got: 1 });
        }
        r.finish()?;
        Ok(Self { stream, consumed_bytes, played_frames, underruns, free_bytes })
    }
}

const _: () = assert!(MAX_PCM_BYTES + 8 <= MAX_PAYLOAD, "a full PCM write must fit one frame");
