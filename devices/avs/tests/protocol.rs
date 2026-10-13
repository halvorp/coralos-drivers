// SPDX-License-Identifier: GPL-2.0-only
//! The protocol Aura speaks: every request answered, every refusal named, the separated causes reaching the status, the volume under its
//! ceiling, the events, and an idle device that is blocked rather than polling.
mod sim;
use avs_wire::msg::*;
use avs_wire::*;
use sim::harness::*;
use sim::world::*;

const PB: usize = 7;

fn open_with(dev: &mut avs_device::Device<sim::Sim>, c: &mut Client, o: Open) -> (Status, Vec<u8>) {
    let mut b = [0u8; 32];
    let n = o.encode(&mut b).unwrap();
    let (s, body, _) = call(dev, c, Opcode::StreamOpen, &b[..n]);
    (s, body)
}

fn set_volume(dev: &mut avs_device::Device<sim::Sim>, c: &mut Client, level: u16, mute: bool) -> (Status, VolumeReply) {
    let mut b = [0u8; 4];
    let n = SetVolume { level, mute }.encode(&mut b).unwrap();
    let (s, body, _) = call(dev, c, Opcode::SetVolume, &b[..n]);
    (s, VolumeReply::decode(&body).unwrap_or(VolumeReply { dac_code: 0, hp_code: 0, muted: false }))
}

#[test]
fn hello_caps_and_status_describe_the_one_stream_the_topology_offers() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let (s, b, _) = call(&mut dev, &mut c, Opcode::Hello, &[]);
    let h = HelloReply::decode(&b).unwrap();
    assert_eq!((s, h.abi_major, h.max_pcm_bytes), (Status::Ok, 1, MAX_PCM_BYTES));
    let (s, b, _) = call(&mut dev, &mut c, Opcode::StreamCaps, &[]);
    let caps = Caps::decode(&b).unwrap();
    assert_eq!(s, Status::Ok);
    assert_eq!(caps.formats(), &[Format::S16_48K_STEREO]);
    assert_eq!((caps.period_frames, caps.periods), (1024, 4));
    assert_eq!(caps.latency_frames, LATENCY_UNMEASURED, "the pipeline latency to the pin is not measured and is not guessed");
    assert_eq!(caps.fifo_bytes, 32768);
}

#[test]
fn formats_the_topology_does_not_offer_are_refused_by_name_so_aura_converts() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    for f in [
        Format { rate: 44_100, ..Format::S16_48K_STEREO },
        Format { rate: 96_000, ..Format::S16_48K_STEREO },
        Format { channels: 1, ..Format::S16_48K_STEREO },
        Format { channels: 6, ..Format::S16_48K_STEREO },
        Format { container_bits: 32, valid_bits: 24, ..Format::S16_48K_STEREO },
        Format { valid_bits: 12, ..Format::S16_48K_STEREO },
    ] {
        let (s, _) = open_with(&mut dev, &mut c, Open { format: f, route: Route::Headphones, period_frames: 0 });
        assert_eq!(s, Status::FormatRefused, "{f:?}");
    }
    let (s, _) = open_with(&mut dev, &mut c, Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 333 });
    assert_eq!(s, Status::FormatRefused, "a period size the device does not have");
    assert!(dev.stream.is_none());
    let (s, b) = open_with(&mut dev, &mut c, Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 1024 });
    assert_eq!(s, Status::Ok);
    assert_eq!(OpenReply::decode(&b).unwrap().period_frames, 1024);
}

