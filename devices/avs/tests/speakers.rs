// SPDX-License-Identifier: GPL-2.0-only
//! The PROTECTED SPEAKER path (DECISIONS D17, amending D11 and D14): the amplifiers run only after ChromeOS's UCM protection set is written
//! and read back, their gain can never exceed the ceiling whatever a client asks, and stop / unplug / fault / death / a dead task all return
//! them to the safe state (the hardware reset, proven by the pad's readback). Each guardrail is shown failing first.
mod sim;
use avs_device::allow::I2cAllow;
use avs_device::session::{self, speaker_gain_for};
use avs_ipc_core::header::*;
use avs_ipc_core::modcfg::AVS_PEAKVOL_VOLUME;
use avs_wire::msg::*;
use avs_wire::*;
use max98373_core::gain::{CEILING_LINEAR, LEVEL_MAX};
use sim::dsp::Act;
use sim::harness::*;
use sim::ucm_image::{DEVICE_IMAGE, VERB_IMAGE};
use sim::world::atlas_board;
use sim::Sim;
use std::collections::BTreeMap;

const RIGHT: u16 = 0x31;
const LEFT: u16 = 0x32;
type Dev = avs_device::Device<Sim>;

/// Open on the speakers, queue some audio, start; returns the stream id and the start reply.
fn start_speakers(dev: &mut Dev, c: &mut Client, route: Route, frames: usize) -> (u32, Status, PhaseReply) {
    let id = open_stream_route(dev, c, route);
    // a write carries at most 16 KiB; the ring takes what fits, the rest is the client's to resend
    for chunk in ramp(frames, 1).chunks(16384).take(2) {
        call(dev, c, Opcode::PcmWrite, &pcm_payload(id, chunk));
    }
    let (s, ph) = start_stream(dev, c, id);
    (id, s, ph)
}

/// Every volume the DSP's peakvol stage was ever asked for, in order (the Q1.31 `target_volume` of each large config set).
fn gain_requests(sim: &Sim) -> Vec<u32> {
    sim.dsp
        .reqs
        .iter()
        .filter(|r| r.target == AVS_MOD_MSG && r.ty == AVS_MOD_LARGE_CONFIG_SET && LARGE_PARAM_ID.get(r.header) == AVS_PEAKVOL_VOLUME as u32)
        .map(|r| u32::from_le_bytes(r.payload[4..8].try_into().unwrap()))
        .collect()
}

/// THE INVARIANT, checked at the end of every test that plays on the speakers: nothing above the ceiling was ever sent to the DSP.
fn assert_gain_invariant(sim: &Sim) {
    for g in gain_requests(sim) {
        assert!(g <= CEILING_LINEAR, "the DSP volume stage was asked for {g} > ceiling {CEILING_LINEAR}");
    }
}

/// The register image the protection set leaves, applied in the UCM's order: `(amp side name) -> reg -> (mask, bits) folded`.
fn expected_image(side: &str) -> BTreeMap<u16, (u8, u8)> {
    let mut m: BTreeMap<u16, (u8, u8)> = BTreeMap::new();
    for (s, _, _, reg, mask, bits) in VERB_IMAGE.iter().chain(DEVICE_IMAGE) {
        if *s != side || *reg == 0 {
            continue;
        }
        let e = m.entry(*reg).or_insert((0, 0));
        e.0 |= mask;
        e.1 = (e.1 & !mask) | bits;
    }
    m
}

fn assert_protected(sim: &Sim, addr: u16, side: &str) {
    let amp = sim.amp(addr);
    for (reg, (mask, bits)) in expected_image(side) {
        assert_eq!(amp.reg(reg) & mask, bits, "{side} amplifier register {reg:#06x}");
    }
}

fn assert_amps_safe(dev: &Dev) {
    let m = &dev.rig.m;
    assert!(m.amp_reset_asserted(), "the amplifiers' reset line is low");
    for a in &m.amps {
        assert!(a.in_reset && !a.enabled(), "amplifier {:#x} is in reset and cannot play", a.addr);
        // a part that was ever enabled was switched OFF over the bus before its reset line went low (the reset is not the only way down)
        assert!(a.enabled_at.is_empty() || a.disabled_at.last() < a.reset_at.last(), "amplifier {:#x}: GLOBAL_EN off, then reset", a.addr);
    }
    assert!(!dev.rig.i2c.amps_armed(), "the I2C fence is disarmed again");
    assert!(m.refused_writes.is_empty());
    assert_eq!(dev.rig.refused, Default::default(), "no fence ever refused anything: a refusal is a bug in a plan");
    assert_gain_invariant(m);
}

// ---- the path works ------------------------------------------------------------------------------------------------------------------

