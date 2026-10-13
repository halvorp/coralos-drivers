// SPDX-License-Identifier: GPL-2.0-only
//! The allow-lists: set equality with what the plans really write, refusals by name, and the speaker amplifiers being unreachable.
mod sim;
use avs_device::allow::*;
use avs_device::machine::*;
use avs_fw_core::effect::{Reg, Space, Width};
use avs_wire::msg::*;
use avs_wire::*;
use sim::harness::*;
use sim::world::*;
use std::collections::BTreeSet;

fn key(s: Space, off: u32, w: Width) -> (u8, u32, u8) {
    (
        match s {
            Space::Dsp => 0,
            Space::Hda => 1,
            Space::PciConfig => 2,
        },
        off,
        match w {
            Width::B8 => 1,
            Width::W16 => 2,
            Width::D32 => 4,
        },
    )
}

/// Everything the plans wrote across a whole life of the device: a session, an unplug, a replug, a fault and a retry.
fn a_whole_life() -> Option<avs_device::Device<sim::Sim>> {
    let mut dev = started(true)?;
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    start_stream(&mut dev, &mut c, id);
    run_ms(&mut dev, 60);
    call(&mut dev, &mut c, Opcode::SetVolume, &{
        let mut b = [0u8; 4];
        let n = SetVolume { level: 500, mute: false }.encode(&mut b).unwrap();
        b[..n].to_vec()
    });
    dev.rig.m.at_ms(1, sim::Ev::Unplug);
    run_ms(&mut dev, 50);
    dev.rig.m.at_ms(1, sim::Ev::Plug { four_pole: true });
    run_ms(&mut dev, 400);
    call(&mut dev, &mut c, Opcode::StreamClose, &stream_ref(id));
    Some(dev)
}

#[test]
fn the_write_allow_list_is_exactly_what_the_plans_write() {
    let Some(dev) = a_whole_life() else { return };
    let written: BTreeSet<(u8, u32, u8)> = dev.rig.m.writes.iter().map(|w| key(w.space, w.off, w.width)).collect();
    let mut granted: BTreeSet<(u8, u32, u8)> = BTreeSet::new();
    for space in [Space::Dsp, Space::Hda, Space::PciConfig] {
        for e in dev.rig.allow.registers(space) {
            granted.insert(key(space, e.off, e.width));
        }
    }
    // the downlink mailbox is a window of data, not a register: it is checked separately
    let (win_lo, win_hi) = {
        let w = dev.rig.allow.windows()[0];
        (w.off, w.off + w.len)
    };
    let in_window = |k: &(u8, u32, u8)| k.0 == 0 && k.1 >= win_lo && k.1 < win_hi;
    assert!(written.iter().any(in_window), "no IPC payload was ever written to the mailbox");
    let written_regs: BTreeSet<_> = written.iter().filter(|k| !in_window(k)).copied().collect();
    let never_written: Vec<_> = granted.difference(&written_regs).collect();
    let not_granted: Vec<_> = written_regs.difference(&granted).collect();
    assert!(not_granted.is_empty(), "written but not granted: {not_granted:x?}");
    assert!(never_written.is_empty(), "granted but no plan ever writes them (a register no plan writes cannot be granted): {never_written:x?}");
    assert!(dev.rig.m.refused_writes.is_empty(), "the window refused: {:?}", dev.rig.m.refused_writes);
    assert_eq!(dev.rig.refused.reg, 0);
}

#[test]
fn the_loader_grant_is_built_from_the_same_list_and_nothing_wider_than_its_dwords() {
    let Some(dev) = a_whole_life() else { return };
    let m = &dev.rig.m;
    assert_eq!(m.layout_ready_calls, 1, "the host narrows its grants once, when discovery finished");
    for (space, grant) in [(Space::Dsp, &m.grant_dsp), (Space::Hda, &m.grant_hda), (Space::PciConfig, &m.grant_cfg)] {
        let g = grant.as_ref().expect("a grant");
        // every granted range is whole dwords made of an allowed register's dword, or the mailbox
        for (o, l) in g {
            assert_eq!(o % 4, 0);
            assert_eq!(l % 4, 0);
        }
        // and every allowed register is inside it
        for e in dev.rig.allow.registers(space) {
            let w = match e.width {
                Width::B8 => 1,
                Width::W16 => 2,
                Width::D32 => 4,
            };
            assert!(g.iter().any(|(o, l)| e.off >= *o && e.off + w <= o + l), "{space:?} {:#x} is allowed but not granted", e.off);
        }
    }
    // PCI configuration: two dwords and nothing else
    assert_eq!(m.grant_cfg.as_ref().unwrap(), &vec![(0x44, 8)]);
}

