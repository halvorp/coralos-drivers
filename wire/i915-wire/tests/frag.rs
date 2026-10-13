// SPDX-License-Identifier: MIT OR Apache-2.0
//! The framing rule (src/frag.rs): where a frame ends, the message-count boundary, and every named refusal of a bad sequence.
//! Expected numbers are written out LITERALLY (kernel EP_CAP 4096, EP_DEPTH 16), never computed from the constants under test.
use i915_wire::frag::{split, Feed, FragError, Joiner, Limits};
use i915_wire::{Header, Opcode, WireError, HEADER_BYTES};

/// A well-formed frame of exactly `total` bytes (a HELLO carrying `total - 40` payload bytes, filled with a position-dependent byte).
fn frame_of(total: usize) -> Vec<u8> {
    assert!(total >= HEADER_BYTES);
    let mut f = vec![0u8; total];
    for (i, b) in f[HEADER_BYTES..].iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    Header::request(Opcode::Hello, 7, 3, (total - HEADER_BYTES) as u32).encode(&mut f).unwrap();
    f
}

/// Join messages the way a receiver does, with the bytes kept in a Vec.
fn join(msgs: &[&[u8]]) -> Result<Vec<u8>, FragError> {
    let mut j = Joiner::new(Limits::KERNEL);
    let mut buf = Vec::new();
    for m in msgs {
        match j.feed(m)? {
            Feed::Whole => return Ok(m.to_vec()),
            Feed::Begin { total } => {
                buf.reserve(total);
                buf.extend_from_slice(m);
            }
            Feed::Middle => buf.extend_from_slice(m),
            Feed::End => {
                buf.extend_from_slice(m);
                return Ok(buf);
            }
        }
    }
    panic!("the sequence never completed ({} bytes kept)", buf.len());
}

fn sizes(total: usize) -> Vec<usize> {
    let f = frame_of(total);
    split(&f, Limits::KERNEL).unwrap().map(|m| m.len()).collect()
}

#[test]
fn the_kernel_geometry_is_the_kernels() {
    // kernel/src/arch/x86_64/endpoint.rs: EP_CAP = 4096 (coral_firmament_abi::EP_CAP), EP_DEPTH = 16 slots per ring
    assert_eq!(Limits::KERNEL.msg_cap, 4096);
    assert_eq!(Limits::KERNEL.max_msgs, 15, "one ring slot of 16 stays free: a full ring drops the newest");
    assert_eq!(Limits::KERNEL.max_frame_bytes(), 61440);
}

#[test]
fn a_frame_ends_exactly_where_the_message_boundary_falls() {
    assert_eq!(sizes(40), vec![40]);
    assert_eq!(sizes(4095), vec![4095]);
    assert_eq!(sizes(4096), vec![4096], "exactly one full message is one message, no empty terminator");
    assert_eq!(sizes(4097), vec![4096, 1], "one byte more is a second message");
    assert_eq!(sizes(8192), vec![4096, 4096]);
    assert_eq!(sizes(8193), vec![4096, 4096, 1]);
}