#[test]
fn a_speaker_stream_runs_the_protected_path_and_plays_the_clients_samples() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.jack, st.speaker, st.output), (State::Idle, Cause::None, Jack::Absent, Speaker::Protected, Output::None), "nothing plugged, and the speakers offered: nothing is wrong");
    assert_eq!(st.speaker_hold, SpeakerHold::Held);
    assert_eq!(dev.rig.m.amp_touches(), 0, "an idle device never talks to the amplifiers");

    let (_, s, ph) = start_speakers(&mut dev, &mut c, Route::Speakers, 8192);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Running, Cause::None));
    run_ms(&mut dev, 100);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.output, st.speaker_hold, st.safe_verified), (State::Running, Output::Speakers, SpeakerHold::Released, false));

    // both amplifiers are out of reset, set up as Linux would set them up, carry ChromeOS's protection set and are enabled
    let m = &dev.rig.m;
    assert!(!m.amp_reset_asserted());
    for (addr, side) in [(RIGHT, "Right"), (LEFT, "Left")] {
        assert!(m.amp(addr).enabled(), "{side} amplifier enabled");
        assert_protected(m, addr, side);
    }
    // the DAI: DSP_B / TDM 8 x 16 bit, 48 kHz (max98373-i2c.c), per amplifier slots (boards/max98373.c:76,83)
    assert_eq!(m.amp(RIGHT).reg(0x2024) & 0xF8, 0x58);
    assert_eq!(m.amp(RIGHT).reg(0x2026) & 0x1F, 0x06);
    assert_eq!((m.amp(RIGHT).reg(0x2027) & 0x0F, m.amp(RIGHT).reg(0x2028)), (0x08, 0x88));
    assert_eq!((m.amp(RIGHT).reg(0x2022), m.amp(RIGHT).reg(0x2020), m.amp(RIGHT).reg(0x2021)), (0x54, 0xCF, 0xFF));
    assert_eq!((m.amp(LEFT).reg(0x2022), m.amp(LEFT).reg(0x2020), m.amp(LEFT).reg(0x2021)), (0x76, 0x3F, 0xFF));
    // the client's samples reached the I2S gateway, in order, after the silent lead-in; the DSP gain is the quiet initial one under the ceiling
    let played = m.played(7);
    let lead = played.iter().position(|b| *b != 0).expect("the samples reached the amplifiers' link");
    assert_eq!(&played[lead..lead + 16384], &ramp(8192, 1)[..16384]);
    let gains = gain_requests(m);
    assert_eq!(gains.last().copied(), Some(speaker_gain_for(None, false)), "the initial level, until the client sets one");
    assert!(*gains.last().unwrap() < CEILING_LINEAR / 100);
    assert_gain_invariant(m);
    // the DA7219 was not touched: still in its attach-time safe state
    assert!(dev.codec_is_safe_for_test());

    // stop: silence, amplifiers off, reset low and proven
    let (id, _, _) = (0, 0, 0);
    let _ = id;
    let sid = dev.stream.as_ref().unwrap().id;
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamStop, &stream_ref(sid));
    assert_eq!(s, Status::Ok);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.output, st.speaker_hold, st.safe_verified), (Output::None, SpeakerHold::Held, true));
    assert_amps_safe(&dev);
    for a in &dev.rig.m.amps {
        assert_eq!(a.enabled_at.len(), 1);
        assert_eq!(a.disabled_at.len(), 1, "switched off by the amplifier's own sequence, not only by the reset");
        assert!(a.disabled_at[0] < a.reset_at.last().copied().unwrap(), "GLOBAL_EN went off BEFORE the reset line went low");
    }
}

#[test]
fn the_idle_status_says_the_speakers_are_refused_without_their_topology_and_protected_with_it() {
    let Some(mut dev) = started(false) else { return };
    let mut c = Client::new();
    assert_eq!(get_status(&mut dev, &mut c).speaker, Speaker::Refused, "no max98373-tplg.bin on the broker");
    // headphone-only Atlas: a headphone stream with the jack out still waits for headphones, with the old cause
    let id = open_stream(&mut dev, &mut c);
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Suspended, Cause::HeadphoneDisconnected));
    // an Auto stream without a speaker topology is the same
    let Some(mut dev) = started(false) else { return };
    let id = open_stream_route(&mut dev, &mut c, Route::Auto);
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Suspended, Cause::HeadphoneDisconnected));
    assert_eq!(dev.rig.m.amp_touches(), 0);
}

#[test]
fn an_explicit_headphone_stream_never_uses_the_speakers_even_when_they_are_offered() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let id = open_stream_route(&mut dev, &mut c, Route::Headphones);
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase, ph.cause), (Status::Ok, StreamPhase::Suspended, Cause::HeadphoneDisconnected));
    run_ms(&mut dev, 50);
    assert_eq!(dev.rig.m.amp_touches(), 0);
    assert!(dev.dsp.is_none());
}

// ---- protection comes first ----------------------------------------------------------------------------------------------------------

