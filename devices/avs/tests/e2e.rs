// SPDX-License-Identifier: GPL-2.0-only
//! End to end against the simulated Atlas: boot, headphone stream, jack, faults, client death.
mod sim;
use sim::harness::*;

/// The playback stream's descriptor index on the simulated controller: seven input streams, then the first output stream.
const PB: usize = 7;

#[test]
fn the_device_starts_idle_with_the_headphones_out_and_says_why_nothing_plays() {
    let Some(mut dev) = started(false) else { return };
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    use avs_wire::*;
    assert_eq!((st.state, st.cause, st.jack), (State::Idle, Cause::HeadphoneDisconnected, Jack::Absent));
    assert!(st.safe_verified);
    assert_eq!(st.speaker, Speaker::Refused);
    assert_eq!(st.speaker_hold, SpeakerHold::Held);
}

use avs_wire::msg::*;
use avs_wire::*;

#[test]
fn a_headphone_stream_boots_the_dsp_plays_the_samples_it_was_given_and_stops_in_the_verified_safe_state() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.jack), (State::Idle, Cause::None, Jack::Headphones));
    let id = open_stream(&mut dev, &mut c);
    // 8192 frames: the ring (4096 frames) is primed with the first half, the queue holds the rest
    let pcm = ramp(8192, 1);
    for chunk in pcm.chunks(16384) {
        let (s, b, _) = call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, chunk));
        assert_eq!(s, Status::Ok);
        assert_eq!(PcmReply::decode(&b).unwrap().accepted, chunk.len() as u32);
    }
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Running, Cause::None));
    // the DSP was booted: the base firmware was delivered whole and in order through the code loader
    assert_eq!(dev.rig.m.dsp.transfers.len(), 1);
    assert_eq!(dev.rig.m.dsp.transfers[0].len(), BASEFW_BODY);
    // the codec is up: not in the safe state any more
    assert!(!dev.st.safe_verified);
    let got = run_ms(&mut dev, 200);
    let played = dev.rig.m.played(PB).to_vec();
    assert!(played.len() >= 16384 + 8192, "the hardware read {} bytes", played.len());
    // the codec came up while the ring held silence; then the client's samples, sample for sample and in order, with none lost
    let lead = played.iter().position(|b| *b != 0).unwrap();
    eprintln!("lead {lead} bytes around: {:?}; pcm head {:?}", &played[lead.saturating_sub(4)..lead + 12], &pcm[..12]);
    assert_eq!(lead % 4, 0, "frame aligned");
    assert!(lead >= 4096, "the unmute takes longer than a period, so there is a silent lead-in ({lead} bytes)");
    assert_eq!(&played[lead..lead + 16384], &pcm[..16384], "the first ring is the first half of what was written, sample for sample");
    assert_eq!(&played[lead + 16384..lead + 16384 + 4096], &pcm[16384..16384 + 4096], "the next period came from the queue, in order");
    assert!(got.iter().any(|g| g.op == Opcode::EvPeriod && g.event), "the client was told about progress");
    // the codec is really playing: the jack's output stage driven, the amplifier and DAC unmuted, gains BELOW the hardware ceiling
    let cr = &dev.rig.m.codec.regs;
    assert_eq!(cr[0x6b] & 0x48, 0x08, "headphone amplifier: output enabled, not muted");
    assert_eq!(cr[0x69] & 0x40, 0, "DAC not muted");
    assert!(cr[0x45] <= 0x6f && cr[0x46] <= 0x6f, "DAC gain {:#x}/{:#x} is above the 0 dB ceiling", cr[0x45], cr[0x46]);
    assert!(cr[0x48] <= 0x39 && cr[0x49] <= 0x39, "headphone gain above the ceiling");
    // nothing but the headphone codec was ever addressed
    assert_eq!(dev.rig.m.amp_touches(), 0);
    assert!(dev.rig.m.i2c.iter().all(|t| t.addr == 0x1a));
    assert!(dev.rig.m.refused_writes.is_empty(), "the window refused a write the device's own list allowed: {:?}", dev.rig.m.refused_writes);
    assert_eq!(dev.rig.refused.reg + dev.rig.refused.i2c + dev.rig.refused.pad, 0);
    // stop
    let (s, b, _) = call(&mut dev, &mut c, Opcode::StreamStop, &stream_ref(id));
    assert_eq!(s, Status::Ok);
    assert_eq!(PhaseReply::decode(&b).unwrap().phase, StreamPhase::Open);
    assert!(dev.st.safe_verified, "stopping ends in the proven safe state");
    assert!(dev.rig.m.codec.regs[0x6b] & 0xc8 == 0x40 && dev.rig.m.codec.regs[0x69] & 0xc0 == 0x40, "amplifier and DAC muted and off");
    assert_eq!(dev.rig.m.dsp.r32(0x04) & (1 << 16), 0, "the DSP core is powered down");
}

