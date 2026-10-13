// SPDX-License-Identifier: GPL-2.0-only
//! The request protocol: one frame in, one reply out, events when subscribed. Every request is answered - an unsupported or malformed one
//! with its name - so a client learns what the device will not do from the device, not from a timeout.
//!
//! One stream at a time (a second OPEN is STREAM-BUSY, never queued behind the first); the format is the one the topology offers
//! (S16 / 48 kHz / stereo, everything else FORMAT-REFUSED so Aura converts); PCM is written inline and accepted in whole frames up to the
//! queue's room (a short write, never a block); the region form of PCM is defined and refused by name until the kernel can map a client
//! region; the SPEAKER route is refused by name with SPEAKER-PROTECTION-UNAVAILABLE and touches nothing.
use crate::codec::dac_code_for;
use crate::device::{Device, Stream};
use crate::machine::{Machine, Recv};
use crate::session::{self, FORMAT, PERIODS, PERIOD_FRAMES};
use crate::stream::FIFO_PERIODS;
use avs_wire::msg::{self, Caps, Format, HelloReply, Open, OpenReply, OpenRing, PcmReply, PcmWrite, PhaseReply, SetVolume, StreamRef, Subscribe, VolumeReply, LATENCY_UNMEASURED, MAX_CAPS_FORMATS};
use avs_wire::{Header, Opcode, Route, Status, MAX_FRAME_BYTES};

impl<M: Machine> Device<M> {
    fn reply(&mut self, op: Opcode, id: u64, status: Status, body: &[u8]) {
        let mut out = [0u8; 200];
        if let Ok(n) = msg::reply(op, id, status, body, &mut out) {
            if !self.send_frame(&out[..n]) {
                self.stats.dropped_replies += 1;
            }
        }
    }

    /// The status a refusal because the device cannot work carries: the separated cause.
    fn cause_status(&self) -> Status {
        self.st.cause.status()
    }

    /// Take one request frame off the endpoint and answer it.
    pub fn on_request(&mut self) {
        if self.rx.len() < MAX_FRAME_BYTES {
            // allocated once; a heap that cannot give 32 KiB refuses the request by leaving it queued
            if self.rx.try_reserve_exact(MAX_FRAME_BYTES).is_err() {
                return;
            }
            self.rx.resize(MAX_FRAME_BYTES, 0);
        }
        let mut buf = core::mem::take(&mut self.rx);
        let r = self.rig.m.recv_request(&mut buf);
        match r {
            Recv::Empty => {}
            Recv::PeerDead => self.client_died(),
            Recv::Oversized(_) | Recv::Error(_) => {
                self.stats.malformed += 1;
                self.reply(Opcode::Status, 0, Status::Malformed, &[]);
            }
            Recv::Frame(n) => {
                self.stats.requests += 1;
                self.dispatch(&buf[..n]);
            }
        };
        self.rx = buf;
        
    }