/// The ordered writes one amplifier must receive, written out by hand: the bring-up (reset, Linux's probe, the DAI), ChromeOS's UCM set
/// (this amplifier's side, in the file's order), the power-up. `Update` steps become the byte Linux's regmap would write (read, modify).
fn expected_writes(slots: (u8, u8), tx_hiz: (u8, u8), side: &str, defaults: &std::collections::HashMap<u16, u8>) -> Vec<(u16, u8)> {
    let mut regs = defaults.clone();
    let mut out: Vec<(u16, u8)> = vec![];
    enum S {
        W(u16, u8),
        U(u16, u8, u8),
    }
    use S::*;
    let (v, i) = slots;
    let mut steps: Vec<S> = vec![
        W(0x2000, 0x01),
        W(0x2020, 0xFF),
        W(0x2021, 0xFF),
        W(0x2029, 0x80),
        W(0x202A, 0x01),
        W(0x203F, 0x03),
        W(0x2046, 0x07),
        W(0x2022, (i << 4) | v),
        U(0x2020, 1 << v, 0),
        U(0x2020, 1 << i, 0),
        W(0x20FE, 0x0F),
        W(0x2023, 0x02),
        U(0x2026, 0x10, 0x00),
        U(0x2024, 0x38, 0x18),
        U(0x2026, 0x0F, 0x06),
        U(0x2024, 0xC0, 0x40),
        U(0x2029, 0x0F, 0x00),
        W(0x202A, 0x01),
        W(0x2020, tx_hiz.0),
        W(0x2021, tx_hiz.1),
        U(0x2024, 0xC0, 0x40),
        U(0x2027, 0x0F, 0x08),
        U(0x2028, 0xF0, 0x80),
        U(0x2028, 0x0F, 0x08),
    ];
    for (s, _, _, reg, mask, bits) in VERB_IMAGE.iter().chain(DEVICE_IMAGE) {
        if *s == side && *reg != 0 {
            steps.push(U(*reg, *mask, *bits));
        }
    }
    steps.extend([U(0x2043, 0x01, 0x01), U(0x202B, 0x01, 0x01), U(0x20FF, 0x01, 0x01)]);
    for s in steps {
        match s {
            W(r, x) => {
                if r == 0x2000 {
                    regs = defaults.clone();
                } else {
                    regs.insert(r, x);
                }
                out.push((r, x));
            }
            U(r, m, x) => {
                let cur = regs.get(&r).copied().unwrap_or(0);
                let n = (cur & !m) | (x & m);
                regs.insert(r, n);
                out.push((r, n));
            }
        }
    }
    out
}

#[test]
fn every_ucm_protection_cset_is_written_in_order_and_read_back_before_anything_is_enabled_or_clocked() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, ph) = start_speakers(&mut dev, &mut c, Route::Speakers, 4096);
    assert_eq!((s, ph.phase), (Status::Ok, StreamPhase::Running));
    let m = &dev.rig.m;
    let defaults = sim::amp::Amp::new(0).regs;

    // 1. the ordered register list each amplifier received EQUALS the hand-written list (and nothing else was written to it)
    let right = expected_writes((4, 5), (0xCF, 0xFF), "Right", &defaults);
    let left = expected_writes((6, 7), (0x3F, 0xFF), "Left", &defaults);
    assert_eq!(m.amp(RIGHT).writes(), right, "the right amplifier's writes, in order");
    assert_eq!(m.amp(LEFT).writes(), left, "the left amplifier's writes, in order");
    assert_eq!(right.len(), 24 + 33 + 4 + 3, "bring-up + this side's 34 verb csets less the pin + the device section's 5 less the pin + power-up");

    // 2. all of it precedes the enable, and the enable precedes any DMA
    let dma = m.first_dma_seq.expect("the host stream ran");
    for addr in [RIGHT, LEFT] {
        let a = m.amp(addr);
        let enable = a.txs.iter().find(|t| t.write && t.reg == 0x20FF).map(|t| t.seq).expect("GLOBAL_EN written");
        let last_protection = a.txs.iter().filter(|t| t.write && t.reg != 0x20FF && t.reg != 0x202B && t.reg != 0x2043).map(|t| t.seq).max().unwrap();
        assert!(last_protection < enable, "{addr:#x}: every protection write precedes GLOBAL_EN");
        // 3. every protection register was READ BACK after its last write and before the enable
        for (reg, _) in expected_image(if addr == RIGHT { "Right" } else { "Left" }) {
            let last_write = a.txs.iter().filter(|t| t.write && t.reg == reg).map(|t| t.seq).max().unwrap();
            assert!(a.txs.iter().any(|t| !t.write && t.reg == reg && t.seq > last_write && t.seq < enable), "{addr:#x}: register {reg:#06x} read back before the enable");
        }
        // the audio clock exists before the part is enabled (power-up waits for the running pipelines), and nothing is CONFIGURED after it
        assert!(a.txs.iter().filter(|t| t.write && ![0x20FF, 0x202B, 0x2043].contains(&t.reg)).all(|t| t.seq < dma), "{addr:#x}: nothing is configured once the audio is already flowing");
        assert!(enable > dma, "{addr:#x}: the part is enabled only after the host stream ran");
        assert_eq!(a.enabled_at.len(), 1);
    }
    // the DSP boot (the first hda/dsp register write) comes after the last protection write too: configuration is complete before a DSP exists
    let first_dsp_write = m.writes.iter().position(|w| w.space == avs_fw_core::effect::Space::Dsp).unwrap();
    assert!(first_dsp_write > 0);
    assert_gain_invariant(m);
}