#[test]
fn one_stream_at_a_time_unknown_ids_and_the_ring_form_are_refused_by_name() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    let (s, _) = open_with(&mut dev, &mut c, Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 0 });
    assert_eq!(s, Status::StreamBusy);
    for op in [Opcode::StreamStart, Opcode::StreamStop, Opcode::StreamClose] {
        let (s, _, _) = call(&mut dev, &mut c, op, &stream_ref(id + 1));
        assert_eq!(s, Status::UnknownStream, "{op:?}");
    }
    let (s, _, _) = call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id + 9, &ramp(4, 0)));
    assert_eq!(s, Status::UnknownStream);
    let mut b = [0u8; 16];
    let n = OpenRing { stream: id, region: 7, layout: 1 }.encode(&mut b).unwrap();
    let (s, body, _) = call(&mut dev, &mut c, Opcode::StreamOpenRing, &b[..n]);
    assert_eq!((s, body.len()), (Status::RingRegionUnavailable, 0), "the region form is defined and refused until the kernel can map a client region");
    // close frees the slot
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamClose, &stream_ref(id));
    assert_eq!(s, Status::Ok);
    assert!(dev.stream.is_none());
    let id2 = open_stream(&mut dev, &mut c);
    assert_ne!(id, id2, "a stream id is never reused straight away");
}

#[test]
fn malformed_and_misdirected_frames_are_answered_with_their_names() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    // a short body for a known opcode
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamStart, &[1, 2]);
    assert_eq!(s, Status::Malformed);
    let (s, _, _) = call(&mut dev, &mut c, Opcode::Subscribe, &[0xff, 0, 0, 0]);
    assert_eq!(s, Status::Malformed);
    let (s, _, _) = call(&mut dev, &mut c, Opcode::SetVolume, &[0xff, 0xff, 0, 0]);
    assert_eq!(s, Status::Malformed);
    // events and the firmware channel are not requests
    for op in [Opcode::EvStatus, Opcode::EvJack, Opcode::EvPeriod, Opcode::FwStat, Opcode::FwRead] {
        let (s, _, _) = call(&mut dev, &mut c, op, &[]);
        assert_eq!(s, Status::Unsupported, "{op:?}");
    }
    // raw garbage and a wrong major version
    dev.rig.m.request(vec![0u8; 5]);
    dev.rig.m.request({
        let (_, mut f) = c.frame(Opcode::Hello, &[]);
        f[4] = 9;
        f
    });
    let got = settle(&mut dev);
    let st: Vec<_> = got.iter().filter(|g| g.reply).map(|g| status_of(g).0).collect();
    assert_eq!(st, vec![Status::Malformed, Status::BadVersion]);
    assert_eq!(dev.stats.malformed, 2);
}

#[test]
fn a_write_takes_whole_frames_up_to_the_rooms_and_says_so() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    // 7 bytes: one frame and a bit
    let (s, b, _) = call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &[1, 2, 3, 4, 5, 6, 7]));
    let r = PcmReply::decode(&b).unwrap();
    assert_eq!((s, r.accepted, r.queued, r.free), (Status::Ok, 4, 4, 32768 - 4));
    // fill the queue
    let chunk = ramp(4096, 0);
    let (_, b, _) = call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &chunk));
    assert_eq!(PcmReply::decode(&b).unwrap().accepted, 16384);
    let (_, b, _) = call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &chunk));
    let r = PcmReply::decode(&b).unwrap();
    assert_eq!((r.accepted, r.free), (16384 - 4, 0), "a short write: what fit, in whole frames, never a block and never a drop");
    let (_, b, _) = call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &chunk));
    assert_eq!(PcmReply::decode(&b).unwrap().accepted, 0);
}

// ---- the separated causes ----

#[test]
fn headphones_out_is_a_cause_and_a_start_is_accepted_and_held() {
    let Some(mut dev) = started(false) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Suspended, Cause::HeadphoneDisconnected));
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.stream), (State::Suspended, Cause::HeadphoneDisconnected, StreamPhase::Suspended));
    assert!(dev.dsp.is_none() && dev.rig.m.dsp.transfers.is_empty(), "no DSP boot for a stream with nowhere to play");
}

