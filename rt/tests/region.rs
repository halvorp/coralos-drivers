//! The region/lease words as the kernel states them (userland/coral-firmament-abi/src/region_map.rs, kernel 113..116, 2026-10-13). Every
//! restated number is pinned here; a renumbering on either side fails this file.
use coralos_rt::nr;
use coralos_rt::region::*;

#[test]
fn the_system_call_numbers() {
    assert_eq!((nr::REGION_MAP, nr::REGION_UNMAP, nr::DMA_LEASE_CREATE, nr::DMA_LEASE_REVOKE), (113, 114, 115, 116));
}

#[test]
fn the_region_family_is_ten_words_from_max_130_down_in_kernel_order() {
    assert_eq!(RegionStatus::ALL.len(), 10);
    for (i, s) in RegionStatus::ALL.iter().enumerate() {
        assert_eq!(s.code(), u64::MAX - 130 - i as u64, "{s:?}");
        assert_eq!(*s as u32, i as u32 + 1);
        assert_eq!(RegionStatus::from_code(s.code()), Some(*s));
    }
    let names = ["REGION-BAD-RIGHTS", "REGION-OUT-OF-REGION", "REGION-NO-GRANT", "REGION-SESSION-ENDED", "REGION-RIGHTS-EXCEED-GRANT", "REGION-NO-VIRTUAL-SPACE", "REGION-NO-MEMORY", "REGION-GONE", "REGION-REVOKED-DURING-MAP", "REGION-NOT-MAPPED"];
    for (s, n) in RegionStatus::ALL.iter().zip(names) {
        assert_eq!(s.name(), n);
    }
    assert_eq!(RegionStatus::from_code(u64::MAX - 140), None, "the next family is not this one");
    assert_eq!(RegionStatus::from_code(u64::MAX - 129), None);
}

#[test]
fn the_lease_family_is_eleven_words_from_max_140_down_in_kernel_order() {
    assert_eq!(LeaseStatus::ALL.len(), 11);
    for (i, s) in LeaseStatus::ALL.iter().enumerate() {
        assert_eq!(s.code(), u64::MAX - 140 - i as u64, "{s:?}");
        assert_eq!(*s as u32, i as u32 + 1);
        assert_eq!(LeaseStatus::from_code(s.code()), Some(*s));
    }
    assert_eq!(LeaseStatus::NotLease.code(), u64::MAX - 150);
    assert_eq!(LeaseStatus::PageAlreadyLeased.name(), "LEASE-PAGE-ALREADY-LEASED");
    assert_eq!(LeaseStatus::from_code(u64::MAX - 151), None);
    assert_eq!(LeaseStatus::from_code(u64::MAX - 139), None, "the region family's last word is not a lease word");
}

#[test]
fn the_lease_block_is_64_bytes_with_the_handle_at_56() {
    assert_eq!(core::mem::size_of::<LeaseRequest>() as u64, LEASE_REQUEST_SIZE);
    assert_eq!(LEASE_REQUEST_SIZE, 64);
    assert_eq!(core::mem::offset_of!(LeaseRequest, out_lease), LEASE_OUT_OFFSET);
    assert_eq!(LEASE_OUT_OFFSET, 56);
    let r = lease_request(0x2000_0000, 0x1000, 0x3000, 3, 1, 70, 0xAA);
    assert_eq!((r.version, r.rights, r.cache, r.recipient_req_ep, r.recipient_node, r.out_lease), (LEASE_REQUEST_V1, 3, 1, 70, 0xAA, 0));
}

#[test]
fn the_window_the_tag_and_the_rights() {
    assert_eq!((RIGHT_READ, RIGHT_WRITE), (1, 2));
    assert_eq!(REGION_MAP_VA, 0x3000_0000_0000);
    assert_eq!(REGION_MAP_LEN, 1 << 40);
    assert!(LEASE_CAP_TAG > 64 && LEASE_CAP_TAG.is_power_of_two(), "a lease handle can never be taken for a shmem slot");
}

#[test]
fn a_map_word_is_a_va_in_the_window_or_a_named_refusal_and_nothing_else() {
    assert_eq!(decode_map(REGION_MAP_VA + 0x123), Ok(REGION_MAP_VA + 0x123));
    assert_eq!(decode_map(RegionStatus::SessionEnded.code()), Err(RegionRefusal::Named(RegionStatus::SessionEnded)));
    assert_eq!(decode_map(0), Err(RegionRefusal::Unknown(0)), "0 is not a VA");
    assert_eq!(decode_map(u64::MAX), Err(RegionRefusal::Unknown(u64::MAX)), "the generic failure word is reported as itself");
    assert_eq!(decode_map(0x40_0000), Err(RegionRefusal::Unknown(0x40_0000)), "a VA outside the window is not a mapping");
    assert_eq!(decode_map(LeaseStatus::NoMemory.code()), Err(RegionRefusal::Unknown(LeaseStatus::NoMemory.code())));
}

#[test]
fn unit_words_decode_to_ok_named_or_unknown() {
    assert_eq!(decode_region(0), Ok(()));
    assert_eq!(decode_region(RegionStatus::NotMapped.code()), Err(RegionRefusal::Named(RegionStatus::NotMapped)));
    assert_eq!(decode_region(1), Err(RegionRefusal::Unknown(1)));
    assert_eq!(decode_lease(0), Ok(()));
    assert_eq!(decode_lease(LeaseStatus::BadRecipient.code()), Err(LeaseRefusal::Named(LeaseStatus::BadRecipient)));
    assert_eq!(decode_lease(RegionStatus::NoGrant.code()), Err(LeaseRefusal::Unknown(RegionStatus::NoGrant.code())));
    assert_eq!(RegionRefusal::Unknown(5).name(), "REGION-UNKNOWN-WORD");
    assert_eq!(LeaseRefusal::Unknown(5).name(), "LEASE-UNKNOWN-WORD");
}

#[test]
fn the_snapshot_copy_reads_every_byte_once_at_any_alignment_and_length() {
    let src: Vec<u8> = (0..300u32).map(|i| (i * 7 + 3) as u8).collect();
    for start in 0..17 {
        for len in [0usize, 1, 7, 8, 9, 15, 16, 17, 100, 255] {
            let mut out = vec![0xEE; len];
            unsafe { copy_in(src.as_ptr().add(start), &mut out) };
            assert_eq!(out, &src[start..start + len], "start {start} len {len}");
        }
    }
}

#[test]
fn the_reply_copy_writes_exactly_the_bytes() {
    let mut dst = vec![0xAAu8; 40];
    unsafe { copy_out(dst.as_mut_ptr().add(5), &[1, 2, 3, 4, 5]) };
    assert_eq!(&dst[..5], &[0xAA; 5]);
    assert_eq!(&dst[5..10], &[1, 2, 3, 4, 5]);
    assert_eq!(&dst[10..], &[0xAA; 30][..]);
}