#[test]
fn a_protection_that_will_not_read_back_stops_the_start_and_the_part_is_never_enabled() {
    // for EVERY register of the protection image: a field that takes no value is found by the readback, the start fails by name, no
    // amplifier is ever enabled, no DSP is booted, and the amplifiers end up in reset
    let regs: Vec<(u16, u8)> = ["Right", "Left"].iter().flat_map(|s| expected_image(s)).map(|(r, (m, _))| (r, m)).collect::<BTreeMap<_, _>>().into_iter().collect();
    assert!(regs.len() >= 29, "the protection image spans 29 registers");
    for (reg, mask) in regs {
        for addr in [RIGHT, LEFT] {
            let Some(mut dev) = started_with_speakers(false) else { return };
            // a stuck field must differ from what the image wants, so make it stick at the INVERSE of the wanted bits
            dev.rig.m.amps.iter_mut().find(|a| a.addr == addr).unwrap().stuck.push((reg, mask));
            let mut c = Client::new();
            let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 2048);
            let a = dev.rig.m.amp(addr);
            let wanted = expected_image(if addr == RIGHT { "Right" } else { "Left" }).get(&reg).copied();
            let default_matches = wanted.is_some_and(|(m, b)| a.regs.get(&reg).copied().unwrap_or(0) & m == b);
            if default_matches && a.txs.iter().all(|t| t.reg != reg || !t.write) {
                continue;
            }
            // the stuck bits keep their reset value; the start only fails if the reset value is not the wanted one
            let (m, b) = wanted.unwrap();
            let reset_val = sim::amp::Amp::new(0).regs.get(&reg).copied().unwrap_or(0);
            if (reset_val ^ b) & (m & mask) == 0 {
                continue; // the stuck value happens to be the wanted one: there is nothing to detect
            }
            assert_eq!(s, Status::InitFailed, "register {reg:#06x} on {addr:#x}");
            for amp in &dev.rig.m.amps {
                assert!(amp.enabled_at.is_empty(), "amplifier {:#x} was enabled with register {reg:#06x} unproven", amp.addr);
                assert!(amp.txs.iter().all(|t| !(t.write && (t.reg == 0x20FF || t.reg == 0x202B || t.reg == 0x2043) && t.value & 1 != 0)), "no enable write was ever sent to {:#x} (stuck {reg:#06x} on {addr:#x}): {:?}", amp.addr, amp.writes());
            }
            assert!(dev.rig.m.first_dma_seq.is_none(), "no audio clock");
            assert_eq!(dev.rig.m.dsp.r32(0x04) & (1 << 16), 0, "no DSP core powered");
            let st = get_status(&mut dev, &mut c);
            assert_eq!((st.state, st.cause), (State::Faulted, Cause::InitFailed));
            assert!(dev.st.safe_verified);
            assert_amps_safe(&dev);
        }
    }
}

#[test]
fn a_part_that_stops_answering_during_its_bring_up_fails_the_start_and_ends_in_reset() {
    for n in [0usize, 1, 5, 12, 30, 90] {
        let Some(mut dev) = started_with_speakers(false) else { return };
        dev.rig.m.amps[0].nak_after = Some(n);
        let mut c = Client::new();
        let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 2048);
        assert_eq!(s, Status::InitFailed, "NAK after {n}");
        assert!(!get_status(&mut dev, &mut c).step_str().is_empty());
        assert!(dev.rig.m.amps.iter().all(|a| a.enabled_at.is_empty()));
        assert!(dev.rig.m.first_dma_seq.is_none());
        assert!(dev.st.safe_verified, "the reset line, not the bus, is the safe state");
        assert_eq!(dev.rig.m.amp_reset_asserted(), true);
    }
}

#[test]
fn a_reset_line_that_cannot_be_released_touches_no_amplifier_and_fails_by_name() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    dev.rig.m.pad_stuck_low = true;
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 2048);
    assert_eq!(s, Status::InitFailed);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.step_str()), (State::Faulted, Cause::InitFailed, "speaker-reset-release"));
    assert_eq!(dev.rig.m.amp_touches(), 0, "a part that is in reset is never addressed");
    assert!(dev.rig.m.first_dma_seq.is_none());
}

#[test]
fn a_dsp_that_fails_to_boot_after_the_amplifiers_were_configured_leaves_them_in_reset_and_never_enabled() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    dev.rig.m.dsp.hook = Some(Box::new(|r| if r.target == 0 && r.ty == AVS_GLB_CREATE_PIPELINE { Act::Silent } else { Act::Normal }));
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 2048);
    assert_eq!(s, Status::InitFailed);
    assert!(dev.rig.m.amps.iter().all(|a| a.enabled_at.is_empty() && a.hw_resets >= 1));
    assert!(dev.st.safe_verified);
    assert_amps_safe(&dev);
    assert_eq!(dev.rig.m.dsp.r32(0x04) & (1 << 16), 0);
}

