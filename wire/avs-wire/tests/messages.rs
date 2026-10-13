// SPDX-License-Identifier: MIT OR Apache-2.0
//! Every message round-trips; every bound is held; PCM never exceeds its limit; the firmware names are safe.
use avs_wire::fw::*;
use avs_wire::msg::*;
use avs_wire::*;

#[test]
fn hello_and_subscribe_round_trip_and_subscribe_refuses_unknown_bits() {
    let mut b = [0u8; 64];
    let h = HelloReply::current();
    let n = h.encode(&mut b).unwrap();
    assert_eq!(HelloReply::decode(&b[..n]).unwrap(), h);
    assert_eq!((h.abi_major, h.max_pcm_bytes), (1, MAX_PCM_BYTES));
    let n = Subscribe { mask: SUB_ALL }.encode(&mut b).unwrap();
    assert_eq!(Subscribe::decode(&b[..n]).unwrap().mask, 7);
    assert_eq!(Subscribe::decode(&8u32.to_le_bytes()).unwrap_err().name(), "bad-value");
    assert_eq!(Subscribe::decode(&[0; 5]).unwrap_err().name(), "trailing-bytes");
}

#[test]
fn caps_round_trip_with_the_one_format_the_topology_offers() {
    let mut formats = [Format { rate: 0, channels: 0, container_bits: 0, valid_bits: 0 }; MAX_CAPS_FORMATS];
    formats[0] = Format::S16_48K_STEREO;
    let c = Caps { formats, n_formats: 1, period_frames: 1024, periods: 4, latency_frames: LATENCY_UNMEASURED, fifo_bytes: 16384 };
    let mut b = [0u8; 128];
    let n = c.encode(&mut b).unwrap();
    let d = Caps::decode(&b[..n]).unwrap();
    assert_eq!(d, c);
    assert_eq!(d.formats(), &[Format { rate: 48_000, channels: 2, container_bits: 16, valid_bits: 16 }]);
    assert_eq!(Format::S16_48K_STEREO.frame_bytes(), 4);
    assert_eq!(d.latency_frames, u32::MAX, "an unmeasured latency is the sentinel, never a made-up number");
    let mut bad = b;
    bad[0] = MAX_CAPS_FORMATS as u8 + 1;
    assert_eq!(Caps::decode(&bad[..n]).unwrap_err().name(), "limit-exceeded");
    let mut c2 = c;
    c2.n_formats = 5;
    assert_eq!(c2.encode(&mut b).unwrap_err().name(), "limit-exceeded");
}

#[test]
fn open_and_its_reply_round_trip_and_refuse_nonzero_reserved() {
    let o = Open { format: Format::S16_48K_STEREO, route: Route::Headphones, period_frames: 0 };
    let mut b = [0u8; 64];
    let n = o.encode(&mut b).unwrap();
    assert_eq!(Open::decode(&b[..n]).unwrap(), o);
    let mut bad = b;
    bad[7] = 1; // the format's reserved byte
    assert_eq!(Open::decode(&bad[..n]).unwrap_err().name(), "reserved-non-zero");
    let mut bad = b;
    bad[8] = 2; // a route that does not exist
    assert_eq!(Open::decode(&bad[..n]).unwrap_err().name(), "bad-value");
    let sp = Open { route: Route::Speakers, ..o };
    let n2 = sp.encode(&mut b).unwrap();
    assert_eq!(Open::decode(&b[..n2]).unwrap().route, Route::Speakers, "the speaker route is expressible so it can be REFUSED by name");
    let r = OpenReply { stream: 3, period_frames: 1024, periods: 4, fifo_bytes: 16384 };
    let n = r.encode(&mut b).unwrap();
    assert_eq!(OpenReply::decode(&b[..n]).unwrap(), r);
    let ring = OpenRing { stream: 3, region: 9, layout: 1 };
    let n = ring.encode(&mut b).unwrap();
    assert_eq!(OpenRing::decode(&b[..n]).unwrap(), ring);
    let s = StreamRef { stream: 3 };
    let n = s.encode(&mut b).unwrap();
    assert_eq!(StreamRef::decode(&b[..n]).unwrap(), s);
}

