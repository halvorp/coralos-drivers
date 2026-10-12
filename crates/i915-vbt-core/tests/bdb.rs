// SPDX-License-Identifier: GPL-2.0-only
mod common;
use common::*;
use i915_vbt_core::bdb::*;
use i915_vbt_core::{BlockId, Vbt, VbtError};

#[test]
fn layout_constants() {
    assert_eq!(VBT_HEADER_SIZE, 48); // intel_vbt_defs.h:97 (20+2+2+2+1+1+4+16)
    assert_eq!(BDB_HEADER_SIZE, 22); // :115 (16+2+2+2)
    assert_eq!(VBT_SIGNATURE, b"$VBT"); // intel_bios.c:3053
    assert_eq!(BDB_SIGNATURE, b"BIOS_DATA_BLOCK");
}

#[test]
fn block_ids_are_linux_values() {
    // intel_vbt_defs.h:141-198
    assert_eq!(BlockId::GeneralFeatures as u8, 1);
    assert_eq!(BlockId::GeneralDefinitions as u8, 2);
    assert_eq!(BlockId::Psr as u8, 9);
    assert_eq!(BlockId::DriverFeatures as u8, 12);
    assert_eq!(BlockId::Edp as u8, 27);
    assert_eq!(BlockId::LfpOptions as u8, 40);
    assert_eq!(BlockId::LfpDataPtrs as u8, 41);
    assert_eq!(BlockId::LfpData as u8, 42);
    assert_eq!(BlockId::LfpBacklight as u8, 43);
    assert_eq!(BlockId::LfpPower as u8, 44);
    assert_eq!(BlockId::MipiSequence as u8, 53);
    assert_eq!(BlockId::GenericDtd as u8, 58);
}

#[test]
fn parses_and_walks_a_synthetic_vbt() {
    let b = vbt(221, &[(1, vec![1, 2, 3]), (27, vec![9; 10]), (43, vec![5])]);
    let v = Vbt::parse(&b).unwrap();
    assert_eq!((v.version, v.vbt_version, v.bdb_header_size), (221, 100, 22));
    assert!(v.checksum_ok && v.bdb_signature_ok);
    let ids: Vec<(u8, usize, u32)> = v.blocks().map(|x| { let x = x.unwrap(); (x.id, x.len(), x.bdb_offset) }).collect();
    assert_eq!(ids, vec![(1, 3, 25), (27, 10, 31), (43, 1, 44)]);
    assert_eq!(v.find_block(27).unwrap().unwrap().data, &[9; 10][..]);
    assert_eq!(v.find_block(99).unwrap(), None);
    assert_eq!(v.require_block(99), Err(VbtError::BlockMissing { id: 99 }));
}

#[test]
fn block_reads_are_zero_extended() {
    // init_bdb_block() copies into a zeroed max(min_size, block_size) buffer (intel_bios.c:456)
    let b = vbt(221, &[(1, vec![0xaa, 0xbb])]);
    let v = Vbt::parse(&b).unwrap();
    let blk = v.find_block(1).unwrap().unwrap();
    assert_eq!(blk.u8_at(1), 0xbb);
    assert_eq!(blk.u8_at(2), 0);
    assert_eq!(blk.u16_at(0), 0xbbaa);
    assert_eq!(blk.u16_at(1), 0x00bb);
    assert_eq!(blk.u32_at(0), 0xbbaa);
    assert_eq!(blk.u64_at(100), 0);
    assert_eq!(blk.require(3), Err(VbtError::BlockTooSmall { id: 1, size: 2, need: 3 }));
}

#[test]
fn header_refusals_are_named() {
    let good = vbt(221, &[(1, vec![1])]);
    assert_eq!(Vbt::parse(&good[..40]).unwrap_err(), VbtError::HeaderIncomplete { have: 40, need: 48 });
    let mut b = good.clone(); b[0] = b'X';
    assert_eq!(Vbt::parse(&b).unwrap_err(), VbtError::BadSignature);
    // vbt_size > buffer
    assert_eq!(Vbt::parse(&good[..good.len() - 1]).unwrap_err(), VbtError::VbtSizeOverflows { vbt_size: good.len(), buffer: good.len() - 1 });
    // bdb_offset leaves no room for the header
    let mut b = good.clone(); b[28..32].copy_from_slice(&(good.len() as u32 - 10).to_le_bytes());
    assert!(matches!(Vbt::parse(&b).unwrap_err(), VbtError::BdbHeaderIncomplete { .. }));
    // bdb_offset + 0xffffffff must not wrap
    let mut b = good.clone(); b[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(Vbt::parse(&b).unwrap_err(), VbtError::BdbHeaderIncomplete { .. }));
    // bdb_size too big
    let mut b = good.clone(); b[48 + 20..48 + 22].copy_from_slice(&0xfff0u16.to_le_bytes());
    assert!(matches!(Vbt::parse(&b).unwrap_err(), VbtError::BdbIncomplete { .. }));
    // bdb header_size smaller than the header, or bigger than the bdb
    let mut b = good.clone(); b[48 + 18] = 10;
    assert_eq!(Vbt::parse(&b).unwrap_err(), VbtError::BadBdbHeaderSize { header_size: 10, bdb_size: 26 });
    let mut b = good.clone(); b[48 + 18] = 200;
    assert_eq!(Vbt::parse(&b).unwrap_err(), VbtError::BadBdbHeaderSize { header_size: 200, bdb_size: 26 });
}