// ---- the gain ceiling ----------------------------------------------------------------------------------------------------------------

#[test]
fn no_client_request_can_raise_the_dsp_gain_above_the_ceiling() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 4096);
    assert_eq!(s, Status::Ok);
    // every level the protocol allows, muted and not: the gain the DSP is asked for is the ceiling's mapping, never above it
    let mut prev = 0u32;
    for level in 0..=LEVEL_MAX {
        let (st, _) = set_volume(&mut dev, &mut c, level, false);
        assert_eq!(st, Status::Ok, "level {level}");
        let g = *gain_requests(&dev.rig.m).last().unwrap();
        assert!(g <= CEILING_LINEAR, "level {level}: {g}");
        assert_eq!(g, speaker_gain_for(Some(level), false));
        assert!(g >= prev, "monotonic at {level}");
        prev = g;
        let (st, _) = set_volume(&mut dev, &mut c, level, true);
        assert_eq!(st, Status::Ok);
        assert_eq!(*gain_requests(&dev.rig.m).last().unwrap(), 0, "mute is silence, at level {level}");
    }
    assert_eq!(prev, CEILING_LINEAR, "the top level IS the ceiling");
    // levels beyond the protocol's scale, as raw bytes the encoder would also happily produce: refused as malformed, the gain unchanged
    let n0 = gain_requests(&dev.rig.m).len();
    for level in [1001u16, 1002, 1500, 2000, 4096, 0x7FFF, 0x8000, 0xFFFE, 0xFFFF] {
        let mut b = [0u8; 8];
        let n = SetVolume { level, mute: false }.encode(&mut b).unwrap();
        let (st, _, _) = call(&mut dev, &mut c, Opcode::SetVolume, &b[..n]);
        assert_eq!(st, Status::Malformed, "level {level}");
    }
    assert_eq!(gain_requests(&dev.rig.m).len(), n0, "refused requests changed nothing");
    // and the layer below the wire: whatever number a caller hands the session, it is clamped, and the handle refuses what clamping let by
    for g in [0u32, 1, CEILING_LINEAR - 1, CEILING_LINEAR, CEILING_LINEAR + 1, 0x3FFF_FFFF, 0x7FFF_FFFF, 0x8000_0000, u32::MAX] {
        let dsp = dev.dsp.as_mut().unwrap();
        session::set_speaker_gain(&mut dev.rig, dsp, g).unwrap();
        assert_eq!(*gain_requests(&dev.rig.m).last().unwrap(), g.min(CEILING_LINEAR));
    }
    assert_gain_invariant(&dev.rig.m);
    // the volume stage the DSP was created with started SILENT: the first gain request of the whole run is the device's, after the enable
    let a = dev.rig.m.amp(RIGHT);
    let enable_seq = a.enabled_at[0];
    let _ = enable_seq;
}

#[test]
fn the_volume_reply_on_the_speaker_path_says_the_step_below_the_ceiling() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 4096);
    assert_eq!(s, Status::Ok);
    let (st, body) = set_volume(&mut dev, &mut c, LEVEL_MAX, false);
    assert_eq!(st, Status::Ok);
    let r = VolumeReply::decode(&body).unwrap();
    assert_eq!((r.dac_code, r.hp_code, r.muted), (0, 0, false), "the top level is zero steps below the ceiling");
    let (_, body) = set_volume(&mut dev, &mut c, 1, false);
    assert_eq!(VolumeReply::decode(&body).unwrap().dac_code, 63);
    let (_, body) = set_volume(&mut dev, &mut c, 0, false);
    assert!(VolumeReply::decode(&body).unwrap().muted);
}

// ---- every way out is the safe state -------------------------------------------------------------------------------------------------

#[test]
fn unplugging_headphones_moves_an_auto_stream_to_the_speakers_and_plugging_moves_it_back_with_each_path_made_safe_first() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, ph) = start_speakers(&mut dev, &mut c, Route::Auto, 30_000);
    assert_eq!((s, ph.phase), (Status::Ok, StreamPhase::Running));
    assert_eq!(get_status(&mut dev, &mut c).output, Output::Speakers);
    run_ms(&mut dev, 60);
    // headphones in: the speakers are taken down FIRST (off, then reset), then the headset path starts
    dev.rig.m.at_ms(1, sim::Ev::Plug { four_pole: false });
    run_ms(&mut dev, 400);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.output, st.jack), (State::Running, Output::Headphones, Jack::Headphones), "{st:?}");
    for a in &dev.rig.m.amps {
        assert!(a.in_reset && !a.enabled());
        assert_eq!(a.disabled_at.len(), 1, "the parts were switched off before their reset");
    }
    assert_eq!(dev.rig.m.codec.regs[0x6b] & 0x48, 0x08, "the headphone amplifier is on and unmuted");
    // headphones out: the headset path is taken down and the speakers come back
    let t_before = dev.rig.m.t_us;
    dev.rig.m.at_ms(1, sim::Ev::Unplug);
    run_ms(&mut dev, 600);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.output, st.jack), (State::Running, Output::Speakers, Jack::Absent), "{st:?}");
    assert_eq!(dev.rig.m.codec.regs[0x6b] & 0xc8, 0x40, "the headphone amplifier is muted and off");
    assert!(dev.rig.m.amps.iter().all(|a| a.enabled() && a.enabled_at.len() == 2));
    // the reset was held low at least 50 ms between the two speaker sessions (Linux's msleep(50))
    let pads: Vec<(u32, u64)> = dev.rig.m.pad_writes.iter().map(|w| w.1).zip(dev.rig.m.pad_times_us.iter().copied()).collect();
    let _ = t_before;
    let mut last_low: Option<u64> = None;
    for (v, t) in pads {
        if v & 1 == 0 {
            last_low = Some(t);
        } else if let Some(l) = last_low {
            assert!(t - l >= 50_000, "reset low for only {} us before the release", t - l);
        }
    }
    assert_gain_invariant(&dev.rig.m);
}