#[test]
fn pcm_writes_carry_their_length_and_never_exceed_the_limit() {
    let data = vec![7u8; 4096];
    let w = PcmWrite { stream: 2, data: &data };
    let mut b = vec![0u8; 8 + 4096];
    let n = w.encode(&mut b).unwrap();
    assert_eq!(PcmWrite::decode(&b[..n]).unwrap(), w);
    // a declared length that the payload does not carry is short; one past the limit is refused before the bytes are looked at
    let mut lie = b.clone();
    lie[4..8].copy_from_slice(&8192u32.to_le_bytes());
    assert_eq!(PcmWrite::decode(&lie[..n]).unwrap_err().name(), "short");
    lie[4..8].copy_from_slice(&(MAX_PCM_BYTES + 1).to_le_bytes());
    assert_eq!(PcmWrite::decode(&lie[..n]).unwrap_err().name(), "limit-exceeded");
    let mut trailing = b.clone();
    trailing.push(0);
    assert_eq!(PcmWrite::decode(&trailing).unwrap_err().name(), "trailing-bytes");
    let max = vec![0u8; MAX_PCM_BYTES as usize];
    let mut big = vec![0u8; 8 + MAX_PCM_BYTES as usize];
    assert!(PcmWrite { stream: 0, data: &max }.encode(&mut big).is_ok());
    let over = vec![0u8; MAX_PCM_BYTES as usize + 1];
    let mut bigger = vec![0u8; 8 + over.len()];
    assert_eq!(PcmWrite { stream: 0, data: &over }.encode(&mut bigger).unwrap_err().name(), "limit-exceeded");
    // the reply
    let r = PcmReply { accepted: 4096, free: 8192, queued: 4096 };
    let mut rb = [0u8; 16];
    let n = r.encode(&mut rb).unwrap();
    assert_eq!(PcmReply::decode(&rb[..n]).unwrap(), r);
}

#[test]
fn volume_requests_are_bounded_by_aura_s_scale() {
    let v = SetVolume { level: VOLUME_MAX, mute: true };
    let mut b = [0u8; 8];
    let n = v.encode(&mut b).unwrap();
    assert_eq!(SetVolume::decode(&b[..n]).unwrap(), v);
    assert_eq!(SetVolume::decode(&[0xe8, 0x03, 0, 0]).unwrap().level, 1000);
    assert_eq!(SetVolume::decode(&[0xe9, 0x03, 0, 0]).unwrap_err().name(), "limit-exceeded", "1001 is past Aura's scale");
    assert_eq!(SetVolume::decode(&[0, 0, 2, 0]).unwrap_err().name(), "bad-value");
    assert_eq!(SetVolume::decode(&[0, 0, 0, 1]).unwrap_err().name(), "bad-value");
    let r = VolumeReply { dac_code: 0x6f, hp_code: 0x33, muted: false };
    let n = r.encode(&mut b).unwrap();
    assert_eq!(VolumeReply::decode(&b[..n]).unwrap(), r);
}

#[test]
fn events_round_trip() {
    let mut b = [0u8; 64];
    let j = JackEvent { jack: Jack::Headset, buttons: 0b101 };
    let n = j.encode(&mut b).unwrap();
    assert_eq!(JackEvent::decode(&b[..n]).unwrap(), j);
    assert_eq!(JackEvent::decode(&[9, 0, 0, 0]).unwrap_err().name(), "bad-value");
    let p = PeriodEvent { stream: 1, consumed_bytes: 1 << 40, played_frames: (1 << 41) + 5, underruns: 3, free_bytes: 4096 };
    let n = p.encode(&mut b).unwrap();
    assert_eq!(PeriodEvent::decode(&b[..n]).unwrap(), p);
    let ph = PhaseReply { phase: StreamPhase::Suspended, cause: Cause::HeadphoneDisconnected };
    let n = ph.encode(&mut b).unwrap();
    assert_eq!(PhaseReply::decode(&b[..n]).unwrap(), ph);
}

