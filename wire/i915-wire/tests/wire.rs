// SPDX-License-Identifier: MIT OR Apache-2.0
//! The wire header against hand-written bytes (the layout table in lib.rs), and every refusal.
use i915_wire::*;

/// A frame written out byte by byte: major 1 minor 0, opcode 5 (SubmitBatch), flags 0, request_id 0x0807060504030201,
/// incarnation 0x1112131415161718, payload 3 bytes.
fn frame() -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    v.extend_from_slice(&[0x49, 0x39, 0x31, 0x35]); // magic "I915"
    v.extend_from_slice(&[0x01, 0x00]); // major 1
    v.extend_from_slice(&[0x00, 0x00]); // minor 0
    v.extend_from_slice(&[0x05, 0x00, 0x00, 0x00]); // opcode 5
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // flags
    v.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]); // request_id
    v.extend_from_slice(&[0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11]); // incarnation
    v.extend_from_slice(&[0x03, 0x00, 0x00, 0x00]); // payload_bytes
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // reserved
    v.extend_from_slice(&[0xaa, 0xbb, 0xcc]); // payload
    v
}

#[test]
fn constants_are_the_documented_values() {
    assert_eq!(MAGIC, 0x3531_3949);
    assert_eq!(&MAGIC.to_le_bytes(), b"I915");
    assert_eq!((ABI_MAJOR, ABI_MINOR), (1, 0));
    assert_eq!(HEADER_BYTES, 40);
    assert_eq!(MAX_PAYLOAD, 65536);
    assert_eq!(FLAG_REPLY, 1);
    assert_eq!(FLAGS_KNOWN, FLAG_REPLY);
}

#[test]
fn the_hand_written_frame_decodes_field_by_field() {
    let f = frame();
    let (h, p) = Header::decode(&f).unwrap();
    assert_eq!((h.major, h.minor), (1, 0));
    assert_eq!(h.opcode, Opcode::SubmitBatch);
    assert_eq!(h.flags, 0);
    assert_eq!(h.request_id, 0x0807_0605_0403_0201);
    assert_eq!(h.session_incarnation, 0x1112_1314_1516_1718);
    assert_eq!(h.payload_bytes, 3);
    assert_eq!(p, &[0xaa, 0xbb, 0xcc]);
    assert!(!h.is_reply());
}

#[test]
fn encode_produces_exactly_the_hand_written_bytes() {
    let h = Header::request(Opcode::SubmitBatch, 0x0807_0605_0403_0201, 0x1112_1314_1516_1718, 3);
    let mut out = [0xffu8; 48];
    h.encode(&mut out).unwrap();
    assert_eq!(&out[..40], &frame()[..40]);
    assert_eq!(&out[40..], &[0xff; 8], "encode writes the header and nothing past it");
}

#[test]
fn every_opcode_round_trips_and_the_values_are_pinned() {
    let all = [(0u32, Opcode::Hello), (1, Opcode::GetReadout), (2, Opcode::SetMode), (3, Opcode::PageFlip), (4, Opcode::AllocScanout), (5, Opcode::SubmitBatch)];
    for (n, op) in all {
        assert_eq!(Opcode::from_u32(n), Ok(op));
        assert_eq!(op as u32, n);
        let h = Header::request(op, 7, 9, 0);
        let mut b = [0u8; 40];
        h.encode(&mut b).unwrap();
        assert_eq!(Header::decode(&b).unwrap().0.opcode, op);
    }
    assert_eq!(Opcode::from_u32(6), Err(WireError::UnknownOpcode { got: 6 }));
    assert_eq!(Opcode::from_u32(u32::MAX), Err(WireError::UnknownOpcode { got: u32::MAX }));
}

#[test]
fn every_status_round_trips() {
    for (n, s) in [(0u32, Status::Ok), (1, Status::Unsupported), (2, Status::Malformed), (3, Status::BadVersion), (4, Status::StaleSession)] {
        assert_eq!(Status::from_u32(n), Some(s));
        assert_eq!(s as u32, n);
    }
    assert_eq!(Status::from_u32(5), None);
    assert_ne!(Status::Ok as u32, Status::Unsupported as u32);
}