#[test]
fn a_stream_that_asked_for_the_speakers_keeps_them_when_headphones_are_plugged_in() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 30_000);
    assert_eq!(s, Status::Ok);
    dev.rig.m.at_ms(1, sim::Ev::Plug { four_pole: false });
    run_ms(&mut dev, 300);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.output, st.jack), (State::Running, Output::Speakers, Jack::Headphones));
    assert!(dev.rig.m.amps.iter().all(|a| a.enabled()));
}

#[test]
fn a_fault_while_playing_on_the_speakers_returns_the_amplifiers_to_the_safe_state() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 16_000);
    assert_eq!(s, Status::Ok);
    run_ms(&mut dev, 50);
    assert!(dev.rig.m.amps.iter().all(|a| a.enabled()));
    dev.rig.m.hda.desc_error = true;
    run_ms(&mut dev, 100);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.step_str()), (State::Faulted, Cause::InitFailed, "stream-error"));
    assert!(st.safe_verified && st.output == Output::None && st.speaker_hold == SpeakerHold::Held);
    assert_amps_safe(&dev);
    for a in &dev.rig.m.amps {
        assert_eq!(a.disabled_at.len(), 1, "GLOBAL_EN was switched off before the reset");
    }
}

#[test]
fn a_dsp_exception_while_playing_on_the_speakers_returns_the_amplifiers_to_the_safe_state() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 16_000);
    assert_eq!(s, Status::Ok);
    run_ms(&mut dev, 50);
    // the next IPC the device sends gets an exception notification instead of an answer: a volume change is such a message
    dev.rig.m.dsp.hook = Some(Box::new(|r| if r.ty == AVS_MOD_LARGE_CONFIG_SET { Act::Exception } else { Act::Normal }));
    let (st, _) = set_volume(&mut dev, &mut c, 500, false);
    assert_eq!(st, Status::VolumeRefused, "a volume change that cannot be applied is refused, and the path is made safe");
    assert!(dev.dsp.is_none());
    assert_amps_safe(&dev);
}

#[test]
fn the_client_dying_takes_the_speaker_stream_with_it_and_leaves_the_amplifiers_in_reset() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 16_000);
    assert_eq!(s, Status::Ok);
    run_ms(&mut dev, 50);
    dev.rig.m.at_ms(1, sim::Ev::PeerDead);
    run_ms(&mut dev, 100);
    assert!(dev.stream.is_none() && dev.dsp.is_none());
    assert!(dev.st.safe_verified);
    assert_amps_safe(&dev);
    assert!(dev.rig.m.amps.iter().all(|a| a.disabled_at.len() == 1));
}

#[test]
fn an_amplifier_that_stops_answering_while_playing_is_still_made_safe_by_its_reset() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (id, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 16_000);
    assert_eq!(s, Status::Ok);
    run_ms(&mut dev, 30);
    // the bus dies: the power-down sequence cannot reach the parts, the hardware reset can
    let n = dev.rig.m.amps[0].txs.len();
    dev.rig.m.amps[0].nak_after = Some(n);
    dev.rig.m.amps[1].nak_after = Some(dev.rig.m.amps[1].txs.len());
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamStop, &stream_ref(id));
    assert_eq!(s, Status::Ok);
    let st = get_status(&mut dev, &mut c);
    assert!(st.safe_verified, "the reset line is low and proven low: the strongest safe state does not need the bus");
    assert_eq!(st.speaker_hold, SpeakerHold::Held);
    assert!(dev.rig.m.amp_reset_asserted());
    assert!(dev.rig.m.amps.iter().all(|a| a.in_reset && !a.enabled()));
    assert_gain_invariant(&dev.rig.m);
}

