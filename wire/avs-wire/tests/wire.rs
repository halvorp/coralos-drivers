// SPDX-License-Identifier: MIT OR Apache-2.0
//! The frame header, the coded enums and the STATUS: every refusal is exercised, every value round-trips, and no two values share a number.
use avs_wire::*;

fn framed(op: Opcode, flags: u32, id: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; HEADER_BYTES + payload.len()];
    let n = frame(op, flags, id, payload, &mut out).unwrap();
    out.truncate(n);
    out
}

#[test]
fn a_header_round_trips_and_pins_its_layout() {
    let f = framed(Opcode::PcmWrite, FLAG_REPLY, 0x1122_3344_5566_7788, &[1, 2, 3]);
    assert_eq!(&f[0..4], b"AVSD");
    assert_eq!(u16::from_le_bytes([f[4], f[5]]), ABI_MAJOR);
    assert_eq!(u32::from_le_bytes([f[8], f[9], f[10], f[11]]), 9, "PCM-WRITE is opcode 9");
    assert_eq!(u32::from_le_bytes([f[12], f[13], f[14], f[15]]), 1, "REPLY is flag bit 0");
    assert_eq!(&f[16..24], &0x1122_3344_5566_7788u64.to_le_bytes());
    assert_eq!(u32::from_le_bytes([f[24], f[25], f[26], f[27]]), 3);
    assert_eq!(&f[28..32], &[0; 4], "reserved is zero");
    let (h, p) = Header::decode(&f).unwrap();
    assert_eq!((h.opcode, h.request_id, h.payload_bytes, h.is_reply(), h.is_event()), (Opcode::PcmWrite, 0x1122_3344_5566_7788, 3, true, false));
    assert_eq!(p, &[1, 2, 3]);
    assert_eq!(h.minor, ABI_MINOR);
}

#[test]
fn every_way_a_header_can_be_wrong_is_a_named_refusal() {
    let good = framed(Opcode::Hello, 0, 1, &[]);
    let with = |off: usize, bytes: &[u8]| {
        let mut f = good.clone();
        f[off..off + bytes.len()].copy_from_slice(bytes);
        f
    };
    assert_eq!(Header::decode(&with(0, b"I915")).unwrap_err().name(), "bad-magic");
    assert_eq!(Header::decode(&with(4, &2u16.to_le_bytes())).unwrap_err().name(), "bad-major");
    assert_eq!(Header::decode(&with(8, &0x7777u32.to_le_bytes())).unwrap_err().name(), "unknown-opcode");
    assert_eq!(Header::decode(&with(12, &4u32.to_le_bytes())).unwrap_err().name(), "unknown-flags");
    assert_eq!(Header::decode(&with(28, &1u32.to_le_bytes())).unwrap_err().name(), "reserved-non-zero");
    assert_eq!(Header::decode(&with(24, &(MAX_PAYLOAD + 1).to_le_bytes())).unwrap_err().name(), "payload-too-big");
    assert_eq!(Header::decode(&with(24, &5u32.to_le_bytes())).unwrap_err().name(), "payload-length", "declares 5, carries 0");
    assert_eq!(Header::decode(&good[..31]).unwrap_err().name(), "short");
    let mut long = good.clone();
    long.push(0);
    assert_eq!(Header::decode(&long).unwrap_err().name(), "payload-length", "a trailing byte is not slack");
    // a minor newer than ours is accepted: additive
    assert!(Header::decode(&with(6, &9u16.to_le_bytes())).is_ok());
}

#[test]
fn a_frame_larger_than_the_limit_is_refused_at_both_ends() {
    let big = vec![0u8; MAX_PAYLOAD as usize + 1];
    let mut out = vec![0u8; MAX_FRAME_BYTES + 8];
    assert_eq!(frame(Opcode::Hello, 0, 1, &big, &mut out).unwrap_err().name(), "payload-too-big");
    let ok = vec![0u8; MAX_PAYLOAD as usize];
    assert_eq!(frame(Opcode::Hello, 0, 1, &ok, &mut out), Ok(MAX_FRAME_BYTES));
    let mut small = [0u8; 40];
    assert_eq!(frame(Opcode::Hello, 0, 1, &[0u8; 16], &mut small).unwrap_err().name(), "no-room");
}

