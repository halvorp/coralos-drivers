// SPDX-License-Identifier: GPL-2.0-only
mod common;
use common::*;
use i915_firmware_core::format::*;

const MAX: u32 = 0x3000; // KBL_DMC_MAX_FW_SIZE, intel_dmc.c:177,185

#[test]
fn layout_sizes_are_the_c_struct_sizes() {
    // intel_dmc.c:262-396 (packed structs): css 32 dwords, package 16, fw_info 12, header base 20,
    // v1 = 20+4+32+32+32+8 = 128, v3 = 20+4+36+32+4+80+80 = 256
    assert_eq!(CSS_HEADER_SIZE, 128);
    assert_eq!(PACKAGE_HEADER_SIZE, 16);
    assert_eq!(FW_INFO_SIZE, 12);
    assert_eq!(DMC_HEADER_BASE_SIZE, 20);
    assert_eq!(DMC_HEADER_V1_SIZE, 128);
    assert_eq!(DMC_HEADER_V3_SIZE, 256);
    // intel_dmc.c:254-258, intel_dmc_regs.h:506-508
    assert_eq!(PACKAGE_MAX_FW_INFO_ENTRIES, 20);
    assert_eq!(PACKAGE_V2_MAX_FW_INFO_ENTRIES, 32);
    assert_eq!(DMC_V1_MAX_MMIO_COUNT, 8);
    assert_eq!(DMC_V3_MAX_MMIO_COUNT, 20);
    assert_eq!(DMC_V1_MMIO_START_RANGE, 0x80000);
    assert_eq!(DMC_MMIO_START_RANGE, 0x80000);
    assert_eq!(DMC_MMIO_END_RANGE, 0x8FFFF);
    assert_eq!(DMC_FW_MAX, 5);
}

#[test]
fn dmc_id_family() {
    // intel_dmc.c:55-61: MAIN=0, PIPEA..PIPED=1..4; 5 is not valid (is_valid_dmc_id, :405)
    assert_eq!(DmcId::from_u8(0), Some(DmcId::Main));
    assert_eq!(DmcId::from_u8(1), Some(DmcId::PipeA));
    assert_eq!(DmcId::from_u8(2), Some(DmcId::PipeB));
    assert_eq!(DmcId::from_u8(3), Some(DmcId::PipeC));
    assert_eq!(DmcId::from_u8(4), Some(DmcId::PipeD));
    assert_eq!(DmcId::from_u8(5), None);
    assert_eq!(DmcId::from_u8(255), None);
    assert_eq!(DmcId::Main as u8, 0);
    assert_eq!(DmcId::PipeD as u8, 4);
}

#[test]
fn parses_a_synthetic_v1_package() {
    let fw = simple(0x0001_0004, &[(0x8F004, 0x1111), (0x8F034, 0xc003b400)], &[0xAABBCCDD, 1, 2]);
    let p = parse_package(&fw, Stepping::WILDCARD, MAX).unwrap();
    assert_eq!(p.version, 0x0001_0004);
    assert_eq!(p.images.len(), 1);
    let m = p.image(DmcId::Main).unwrap();
    assert_eq!(m.header_ver, 1);
    assert_eq!(m.start_mmioaddr, 0x80000);
    assert_eq!(m.payload, vec![0xAABBCCDD, 1, 2]);
    assert_eq!(m.mmio, vec![(0x8F004, 0x1111), (0x8F034, 0xc003b400)]);
    assert_eq!(m.fw_version, 0x0001_0004);
}

#[test]
fn stepping_match_family() {
    // fw_info_matches_stepping(), intel_dmc.c:966-985, one input per clause and the refusing case
    let w = Stepping::WILDCARD;
    let b0 = Stepping { stepping: b'B', substepping: b'0' };
    // clause 1: fw sub '*' and stepping equal
    assert!(fw_info_matches_stepping(b'B', b'*', b0));
    assert!(!fw_info_matches_stepping(b'C', b'*', b0));
    // clause 2: both equal
    assert!(fw_info_matches_stepping(b'B', b'0', b0));
    assert!(!fw_info_matches_stepping(b'B', b'1', b0));
    // clause 3: si stepping wildcard and substepping equal
    assert!(fw_info_matches_stepping(b'A', b'*', w)); // also clause 4? no: fw_stepping != '*'; matches via clause 3 ('*' sub)
    assert!(fw_info_matches_stepping(b'A', b'*', Stepping { stepping: b'*', substepping: b'*' }));
    assert!(!fw_info_matches_stepping(b'A', b'0', w)); // si.sub '*' != fw.sub '0'
    assert!(fw_info_matches_stepping(b'A', b'0', Stepping { stepping: b'*', substepping: b'0' }));
    // clause 4: fw wildcard/wildcard
    assert!(fw_info_matches_stepping(b'*', b'*', b0));
    assert!(!fw_info_matches_stepping(b'*', b'0', b0));
}