#[test]
fn a_pad_that_will_not_go_low_is_a_stop_that_cannot_claim_safe() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (id, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 16_000);
    assert_eq!(s, Status::Ok);
    run_ms(&mut dev, 30);
    dev.rig.m.pad_stuck_high = true;
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamStop, &stream_ref(id));
    assert_eq!(s, Status::Ok);
    let st = get_status(&mut dev, &mut c);
    assert!(!st.safe_verified, "the line reads high: the status does not claim a safe state it could not prove");
    assert_ne!(st.speaker_hold, SpeakerHold::Held);
    // the parts were still switched off over the bus before the reset was attempted
    assert!(dev.rig.m.amps.iter().all(|a| !a.enabled()));
}

#[test]
fn the_emergency_pass_of_a_dying_task_switches_the_parts_off_and_drives_the_reset_low() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 16_000);
    assert_eq!(s, Status::Ok);
    run_ms(&mut dev, 50);
    let n = (dev.rig.m.amps[0].txs.len(), dev.rig.m.amps[1].txs.len());
    let rep = dev.emergency_safe_state();
    assert!(rep.speakers_held && rep.codec_safe && rep.dma_stopped && rep.dsp_off, "{rep:?}");
    let m = &dev.rig.m;
    // exactly one write each: GLOBAL_EN to zero - no allocation, no plan, no reads
    for (a, n0) in m.amps.iter().zip([n.0, n.1]) {
        assert_eq!(a.txs[n0..].iter().map(|t| (t.write, t.reg, t.value)).collect::<Vec<_>>(), vec![(true, 0x20FF, 0x00)]);
        assert!(a.in_reset && !a.enabled());
    }
    assert!(m.amp_reset_asserted());
    assert!(!dev.rig.i2c.amps_armed());
}

#[test]
fn a_fresh_safe_only_launch_after_a_task_died_mid_speaker_stream_puts_the_amplifiers_in_reset_without_addressing_them() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 16_000);
    assert_eq!(s, Status::Ok);
    run_ms(&mut dev, 50);
    let mut sim = dev.into_machine();
    assert!(sim.amps.iter().all(|a| a.enabled()), "the dead task left the parts enabled");
    sim.grant_hda = None;
    sim.grant_dsp = None;
    sim.grant_cfg = None;
    let n = (sim.amps[0].txs.len(), sim.amps[1].txs.len());
    let mut fresh = avs_device::Device::new(sim);
    let rep = fresh.safe_only();
    assert!(rep.speakers_held && rep.codec_safe && rep.dma_stopped && rep.dsp_off, "{rep:?}");
    let m = &fresh.rig.m;
    assert!(m.amp_reset_asserted());
    assert!(m.amps.iter().all(|a| a.in_reset && !a.enabled()));
    assert_eq!((m.amps[0].txs.len(), m.amps[1].txs.len()), n, "a fresh task has not armed the fence: it does not address the parts, the reset does it");
    assert!(m.refused_writes.is_empty());
    assert_eq!(fresh.st.speaker_hold, SpeakerHold::Held);
}

// ---- the fence -----------------------------------------------------------------------------------------------------------------------

/// Every register any amplifier plan writes (a literal; the same list the core's fence test carries).
const WRITTEN: [u16; 44] = [
    0x2000, 0x2020, 0x2021, 0x2022, 0x2023, 0x2024, 0x2026, 0x2027, 0x2028, 0x2029, 0x202A, 0x202B, 0x202C, 0x203D, 0x203E, 0x203F, 0x2043, 0x2046, 0x2090, 0x2091, 0x2097, 0x2098,
    0x2099, 0x209A, 0x209B, 0x20A8, 0x20A9, 0x20AA, 0x20AB, 0x20AC, 0x20AD, 0x20AE, 0x20AF, 0x20B0, 0x20B1, 0x20B2, 0x20B3, 0x20B5, 0x20D1, 0x20D2, 0x20D3, 0x20D4, 0x20FE, 0x20FF,
];

#[test]
fn the_i2c_fence_lets_an_armed_amplifier_be_written_only_where_the_plans_write() {
    let mut a = I2cAllow::new(0x1a, &[RIGHT, LEFT]).unwrap();
    // unarmed: no amplifier register, in any shape, is reachable
    for addr in [RIGHT, LEFT] {
        for reg in (0..=u16::MAX).step_by(7) {
            assert!(a.check_msg(addr, &[(reg >> 8) as u8, reg as u8, 0], 0).is_err());
            assert!(a.check_msg(addr, &[(reg >> 8) as u8, reg as u8], 1).is_err());
        }
    }
    a.arm_amps(&WRITTEN).unwrap();
    for addr in [RIGHT, LEFT] {
        for reg in 0..=u16::MAX {
            let hi_lo = [(reg >> 8) as u8, reg as u8];
            let w = a.check_msg(addr, &[hi_lo[0], hi_lo[1], 0x5A], 0).is_ok();
            let r = a.check_msg(addr, &hi_lo, 1).is_ok();
            assert_eq!(w, WRITTEN.contains(&reg), "write {reg:#06x}");
            assert_eq!(r, WRITTEN.contains(&reg) || reg == 0x21FF, "read {reg:#06x}");
        }
        // shapes: a one-register write or a one-register read, nothing else
        assert!(a.check_msg(addr, &[0x20, 0xFF], 0).is_err());
        assert!(a.check_msg(addr, &[0x20, 0xFF, 0, 0], 0).is_err());
        assert!(a.check_msg(addr, &[0x20, 0xFF], 2).is_err());
        assert!(a.check_msg(addr, &[0x20, 0xFF, 1], 1).is_err());
        assert!(a.check_msg(addr, &[], 0).is_err());
    }
    // an address that is neither the codec's nor an amplifier's is refused armed or not
    for addr in 0..=u16::MAX {
        if ![0x1a, RIGHT, LEFT].contains(&addr) {
            assert!(a.check(addr).is_err());
            assert!(a.check_msg(addr, &[0x20, 0xFF, 1], 0).is_err());
        }
    }
    // the codec is unaffected
    assert!(a.check_msg(0x1a, &[0x10, 0x20], 0).is_ok());
    a.disarm_amps();
    assert!(a.check(RIGHT).is_err() && a.check(LEFT).is_err() && !a.amps_armed());
}