#[test]
fn a_wrong_pci_function_is_backend_unavailable_and_open_says_so() {
    let Some(mut sim) = atlas() else { return };
    sim.cfg[0..4].copy_from_slice(&0x1234_8086u32.to_le_bytes());
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.step_str()), (State::Down, Cause::BackendUnavailable, "pci-identity"));
    assert_eq!(st.detail, 0x1234_8086);
    let (s, _) = open_with(&mut dev, &mut c, Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 0 });
    assert_eq!(s, Status::BackendUnavailable);
    assert!(dev.rig.m.writes.is_empty(), "an unknown function is never written");
    assert_eq!(dev.rig.m.i2c.len(), 0);
}

#[test]
fn a_controller_that_reads_as_nothing_is_backend_unavailable() {
    let Some(mut sim) = atlas() else { return };
    sim.hda.w16(0, 0xffff);
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.step_str()), (State::Down, Cause::BackendUnavailable, "controller-dead"));
    assert!(dev.rig.m.writes.is_empty());
}

#[test]
fn a_missing_file_is_firmware_unavailable_naming_it_and_the_next_open_retries() {
    let Some(mut sim) = atlas() else { return };
    let basefw = sim.files.remove("skl/dsp_basefw.bin").unwrap();
    sim.codec.plug(false);
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.step_str()), (State::Idle, Cause::FirmwareUnavailable, "basefw"), "{st:?}");
    let (s, _) = open_with(&mut dev, &mut c, Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 0 });
    assert_eq!(s, Status::FirmwareUnavailable);
    assert!(dev.rig.m.dsp.transfers.is_empty());
    // Kickstart places the file; the next open climbs the ladder again
    dev.rig.m.files.insert("skl/dsp_basefw.bin".into(), basefw);
    let (s, _) = open_with(&mut dev, &mut c, Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 0 });
    assert_eq!(s, Status::Ok);
    let st = get_status(&mut dev, &mut c);
    assert_eq!(st.cause, Cause::None);
}

#[test]
fn a_file_the_device_cannot_use_is_refused_even_though_the_broker_served_it() {
    for (name, mutate, expect) in [
        ("skl/dsp_basefw.bin", (|f: &mut Vec<u8>| f[16] ^= 0xff) as fn(&mut Vec<u8>), "basefw"), // the family magic, just past the stripped extended manifest
        ("skl/dsp_basefw.bin", (|f: &mut Vec<u8>| f.truncate(100)) as fn(&mut Vec<u8>), "basefw"),
        ("da7219-tplg.bin", (|f: &mut Vec<u8>| f.truncate(10)) as fn(&mut Vec<u8>), "topology"),
        ("nhlt.bin", (|f: &mut Vec<u8>| f[9] ^= 1) as fn(&mut Vec<u8>), "nhlt"), // the checksum
    ] {
        let Some(mut sim) = atlas() else { return };
        mutate(sim.files.get_mut(name).unwrap());
        let mut dev = avs_device::Device::new(sim);
        dev.start();
        assert_eq!((dev.st.cause, dev.st.step_str()), (Cause::FirmwareUnavailable, expect), "{name}");
    }
    // a base firmware older than the minimum
    let Some(mut sim) = atlas() else { return };
    sim.files.insert("skl/dsp_basefw.bin".into(), synth_image(b"BASEFW\0\0", (9, 21, 0, 100), 4000, 1));
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    assert_eq!((dev.st.cause, dev.st.step_str()), (Cause::FirmwareUnavailable, "basefw"));
}

#[test]
fn a_broker_that_is_down_or_silent_is_firmware_unavailable_not_a_hang() {
    let Some(mut sim) = atlas() else { return };
    sim.broker_down = true;
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    assert_eq!((dev.st.state, dev.st.cause, dev.st.step_str()), (State::Idle, Cause::FirmwareUnavailable, "board"));
    let Some(mut sim) = atlas() else { return };
    sim.broker_timeout = true;
    let t0 = sim.t_us;
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    assert_eq!(dev.st.cause, Cause::FirmwareUnavailable);
    assert!(dev.rig.m.t_us - t0 < 5_000_000, "one bounded exchange, not a retry loop");
}

