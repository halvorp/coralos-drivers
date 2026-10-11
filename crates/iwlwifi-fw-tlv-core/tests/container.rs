// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the TLV ucode header and the TLV walk.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/fw/file.h:126-156` and
//! `drivers/net/wireless/intel/iwlwifi/iwl-drv.c:807-855`. Copyright (C) 2005-2014, 2018-2025 Intel
//! Corporation, and the Linux iwlwifi authors.

mod common;
use common::*;
use iwlwifi_fw_tlv_core::container::*;

#[test]
fn the_header_layout_is_file_h_135_156() {
    assert_eq!(IWL_TLV_UCODE_MAGIC, 0x0a4c5749); // file.h:132
    assert_eq!(FW_VER_HUMAN_READABLE_SZ, 64); // file.h:133
    assert_eq!(TLV_UCODE_HEADER_LEN, 88); // 4+4+64+4+4+8
    assert_eq!(TLV_HEADER_LEN, 8); // file.h:126-130
    let b = header(0x2e00_0000, 7, b"release/core43::7e3e4b69");
    assert_eq!(b.len(), TLV_UCODE_HEADER_LEN);
    let h = parse_header(&b).unwrap();
    assert_eq!((h.ver, h.build), (0x2e00_0000, 7));
    assert_eq!(&h.human_readable[..24], b"release/core43::7e3e4b69");
    assert_eq!(h.human_readable.len(), 64);
}

#[test]
fn a_short_file_a_v1_header_and_a_bad_magic_are_refused_by_name() {
    let b = header(0, 0, b"");
    assert_eq!(parse_header(&b[..87]), Err(ContainerError::TooShort { len: 87, need: 88 }));
    let mut v1 = b.clone();
    v1[0] = 1;
    assert_eq!(parse_header(&v1), Err(ContainerError::NotTlvFormat { zero: 1 }));
    let mut bad = b.clone();
    bad[4] = 0x48;
    assert_eq!(parse_header(&bad), Err(ContainerError::BadMagic { magic: 0x0a4c5748 }));
    assert!(tlvs(&bad).is_err());
}

#[test]
fn tlvs_are_walked_with_4_byte_padding_and_the_payload_unpadded() {
    let f = file(&[(19, vec![1, 2, 3, 4, 5]), (31, words(&[52])), (99, vec![])]);
    let got: Vec<_> = tlvs(&f).unwrap().collect::<Result<_, _>>().unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], Tlv { ty: 19, data: &[1, 2, 3, 4, 5] });
    assert_eq!(got[1], Tlv { ty: 31, data: &[52, 0, 0, 0] });
    assert_eq!(got[2], Tlv { ty: 99, data: &[] });
    // iwl-drv.c:841: fewer than 8 trailing bytes end the walk silently.
    let mut tail = f.clone();
    tail.extend_from_slice(&[0xEE; 7]);
    assert_eq!(tlvs(&tail).unwrap().count(), 3);
}

#[test]
fn a_tlv_longer_than_the_file_or_whose_padding_overruns_it_is_refused_then_the_walk_ends() {
    let mut f = header(0, 0, b"");
    f.extend_from_slice(&19u32.to_le_bytes());
    f.extend_from_slice(&0xFFFFu32.to_le_bytes());
    f.extend_from_slice(&[0; 4]);
    let mut it = tlvs(&f).unwrap();
    assert_eq!(
        it.next(),
        Some(Err(ContainerError::TlvOverrun { offset: 88, ty: 19, tlv_len: 0xFFFF, remaining: 4 }))
    );
    assert_eq!(it.next(), None);
    // 5 payload bytes fit exactly, the 3 padding bytes do not (Linux underflows here).
    let mut p = header(0, 0, b"");
    p.extend_from_slice(&6u32.to_le_bytes());
    p.extend_from_slice(&5u32.to_le_bytes());
    p.extend_from_slice(&[0; 5]);
    let mut it = tlvs(&p).unwrap();
    assert_eq!(
        it.next(),
        Some(Err(ContainerError::PaddingOverrun { offset: 88, ty: 6, tlv_len: 5, remaining: 5 }))
    );
    assert_eq!(it.next(), None);
}
