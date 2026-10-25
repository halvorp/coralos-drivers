// SPDX-License-Identifier: GPL-2.0-only
//! The device: its state, its bring-up ladder, its STATUS, its sessions and its faults. The request protocol is [`crate::serve`], the
//! accessory-detect handling is [`crate::jack`]; both are `impl Device` blocks over the state defined here.
//!
//! THE LADDER (`ensure_ready`), each rung idempotent and re-tried at the next OPEN or START until it holds, and each failure naming its
//! cause: board record -> speaker amplifiers held in reset -> PCI identity -> controller discovery (the write list is built from it) ->
//! firmware set -> codec attach (initialised, safe state proven) -> the speaker topology (NOT fatal: the headphones do not depend on it;
//! DECISIONS D17). Backend problems (windows, identity, controller) are BACKEND-UNAVAILABLE;
//! a missing or refused file is FIRMWARE-UNAVAILABLE naming the file; a failed step is INIT-FAILED naming the step.
//!
//! TWO OUTPUTS. A stream asks for the headphones, the speakers or `Auto` ([`Device::desired_output`]); the device runs the path it belongs on and
//! moves it when the jack changes ([`Device::reroute`]), always taking the running path down - silence, off, proven - before the other starts.
//!
//! IDLE IS BLOCKED. [`Device::step`] is one blocking wait on the cAVS interrupt (only while a DSP session is running), the jack event, the
//! client's requests, and at most two deadlines: the codec's delayed ground switch and the running stream's watchdog. With nothing running
//! there is no deadline and the task sleeps until a request or a jack event.
use crate::allow::{Allow, I2cAllow};
use crate::codec::{jack_after, Codec};
use crate::controller::{discover, Controller};
use crate::firmware::{self, Files, MAX_BOARD_BYTES};
use crate::gpio;
use crate::log::emit;
use crate::machine::{Machine, Want, Wake};
use crate::rig::Rig;
use crate::session::{self, Bufs, Dsp, StartFail};
use crate::speakers::Speakers;
use crate::stream::{Fifo, FIFO_PERIODS};
use avs_wire::board::{Board, BOARD_FILE};
use avs_wire::{Cause, Jack, Output, Route, SpeakerHold, State, StatusBody};
use avs_fw_core::effect::Space;

/// A running stream that has seen no period interrupt for this long has stalled: three times the ring.
pub const WATCHDOG_MS: u64 = 3 * (session::PERIODS as u64 * session::PERIOD_FRAMES as u64 * 1000 / 48_000) + 1;

/// What a safe-state pass managed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SafeReport {
    pub codec_safe: bool,
    pub dma_stopped: bool,
    pub dsp_off: bool,
    /// The speaker amplifiers' reset line was driven low and read back low.
    pub speakers_held: bool,
}

/// A named reason with the number that goes with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Why {
    pub step: &'static str,
    pub detail: u32,
}

/// The open stream. One at a time: a second open is refused by name.
pub struct Stream {
    pub id: u32,
    /// Where the client asked to be played.
    pub route: Route,
    pub fifo: Fifo,
    /// The client asked for it to run (it may still be suspended: no headphones, a fault).
    pub want_running: bool,
    /// Frames the hardware read in earlier DSP sessions of this stream (a suspend and resume continue the count).
    pub played_base: u64,
    pub consumed_base: u64,
    pub underruns_base: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub requests: u64,
    pub malformed: u64,
    pub dropped_events: u64,
    pub dropped_replies: u64,
    pub sessions: u64,
    pub faults: u64,
    pub jack_passes: u64,
    pub periods: u64,
}