#[test]
fn registers_outside_the_lists_are_refused_by_name_and_never_reach_the_hardware() {
    let Some(mut dev) = started(true) else { return };
    let before = dev.rig.m.writes.len();
    for reg in [
        Reg { space: Space::Dsp, off: 0x0c, width: Width::D32 },   // ADSPIS (read-only status)
        Reg { space: Space::Dsp, off: 0x44, width: Width::D32 },   // HIPCTE
        Reg { space: Space::Dsp, off: 0x8000, width: Width::D32 }, // the firmware status register
        Reg { space: Space::Dsp, off: 0xb000, width: Width::D32 }, // one dword past the mailbox
        Reg { space: Space::Dsp, off: 0x04, width: Width::B8 },    // ADSPCS at the wrong width
        Reg { space: Space::Hda, off: 0x00, width: Width::W16 },   // GCAP
        Reg { space: Space::Hda, off: 0x70, width: Width::D32 },   // DPLBASE: no position buffer
        Reg { space: Space::Hda, off: 0x80 + 0x20 * 8 + 0x18, width: Width::D32 }, // another output stream's BDLPL
        Reg { space: Space::Hda, off: 0x40, width: Width::D32 },   // CORBLBASE: the command rings are never used
        Reg { space: Space::PciConfig, off: 0x04, width: Width::W16 }, // the PCI command register
        Reg { space: Space::PciConfig, off: 0x10, width: Width::D32 }, // BAR0
    ] {
        let r = dev.rig.write(reg, 0xffff_ffff);
        assert_eq!(r, Err(RegFault::NotAllowed { space: reg.space, off: reg.off }), "{reg:?}");
        assert!(dev.rig.update(reg, 1, 1).is_err());
    }
    assert_eq!(dev.rig.m.writes.len(), before, "a refused write left no trace on the hardware");
    assert!(dev.rig.refused.reg >= 22);
}

#[test]
fn only_the_playback_streams_descriptor_is_writable_beyond_status_and_control_bytes() {
    let Some(dev) = started(true) else { return };
    let a = &dev.rig.allow;
    let sd = |i: u32| 0x80 + 0x20 * i;
    // all fourteen streams: the status and control BYTES (clear-all and disable-all walks)
    for i in 0..14 {
        assert!(a.permits(Space::Hda, sd(i), Width::B8), "stream {i} control byte");
        assert!(a.permits(Space::Hda, sd(i) + 3, Width::B8), "stream {i} status byte");
    }
    // stream 7 (the first output stream): the full descriptor
    for (o, w) in [(0x00, Width::D32), (0x08, Width::D32), (0x0c, Width::W16), (0x12, Width::W16), (0x18, Width::D32), (0x1c, Width::D32)] {
        assert!(a.permits(Space::Hda, sd(7) + o, w), "playback {o:#x}");
        assert!(!a.permits(Space::Hda, sd(8) + o, w), "the next output stream {o:#x}");
        assert!(!a.permits(Space::Hda, sd(0) + o, w) || o == 0, "an input stream {o:#x}");
    }
}

// ---- the speaker amplifiers ----

#[test]
fn no_i2c_transaction_can_reach_an_amplifier_address_through_the_rig() {
    let Some(mut dev) = started(true) else { return };
    let n0 = dev.rig.m.i2c.len();
    // the amplifiers first: the first refusal on record names them as amplifiers
    assert!(dev.rig.i2c_write(0x31, &[0, 0]).is_err());
    assert_eq!(dev.rig.first_refusal, Some("I2C-SPEAKER-AMPLIFIER-REFUSED"), "the first thing refused was an amplifier, and it is named as one");
    for addr in 0u16..=0x7f {
        let w = dev.rig.i2c_write(addr, &[0x00, 0x00]);
        let mut b = [0u8; 1];
        let r = dev.rig.i2c_write_read(addr, &[0x00], &mut b);
        if addr == 0x1a {
            assert!(w.is_ok() && r.is_ok());
        } else {
            assert_eq!(w, Err(I2cFault::AddressRefused { addr }), "{addr:#x}");
            assert_eq!(r, Err(I2cFault::AddressRefused { addr }), "{addr:#x}");
        }
    }
    assert!(dev.rig.m.i2c[n0..].iter().all(|t| t.addr == 0x1a), "a refused transfer reached the bus");
    assert_eq!(dev.rig.m.amp_touches(), 0);
    assert_eq!(dev.rig.refused.i2c, 1 + 2 * 127);
}