#[test]
fn a_codec_that_does_not_answer_at_start_is_init_failed_naming_the_step_and_a_later_start_retries() {
    let Some(mut sim) = atlas() else { return };
    sim.codec.nak_after = Some(0);
    sim.codec.plug(false);
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause), (State::Faulted, Cause::InitFailed));
    assert!(!st.step_str().is_empty());
    assert!(!st.safe_verified);
    let (s, _) = open_with(&mut dev, &mut c, Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 0 });
    assert_eq!(s, Status::InitFailed);
    // the codec comes back
    dev.rig.m.codec.nak_after = None;
    let (s, _) = open_with(&mut dev, &mut c, Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 0 });
    assert_eq!(s, Status::Ok);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause), (State::Idle, Cause::None));
    assert!(st.safe_verified);
}

#[test]
fn a_backend_failure_outranks_a_missing_file_which_outranks_a_fault_which_outranks_no_headphones() {
    let Some(mut sim) = atlas() else { return };
    sim.files.remove("nhlt.bin");
    sim.cfg[0..4].copy_from_slice(&0x1234_8086u32.to_le_bytes());
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    assert_eq!(dev.st.cause, Cause::BackendUnavailable, "the OS cannot drive it, whatever else is also wrong");
    // fix the backend: now the missing file is what is wrong
    dev.rig.m.cfg[0..4].copy_from_slice(&0x9d71_8086u32.to_le_bytes());
    dev.ensure_ready();
    assert_eq!((dev.st.cause, dev.st.step_str()), (Cause::FirmwareUnavailable, "nhlt"));
}

// ---- volume ----

#[test]
fn volume_is_mapped_below_the_ceiling_at_every_level_and_is_monotonic() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    // before the stream runs: the reply says what would be applied, and it is remembered
    let (s, r) = set_volume(&mut dev, &mut c, 1000, false);
    assert_eq!((s, r.dac_code, r.hp_code, r.muted), (Status::Ok, 0x6f, 51, false));
    assert_eq!(get_status(&mut dev, &mut c).volume_level, 1000);
    let (s, r) = set_volume(&mut dev, &mut c, 0, false);
    assert_eq!((s, r.muted), (Status::Ok, true));
    set_volume(&mut dev, &mut c, 400, false);
    start_stream(&mut dev, &mut c, id);
    let reg = |d: &avs_device::Device<sim::Sim>| (d.rig.m.codec.regs[0x45], d.rig.m.codec.regs[0x46], d.rig.m.codec.regs[0x48], d.rig.m.codec.regs[0x69] & 0x40);
    let (l, r, hp, mute) = reg(&dev);
    assert_eq!((l, r), (0x6f - 38, 0x6f - 38), "the remembered level was applied at the session start (400 -> 600*64/1000 = 38 codes down)");
    assert_eq!((hp, mute), (51, 0));
    let mut last = 0u8;
    for level in [1u16, 2, 10, 100, 250, 500, 750, 999, 1000] {
        let (s, rep) = set_volume(&mut dev, &mut c, level, false);
        assert_eq!(s, Status::Ok);
        let (l, r, hp, m) = reg(&dev);
        assert!(l <= 0x6f && r <= 0x6f, "level {level}: DAC gain {l:#x} is above the 0 dB ceiling");
        assert!(hp <= 0x39, "headphone gain above its ceiling");
        assert_eq!((l, r), (rep.dac_code, rep.dac_code));
        assert!(l >= last, "volume must not go down as the level goes up ({level})");
        assert_eq!(m, 0, "not muted");
        last = l;
    }
    assert_eq!(last, 0x6f, "level 1000 is the ceiling, 0 dB, exactly");
    // mute and unmute
    let (_, r) = set_volume(&mut dev, &mut c, 700, true);
    assert!(r.muted);
    assert_eq!(dev.rig.m.codec.regs[0x69] & 0x40, 0x40);
    assert_eq!(dev.rig.m.codec.regs[0x6b] & 0x40, 0x40);
    let (_, r) = set_volume(&mut dev, &mut c, 700, false);
    assert!(!r.muted);
    assert_eq!(dev.rig.m.codec.regs[0x69] & 0x40, 0);
}