pub struct Device<M: Machine> {
    pub rig: Rig<M>,
    pub board: Option<Board>,
    pub(crate) codec: Option<Codec>,
    /// The speaker amplifiers of a board this port knows (the Pixelbook Go's two MAX98373), when the board record lists them.
    pub(crate) speakers: Option<Speakers>,
    /// `max98373-tplg.bin`, once fetched and checked.
    pub(crate) speaker_topology: Option<alloc::vec::Vec<u8>>,
    /// Why the speaker topology is not here (the last attempt).
    pub(crate) speaker_fw_fail: Option<Why>,
    pub(crate) ctl: Option<Controller>,
    pub(crate) files: Option<Files>,
    pub(crate) bufs: Option<Bufs>,
    pub dsp: Option<Dsp>,
    pub stream: Option<Stream>,
    pub jack: Jack,
    pub buttons: u8,
    pub(crate) gnd_deadline: Option<u64>,
    /// The codec's interrupt line is left masked at the pad owner after a storm, until a client's retry.
    pub(crate) jack_masked: bool,
    /// The pad interrupt owner is gone for good.
    pub(crate) jack_gone: bool,
    /// A client's session is open (its endpoints are held until [`Machine::release_session`]).
    pub(crate) session: bool,
    pub subs: u32,
    pub st: StatusBody,
    pub(crate) hold_done: bool,
    pub(crate) pci_ok: bool,
    pub(crate) codec_ready: bool,
    pub(crate) started: bool,
    pub(crate) backend_fail: Option<Why>,
    pub(crate) fw_fail: Option<Why>,
    pub(crate) fault: Option<Why>,
    pub level: Option<u16>,
    pub mute: bool,
    pub(crate) next_stream: u32,
    /// The receive buffer for request frames, allocated once.
    pub(crate) rx: alloc::vec::Vec<u8>,
    pub stats: Stats,
}

impl<M: Machine> Device<M> {
    /// A device that has touched nothing. [`Device::start`] runs the ladder.
    pub fn new(m: M) -> Self {
        let mut st = StatusBody::new();
        st.state = State::Starting;
        st.cause = Cause::None;
        Self {
            rig: Rig::new(m, Allow::read_only(), I2cAllow::none(), crate::allow::PadAllow::none()),
            board: None,
            codec: None,
            speakers: None,
            speaker_topology: None,
            speaker_fw_fail: None,
            ctl: None,
            files: None,
            bufs: None,
            dsp: None,
            stream: None,
            jack: Jack::Absent,
            buttons: 0,
            gnd_deadline: None,
            jack_masked: false,
            jack_gone: false,
            session: false,
            subs: 0,
            st,
            hold_done: false,
            pci_ok: false,
            codec_ready: false,
            started: false,
            backend_fail: None,
            fw_fail: None,
            fault: None,
            level: None,
            mute: false,
            next_stream: 1,
            rx: alloc::vec::Vec::new(),
            stats: Stats::default(),
        }
    }

    /// Climb the ladder once, then take a first look at the jack.
    pub fn start(&mut self) {
        self.ensure_ready();
        self.started = true;
        self.publish();
        if self.codec_ready {
            self.service_jack_pass();
        }
        self.publish();
        let s = self.st;
        emit(&mut self.rig.m, format_args!("[AVS-DEV START state={} cause={} jack={} hold={} safe={}]", s.state.name(), s.cause.name(), s.jack.name(), s.speaker_hold.name(), s.safe_verified));
    }

    // ---- the ladder ----

    fn fw_why(&mut self, step: &'static str, detail: u32) {
        self.fw_fail = Some(Why { step, detail });
    }

