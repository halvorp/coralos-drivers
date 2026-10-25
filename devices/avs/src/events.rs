// SPDX-License-Identifier: GPL-2.0-only
//! What the device TELLS: the STATUS recomputed from its state (the separated causes, in their priority), and the three events sent to a
//! subscriber (status, jack, period). One `impl Device` block; the state is [`crate::device`]'s.
//!
//! The cause priority is: the backend cannot be driven (windows, identity, controller, the jack owner), then a file is missing or refused,
//! then a step failed, then nowhere to play (no headphones, and the speakers not offered or not asked for). The speakers say whether they are
//! offered in their own field; they are the primary cause only for a stream that asked for them and cannot have them.
use crate::device::Device;
use crate::machine::Machine;
use avs_wire::msg::{self, SUB_JACK, SUB_PERIOD, SUB_STATUS};
use avs_wire::{Cause, Jack, Opcode, Output, Route, Speaker, State, StreamPhase};

impl<M: Machine> Device<M> {
    /// Recompute the STATUS from the state. True when it changed (the generation moved).
    pub(crate) fn recompute(&mut self) -> bool {
        let mut n = self.st;
        n.jack = self.jack;
        n.stream = match &self.stream {
            None => StreamPhase::None,
            Some(_) if self.dsp.is_some() => StreamPhase::Running,
            Some(s) if s.want_running => StreamPhase::Suspended,
            Some(_) => StreamPhase::Open,
        };
        // where the open stream (or, with none, a stream that could be opened) may play
        let route = self.stream.as_ref().map_or(Route::Auto, |s| s.route);
        let plug = self.jack != Jack::Absent;
        let spk = self.speaker_ready();
        let can_play = match route {
            Route::Headphones => plug,
            Route::Speakers => spk,
            Route::Auto => plug || spk,
        };
        let (cause, why) = if let Some(w) = self.backend_fail {
            (Cause::BackendUnavailable, Some(w))
        } else if let Some(w) = self.fw_fail {
            (Cause::FirmwareUnavailable, Some(w))
        } else if let Some(w) = self.fault {
            (Cause::InitFailed, Some(w))
        } else if !can_play && route == Route::Speakers {
            match self.speaker_fw_fail {
                Some(w) => (Cause::FirmwareUnavailable, Some(w)),
                None => (Cause::SpeakerProtectionUnavailable, None),
            }
        } else if !can_play {
            (Cause::HeadphoneDisconnected, None)
        } else {
            (Cause::None, None)
        };
        n.speaker = if spk { Speaker::Protected } else { Speaker::Refused };
        n.output = self.dsp.as_ref().map_or(Output::None, |d| d.out);
        n.cause = cause;
        n.detail = why.map_or(0, |w| w.detail);
        match why {
            Some(w) => n.set_step(w.step),
            None => n.set_step(""),
        }
        n.state = if !self.started {
            State::Starting
        } else if self.backend_fail.is_some() {
            State::Down
        } else if self.fault.is_some() && self.fw_fail.is_none() {
            State::Faulted
        } else if self.dsp.is_some() {
            State::Running
        } else if self.stream.as_ref().is_some_and(|s| s.want_running) {
            State::Suspended
        } else {
            State::Idle
        };
        n.volume_level = self.level.unwrap_or(0);
        if n != self.st {
            n.generation = self.st.generation.wrapping_add(1);
            self.st = n;
            true
        } else {
            false
        }
    }

    /// Recompute and, if it changed, tell a subscriber.
    pub fn publish(&mut self) {
        if self.recompute() && self.subs & SUB_STATUS != 0 {
            let mut body = [0u8; avs_wire::STATUS_BODY_BYTES];
            if self.st.encode(&mut body).is_ok() {
                self.send_event(Opcode::EvStatus, &body);
            }
        }
    }

    pub(crate) fn send_event(&mut self, op: Opcode, body: &[u8]) {
        let mut out = [0u8; 160];
        let Ok(n) = msg::event(op, body, &mut out) else { return };
        if !self.send_frame(&out[..n]) {
            self.stats.dropped_events += 1;
        }
    }

    /// Send a frame; a full ring is retried a few times at a millisecond each, then the frame is dropped (counted). False when dropped.
    pub(crate) fn send_frame(&mut self, frame: &[u8]) -> bool {
        use crate::machine::SendOutcome;
        for _ in 0..4 {
            match self.rig.m.send_to_client(frame) {
                SendOutcome::Sent => return true,
                SendOutcome::RingFull => self.rig.sleep_ms(1),
                SendOutcome::PeerDead | SendOutcome::Denied => return false,
            }
        }
        false
    }

    pub(crate) fn send_period_event(&mut self) {
        if self.subs & SUB_PERIOD == 0 {
            return;
        }
        let (Some(s), Some(d)) = (self.stream.as_ref(), self.dsp.as_ref()) else { return };
        let ev = msg::PeriodEvent {
            stream: s.id,
            consumed_bytes: s.consumed_base + d.acct.pcm_fed,
            played_frames: s.played_base + d.acct.played_frames(),
            underruns: s.underruns_base.saturating_add(d.acct.underruns),
            free_bytes: s.fifo.free() as u32,
        };
        let mut body = [0u8; 40];
        if let Ok(n) = ev.encode(&mut body) {
            self.send_event(Opcode::EvPeriod, &body[..n]);
        }
    }

    pub(crate) fn send_jack_event(&mut self) {
        if self.subs & SUB_JACK == 0 {
            return;
        }
        let mut body = [0u8; 8];
        if let Ok(n) = (msg::JackEvent { jack: self.jack, buttons: self.buttons }).encode(&mut body) {
            self.send_event(Opcode::EvJack, &body[..n]);
        }
    }
}