#[test]
#[ignore = "prints the device log; run with --ignored --nocapture"]
fn print_the_log_of_a_whole_session() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    start_stream(&mut dev, &mut c, id);
    run_ms(&mut dev, 100);
    call(&mut dev, &mut c, Opcode::StreamStop, &stream_ref(id));
    for l in &dev.rig.m.logs {
        print!("{l}");
    }
    println!("virtual time {} ms; {} waits; {} i2c transfers; {} reg writes", dev.rig.m.t_us / 1000, dev.rig.m.waits.len(), dev.rig.m.i2c.len(), dev.rig.m.writes.len());
}

fn period_events(frames: &[Got]) -> Vec<PeriodEvent> {
    frames.iter().filter(|g| g.op == Opcode::EvPeriod).map(|g| PeriodEvent::decode(&g.payload).unwrap()).collect()
}

fn jack_events(frames: &[Got]) -> Vec<JackEvent> {
    frames.iter().filter(|g| g.op == Opcode::EvJack).map(|g| JackEvent::decode(&g.payload).unwrap()).collect()
}

fn status_events(frames: &[Got]) -> Vec<StatusBody> {
    frames.iter().filter(|g| g.op == Opcode::EvStatus).map(|g| StatusBody::decode(&g.payload).unwrap()).collect()
}

#[test]
fn unplugging_suspends_with_the_cause_and_plugging_back_resumes_the_same_stream() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    let id = open_stream(&mut dev, &mut c);
    let all = ramp(20_000, 0);
    for chunk in all[..32768].chunks(16384) {
        call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, chunk));
    }
    let (_, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!(ph.phase, StreamPhase::Running);
    let before = run_ms(&mut dev, 60);
    assert!(!period_events(&before).is_empty());
    // the headphones come out
    dev.rig.m.at_ms(1, sim::Ev::Unplug);
    let ev = run_ms(&mut dev, 50);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.jack, st.stream), (State::Suspended, Cause::HeadphoneDisconnected, Jack::Absent, StreamPhase::Suspended));
    assert!(st.safe_verified, "the codec was proven safe when the path came down");
    assert_eq!(jack_events(&ev).last().map(|j| j.jack), Some(Jack::Absent));
    assert!(status_events(&ev).iter().any(|s| s.cause == Cause::HeadphoneDisconnected && s.state == State::Suspended));
    assert_eq!(dev.rig.m.dsp.r32(0x04) & (1 << 16), 0, "DSP core off while suspended");
    assert!(dev.dsp.is_none());
    let hw_before = dev.rig.m.played(PB).len();
    // the stream is still open: writes are accepted (and queue), nothing plays
    let (s, b, _) = call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(512, 30_000)));
    assert_eq!(s, Status::Ok);
    assert_eq!(PcmReply::decode(&b).unwrap().accepted, 2048);
    run_ms(&mut dev, 100);
    assert_eq!(dev.rig.m.played(PB).len(), hw_before, "nothing reached the jack while it was unplugged");
    // plugged back in: the same stream runs again
    dev.rig.m.at_ms(1, sim::Ev::Plug { four_pole: true });
    let ev = run_ms(&mut dev, 400);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.jack, st.stream), (State::Running, Cause::None, Jack::Headset, StreamPhase::Running));
    assert!(jack_events(&ev).iter().any(|j| j.jack == Jack::Headset));
    assert_eq!(dev.rig.m.dsp.transfers.len(), 2, "the DSP was booted again for the second session");
    assert!(dev.rig.m.played(PB).len() > hw_before, "audio reached the jack again");
    // the count of frames the hardware read continues across the suspend (the audio clock never goes backwards)
    let pe = period_events(&ev);
    assert!(pe.windows(2).all(|w| w[1].played_frames >= w[0].played_frames));
    assert!(pe.iter().all(|p| p.stream == id));
    assert_eq!(dev.rig.m.amp_touches(), 0);
}