    pub fn dispatch(&mut self, frame: &[u8]) {
        let (h, payload) = match Header::decode(frame) {
            Ok(x) => x,
            Err(e) => {
                self.stats.malformed += 1;
                let st = if e.name() == "bad-major" { Status::BadVersion } else { Status::Malformed };
                self.reply(Opcode::Status, 0, st, &[]);
                return;
            }
        };
        let id = h.request_id;
        if h.is_reply() || h.is_event() || h.opcode.is_event() || h.opcode.is_firmware() {
            self.reply(h.opcode, id, Status::Unsupported, &[]);
            return;
        }
        let mut body = [0u8; 120];
        match h.opcode {
            Opcode::Hello => {
                let n = HelloReply::current().encode(&mut body).unwrap_or(0);
                self.reply(h.opcode, id, Status::Ok, &body[..n]);
            }
            Opcode::Status => {
                self.publish();
                let n = self.st.encode(&mut body).unwrap_or(0);
                self.reply(h.opcode, id, Status::Ok, &body[..n]);
            }
            Opcode::Subscribe => match Subscribe::decode(payload) {
                Ok(s) => {
                    self.subs = s.mask;
                    self.reply(h.opcode, id, Status::Ok, &[]);
                }
                Err(_) => self.reply(h.opcode, id, Status::Malformed, &[]),
            },
            Opcode::StreamCaps => {
                let mut formats = [Format { rate: 0, channels: 0, container_bits: 0, valid_bits: 0 }; MAX_CAPS_FORMATS];
                formats[0] = FORMAT;
                let caps = Caps { formats, n_formats: 1, period_frames: PERIOD_FRAMES, periods: PERIODS, latency_frames: LATENCY_UNMEASURED, fifo_bytes: session::RING_BYTES * FIFO_PERIODS };
                match caps.encode(&mut body) {
                    Ok(n) => self.reply(h.opcode, id, Status::Ok, &body[..n]),
                    Err(_) => self.reply(h.opcode, id, Status::TooLarge, &[]),
                }
            }
            Opcode::StreamOpen => self.on_open(id, payload),
            Opcode::StreamOpenRing => match OpenRing::decode(payload) {
                // defined, refused: the kernel cannot map a client's region into a device (2026-10-13). delete-when: the kernel has a region-map
                // call and this arm opens the ring the client names
                Ok(_) => self.reply(h.opcode, id, Status::RingRegionUnavailable, &[]),
                Err(_) => self.reply(h.opcode, id, Status::Malformed, &[]),
            },
            Opcode::StreamStart => self.on_start(id, payload),
            Opcode::StreamStop | Opcode::StreamClose => self.on_stop_close(h.opcode, id, payload),
            Opcode::PcmWrite => self.on_pcm(id, payload),
            Opcode::SetVolume => self.on_volume(id, payload),
            Opcode::EvStatus | Opcode::EvJack | Opcode::EvPeriod | Opcode::FwStat | Opcode::FwRead => self.reply(h.opcode, id, Status::Unsupported, &[]),
        }
    }

    fn on_open(&mut self, id: u64, payload: &[u8]) {
        let op = Opcode::StreamOpen;
        let Ok(o) = Open::decode(payload) else { return self.reply(op, id, Status::Malformed, &[]) };
        if o.route == Route::Speakers {
            // refused before anything else is looked at, and nothing is touched: the speakers are not served in any state
            return self.reply(op, id, Status::SpeakerProtectionUnavailable, &[]);
        }
        if o.format != FORMAT || (o.period_frames != 0 && o.period_frames != PERIOD_FRAMES) {
            return self.reply(op, id, Status::FormatRefused, &[]);
        }
        if self.stream.is_some() {
            return self.reply(op, id, Status::StreamBusy, &[]);
        }
        // a device that is not ready says WHY, with the cause's own status
        if !self.ensure_ready() {
            let st = self.cause_status();
            return self.reply(op, id, if st == Status::Ok { Status::InitFailed } else { st }, &[]);
        }
        let Some(fifo) = Self::new_fifo() else { return self.reply(op, id, Status::NoMemory, &[]) };
        let sid = self.next_stream;
        self.next_stream = self.next_stream.wrapping_add(1).max(1);
        self.stream = Some(Stream { id: sid, fifo, want_running: false, played_base: 0, consumed_base: 0, underruns_base: 0 });
        self.publish();
        let r = OpenReply { stream: sid, period_frames: PERIOD_FRAMES, periods: PERIODS, fifo_bytes: session::RING_BYTES * FIFO_PERIODS };
        let mut body = [0u8; 32];
        let n = r.encode(&mut body).unwrap_or(0);
        self.reply(op, id, Status::Ok, &body[..n]);
    }

    fn stream_ref(&mut self, op: Opcode, id: u64, payload: &[u8]) -> Option<u32> {
        let Ok(r) = StreamRef::decode(payload) else {
            self.reply(op, id, Status::Malformed, &[]);
            return None;
        };
        match &self.stream {
            Some(s) if s.id == r.stream => Some(r.stream),
            _ => {
                self.reply(op, id, Status::UnknownStream, &[]);
                None
            }
        }
    }

    fn phase_reply(&mut self, op: Opcode, id: u64, status: Status) {
        self.publish();
        let phase = self.st.stream;
        let cause = self.st.cause;
        let mut body = [0u8; 8];
        let n = PhaseReply { phase, cause }.encode(&mut body).unwrap_or(0);
        self.reply(op, id, status, &body[..n]);
    }