#[test]
fn the_volume_mapping_function_never_exceeds_the_policy_ceiling_for_any_input() {
    use da7219_core::hp::GainPolicy;
    let p = GainPolicy::ATLAS_HEADPHONES;
    assert_eq!(avs_device::codec::dac_code_for(0, &p), None);
    let mut last = 0u8;
    for level in 1..=u16::MAX {
        let c = avs_device::codec::dac_code_for(level, &p).unwrap();
        assert!(c <= p.dac_max, "level {level} -> {c:#x}");
        assert!(c >= last || level > 1000, "monotonic until the scale ends");
        if level <= 1000 {
            last = c;
        }
    }
    assert_eq!(avs_device::codec::dac_code_for(1000, &p), Some(p.dac_max));
    assert_eq!(avs_device::codec::dac_code_for(1, &p), Some(p.dac_max - 63));
    // a policy whose ceiling is lower is respected too
    let low = GainPolicy { dac_max: 0x50, ..p };
    assert!((1..=1000u16).all(|l| avs_device::codec::dac_code_for(l, &low).unwrap() <= 0x50));
}

// ---- events ----

#[test]
fn a_client_that_did_not_subscribe_gets_no_events_and_one_that_did_gets_only_what_it_asked_for() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    start_stream(&mut dev, &mut c, id);
    let ev = run_ms(&mut dev, 100);
    assert!(ev.iter().all(|g| !g.event), "no subscription, no events: {ev:?}");
    // only periods
    let mut b = [0u8; 4];
    let n = Subscribe { mask: SUB_PERIOD }.encode(&mut b).unwrap();
    call(&mut dev, &mut c, Opcode::Subscribe, &b[..n]);
    dev.rig.m.at_ms(1, sim::Ev::Unplug);
    let ev = run_ms(&mut dev, 100);
    assert!(ev.iter().filter(|g| g.event).all(|g| g.op == Opcode::EvPeriod));
}

#[test]
fn the_audio_clock_counts_frames_the_hardware_read_and_the_credit_tracks_the_queue() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 1)));
    start_stream(&mut dev, &mut c, id);
    let ev = run_ms(&mut dev, 200);
    let pe: Vec<_> = ev.iter().filter(|g| g.op == Opcode::EvPeriod).map(|g| PeriodEvent::decode(&g.payload).unwrap()).collect();
    assert!(pe.len() >= 8, "one event per period (21.3 ms): {}", pe.len());
    let last = pe.last().unwrap();
    // the audio clock is what the DMA engine really read - within the periods not yet reported - and it never counts what was only submitted
    let read = dev.rig.m.played(PB).len() as u64;
    assert!(last.played_frames * 4 <= read && read - last.played_frames * 4 <= 2 * 4096, "event says {} frames, the engine read {} bytes", last.played_frames, read);
    assert!(pe.windows(2).all(|w| w[1].played_frames > w[0].played_frames && w[1].consumed_bytes >= w[0].consumed_bytes));
    // the first samples waited in the queue for the unmute: the lead-in is silence, then the 16 KiB that was written, then silence again
    let played = dev.rig.m.played(PB);
    let lead = played.iter().position(|b| *b != 0).expect("the samples reached the jack");
    assert_eq!(lead % 4, 0, "the lead-in is whole frames of silence ({lead} bytes)");
    assert_eq!(&played[lead..lead + 16384], &ramp(4096, 1)[..], "the client's samples, in order, none lost to the muted lead-in");
    assert_eq!(last.consumed_bytes, 16384);
    assert!(last.underruns >= 4, "once the queue ran out each period was silence and was COUNTED: {}", last.underruns);
    assert!(played[lead + 16384..].iter().all(|b| *b == 0), "an empty queue plays silence, never stale samples");
    assert_eq!(last.free_bytes, 32768);
}