#[test]
fn fifteen_messages_travel_and_sixteen_are_refused() {
    let f = frame_of(61440);
    let c = split(&f, Limits::KERNEL).unwrap();
    assert_eq!(c.messages(), 15);
    let msgs: Vec<&[u8]> = c.collect();
    assert_eq!(msgs.len(), 15);
    assert!(msgs.iter().all(|m| m.len() == 4096));
    assert_eq!(join(&msgs).unwrap(), f, "fifteen full messages join to the frame");

    // one byte more would need a sixteenth message: refused when cutting, by name, with nothing cut
    let big = frame_of(61441);
    assert_eq!(split(&big, Limits::KERNEL).unwrap_err(), FragError::TooManyMessages { needed: 16, max: 15 });
    assert_eq!(FragError::TooManyMessages { needed: 16, max: 15 }.name(), "FRAG-TOO-MANY-MESSAGES");

    // and refused when RECEIVING: the first message declares more than 15 messages can carry, so nothing is kept
    let mut j = Joiner::new(Limits::KERNEL);
    assert_eq!(j.feed(&big[..4096]).unwrap_err(), FragError::TooManyMessages { needed: 16, max: 15 });
    assert!(!j.in_progress());

    // a sixteenth message after a complete 15-message frame is a NEW frame (garbage), not a continuation
    let mut j = Joiner::new(Limits::KERNEL);
    for (i, m) in msgs.iter().enumerate() {
        let r = j.feed(m).unwrap();
        assert_eq!(r, if i == 0 { Feed::Begin { total: 61440 } } else if i == 14 { Feed::End } else { Feed::Middle }, "message {i}");
    }
    assert!(matches!(j.feed(&[0u8; 4096]), Err(FragError::Header(WireError::BadMagic { .. }))));

    // a maximal payload (64 KiB) cannot fragment at all: it needs 17 messages, and goes through a payload region instead
    let max = frame_of(HEADER_BYTES + 65536);
    assert_eq!(split(&max, Limits::KERNEL).unwrap_err(), FragError::TooManyMessages { needed: 17, max: 15 });
}

#[test]
fn every_size_round_trips_byte_for_byte() {
    let mut total = 40;
    while total <= 61440 {
        let f = frame_of(total);
        let c = split(&f, Limits::KERNEL).unwrap();
        let n = c.messages();
        let msgs: Vec<&[u8]> = c.collect();
        assert_eq!(msgs.len(), n, "count() is the number of messages produced, at {total}");
        assert_eq!(n, (total + 4095) / 4096, "at {total}");
        assert!(msgs[..n - 1].iter().all(|m| m.len() == 4096), "every message but the last is full, at {total}");
        assert_eq!(join(&msgs).unwrap(), f, "at {total}");
        // step through the boundaries densely and the middle sparsely
        total += if total % 4096 > 4090 || total % 4096 < 6 { 1 } else { 997 };
    }
}

#[test]
fn a_short_first_message_is_refused_by_name() {
    let f = frame_of(100);
    let mut j = Joiner::new(Limits::KERNEL);
    assert_eq!(j.feed(&f[..39]).unwrap_err(), FragError::FirstShort { got: 39 });
    assert_eq!(j.feed(&[]).unwrap_err(), FragError::FirstShort { got: 0 });
    assert_eq!(FragError::FirstShort { got: 39 }.name(), "FRAG-FIRST-SHORT");
    // a first message that is long enough to read the header but stops short of its declared end without filling a message
    let big = frame_of(5000);
    assert_eq!(j.feed(&big[..2000]).unwrap_err(), FragError::ShortFragment { index: 0, got: 2000, cap: 4096 });
    assert!(!j.in_progress());
}

#[test]
fn a_sequence_that_stops_early_is_refused_when_a_short_message_arrives_mid_sequence() {
    let f = frame_of(10000); // 4096 + 4096 + 1808
    let msgs: Vec<&[u8]> = split(&f, Limits::KERNEL).unwrap().collect();
    let mut j = Joiner::new(Limits::KERNEL);
    assert_eq!(j.feed(msgs[0]).unwrap(), Feed::Begin { total: 10000 });
    assert!(j.in_progress());
    assert_eq!((j.received(), j.declared()), (4096, 10000));
    // the second message is cut short (1000 of 4096) while 5904 bytes are still owed
    let e = j.feed(&msgs[1][..1000]).unwrap_err();
    assert_eq!(e, FragError::ShortFragment { index: 1, got: 1000, cap: 4096 });
    assert_eq!(e.name(), "FRAG-SHORT-FRAGMENT");
    assert!(!j.in_progress(), "the partial frame is forgotten");
    // an empty message mid-sequence is short too
    let mut j = Joiner::new(Limits::KERNEL);
    j.feed(msgs[0]).unwrap();
    assert_eq!(j.feed(&[]).unwrap_err(), FragError::ShortFragment { index: 1, got: 0, cap: 4096 });
}