#[test]
fn first_matching_entry_wins_and_unmatched_is_skipped() {
    // dmc_set_fw_offset(), intel_dmc.c:987-1017: "More specific versions come first"
    let mut fw = css(1, 32);
    fw.extend(package(1, &[(0, b'B', b'0', 0), (0, b'*', b'*', 100)], None)); // v1: id ignored -> MAIN
    fw.extend(dmc_v1(&[], &[7], None, None));
    let p = parse_package(&fw, Stepping { stepping: b'B', substepping: b'0' }, MAX).unwrap();
    assert_eq!(p.image(DmcId::Main).unwrap().payload, vec![7]);
    // a stepping that matches only the second (wildcard) entry points at offset 100 dwords: beyond the blob
    let mut fw2 = css(1, 32);
    fw2.extend(package(1, &[(0, b'B', b'0', 0), (0, b'*', b'*', 100)], None));
    fw2.extend(dmc_v1(&[], &[7], None, None));
    let e = parse_package(&fw2, Stepping { stepping: b'C', substepping: b'0' }, MAX).unwrap_err();
    assert_eq!(e, DmcError::OffsetBeyondFirmware { offset_dwords: 100, size: fw2.len() });
}

#[test]
fn package_v2_uses_the_dmc_id_and_32_slots() {
    // header_ver 2: id from fw_info (intel_dmc.c:998), 32 entries (:255). Pipe images are parsed but v1-range
    // sanity is the only range KBL knows; a v2 package whose main is first still yields MAIN.
    let mut fw = css(1, 32);
    fw.extend(package(2, &[(0, b'*', b'*', 0), (9, b'*', b'*', 0)], None)); // id 9 is invalid -> skipped
    fw.extend(dmc_v1(&[], &[1, 2], None, None));
    let p = parse_package(&fw, Stepping::WILDCARD, MAX).unwrap();
    assert_eq!(p.images.len(), 1);
    assert_eq!(p.images[0].id, DmcId::Main);
}

#[test]
fn refusals_are_named() {
    let ok = simple(1, &[(0x8F004, 1)], &[1, 2]);
    let w = Stepping::WILDCARD;
    // truncated css
    assert_eq!(parse_package(&ok[..100], w, MAX), Err(DmcError::Truncated { what: "css header", need: 128, have: 100 }));
    // css header_len wrong (intel_dmc.c:1261)
    let mut b = ok.clone(); b[4] = 31;
    assert_eq!(parse_package(&b, w, MAX), Err(DmcError::WrongCssHeaderLen { bytes: 124 }));
    // truncated package header
    assert!(matches!(parse_package(&ok[..130], w, MAX), Err(DmcError::Truncated { what: "package header", .. })));
    // package version 3 unknown (:1211)
    let mut b = ok.clone(); b[128 + 1] = 3;
    assert_eq!(parse_package(&b, w, MAX), Err(DmcError::UnknownPackageVersion { ver: 3 }));
    // package table truncated
    assert!(matches!(parse_package(&ok[..128 + 100], w, MAX), Err(DmcError::Truncated { what: "package fw_info table", .. })));
    // package header_len wrong (:1225): v1 expects (16+240)/4 = 64 dwords
    let mut b = ok.clone(); b[128] = 63;
    assert_eq!(parse_package(&b, w, MAX), Err(DmcError::WrongPackageHeaderLen { bytes: 252, expected: 256 }));
    // num_entries > 20 (:1231)
    let mut b = ok.clone(); b[128 + 12] = 21;
    assert_eq!(parse_package(&b, w, MAX), Err(DmcError::TooManyEntries { num: 21, max: 20 }));
    // no entry matches -> MainProgramMissing (:1318)
    let mut fw = css(1, 32); fw.extend(package(1, &[(0, b'B', b'0', 0)], None)); fw.extend(dmc_v1(&[], &[1], None, None));
    assert_eq!(parse_package(&fw, Stepping { stepping: b'C', substepping: b'1' }, MAX), Err(DmcError::MainProgramMissing));
    let off = 128 + 256; // start of the DMC header in `ok`
    // unknown dmc header version (:1104)
    let mut b = ok.clone(); b[off + 5] = 2;
    assert_eq!(parse_package(&b, w, MAX), Err(DmcError::UnknownHeaderVersion { ver: 2 }));
    // wrong v1 header length in bytes (:1110)
    let mut b = ok.clone(); b[off + 4] = 127;
    assert_eq!(parse_package(&b, w, MAX), Err(DmcError::WrongDmcHeaderLen { bytes: 127, expected: 128 }));
    // v3 header_len is DWORDS: 128 dwords = 512 bytes != 256
    let mut b = ok.clone(); b[off + 5] = 3; b[off + 4] = 128;
    assert!(matches!(parse_package(&b, w, MAX), Err(DmcError::Truncated { what: "dmc header v3", .. } | DmcError::WrongDmcHeaderLen { .. })));
    // mmio_count 9 > 8 (:1117)
    let mut fw = css(1, 32); fw.extend(package(1, &[(0, b'*', b'*', 0)], None)); fw.extend(dmc_v1(&[], &[1], Some(9), None));
    assert_eq!(parse_package(&fw, w, MAX), Err(DmcError::WrongMmioCount { count: 9, max: 8 }));
    // mmio below 0x80000 and above 0x8FFFF (:1028-1029, :1123)
    for bad in [0x7FFFFu32, 0x90000] {
        let fw = simple(1, &[(0x8F004, 1), (bad, 2)], &[1]);
        assert_eq!(parse_package(&fw, w, MAX), Err(DmcError::MmioAddrOutOfRange { addr: bad, start: 0x80000, end: 0x8FFFF }));
    }
    // boundary addresses are accepted
    assert!(parse_package(&simple(1, &[(0x80000, 1), (0x8FFFF, 2)], &[1]), w, MAX).is_ok());
    // payload too big for the ceiling (:1173): 0x3000 bytes = 0xC00 dwords OK, 0xC01 refused
    let fw = simple(1, &[], &vec![0u32; 0xC00]);
    assert!(parse_package(&fw, w, MAX).is_ok());
    let fw = simple(1, &[], &vec![0u32; 0xC01]);
    assert_eq!(parse_package(&fw, w, MAX), Err(DmcError::PayloadTooBig { bytes: 0x3004, max: 0x3000 }));
    // payload shorter than fw_size says (truncated)
    let mut fw = simple(1, &[], &[1, 2, 3, 4]); fw.truncate(fw.len() - 4);
    assert!(matches!(parse_package(&fw, w, MAX), Err(DmcError::Truncated { what: "dmc payload", need: 16, have: 12 })));
}