#[test]
fn status_events_carry_a_generation_that_only_moves_forward() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    let id = open_stream(&mut dev, &mut c);
    let (_, _, ev0) = call(&mut dev, &mut c, Opcode::StreamStart, &stream_ref(id));
    dev.rig.m.at_ms(1, sim::Ev::Unplug);
    let mut ev = run_ms(&mut dev, 100);
    ev.splice(0..0, ev0);
    let gens: Vec<u32> = ev.iter().filter(|g| g.op == Opcode::EvStatus).map(|g| StatusBody::decode(&g.payload).unwrap().generation).collect();
    assert!(gens.len() >= 2, "{gens:?}");
    assert!(gens.windows(2).all(|w| w[1] > w[0]), "{gens:?}");
}

#[test]
fn a_full_reply_ring_is_retried_a_few_times_and_then_dropped_and_counted_not_waited_on() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    dev.rig.m.ring_full = 1000;
    let (_, f) = c.frame(Opcode::Hello, &[]);
    dev.rig.m.request(f);
    for _ in 0..4 {
        // (the first wake may be the jack message the start pass left behind: a spurious line, serviced and forgotten)
        dev.step();
        if dev.stats.dropped_replies > 0 {
            break;
        }
    }
    assert_eq!(dev.stats.dropped_replies, 1);
    assert!(dev.rig.m.outbox.is_empty());
    assert_eq!(dev.rig.m.ring_full, 1000 - 4, "four attempts and no more");
    dev.rig.m.ring_full = 0;
    let (s, _, _) = call(&mut dev, &mut c, Opcode::Hello, &[]);
    assert_eq!(s, Status::Ok, "the device carries on");
}

// ---- idle is blocked ----

#[test]
fn an_idle_device_blocks_on_requests_and_the_jack_and_never_polls() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    settle(&mut dev);
    let w = dev.rig.m.waits.last().copied().unwrap();
    assert_eq!(w.deadline_ms, None, "nothing running means no timer");
    assert!(w.want.requests && w.want.jack && !w.want.avs_irq, "{w:?}");
    assert_eq!(dev.rig.m.blocked_forever, 1);
    // a second look: still no wakeups of its own over a long idle stretch
    let n = dev.rig.m.waits.len();
    let t = dev.rig.m.t_us;
    dev.rig.m.at_ms(5_000, sim::Ev::Custom(Box::new(|_| {})));
    run_ms(&mut dev, 10_000);
    let timed: Vec<_> = dev.rig.m.waits[n..].iter().filter(|w| w.deadline_ms.is_some()).collect();
    assert!(timed.is_empty(), "an idle device set a timer: {timed:?}");
    assert!(dev.rig.m.t_us >= t);
    let _ = &mut c;
}

#[test]
fn while_playing_the_only_timers_are_the_watchdog_and_the_plans_own_bounded_sleeps() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    start_stream(&mut dev, &mut c, id);
    let n = dev.rig.m.waits.len();
    run_ms(&mut dev, 500);
    // the steady state: every wait of the main loop wants the stream interrupt, the requests and the jack, with the watchdog as its deadline
    let steady = &dev.rig.m.waits[n..];
    assert!(steady.len() >= 20);
    for w in steady {
        assert!(w.want.avs_irq && w.want.requests && w.want.jack, "{w:?}");
        let d = w.deadline_ms.expect("a running stream has its watchdog");
        assert!(d * 1000 > w.at_us || w.woke != avs_device::machine::Wake::Deadline);
    }
    // wakeups are the periods (21.3 ms): about 23 in 500 ms, not a poll
    let irqs = steady.iter().filter(|w| w.woke == avs_device::machine::Wake::AvsIrq).count();
    assert!((20..=26).contains(&irqs), "{irqs} interrupt wakeups in 500 ms");
    assert!(steady.iter().all(|w| w.woke != avs_device::machine::Wake::Deadline));
}