#[test]
fn checksum_and_signature_are_reported_not_enforced() {
    let mut b = vbt(221, &[(1, vec![1])]);
    b[26] ^= 1;
    b[48] = b'X';
    let v = Vbt::parse(&b).unwrap(); // Linux accepts it too (intel_bios.c:3064-3106)
    assert!(!v.checksum_ok);
    assert!(!v.bdb_signature_ok);
}

#[test]
fn overflowing_block_hides_later_blocks_like_find_raw_section() {
    let mut b = vbt(221, &[(1, vec![1, 2]), (2, vec![0; 4])]);
    // second block claims 100 bytes (intel_bios.c:117)
    let off = 48 + 22 + 3 + 2;
    b[off + 1] = 100;
    let v = Vbt::parse(&b).unwrap();
    assert!(v.find_block(1).unwrap().is_some()); // found before the bad block
    assert_eq!(v.find_block(2), Err(VbtError::BlockOverflows { id: 2, size: 100, offset: 22 + 5 + 3, bdb_size: 22 + 5 + 7 }));
    assert_eq!(v.blocks().count(), 2); // one Ok then one Err, then the iterator ends
}

#[test]
fn walk_stops_when_fewer_than_four_bytes_remain() {
    // `while (index + 3 < total)` — intel_bios.c:109: a 3-byte tail is not a block
    let mut b = vbt(221, &[(1, vec![1])]);
    b.extend_from_slice(&[7, 0, 0]);
    let n = (b.len() - 48) as u16;
    b[48 + 20..48 + 22].copy_from_slice(&n.to_le_bytes());
    let l = b.len() as u16; b[24..26].copy_from_slice(&l.to_le_bytes());
    let v = Vbt::parse(&b).unwrap();
    assert_eq!(v.blocks().count(), 1);
}

#[test]
fn mipi_sequence_v3_has_its_own_u32_size() {
    // _get_blocksize(): block 53 with version byte >= 3 uses the u32 at +4 (intel_bios.c:83-91)
    let mut data = vec![3u8]; // version
    data.extend_from_slice(&[0, 0, 0, 0]); // u32 @ +4..8 of the block header -> overlaps data[1..5]
    // header: id@0, u16 size@1..3, version@3, u32 size@4..8 ; data starts at +3
    let mut raw = vec![53u8, 0, 0, 3];
    raw.extend_from_slice(&6u32.to_le_bytes());
    raw.extend_from_slice(&[0xee, 0xff]); // 6 bytes of "data" counted from +3: [3, 6,0,0,0, ee] -> size 6 => ends at +9
    let mut b = vbt(221, &[]);
    b.extend_from_slice(&raw);
    let n = (b.len() - 48) as u16;
    b[48 + 20..48 + 22].copy_from_slice(&n.to_le_bytes());
    let l = b.len() as u16; b[24..26].copy_from_slice(&l.to_le_bytes());
    let v = Vbt::parse(&b).unwrap();
    let blk = v.find_block(53).unwrap().unwrap();
    assert_eq!(blk.len(), 6);
    assert_eq!(blk.u8_at(0), 3);
    // a version-2 block uses the u16 at +1 instead
    let mut raw2 = vec![53u8, 2, 0, 2, 0xaa, 0xbb];
    raw2[1] = 2;
    let mut b2 = vbt(221, &[]); b2.extend_from_slice(&raw2);
    let n = (b2.len() - 48) as u16; b2[48 + 20..48 + 22].copy_from_slice(&n.to_le_bytes());
    let l = b2.len() as u16; b2[24..26].copy_from_slice(&l.to_le_bytes());
    let v2 = Vbt::parse(&b2).unwrap();
    assert_eq!(v2.find_block(53).unwrap().unwrap().len(), 2);
    // the u32 field itself cut off by the BDB end is a named refusal
    let mut b3 = vbt(221, &[]); b3.extend_from_slice(&[53, 0, 0, 3, 0]);
    let n = (b3.len() - 48) as u16; b3[48 + 20..48 + 22].copy_from_slice(&n.to_le_bytes());
    let l = b3.len() as u16; b3[24..26].copy_from_slice(&l.to_le_bytes());
    let v3 = Vbt::parse(&b3).unwrap();
    // 22 + 5 bytes: only 5 left after header -> `index + 3 < total` is true (22+3 < 27) -> size field truncated
    assert_eq!(v3.find_block(53), Err(VbtError::MipiSizeFieldTruncated { offset: 22 }));
}

#[test]
fn hostile_bytes_never_panic() {
    let b = vbt(221, &Lfp::new().blocks());
    for n in 0..b.len() { if let Ok(v) = Vbt::parse(&b[..n]) { for x in v.blocks() { let _ = x; } } }
    for i in (0..b.len()).step_by(7) {
        let mut c = b.clone(); c[i] ^= 0xff;
        if let Ok(v) = Vbt::parse(&c) { for x in v.blocks() { let _ = x; } let _ = i915_vbt_core::panel::parse_lfp_data_ptrs(&v); let _ = i915_vbt_core::general::parse_general_definitions(&v); }
    }
}
