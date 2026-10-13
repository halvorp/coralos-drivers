// SPDX-License-Identifier: MIT OR Apache-2.0
//! The payload region (ABI 1.2, reply region 1.3): the reference layout, every bound, the flag-vs-minor rule, and that a reply does not inherit it.
use i915_wire::region::*;
use i915_wire::*;

/// The layout written out by hand, byte for byte (docs in region.rs): region 7, layout 1, offset 0x1000, len 100_000, cap 200_000,
/// reply_offset 0x40_0000, reply_region 0, reply_flags 0 (the 1.2 reserved zero).
fn bytes() -> Vec<u8> {
    let mut v = vec![];
    v.extend_from_slice(&[7, 0, 0, 0]);
    v.extend_from_slice(&[1, 0, 0, 0]);
    v.extend_from_slice(&[0x00, 0x10, 0, 0, 0, 0, 0, 0]);
    v.extend_from_slice(&[0xa0, 0x86, 0x01, 0]);
    v.extend_from_slice(&[0x40, 0x0d, 0x03, 0]);
    v.extend_from_slice(&[0, 0, 0x40, 0, 0, 0, 0, 0]);
    v.extend_from_slice(&[0; 8]);
    v
}

#[test]
fn the_documented_constants_and_layout() {
    assert_eq!((REGION_SINCE_MINOR, REGION_LAYOUT, REGION_REF_BYTES, MAX_REGION_PAYLOAD), (2, 1, 40, 4 << 20));
    assert!(MAX_REGION_PAYLOAD > MAX_PAYLOAD, "a region exists to carry what an inline frame cannot");
    let r = RegionRef::decode(&bytes()).unwrap();
    assert_eq!(r, RegionRef { region: 7, offset: 0x1000, len: 100_000, reply_cap: 200_000, reply_offset: 0x40_0000, reply_region: 7 });
    let mut out = [0u8; 64];
    assert_eq!(r.encode(&mut out).unwrap(), REGION_REF_BYTES);
    assert_eq!(&out[..40], &bytes()[..], "encode writes the hand-written bytes");
}

#[test]
fn every_bound_is_enforced_by_name() {
    let with = |off: usize, v: &[u8]| {
        let mut b = bytes();
        b[off..off + v.len()].copy_from_slice(v);
        b
    };
    assert_eq!(RegionRef::decode(&bytes()[..39]).unwrap_err().name(), "WIRE-TRUNCATED");
    let mut long = bytes();
    long.push(0);
    assert_eq!(RegionRef::decode(&long).unwrap_err(), WireError::TrailingBytes { extra: 1 });
    assert_eq!(RegionRef::decode(&with(4, &[2, 0, 0, 0])).unwrap_err(), WireError::BadValue { what: "region.layout", got: 2 });
    assert_eq!(RegionRef::decode(&with(16, &[0; 4])).unwrap_err(), WireError::BadValue { what: "region.len", got: 0 });
    let over = (MAX_REGION_PAYLOAD + 1).to_le_bytes();
    assert_eq!(RegionRef::decode(&with(16, &over)).unwrap_err(), WireError::LimitExceeded { what: "region.len", got: MAX_REGION_PAYLOAD as u64 + 1, max: MAX_REGION_PAYLOAD as u64 });
    assert_eq!(RegionRef::decode(&with(20, &over)).unwrap_err().name(), "WIRE-LIMIT-EXCEEDED");
    let at = MAX_REGION_PAYLOAD.to_le_bytes();
    assert!(RegionRef::decode(&with(16, &at)).is_ok() && RegionRef::decode(&with(20, &at)).is_ok(), "the limit itself is allowed");
    assert_eq!(RegionRef::decode(&with(8, &[0xff; 8])).unwrap_err().name(), "WIRE-BAD-VALUE", "offset + len wraps");
    assert_eq!(RegionRef::decode(&with(24, &[0xff; 8])).unwrap_err().name(), "WIRE-BAD-VALUE", "reply_offset + reply_cap wraps");
    assert_eq!(RegionRef::decode(&with(32, &[1])).unwrap_err().name(), "WIRE-FIELD-RESERVED-NONZERO", "a reply region without REPLY_ELSEWHERE");
    assert_eq!(RegionRef::decode(&with(36, &[2])).unwrap_err().name(), "WIRE-FIELD-RESERVED-NONZERO", "an unknown reply flag");
}

#[test]
fn a_reply_can_live_in_its_own_region_and_a_1_2_frame_still_means_the_same_region() {
    assert_eq!((REPLY_ELSEWHERE, REPLY_REGION_SINCE_MINOR), (1, 3));
    let mut b = bytes();
    b[32..36].copy_from_slice(&11u32.to_le_bytes());
    b[36..40].copy_from_slice(&1u32.to_le_bytes());
    let r = RegionRef::decode(&b).unwrap();
    assert_eq!((r.region, r.reply_region), (7, 11));
    let mut out = [0u8; 40];
    r.encode(&mut out).unwrap();
    assert_eq!(&out[..], &b[..], "a separate reply region round-trips byte for byte");
    // reply region 0 is a real region id when the flag says so
    let mut z = bytes();
    z[36..40].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(RegionRef::decode(&z).unwrap().reply_region, 0);
    // the 1.2 bytes decode as "the same region" and re-encode to the same bytes
    let old = RegionRef::decode(&bytes()).unwrap();
    assert_eq!(old.reply_region, old.region);
    old.encode(&mut out).unwrap();
    assert_eq!(&out[..], &bytes()[..]);
}

#[test]
fn the_flag_needs_minor_2_and_a_reply_does_not_inherit_it() {
    let mut f = vec![0u8; HEADER_BYTES + REGION_REF_BYTES];
    let mut h = Header::request(Opcode::Execbuffer2, 5, 9, REGION_REF_BYTES as u32);
    h.flags = FLAG_PAYLOAD_REGION;
    h.encode(&mut f).unwrap();
    f[HEADER_BYTES..].copy_from_slice(&bytes());
    let (d, p) = Header::decode(&f).unwrap();
    assert!(d.is_region() && !d.is_reply());
    assert_eq!(RegionRef::decode(p).unwrap().len, 100_000);
    f[6] = 1; // a minor-1 frame cannot use a minor-2 flag
    assert_eq!(Header::decode(&f).unwrap_err(), WireError::FlagNewerThanMinor { flag: FLAG_PAYLOAD_REGION, since: 2, got: 1 });
    // the inline limit still holds for an inline frame, region flag or not
    let mut big = Header::request(Opcode::Execbuffer2, 1, 1, MAX_PAYLOAD + 1);
    big.flags = 0;
    let mut buf = vec![0u8; HEADER_BYTES];
    assert_eq!(big.encode(&mut buf).unwrap_err(), WireError::PayloadTooLarge { got: MAX_PAYLOAD + 1, max: MAX_PAYLOAD });
    // replies
    let rq = d;
    assert!(!rq.reply(8).is_region(), "an inline reply to a region request is inline");
    let rr = rq.reply_in_region();
    assert!(rr.is_region() && rr.is_reply());
    assert_eq!(rr.payload_bytes as usize, REGION_REF_BYTES);
}