    fn load_board(&mut self) -> bool {
        let bytes = match firmware::fetch(&mut self.rig.m, BOARD_FILE, MAX_BOARD_BYTES) {
            Ok(b) => b,
            Err(e) => {
                self.fw_why("board", 0);
                emit(&mut self.rig.m, format_args!("[AVS-DEV FAIL step=BOARD why={}]", e.name()));
                return false;
            }
        };
        let board = match Board::decode(&bytes) {
            Ok(b) => b,
            Err(e) => {
                self.fw_why("board", 0);
                emit(&mut self.rig.m, format_args!("[AVS-DEV FAIL step=BOARD why={}]", e.name()));
                return false;
            }
        };
        let codec = match Codec::new(board.codec_addr, board.amps(), board.mclk_hz) {
            Ok(c) => c,
            Err(_) => {
                self.fw_why("board", 0);
                return false;
            }
        };
        match I2cAllow::new(board.codec_addr, board.amps()) {
            Ok(a) => self.rig.i2c = a,
            Err(_) => return false,
        }
        self.rig.m.board_loaded(&board);
        // the speakers: a board whose amplifiers are the Pixelbook Go's; any other board has no speaker path (SPEAKER-PROTECTION-UNAVAILABLE)
        self.speakers = match Speakers::for_board(board.amps(), board.spkr_rst_pad) {
            Ok(s) => s,
            Err(e) => {
                emit(&mut self.rig.m, format_args!("[AVS-DEV GAP step=SPEAKERS why={}]", e.step()));
                None
            }
        };
        self.codec = Some(codec);
        self.board = Some(board);
        true
    }

    /// Run every rung that has not yet held. Returns whether the device is ready to start a session.
    pub fn ensure_ready(&mut self) -> bool {
        self.backend_fail = None;
        self.fw_fail = None;
        let attached_before = self.codec_ready;
        let ok = self.climb();
        if ok && (!attached_before || self.jack_masked) && self.started {
            self.jack_masked = false;
            // the codec has just been attached on a retry: look at the jack now, as `start` does, so the status is not stale
            self.service_jack_pass();
        }
        self.publish();
        ok
    }

    fn climb(&mut self) -> bool {
        if self.board.is_none() && !self.load_board() {
            return false;
        }
        let Some(board) = self.board else { return false };
        if self.jack_gone {
            self.backend_fail = Some(Why { step: "jack-owner-gone", detail: 0 });
            return false;
        }
        if !self.hold_done {
            let hold = gpio::hold_speakers(&mut self.rig, board.spkr_rst_pad);
            self.st.speaker_hold = hold;
            emit(&mut self.rig.m, format_args!("[AVS-DEV SPEAKER-HOLD {}]", hold.name()));
            if hold == SpeakerHold::Failed {
                // the amplifiers' reset line cannot be held (refused, or something else holds it up): nothing is played on a board whose
                // speakers cannot be shown to be silent. Retried at the next open or start.
                self.fault = Some(Why { step: "speaker-hold", detail: 0 });
                return false;
            }
            self.hold_done = true;
            if hold == SpeakerHold::NotGranted {
                emit(&mut self.rig.m, format_args!("[AVS-DEV GAP step=SPEAKER-RESET-PAD why=pad-write-not-granted - the amplifiers are not held in reset by this device]"));
            }
        }
        if !self.pci_ok {
            match self.rig.read32(Space::PciConfig, 0) {
                Ok(v) if v == (board.pci_vendor as u32 | (board.pci_device as u32) << 16) => self.pci_ok = true,
                Ok(v) => {
                    self.backend_fail = Some(Why { step: "pci-identity", detail: v });
                    emit(&mut self.rig.m, format_args!("[AVS-DEV FAIL step=PCI-IDENTITY why=unknown-device id={:#010x}]", v));
                    return false;
                }
                Err(e) => {
                    self.backend_fail = Some(Why { step: "config-window", detail: 0 });
                    emit(&mut self.rig.m, format_args!("[AVS-DEV FAIL step=CONFIG-WINDOW why={}]", e.name()));
                    return false;
                }
            }
        }
        if self.ctl.is_none() {
            match discover(&mut self.rig, board.playback_stream as u32) {
                Ok(c) => match c.allow() {
                    Ok(a) => {
                        self.rig.m.layout_ready(&a);
                        self.rig.allow = a;
                        emit(&mut self.rig.m, format_args!("[AVS-DEV CONTROLLER streams={}+{} pp={:?} spb={:?} links={}]", c.streams.capture, c.streams.playback, c.caps.pp, c.caps.spb, c.links.len()));
                        self.ctl = Some(c);
                    }
                    Err(_) => {
                        self.backend_fail = Some(Why { step: "out-of-memory", detail: 0 });
                        return false;
                    }
                },
                Err(e) => {
                    self.backend_fail = Some(Why { step: e.name(), detail: 0 });
                    emit(&mut self.rig.m, format_args!("[AVS-DEV FAIL step=CONTROLLER why={}]", e.name()));
                    return false;
                }
            }
        }
        if self.files.is_none() {
            match firmware::load_all(&mut self.rig.m) {
                Ok(f) => {
                    emit(&mut self.rig.m, format_args!("[AVS-DEV FIRMWARE basefw={}B libs={} topology={}B nhlt={}B]", f.base.len(), f.libs.len(), f.topology.len(), f.nhlt.len()));
                    self.files = Some(f);
                }
                Err(f) => {
                    self.fw_why(f.file, 0);
                    emit(&mut self.rig.m, format_args!("[AVS-DEV FAIL step=FIRMWARE file={} why={}]", f.file, f.why));
                    return false;
                }
            }
        }
        if !self.codec_ready {
            let Some(codec) = self.codec.as_mut() else { return false };
            match codec.attach(&mut self.rig) {
                Ok(()) => {
                    self.codec_ready = true;
                    self.st.safe_verified = true;
                    self.fault = None;
                    emit(&mut self.rig.m, format_args!("[AVS-DEV CODEC attached, safe state verified]"));
                }
                Err(e) => {
                    self.fault = Some(Why { step: e.step(), detail: e.detail() });
                    self.st.safe_verified = false;
                    emit(&mut self.rig.m, format_args!("[AVS-DEV FAIL step=CODEC-ATTACH why={}]", e.step()));
                    return false;
                }
            }
        }
        self.load_speaker_topology();
        true
    }