#[test]
fn opcodes_are_unique_and_each_round_trips() {
    let mut seen = vec![];
    for op in Opcode::ALL {
        assert!(!seen.contains(&(*op as u32)), "{op:?} reuses a number");
        seen.push(*op as u32);
        assert_eq!(Opcode::from_u32(*op as u32), Ok(*op));
        assert!(!op.name().is_empty());
    }
    assert_eq!(Opcode::ALL.len(), 16);
    // the three classes: requests below 0x80, events 0x80..0x100, the firmware channel from 0x100
    for op in Opcode::ALL {
        let n = *op as u32;
        assert_eq!(op.is_event(), (0x80..0x100).contains(&n), "{op:?}");
        assert_eq!(op.is_firmware(), n >= 0x100, "{op:?}");
    }
}

#[test]
fn the_separated_causes_have_their_own_numbers_and_statuses() {
    // Astra's five: backend-unavailable, firmware-unavailable, init-failed, headphone-disconnected, speaker-protection-unavailable
    let causes = [
        (Cause::BackendUnavailable, Status::BackendUnavailable, 0x10u32),
        (Cause::FirmwareUnavailable, Status::FirmwareUnavailable, 0x11),
        (Cause::InitFailed, Status::InitFailed, 0x12),
        (Cause::HeadphoneDisconnected, Status::HeadphoneDisconnected, 0x13),
        (Cause::SpeakerProtectionUnavailable, Status::SpeakerProtectionUnavailable, 0x14),
    ];
    for (c, s, n) in causes {
        assert_eq!(c.status(), s);
        assert_eq!(s as u32, n);
        assert_eq!(Status::from_raw(n), Some(s));
    }
    assert_eq!(Cause::None.status(), Status::Ok);
    assert_eq!(Cause::ALL.len(), 6);
    // names are distinct
    let mut names: Vec<&str> = Cause::ALL.iter().map(|c| c.name()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 6);
}

#[test]
fn every_coded_enum_round_trips_and_rejects_the_next_value() {
    for s in Status::ALL {
        assert_eq!(Status::from_raw(*s as u32), Some(*s));
    }
    assert_eq!(Status::from_raw(0x15), None);
    assert_eq!(Status::from_raw(14), None);
    for s in State::ALL {
        assert_eq!(State::from_raw(*s as u8), Some(*s));
    }
    assert_eq!(State::from_raw(6), None);
    for s in Jack::ALL {
        assert_eq!(Jack::from_raw(*s as u8), Some(*s));
    }
    assert_eq!(Jack::from_raw(3), None);
    for s in StreamPhase::ALL {
        assert_eq!(StreamPhase::from_raw(*s as u8), Some(*s));
    }
    assert_eq!(StreamPhase::from_raw(4), None);
    for s in SpeakerHold::ALL {
        assert_eq!(SpeakerHold::from_raw(*s as u8), Some(*s));
    }
    assert_eq!(SpeakerHold::from_raw(4), None);
    // the speakers have exactly one state: refused
    assert_eq!(Speaker::ALL, &[Speaker::Refused]);
    assert_eq!(Speaker::from_raw(1), None);
}

#[test]
fn a_status_body_round_trips_and_names_its_step() {
    let mut s = StatusBody::new();
    s.state = State::Faulted;
    s.cause = Cause::InitFailed;
    s.jack = Jack::Headphones;
    s.stream = StreamPhase::Suspended;
    s.safe_verified = true;
    s.speaker_hold = SpeakerHold::Held;
    s.generation = 77;
    s.volume_level = 640;
    s.detail = 0xdead;
    s.set_step("rom-init");
    let mut b = [0u8; 128];
    let n = s.encode(&mut b).unwrap();
    assert_eq!(n, STATUS_BODY_BYTES);
    let d = StatusBody::decode(&b[..n]).unwrap();
    assert_eq!(d, s);
    assert_eq!(d.step_str(), "rom-init");
    assert_eq!(d.speaker, Speaker::Refused, "the speakers are refused in every status");
    // a long step name is cut with a NUL kept
    s.set_step("a-step-name-that-is-much-longer-than-thirty-one-bytes");
    assert_eq!(s.step_str().len(), STEP_BYTES - 1);
    assert_eq!(s.step[STEP_BYTES - 1], 0);
}