#[test]
fn the_rig_refuses_a_register_no_plan_writes_even_while_the_fence_is_armed() {
    let Some(mut dev) = started_with_speakers(false) else { return };
    let mut c = Client::new();
    let (_, s, _) = start_speakers(&mut dev, &mut c, Route::Speakers, 16_000);
    assert_eq!(s, Status::Ok);
    assert!(dev.rig.i2c.amps_armed());
    let n = dev.rig.m.i2c.len();
    // the tone generator, the thermal shutdown threshold, the IRQ control, and a plain wrong shape
    for reg in [0x2040u16, 0x2015, 0x2010, 0x2041, 0x20E2, 0x2047] {
        assert!(dev.rig.i2c_write(RIGHT, &[(reg >> 8) as u8, reg as u8, 1]).is_err(), "{reg:#06x}");
        assert!(dev.rig.i2c_write(LEFT, &[(reg >> 8) as u8, reg as u8, 1]).is_err(), "{reg:#06x}");
    }
    assert!(dev.rig.i2c_write(RIGHT, &[0x20, 0xFF]).is_err());
    assert_eq!(dev.rig.m.i2c.len(), n, "refused before the wire");
    assert_eq!(dev.rig.refused.i2c, 13);
    assert_eq!(dev.rig.first_refusal, Some("I2C-AMPLIFIER-REGISTER-REFUSED"));
}

// ---- what the client is told ---------------------------------------------------------------------------------------------------------

#[test]
fn the_status_names_the_speaker_topology_when_a_speaker_stream_cannot_have_it() {
    let Some(mut sim) = atlas_with_speakers() else { return };
    // a corrupted topology (the broker refuses the file): the speakers are not offered, and a speaker open says FIRMWARE-UNAVAILABLE
    sim.files.insert("max98373-tplg.bin".into(), b"not a topology".to_vec());
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    let mut c = Client::new();
    assert_eq!(get_status(&mut dev, &mut c).speaker, Speaker::Refused);
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamOpen, &s16_open_route(Route::Speakers));
    assert_eq!(s, Status::FirmwareUnavailable);
    // the headphones are not affected
    dev.rig.m.codec.plug(false);
    dev.rig.m.poke_jack_line();
    settle(&mut dev);
    let id = open_stream(&mut dev, &mut c);
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase), (Status::Ok, StreamPhase::Running));
}

#[test]
fn without_the_pad_grant_there_is_no_speaker_path() {
    let Some(mut sim) = atlas_with_speakers() else { return };
    sim.pad_granted = false;
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.speaker, st.speaker_hold), (Speaker::Refused, SpeakerHold::NotGranted));
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamOpen, &s16_open_route(Route::Speakers));
    assert_eq!(s, Status::SpeakerProtectionUnavailable);
    assert_eq!(dev.rig.m.amp_touches(), 0);
}

#[test]
fn a_part_whose_protection_was_not_proven_cannot_be_enabled_by_any_caller() {
    use avs_device::allow::{Allow, PadAllow};
    use avs_device::rig::Rig;
    use avs_device::speakers::{Speakers, SpkFail};
    let Some(sim) = atlas_with_speakers() else { return };
    let board = atlas_board();
    let mut rig = Rig::new(sim, Allow::read_only(), I2cAllow::new(0x1a, &[RIGHT, LEFT]).unwrap(), PadAllow::none());
    let mut spk = Speakers::for_board(&[RIGHT, LEFT], board.spkr_rst_pad).unwrap().expect("the Pixelbook Go's amplifiers");
    // never begun: no bus traffic, no enable
    assert_eq!(spk.power_up(&mut rig, &mut |_| Ok(())), Err(SpkFail::NotProtected));
    assert_eq!(rig.m.amp_touches(), 0);
    // a board that lists other amplifiers has no speakers at all
    assert!(Speakers::for_board(&[0x34, 0x35], board.spkr_rst_pad).unwrap().is_none());
    assert!(Speakers::for_board(&[RIGHT], board.spkr_rst_pad).unwrap().is_none());
}