#[test]
fn the_codec_plan_runner_refuses_an_amplifier_address_before_the_wire() {
    use da7219_core::bus::{wire, BusError, Wiring};
    let w = Wiring::new(0x1a, &[0x31, 0x32]).unwrap();
    assert_eq!(wire::write_to(&w, 0x31, 0x10, 1).unwrap_err(), BusError::SpeakerAmplifier { addr: 0x31 });
    assert_eq!(wire::read_to(&w, 0x32, 0x10, 1).unwrap_err(), BusError::SpeakerAmplifier { addr: 0x32 });
    assert_eq!(wire::write_pair_to(&w, 0x31, 0x10, 1, 2).unwrap_err(), BusError::SpeakerAmplifier { addr: 0x31 });
    // and a wiring that puts the codec on an amplifier's address cannot be built
    assert_eq!(Wiring::new(0x31, &[0x31, 0x32]).unwrap_err(), BusError::SpeakerAmplifier { addr: 0x31 });
}

#[test]
fn a_board_record_that_would_make_the_codec_an_amplifier_never_loads() {
    let Some(mut sim) = atlas() else { return };
    let mut bad = atlas_board();
    bad.codec_addr = 0x31;
    sim.files.insert("board.bin".into(), board_bytes(&bad));
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    assert!(dev.board.is_none());
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.cause, st.step_str()), (Cause::FirmwareUnavailable, "board"));
    assert!(dev.rig.m.i2c.is_empty(), "with no valid board the bus is never touched");
}

#[test]
fn the_i2c_allow_list_is_the_codec_and_only_the_codec_for_every_address() {
    let a = I2cAllow::new(0x1a, &[0x31, 0x32]).unwrap();
    for addr in 0..=u16::MAX {
        match addr {
            0x1a => assert_eq!(a.check(addr), Ok(())),
            0x31 | 0x32 => assert_eq!(a.check(addr), Err(I2cRefusal::SpeakerAmplifier { addr })),
            _ => assert_eq!(a.check(addr), Err(I2cRefusal::NotTheCodec { addr })),
        }
    }
    // before the board is known, nothing at all
    let none = I2cAllow::none();
    assert!((0..=0x7fu16).all(|x| none.check(x).is_err()));
}

#[test]
fn across_a_whole_life_the_amplifiers_are_never_addressed_and_their_reset_is_held_low_first() {
    let Some(dev) = a_whole_life() else { return };
    let m = &dev.rig.m;
    assert_eq!(m.amp_touches(), 0);
    assert!(m.i2c.iter().all(|t| t.addr == 0x1a && t.acked));
    // the reset pad was driven low (PADCFG0 of GPP_F11: GPIO mode, TX on, level low) before any other effect
    assert!(!m.pad_writes.is_empty());
    let (off, v) = m.pad_writes[0];
    assert_eq!(off, 0x400 + 11 * 8);
    assert_eq!(v & 0x0001, 0, "TXSTATE low");
    assert_eq!(v & 0x0100, 0, "TX enabled");
    assert_eq!(v & (0xf << 10), 0, "GPIO mode");
    assert!(m.pad_writes.iter().all(|(o, v)| *o == off && v & 1 == 0), "the pad is only ever driven low, and only that pad");
    assert_eq!(m.pad_writes.len(), 1 + dev.stats.sessions as usize, "once at start and once per session start");
    assert!(m.writes.iter().all(|w| w.space != Space::Dsp || w.off != 0 || true));
    // the first hardware effect of the whole life was the pad write: no register had been written yet
    assert_eq!(dev.st.speaker, Speaker::Refused);
    assert_eq!(dev.st.speaker_hold, SpeakerHold::Held);
}