#[test]
fn v3_headers_have_no_known_kbl_range() {
    // Build a v3 header (256 bytes, header_len 64 dwords); KBL has no sanity range for it -> named refusal
    let mut h = Vec::new();
    le(&mut h, 0x40403E3E); h.push(64); h.push(3); h.extend_from_slice(&[0, 0]);
    le(&mut h, 0); le(&mut h, 0); le(&mut h, 0);
    le(&mut h, 0x8F000); h.extend_from_slice(&[0u8; 36 + 32]); le(&mut h, 0);
    h.extend_from_slice(&[0u8; 160]);
    assert_eq!(h.len(), 256);
    let mut fw = css(1, 32); fw.extend(package(1, &[(0, b'*', b'*', 0)], None)); fw.extend(h);
    assert_eq!(parse_package(&fw, Stepping::WILDCARD, MAX), Err(DmcError::UnknownMmioRange { id: 0 }));
}

#[test]
fn total_on_garbage() {
    // never panics for any prefix of a valid blob or for corrupted bytes
    let ok = simple(1, &[(0x8F004, 1)], &[1, 2, 3]);
    for n in 0..ok.len() { let _ = parse_package(&ok[..n], Stepping::WILDCARD, MAX); }
    for i in 0..ok.len() { let mut b = ok.clone(); b[i] ^= 0xff; let _ = parse_package(&b, Stepping::WILDCARD, MAX); }
    let mut b = ok.clone(); b[128 + 16 + 4..128 + 16 + 8].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
    assert!(parse_package(&b, Stepping::WILDCARD, MAX).is_err());
}

#[test]
fn package_v1_ignores_the_dmc_id_byte() {
    // dmc_set_fw_offset(): `package_ver <= 1 ? DMC_FW_MAIN : fw_info[i].dmc_id` (intel_dmc.c:998); the byte is
    // reserved on v1 ("reserved on package_header version 1", intel_dmc.c:310) and may hold junk.
    for junk in [1u8, 3, 4, 9, 255] {
        let mut fw = css(1, 32);
        fw.extend(package(1, &[(junk, b'*', b'*', 0)], None));
        fw.extend(dmc_v1(&[], &[5], None, None));
        let p = parse_package(&fw, Stepping::WILDCARD, MAX).unwrap();
        assert_eq!(p.images.len(), 1, "junk {junk}");
        assert_eq!(p.images[0].id, DmcId::Main);
    }
}