    /// The speaker topology, once. A refusal is recorded and shown (status `speaker`, a speaker open's cause) and retried at the next open or
    /// start; it never fails the ladder.
    fn load_speaker_topology(&mut self) {
        if self.speaker_topology.is_some() || self.speakers.is_none() {
            return;
        }
        let Some(board) = self.board else { return };
        let ssp = board.speaker_ssps().first().copied().unwrap_or(0);
        match firmware::load_speaker_topology(&mut self.rig.m, ssp) {
            Ok(b) => {
                emit(&mut self.rig.m, format_args!("[AVS-DEV FIRMWARE speaker-topology={}B]", b.len()));
                self.speaker_topology = Some(b);
                self.speaker_fw_fail = None;
            }
            Err(f) => {
                emit(&mut self.rig.m, format_args!("[AVS-DEV GAP step=SPEAKER-TOPOLOGY file={} why={}]", f.file, f.why));
                self.speaker_fw_fail = Some(Why { step: f.why, detail: 0 });
            }
        }
    }

    /// Can the speaker path run now? The board's amplifiers, the checked topology, and the reset line proven held.
    pub fn speaker_ready(&self) -> bool {
        self.speakers.is_some() && self.speaker_topology.is_some() && matches!(self.st.speaker_hold, SpeakerHold::Held | SpeakerHold::Released)
    }

    /// Does this board have a speaker path at all (the record lists the Pixelbook Go's amplifiers)?
    pub fn speakers_on_board(&self) -> bool {
        self.speakers.is_some()
    }

    /// Where the open stream should be playing right now, if anywhere. Headphones need a plug; speakers need the protected path ready;
    /// `Auto` is the headphones while there is a plug and the speakers while there is not.
    pub fn desired_output(&self) -> Option<Output> {
        let s = self.stream.as_ref()?;
        let plug = self.jack != Jack::Absent;
        let spk = self.speaker_ready();
        match s.route {
            Route::Headphones => plug.then_some(Output::Headphones),
            Route::Speakers => spk.then_some(Output::Speakers),
            Route::Auto => {
                if plug {
                    Some(Output::Headphones)
                } else if spk {
                    Some(Output::Speakers)
                } else {
                    None
                }
            }
        }
    }