#[test]
fn a_status_body_with_a_bad_field_is_refused_by_name() {
    let mut b = [0u8; 128];
    let n = StatusBody::new().encode(&mut b).unwrap();
    for (off, v, why) in [(0usize, 6u8, "state"), (1, 6, "cause"), (2, 3, "jack"), (3, 4, "stream"), (4, 2, "safe"), (5, 1, "speaker"), (6, 4, "hold"), (7, 1, "reserved")] {
        let mut bad = b;
        bad[off] = v;
        assert!(StatusBody::decode(&bad[..n]).is_err(), "{why}");
    }
    let mut bad = b;
    bad[12..14].copy_from_slice(&(VOLUME_MAX + 1).to_le_bytes());
    assert_eq!(StatusBody::decode(&bad[..n]).unwrap_err().name(), "limit-exceeded");
    assert_eq!(StatusBody::decode(&b[..n - 1]).unwrap_err().name(), "short");
    assert_eq!(StatusBody::decode(&b[..n + 1]).unwrap_err().name(), "trailing-bytes");
}

#[test]
fn replies_and_events_carry_their_flags_and_the_status_first() {
    let mut out = [0u8; 256];
    let n = msg::reply(Opcode::SetVolume, 9, Status::VolumeRefused, &[1, 2], &mut out).unwrap();
    let (h, p) = Header::decode(&out[..n]).unwrap();
    assert!(h.is_reply() && !h.is_event());
    assert_eq!(h.request_id, 9);
    let (st, body) = msg::split_reply(p).unwrap();
    assert_eq!((st, body), (Status::VolumeRefused, &[1u8, 2][..]));
    let n = msg::event(Opcode::EvJack, &[1, 0, 0, 0], &mut out).unwrap();
    let (h, _) = Header::decode(&out[..n]).unwrap();
    assert!(h.is_event() && !h.is_reply());
    assert_eq!(h.request_id, 0);
    assert_eq!(msg::split_reply(&[0xff, 0, 0, 0]).unwrap_err().name(), "bad-value");
    assert_eq!(msg::split_reply(&[0, 0]).unwrap_err().name(), "short");
}

#[test]
fn a_board_record_round_trips_and_refuses_what_would_make_the_speakers_reachable() {
    use avs_wire::board::*;
    let atlas = Board {
        pci_vendor: 0x8086,
        pci_device: 0x9d71,
        codec_addr: 0x1a,
        amps: [0x31, 0x32, 0, 0],
        n_amps: 2,
        headset_ssp: 1,
        speaker_ssp: [0, 0, 0, 0],
        n_speaker_ssp: 1,
        playback_stream: 0,
        mclk_hz: 24_576_000,
        i2c_bus_hz: 400_000,
        i2c_fs_hcnt: 0x96,
        i2c_fs_lcnt: 0xe4,
        i2c_sda_hold: 0,
        jack_pad: Pad { community: 2, pad: 10 },
        spkr_rst_pad: Pad { community: 2, pad: 11 },
        pad_window_len: 0x10000,
    };
    atlas.check().unwrap();
    let mut b = [0u8; 128];
    let n = atlas.encode(&mut b).unwrap();
    assert_eq!(Board::decode(&b[..n]).unwrap(), atlas);
    assert_eq!(atlas.amps(), &[0x31, 0x32]);
    assert_eq!(atlas.speaker_ssps(), &[0]);
    // the codec at an amplifier's address, the headset on a speaker SSP, a bad address, no reset pad, colliding pads: each refused by name
    let mut bad = atlas;
    bad.codec_addr = 0x31;
    assert_eq!(bad.check().unwrap_err().name(), "codec-is-an-amplifier");
    let mut bad = atlas;
    bad.headset_ssp = 0;
    assert_eq!(bad.check().unwrap_err().name(), "headset-on-speaker-ssp");
    let mut bad = atlas;
    bad.codec_addr = 0x80;
    assert_eq!(bad.check().unwrap_err().name(), "bad-address");
    let mut bad = atlas;
    bad.spkr_rst_pad.pad = u16::MAX;
    assert_eq!(bad.check().unwrap_err().name(), "no-speaker-reset-pad");
    let mut bad = atlas;
    bad.spkr_rst_pad = bad.jack_pad;
    assert_eq!(bad.check().unwrap_err().name(), "pads-collide");
    // decode re-runs the check, so a record that encodes can still not load
    let mut raw = [0u8; 128];
    let n = bad.encode(&mut raw).unwrap();
    assert!(Board::decode(&raw[..n]).is_err());
    // framing errors
    assert!(Board::decode(&b[..n - 1]).is_err());
    let mut wrong = b;
    wrong[0] = b'X';
    assert_eq!(Board::decode(&wrong[..n]).unwrap_err().name(), "board-wire");
}