#[test]
fn a_message_that_runs_past_the_declared_end_is_refused() {
    let f = frame_of(5000); // 4096 + 904
    let msgs: Vec<&[u8]> = split(&f, Limits::KERNEL).unwrap().collect();
    let mut j = Joiner::new(Limits::KERNEL);
    j.feed(msgs[0]).unwrap();
    let mut long = msgs[1].to_vec();
    long.push(0);
    let e = j.feed(&long).unwrap_err();
    assert_eq!(e, FragError::Overlong { declared: 5000, got: 5001 });
    assert_eq!(e.name(), "FRAG-OVERLONG");
    // a single message longer than its own header declares
    let mut one = frame_of(100);
    one.push(0);
    assert_eq!(Joiner::new(Limits::KERNEL).feed(&one).unwrap_err(), FragError::Overlong { declared: 100, got: 101 });
}

#[test]
fn two_frames_interleaved_are_refused_by_name_and_the_next_frame_starts_clean() {
    let a = frame_of(9000);
    let b = frame_of(64); // a complete small frame arrives while `a` is still owed bytes
    let am: Vec<&[u8]> = split(&a, Limits::KERNEL).unwrap().collect();
    let mut j = Joiner::new(Limits::KERNEL);
    j.feed(am[0]).unwrap();
    let e = j.feed(&b).unwrap_err();
    assert_eq!(e, FragError::Interleaved { index: 1 });
    assert_eq!(e.name(), "FRAG-INTERLEAVED");
    assert!(!j.in_progress());
    // after the refusal the stream is between frames: a new frame is taken whole
    assert_eq!(j.feed(&b).unwrap(), Feed::Whole);
    // a bad frame never leaves a half-state behind either
    let mut j = Joiner::new(Limits::KERNEL);
    j.feed(am[0]).unwrap();
    assert!(j.feed(&b[..10]).is_err());
    assert_eq!(join(&split(&a, Limits::KERNEL).unwrap().collect::<Vec<_>>()).unwrap(), a);
}

#[test]
fn a_header_the_wire_refuses_is_refused_with_the_wires_own_name() {
    let mut f = frame_of(100);
    f[0] = b'X';
    let e = Joiner::new(Limits::KERNEL).feed(&f).unwrap_err();
    assert!(matches!(e, FragError::Header(WireError::BadMagic { .. })));
    assert_eq!(e.name(), "WIRE-BAD-MAGIC");
    // a payload past the wire's own bound
    let mut f = frame_of(100);
    f[32..36].copy_from_slice(&(65537u32).to_le_bytes());
    let e = Joiner::new(Limits::KERNEL).feed(&f).unwrap_err();
    assert!(matches!(e, FragError::Header(WireError::PayloadTooLarge { got: 65537, .. })));
    // cutting refuses a frame whose bytes are not what the header declares
    let mut short = frame_of(100);
    short.pop();
    assert!(matches!(split(&short, Limits::KERNEL), Err(FragError::Header(WireError::PayloadLengthMismatch { .. }))));
    assert!(matches!(split(&[], Limits::KERNEL), Err(FragError::Header(WireError::ShortBuffer { .. }))));
}

#[test]
fn every_refusal_maps_to_a_malformed_wire_error_with_a_name() {
    for e in [
        FragError::FirstShort { got: 3 },
        FragError::TooManyMessages { needed: 16, max: 15 },
        FragError::ShortFragment { index: 1, got: 5, cap: 4096 },
        FragError::Interleaved { index: 2 },
        FragError::Overlong { declared: 100, got: 101 },
    ] {
        assert_eq!(e.wire().status(), i915_wire::Status::Malformed, "{}", e.name());
        assert!(!e.name().is_empty());
    }
}

#[test]
fn smaller_limits_move_the_boundary() {
    // the same code with a 64-byte message and 3 messages: 192 bytes is the largest frame
    let l = Limits { msg_cap: 64, max_msgs: 3 };
    let ok = frame_of(192);
    assert_eq!(split(&ok, l).unwrap().map(|m| m.len()).collect::<Vec<_>>(), vec![64, 64, 64]);
    let over = frame_of(193);
    assert_eq!(split(&over, l).unwrap_err(), FragError::TooManyMessages { needed: 4, max: 3 });
}