    // ---- sessions ----

    /// Open a stream's FIFO for the negotiated geometry.
    pub(crate) fn new_fifo() -> Option<Fifo> {
        Fifo::new((session::RING_BYTES * FIFO_PERIODS) as usize)
    }

    fn try_start(&mut self) -> Result<Dsp, StartFail> {
        let Some(out) = self.desired_output() else { return Err(StartFail::Layout("no-output")) };
        let (Some(board), Some(ctl), Some(codec), Some(stream)) = (self.board, self.ctl.as_ref(), self.codec.as_mut(), self.stream.as_mut()) else {
            return Err(StartFail::Layout("not-ready"));
        };
        let Some(files) = self.files.as_ref() else { return Err(StartFail::Layout("not-ready")) };
        if self.bufs.is_none() {
            self.bufs = Some(session::alloc_bufs(&mut self.rig.m).map_err(StartFail::Dma)?);
        }
        let Some(bufs) = self.bufs.as_ref() else { return Err(StartFail::OutOfMemory) };
        // the speakers stay held: re-asserted at every session start, so a reset line something else released is caught here
        self.st.speaker_hold = match self.speakers.as_mut() {
            Some(s) => s.hold(&mut self.rig),
            None => gpio::hold_speakers(&mut self.rig, board.spkr_rst_pad),
        };
        if self.st.speaker_hold == SpeakerHold::Failed {
            return Err(StartFail::Layout("speaker-hold"));
        }
        let level = if self.mute { Some(0) } else { self.level };
        let args = session::StartArgs { ctl, files, bufs, board: &board, level, out, speaker_topology: self.speaker_topology.as_deref() };
        session::start_dsp(&mut self.rig, &args, codec, self.speakers.as_mut(), &mut stream.fifo)
    }

    /// Bring a DSP session up for the open stream. On failure the device is left in the safe state with the fault recorded.
    pub fn start_session(&mut self) -> Result<(), StartFail> {
        let t0 = self.rig.m.now_ms();
        match self.try_start() {
            Ok(dsp) => {
                self.stats.sessions += 1;
                let ms = self.rig.m.now_ms().saturating_sub(t0);
                self.dsp = Some(dsp);
                self.fault = None;
                self.st.safe_verified = false; // the path is live: the safe state is no longer the state
                if self.dsp.as_ref().is_some_and(|d| d.out == Output::Speakers) {
                    self.st.speaker_hold = SpeakerHold::Released;
                }
                self.rig.m.rearm_avs_irq();
                emit(&mut self.rig.m, format_args!("[AVS-DEV SESSION up in {} ms output={}]", ms, self.dsp.as_ref().map_or("none", |d| d.out.name())));
                Ok(())
            }
            Err(e) => {
                self.stats.faults += 1;
                self.fault = Some(Why { step: e.step(), detail: e.detail() });
                self.st.safe_verified = self.output_stage_is_safe();
                self.settle_speaker_hold();
                emit(&mut self.rig.m, format_args!("[AVS-DEV FAIL step={} stage={} detail={:#x} safe={}]", e.step(), e.stage(), e.detail(), self.st.safe_verified));
                Err(e)
            }
        }
    }

    /// Take the session down (if there is one), fold its counters into the stream, and record whether the codec is verifiably safe.
    pub fn stop_session(&mut self, graceful: bool) {
        let Some(dsp) = self.dsp.take() else { return };
        if let Some(s) = self.stream.as_mut() {
            s.played_base += dsp.acct.played_frames();
            s.consumed_base += dsp.acct.pcm_fed;
            s.underruns_base = s.underruns_base.saturating_add(dsp.acct.underruns);
        }
        let (Some(ctl), Some(codec)) = (self.ctl.as_ref(), self.codec.as_mut()) else { return };
        let rep = session::stop_dsp(&mut self.rig, ctl, codec, self.speakers.as_mut(), dsp, graceful);
        self.st.safe_verified = rep.codec_safe;
        if let Some(h) = rep.spk_hold {
            self.st.speaker_hold = h;
        }
        emit(&mut self.rig.m, format_args!("[AVS-DEV SESSION down codec_safe={} dsp_off={} errors={}]", rep.codec_safe, rep.dsp_off, rep.errors));
    }

