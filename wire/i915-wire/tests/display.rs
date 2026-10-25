// SPDX-License-Identifier: MIT OR Apache-2.0
//! ABI 1.5, display ownership: the DISPLAY-QUERY reply laid out byte by byte, the body sizes, the refusal codes (numbers AND
//! names written out by hand), and the minor rule for the new opcodes.
use i915_wire::msg::display::{self, *};
use i915_wire::msg::Body;
use i915_wire::*;

#[test]
fn display_query_is_laid_out_field_by_field() {
    // the Pixelbook Go: adopted, pipe A, plane 0, 1920x1080, stride 7680, 'XR24', linear, panel 3840x2160, 60000 mHz, scaled, one mode
    let rep = DisplayQueryRep {
        adopted: display::state::ADOPTED,
        refusal: display::refusal::NONE,
        pipe: 0,
        plane: 0,
        width: 1920,
        height: 1080,
        stride: 7680,
        fourcc: 0x3432_5258,
        modifier: 0x0100_0000_0000_0001,
        active_width: 3840,
        active_height: 2160,
        refresh_millihz: 60_000,
        scaled: 1,
        mode_count: 1,
        ..Default::default()
    };
    let mut o = [0u8; 64];
    assert_eq!(rep.to_payload(&mut o).unwrap(), 64);
    let w = |i: usize| u32::from_le_bytes([o[i], o[i + 1], o[i + 2], o[i + 3]]);
    assert_eq!([w(0), w(4), w(8), w(12), w(16), w(20), w(24), w(28)], [1, 0, 0, 0, 1920, 1080, 7680, 0x3432_5258]);
    assert_eq!(&o[32..40], &[1, 0, 0, 0, 0, 0, 0, 1], "modifier, little-endian u64");
    assert_eq!([w(40), w(44), w(48), w(52), w(56), w(60)], [3840, 2160, 60_000, 1, 1, 0]);
    assert_eq!(DisplayQueryRep::from_payload(&o), Ok(rep));
    let mut bad = o;
    bad[60] = 1;
    assert!(matches!(DisplayQueryRep::from_payload(&bad), Err(WireError::FieldReservedNonZero { .. })), "the pad is checked");
}

#[test]
fn the_body_sizes_are_the_wire_sizes() {
    assert_eq!(<DisplayQueryRep as Body>::SIZE, 64);
    assert_eq!(<DisplayModeReq as Body>::SIZE, 16);
    assert_eq!(<ScanoutAcquireRep as Body>::SIZE, 40);
    assert_eq!((<ScanoutBeginReq as Body>::SIZE, <ScanoutPresentReq as Body>::SIZE, <ScanoutPresentRep as Body>::SIZE), (8, 8, 8));
    assert_eq!((<ScanoutWaitReq as Body>::SIZE, <ScanoutStatusRep as Body>::SIZE), (16, 24));
    assert_eq!(display::SCANOUT_IMAGES, 2);
    assert_eq!((display::state::NOT_ADOPTED, display::state::ADOPTED), (0, 1));
}

#[test]
fn the_refusal_codes_are_pinned_by_number_and_name() {
    let table: [(u32, &str); 16] = [
        (0, "NONE"),
        (1, "ADOPT-NO-ACTIVE-PIPE"),
        (2, "ADOPT-MULTIPLE-ACTIVE-PIPES"),
        (3, "ADOPT-PLANE-UNREADABLE"),
        (4, "ADOPT-PLANE-DISABLED"),
        (5, "ADOPT-FORMAT-NOT-RGB"),
        (6, "ADOPT-COMPRESSED"),
        (7, "ADOPT-ROTATED"),
        (8, "ADOPT-OFFSET-NONZERO"),
        (9, "ADOPT-STRIDE-UNKNOWN"),
        (10, "ADOPT-PLANE-NOT-PIPE-SOURCE"),
        (11, "ADOPT-PLANE-SCALER"),
        (12, "ADOPT-UNSCALED-SIZE-MISMATCH"),
        (13, "ADOPT-INTERLACED"),
        (14, "ADOPT-EXTENT-OVERFLOW"),
        (15, "ADOPT-READOUT-FAILED"),
    ];
    assert_eq!(display::refusal::ALL.len(), table.len());
    for (i, (code, name)) in table.iter().enumerate() {
        assert_eq!(display::refusal::ALL[i], *code, "ALL is in code order");
        assert_eq!(display::refusal::name(*code), *name);
    }
    assert_eq!(display::refusal::name(16), "UNKNOWN");
}

#[test]
fn a_display_call_under_minor_4_is_refused() {
    let mut buf = [0u8; HEADER_BYTES];
    let mut h = Header::request(Opcode::DisplayQuery, 1, 0, 0);
    h.minor = 4;
    h.encode(&mut buf).unwrap();
    let e = Header::decode_header(&buf).unwrap_err();
    assert_eq!(e, WireError::OpcodeNewerThanMinor { opcode: 0x170, since: 5, got: 4 });
    assert_eq!((e.status(), e.errno()), (Status::Malformed, errno::EINVAL));
    h.minor = 5;
    h.encode(&mut buf).unwrap();
    assert!(Header::decode_header(&buf).is_ok());
}