#[test]
fn a_reply_echoes_the_request_and_sets_the_reply_flag() {
    let rq = Header::request(Opcode::Hello, 42, 7, 0);
    let rp = rq.reply(4);
    assert!(rp.is_reply());
    assert_eq!((rp.opcode, rp.request_id, rp.session_incarnation, rp.payload_bytes), (Opcode::Hello, 42, 7, 4));
    let mut b = [0u8; 44];
    rp.encode(&mut b).unwrap();
    b[40..44].copy_from_slice(&(Status::Ok as u32).to_le_bytes());
    let (h, p) = Header::decode(&b).unwrap();
    assert!(h.is_reply());
    assert_eq!(p, &[0, 0, 0, 0]);
}

#[test]
fn a_minor_newer_than_ours_is_accepted_a_major_is_not() {
    let mut f = frame();
    f[6] = 9; // minor 9
    assert_eq!(Header::decode(&f).unwrap().0.minor, 9);
    f[4] = 2; // major 2
    let e = Header::decode(&f).unwrap_err();
    assert_eq!(e, WireError::UnsupportedMajor { got: 2 });
    assert_eq!(e.status(), Status::BadVersion);
    f[4] = 0;
    assert_eq!(Header::decode(&f).unwrap_err(), WireError::UnsupportedMajor { got: 0 });
}

#[test]
fn each_refusal_is_named_and_carries_the_value() {
    let ok = frame();
    let mut f = ok.clone();
    f[0] = b'X';
    let e = Header::decode(&f).unwrap_err();
    assert_eq!(e, WireError::BadMagic { got: 0x3531_3958 });
    assert_eq!((e.name(), e.status()), ("WIRE-BAD-MAGIC", Status::Malformed));

    assert_eq!(Header::decode(&ok[..39]).unwrap_err(), WireError::ShortBuffer { need: 40, have: 39 });
    assert_eq!(Header::decode(&[]).unwrap_err().name(), "WIRE-SHORT-BUFFER");

    let mut f = ok.clone();
    f[8] = 77;
    assert_eq!(Header::decode(&f).unwrap_err(), WireError::UnknownOpcode { got: 77 });

    let mut f = ok.clone();
    f[12] = 0x02; // an unknown flag bit
    assert_eq!(Header::decode(&f).unwrap_err(), WireError::UnknownFlags { got: 2 });
    f[12] = 0x03; // REPLY plus an unknown bit
    assert_eq!(Header::decode(&f).unwrap_err(), WireError::UnknownFlags { got: 3 });
    f[12] = 0x01; // REPLY alone is fine
    assert!(Header::decode(&f).unwrap().0.is_reply());

    let mut f = ok.clone();
    f[36] = 1;
    assert_eq!(Header::decode(&f).unwrap_err(), WireError::ReservedNonZero { got: 1 });

    let mut f = ok.clone();
    f[32..36].copy_from_slice(&(MAX_PAYLOAD + 1).to_le_bytes());
    assert_eq!(Header::decode(&f).unwrap_err(), WireError::PayloadTooLarge { got: MAX_PAYLOAD + 1, max: MAX_PAYLOAD });
    f[32..36].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(Header::decode(&f).unwrap_err().name(), "WIRE-PAYLOAD-TOO-LARGE");
}

#[test]
fn the_payload_must_be_exactly_the_declared_length() {
    let f = frame();
    assert_eq!(Header::decode(&f[..42]).unwrap_err(), WireError::PayloadLengthMismatch { declared: 3, actual: 2 });
    let mut longer = f.clone();
    longer.push(0);
    let e = Header::decode(&longer).unwrap_err();
    assert_eq!(e, WireError::PayloadLengthMismatch { declared: 3, actual: 4 });
    assert_eq!(e.name(), "WIRE-PAYLOAD-LENGTH-MISMATCH");
    // decode_header alone does not look past the header
    assert!(Header::decode_header(&longer).is_ok());
    // the maximum payload is legal
    let mut big = vec![0u8; 40 + MAX_PAYLOAD as usize];
    Header::request(Opcode::Hello, 1, 1, MAX_PAYLOAD).encode(&mut big).unwrap();
    assert!(Header::decode(&big).is_ok());
}

#[test]
fn encode_refuses_a_short_buffer_and_an_oversize_payload() {
    let h = Header::request(Opcode::Hello, 1, 1, 0);
    assert_eq!(h.encode(&mut [0u8; 39]), Err(WireError::ShortBuffer { need: 40, have: 39 }));
    let big = Header::request(Opcode::Hello, 1, 1, MAX_PAYLOAD + 1);
    assert_eq!(big.encode(&mut [0u8; 40]), Err(WireError::PayloadTooLarge { got: MAX_PAYLOAD + 1, max: MAX_PAYLOAD }));
}