    /// A fault while running: the safe state first, then the record. The stream stays open and suspended; the client decides.
    pub fn fault(&mut self, step: &'static str, detail: u32) {
        self.stats.faults += 1;
        emit(&mut self.rig.m, format_args!("[AVS-DEV FAULT step={} detail={:#x}]", step, detail));
        self.stop_session(false);
        if self.dsp.is_none() && !self.st.safe_verified {
            // no session to unwind (or its unwind could not prove it): ask the output stages directly
            let codec_safe = self.codec.as_mut().is_some_and(|c| c.safe_state(&mut self.rig).is_ok());
            let spk_safe = self.speakers.as_mut().is_none_or(|s| s.safe(&mut self.rig, true).verified());
            self.st.safe_verified = codec_safe && spk_safe;
        }
        self.fault = Some(Why { step, detail });
        self.publish();
    }

    /// After a failed start: the status's hold is what the pad says now, not what it said before the amplifiers were released.
    fn settle_speaker_hold(&mut self) {
        if matches!(self.st.speaker_hold, SpeakerHold::Held | SpeakerHold::Released) {
            let held = self.speakers.as_ref().map(|s| s.pad()).is_some_and(|pad| crate::gpio::speakers_held(&mut self.rig, pad));
            self.st.speaker_hold = if held { SpeakerHold::Held } else { SpeakerHold::Failed };
        }
    }

    /// Is every output stage in its verified safe state right now? The codec by readback; the amplifiers in hardware reset by the pad.
    fn output_stage_is_safe(&mut self) -> bool {
        let codec = self.codec.as_mut().is_some_and(|c| c.is_safe(&mut self.rig));
        let spk = match self.speakers.as_ref() {
            Some(s) => {
                let pad = s.pad();
                crate::gpio::speakers_held(&mut self.rig, pad)
            }
            None => true,
        };
        codec && spk
    }

    /// What a dying task does: the codec to its safe state, the playback stream's DMA stopped, the DSP core off. Register writes and the
    /// codec's plans only - no allocation, no messages - because it runs when the task is already broken (a panic). It needs no session
    /// record: the stream and the core are the ones the board names, found by discovery.
    pub fn emergency_safe_state(&mut self) -> SafeReport {
        self.safe_pass(true)
    }

    fn safe_pass(&mut self, speakers: bool) -> SafeReport {
        let mut rep = SafeReport::default();
        // the amplifiers first: they are what can hurt something
        if let (true, Some(s)) = (speakers, self.speakers.as_mut()) {
            rep.speakers_held = s.emergency(&mut self.rig).verified();
        }
        if let Some(c) = self.codec.as_mut() {
            rep.codec_safe = c.safe_state(&mut self.rig).is_ok();
        }
        if let Some(ctl) = self.ctl.as_ref() {
            if let Ok(p) = hda_ext_core::stream::stop(&ctl.streams, ctl.playback.index) {
                rep.dma_stopped = crate::fwexec::run_program(&mut self.rig, &p).is_ok();
            }
            rep.dsp_off = crate::controller::dsp_off(&mut self.rig).is_ok();
        }
        rep
    }