    fn on_start(&mut self, id: u64, payload: &[u8]) {
        let op = Opcode::StreamStart;
        if self.stream_ref(op, id, payload).is_none() {
            return;
        }
        if self.dsp.is_some() {
            return self.phase_reply(op, id, Status::Ok);
        }
        if let Some(s) = self.stream.as_mut() {
            s.want_running = true;
        }
        // a start is a retry: a recorded fault is cleared and the ladder climbed again
        self.fault = None;
        self.codec_ready = self.codec_ready && self.st.safe_verified;
        if !self.ensure_ready() {
            if let Some(s) = self.stream.as_mut() {
                s.want_running = true;
            }
            let st = self.cause_status();
            return self.phase_reply(op, id, if st == Status::Ok { Status::InitFailed } else { st });
        }
        if self.jack == avs_wire::Jack::Absent {
            // accepted, and held: the stream runs the moment headphones are in. The cause says why it is not running.
            return self.phase_reply(op, id, Status::Ok);
        }
        match self.start_session() {
            Ok(()) => self.phase_reply(op, id, Status::Ok),
            Err(_) => self.phase_reply(op, id, Status::InitFailed),
        }
    }

    fn on_stop_close(&mut self, op: Opcode, id: u64, payload: &[u8]) {
        if self.stream_ref(op, id, payload).is_none() {
            return;
        }
        self.stop_session(true);
        if op == Opcode::StreamClose {
            self.stream = None;
        } else if let Some(s) = self.stream.as_mut() {
            s.want_running = false;
            s.fifo.clear();
        }
        self.phase_reply(op, id, Status::Ok);
    }

    fn on_pcm(&mut self, id: u64, payload: &[u8]) {
        let op = Opcode::PcmWrite;
        let Ok(w) = PcmWrite::decode(payload) else { return self.reply(op, id, Status::Malformed, &[]) };
        let Some(s) = self.stream.as_mut() else { return self.reply(op, id, Status::UnknownStream, &[]) };
        if s.id != w.stream {
            return self.reply(op, id, Status::UnknownStream, &[]);
        }
        let accepted = s.fifo.push(w.data, session::FRAME_BYTES as usize) as u32;
        let (free, queued) = (s.fifo.free() as u32, s.fifo.queued() as u32);
        let mut body = [0u8; 16];
        let n = PcmReply { accepted, free, queued }.encode(&mut body).unwrap_or(0);
        self.reply(op, id, Status::Ok, &body[..n]);
    }

    fn on_volume(&mut self, id: u64, payload: &[u8]) {
        let op = Opcode::SetVolume;
        let Ok(v) = SetVolume::decode(payload) else { return self.reply(op, id, Status::Malformed, &[]) };
        self.level = Some(v.level);
        self.mute = v.mute;
        let applied = if self.dsp.is_some() {
            let Some(c) = self.codec.as_mut() else { return self.reply(op, id, Status::InitFailed, &[]) };
            match c.set_level(&mut self.rig, v.level, v.mute) {
                Ok(a) => a,
                Err(e) => {
                    // a volume change that cannot be applied leaves the output in an unknown gain: the safe state
                    self.fault(e.step(), e.detail());
                    return self.reply(op, id, Status::VolumeRefused, &[]);
                }
            }
        } else {
            // not running: what WOULD be applied, so the client can show it
            let policy = da7219_core::hp::GainPolicy::ATLAS_HEADPHONES;
            match dac_code_for(v.level, &policy) {
                Some(c) if !v.mute => crate::codec::Applied { dac_code: c, hp_code: policy.hp_initial, muted: false },
                _ => crate::codec::Applied { dac_code: policy.dac_initial, hp_code: policy.hp_initial, muted: true },
            }
        };
        let mut body = [0u8; 8];
        let n = VolumeReply { dac_code: applied.dac_code, hp_code: applied.hp_code, muted: applied.muted }.encode(&mut body).unwrap_or(0);
        self.publish();
        self.reply(op, id, Status::Ok, &body[..n]);
    }
}