#[test]
fn firmware_names_are_relative_ascii_paths_with_no_dots() {
    for ok in ["skl/dsp_basefw.bin", "da7219-tplg.bin", "nhlt.bin", "skl/dsp_mod_C75061F3-F2B2-4DCC-8F9F-82ABB4131E66.bin"] {
        assert!(check_name(ok.as_bytes()).is_ok(), "{ok}");
    }
    let long = "a".repeat(MAX_FW_NAME + 1);
    for bad in ["", "/etc/passwd", "skl/", "skl//x", "../x", "skl/../x", "skl/./x", "a b", "a\0b", "caf\u{e9}", long.as_str()] {
        assert!(check_name(bad.as_bytes()).is_err(), "{bad:?}");
    }
    assert!(check_name("a".repeat(MAX_FW_NAME).as_bytes()).is_ok());
}

#[test]
fn firmware_requests_round_trip_and_hold_their_bounds() {
    let mut b = [0u8; 256];
    let s = FwStat { name: b"skl/dsp_basefw.bin" };
    let n = s.encode(&mut b).unwrap();
    assert_eq!(FwStat::decode(&b[..n]).unwrap(), s);
    let r = FwRead { name: b"skl/dsp_basefw.bin", offset: 16384, len: FW_SLICE };
    let n = r.encode(&mut b).unwrap();
    assert_eq!(FwRead::decode(&b[..n]).unwrap(), r);
    for bad in [FwRead { len: 0, ..r }, FwRead { len: MAX_FW_SLICE + 1, ..r }] {
        assert_eq!(bad.encode(&mut b).unwrap_err().name(), "limit-exceeded");
    }
    assert!(FwRead { len: MAX_FW_SLICE, ..r }.encode(&mut b).is_ok());
    assert_eq!(FwRead { offset: u64::MAX, ..r }.encode(&mut b).map(|n| FwRead::decode(&b[..n]).unwrap_err().name()), Ok("bad-value"), "offset + len overflows");
    // a decoder re-checks the name even when the encoder was bypassed
    let mut bad = [0u8; 64];
    bad[12..14].copy_from_slice(&2u16.to_le_bytes());
    bad[14..16].copy_from_slice(b"..");
    assert!(FwRead::decode(&bad[..16]).is_err());
}

#[test]
fn firmware_replies_carry_status_then_data() {
    let mut out = vec![0u8; MAX_FRAME_BYTES];
    let n = read_reply(5, Status::Ok, &[1, 2, 3], &mut out).unwrap();
    let (h, p) = Header::decode(&out[..n]).unwrap();
    assert_eq!((h.opcode, h.request_id, h.is_reply()), (Opcode::FwRead, 5, true));
    assert_eq!(split_reply(p).unwrap(), (Status::Ok, &[1u8, 2, 3][..]));
    let n = read_reply(6, Status::FwNotFound, &[], &mut out).unwrap();
    let (_, p) = Header::decode(&out[..n]).unwrap();
    assert_eq!(split_reply(p).unwrap(), (Status::FwNotFound, &[][..]));
    let n = stat_reply(7, Status::Ok, 243_016, &mut out).unwrap();
    let (h, p) = Header::decode(&out[..n]).unwrap();
    assert_eq!(h.opcode, Opcode::FwStat);
    let (st, body) = split_reply(p).unwrap();
    assert_eq!((st, decode_stat_reply(body).unwrap()), (Status::Ok, 243_016));
    let n = stat_reply(8, Status::FwNotFound, 0, &mut out).unwrap();
    let (_, p) = Header::decode(&out[..n]).unwrap();
    assert_eq!(split_reply(p).unwrap(), (Status::FwNotFound, &[][..]), "a refused stat carries no size");
    let too_big = vec![0u8; MAX_FW_SLICE as usize + 1];
    assert!(read_reply(1, Status::Ok, &too_big, &mut out).is_err());
    assert!(read_reply(1, Status::Ok, &vec![0u8; MAX_FW_SLICE as usize], &mut out).is_ok());
}