    /// The SAFE-ONLY role (the loader launches the device with role 1 after a task died without a clean exit): find the hardware, hold the
    /// speakers, and make everything safe, which a dead predecessor may have left running - a DMA engine reading a freed buffer, a DSP
    /// core powered, a codec driving the jack. It serves nothing and exits.
    pub fn safe_only(&mut self) -> SafeReport {
        if self.board.is_none() && !self.load_board() {
            emit(&mut self.rig.m, format_args!("[AVS-DEV SAFE-ONLY FAIL step=BOARD]"));
            return SafeReport::default();
        }
        let Some(board) = self.board else { return SafeReport::default() };
        // the reset line first of all: through the speakers' own emergency pass (their parts off if the fence is armed, then the line low) on a
        // board that has them, directly on any other
        let speakers_held = match self.speakers.as_mut() {
            Some(s) => {
                let h = s.emergency(&mut self.rig).verified();
                self.st.speaker_hold = if h { SpeakerHold::Held } else { SpeakerHold::Failed };
                Some(h)
            }
            None => {
                self.st.speaker_hold = gpio::hold_speakers(&mut self.rig, board.spkr_rst_pad);
                None
            }
        };
        if let Ok(v) = self.rig.read32(Space::PciConfig, 0) {
            self.pci_ok = v == (board.pci_vendor as u32 | (board.pci_device as u32) << 16);
        }
        if self.pci_ok && self.ctl.is_none() {
            if let Ok(c) = discover(&mut self.rig, board.playback_stream as u32) {
                if let Ok(a) = c.allow() {
                    self.rig.m.layout_ready(&a);
                    self.rig.allow = a;
                    self.ctl = Some(c);
                }
            }
        }
        let mut rep = self.safe_pass(false);
        rep.speakers_held = speakers_held.unwrap_or(self.st.speaker_hold == SpeakerHold::Held);
        emit(&mut self.rig.m, format_args!("[AVS-DEV SAFE-ONLY codec_safe={} dma_stopped={} dsp_off={} hold={} speakers_held={}]", rep.codec_safe, rep.dma_stopped, rep.dsp_off, self.st.speaker_hold.name(), rep.speakers_held));
        rep
    }

    /// Give the machine back (for a test that restarts the device over the same hardware).
    pub fn into_machine(self) -> M {
        self.rig.m
    }

    /// When the delayed ground-switch enable is due (virtual or real milliseconds), if one is armed.
    pub fn ground_switch_due(&self) -> Option<u64> {
        self.gnd_deadline
    }

    /// Is the codec's interrupt left masked after a storm? For tests.
    pub fn jack_masked_for_test(&self) -> bool {
        self.jack_masked
    }

    /// Is the codec in its safe state right now (a readback, no change)? For tests and the status line.
    pub fn codec_is_safe_for_test(&mut self) -> bool {
        self.codec.as_mut().is_some_and(|c| c.is_safe(&mut self.rig))
    }

    /// The client is gone: nothing it opened survives it.
    pub fn client_died(&mut self) {
        emit(&mut self.rig.m, format_args!("[AVS-DEV CLIENT-GONE]"));
        self.stop_session(true);
        self.stream = None;
        self.subs = 0;
        // the kernel never reclaims a server's side of a session: hand the pair back, then the next connect may be accepted
        self.rig.m.release_session();
        self.session = false;
        self.publish();
    }

    // ---- the loop ----