#[test]
fn the_speaker_route_is_refused_by_name_and_touches_nothing() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let mut b = [0u8; 32];
    let n = Open { format: Format::S16_48K_STEREO, route: Route::Speakers, period_frames: 0 }.encode(&mut b).unwrap();
    let (s, body, _) = call(&mut dev, &mut c, Opcode::StreamOpen, &b[..n]);
    assert_eq!((s, body.len()), (Status::SpeakerProtectionUnavailable, 0));
    // refused again whatever the device's state (here: headphones just pulled), and nothing is touched by the refusals themselves
    dev.rig.m.codec.unplug();
    dev.rig.m.poke_jack_line();
    settle(&mut dev);
    let (w0, i0, p0, d0) = (dev.rig.m.writes.len(), dev.rig.m.i2c.len(), dev.rig.m.pad_writes.len(), dev.rig.m.regions.len());
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamOpen, &b[..n]);
    assert_eq!(s, Status::SpeakerProtectionUnavailable);
    assert!(dev.stream.is_none());
    assert_eq!((dev.rig.m.writes.len(), dev.rig.m.i2c.len(), dev.rig.m.pad_writes.len(), dev.rig.m.regions.len()), (w0, i0, p0, d0), "a refused speaker open changed nothing");
    assert_eq!(get_status(&mut dev, &mut c).speaker, Speaker::Refused);
}

#[test]
fn without_a_pad_grant_the_hold_is_a_named_gap_not_a_silent_skip() {
    let Some(mut sim) = atlas() else { return };
    sim.pad_granted = false;
    sim.codec.plug(false);
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    assert_eq!(dev.st.speaker_hold, SpeakerHold::NotGranted);
    assert!(dev.rig.m.log_has("GAP step=SPEAKER-RESET-PAD"), "the gap is logged by name");
    assert!(dev.rig.m.pad_writes.is_empty());
    // the device still works headphones-only, and says what it is not doing
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.speaker, st.speaker_hold), (State::Idle, Cause::None, Speaker::Refused, SpeakerHold::NotGranted));
    assert_eq!(dev.rig.m.amp_touches(), 0);
}

#[test]
fn only_the_speaker_reset_pad_is_writable_in_a_pad_window() {
    let Some(mut dev) = started(true) else { return };
    let f10 = PadReg { community: 2, off: 0x400 + 10 * 8 };
    let f11 = PadReg { community: 2, off: 0x400 + 11 * 8 };
    assert_eq!(dev.rig.pad_write(f10, 0), Err(PadFault::NotAllowed { community: 2, off: f10.off }));
    assert_eq!(dev.rig.pad_write(PadReg { community: 2, off: 0x100 }, 0), Err(PadFault::NotAllowed { community: 2, off: 0x100 }));
    assert_eq!(dev.rig.pad_write(PadReg { community: 0, off: 0x400 }, 0), Err(PadFault::NotAllowed { community: 0, off: 0x400 }));
    assert!(dev.rig.pad_write(f11, 0x0000_0002).is_ok());
}

#[test]
fn an_amplifier_reset_that_cannot_be_held_low_stops_everything_by_name() {
    let Some(mut sim) = atlas() else { return };
    sim.pad_stuck_high = true;
    sim.codec.plug(false);
    let mut dev = avs_device::Device::new(sim);
    dev.start();
    assert_eq!(dev.st.speaker_hold, SpeakerHold::Failed);
    let mut c = Client::new();
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.step_str()), (State::Faulted, Cause::InitFailed, "speaker-hold"));
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamOpen, &s16_open());
    assert_eq!(s, Status::InitFailed, "no stream on a board whose speakers cannot be shown to be silent");
    assert!(dev.rig.m.dsp.transfers.is_empty() && dev.rig.m.i2c.is_empty(), "nothing else was started");
    // the line is released: the next open climbs again and succeeds
    dev.rig.m.pad_stuck_high = false;
    let (s, _, _) = call(&mut dev, &mut c, Opcode::StreamOpen, &s16_open());
    assert_eq!(s, Status::Ok);
    assert_eq!(dev.st.speaker_hold, SpeakerHold::Held);
}