    fn next_deadline(&self) -> Option<u64> {
        let wd = self.dsp.as_ref().map(|d| d.last_period_ms + WATCHDOG_MS);
        match (self.gnd_deadline, wd) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// One blocking wait and what it woke for.
    pub fn step(&mut self) -> Wake {
        let jack_ok = self.codec_ready && !self.jack_masked;
        // a jack event is served before anything else that is waiting: a removal must not queue behind requests
        if jack_ok && self.rig.m.take_jack_event() {
            self.on_jack_event();
            return Wake::Jack;
        }
        let deadline = self.next_deadline();
        let want = Want { avs_irq: self.dsp.is_some(), jack: jack_ok, requests: self.session, accept: !self.session };
        let wake = self.rig.wait(deadline, want);
        // ... and the same after the wait: whatever woke the task, a jack event that arrived with it goes first, so the request or the
        // interrupt that follows sees the state AFTER the removal
        let mut served = false;
        if jack_ok && self.rig.m.take_jack_event() {
            self.on_jack_event();
            served = true;
        }
        match wake {
            Wake::AvsIrq => self.on_avs_irq(),
            Wake::Jack => {
                if !served && self.rig.m.take_jack_event() {
                    self.on_jack_event();
                }
            }
            Wake::Accept => {
                if self.rig.m.accept() {
                    self.session = true;
                    emit(&mut self.rig.m, format_args!("[AVS-DEV SESSION-OPEN client connected]"));
                }
            }
            Wake::Request => self.on_request(),
            Wake::PeerDead => self.client_died(),
            Wake::JackGone => {
                // without the codec's interrupt an unplug goes unseen: the path is taken down and the device says it cannot run
                self.jack_gone = true;
                emit(&mut self.rig.m, format_args!("[AVS-DEV JACK-OWNER-GONE]"));
                self.stop_session(true);
                self.publish_after_climb();
            }
            Wake::Deadline => self.on_deadline(),
        }
        wake
    }

    fn publish_after_climb(&mut self) {
        self.ensure_ready();
    }

    fn on_jack_event(&mut self) {
        self.service_jack_pass();
        self.publish();
    }

    fn on_avs_irq(&mut self) {
        let (Some(dsp), Some(bufs), Some(stream)) = (self.dsp.as_mut(), self.bufs.as_ref(), self.stream.as_mut()) else {
            self.rig.m.rearm_avs_irq();
            return;
        };
        match dsp.service(&mut self.rig, bufs, &mut stream.fifo) {
            Ok(true) => {
                self.stats.periods += 1;
                self.send_period_event();
            }
            Ok(false) => {}
            Err(e) => {
                let detail = match e {
                    crate::pump::PumpError::DspException(x) => x.core_id,
                    crate::pump::PumpError::StreamError { sts } => sts as u32,
                    _ => 0,
                };
                self.fault(e.name(), detail);
            }
        }
    }

    fn on_deadline(&mut self) {
        let now = self.rig.now();
        if self.gnd_deadline.is_some_and(|d| now >= d) {
            self.gnd_deadline = None;
            // armed by the contact being made, which is before the jack has been classified: it is the codec's own record that decides
            if self.codec.as_ref().is_some_and(|c| c.aad.jack_inserted) {
                if let Some(c) = self.codec.as_mut() {
                    if let Err(e) = c.ground_switch(&mut self.rig, true) {
                        self.fault(e.step(), e.detail());
                        return;
                    }
                }
            }
        }
        if self.dsp.as_ref().is_some_and(|d| now >= d.last_period_ms + WATCHDOG_MS) {
            self.fault("period-timeout", 0);
        }
    }

    /// The jack changed to `to`: move the stream to the path it now belongs on. The path in use is taken down FIRST (silence, off, proven) and
    /// only then is the other one started; a stream with nowhere to play is held, and the status says why.
    pub(crate) fn jack_changed(&mut self, to: Jack) {
        let was = self.jack;
        self.jack = to;
        emit(&mut self.rig.m, format_args!("[AVS-DEV JACK {} -> {}]", was.name(), to.name()));
        if to == Jack::Absent {
            self.gnd_deadline = None;
        }
        self.reroute();
        self.send_jack_event();
        self.publish();
    }

    /// Make the running path the desired one: stop what should not be running, then start what should.
    pub(crate) fn reroute(&mut self) {
        let want = self.desired_output();
        let have = self.dsp.as_ref().map(|d| d.out);
        if have.is_some() && have != want {
            // the headphones are out (mute and take the path down NOW, before anything else is serviced), or the other path is wanted
            self.stop_session(true);
        }
        if self.dsp.is_none() && want.is_some() && self.stream.as_ref().is_some_and(|s| s.want_running) && self.fault.is_none() && self.ensure_ready() {
            let _ = self.start_session();
        }
    }

    /// For the jack module.
    pub(crate) fn classify(&self, report: &da7219_core::aad::JackReport) -> Jack {
        jack_after(report, self.jack)
    }
}
