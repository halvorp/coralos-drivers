// SPDX-License-Identifier: GPL-2.0-only
//! Expected bytes are written out from the Linux structs (fw/api/sta.h:364-402) and
//! `iwl_mvm_send_sta_key` (mvm/sta.c:3532-3686), never derived from this crate's tables.

use iwlwifi_mvm_sta_key_core::decode::{status, StatusRefusal};
use iwlwifi_mvm_sta_key_core::encode::{ccmp, remove, rx_seq, EncodeRefusal};
use iwlwifi_mvm_sta_key_core::ids::*;
use iwlwifi_mvm_sta_key_core::slots::{KeySlots, SlotRefusal};

#[test]
fn constants_are_the_linux_literals() {
    assert_eq!(ADD_STA_KEY, 0x17); // fw/api/commands.h:134
    assert_eq!(STA_KEY_FLG_CCM, 2); // fw/api/sta.h:124
    assert_eq!(STA_KEY_FLG_EN_MSK, 7); // fw/api/sta.h:130
    assert_eq!(STA_KEY_FLG_WEP_KEY_MAP, 8); // fw/api/sta.h:132
    assert_eq!(STA_KEY_FLG_KEYID_POS, 8); // fw/api/sta.h:134
    assert_eq!(STA_KEY_FLG_KEYID_MSK, 0x300); // fw/api/sta.h:135
    assert_eq!(STA_KEY_NOT_VALID, 0x800); // fw/api/sta.h:136
    assert_eq!(STA_KEY_MULTICAST, 0x4000); // fw/api/sta.h:139
    assert_eq!(STA_KEY_MAX_NUM, 16); // fw/api/sta.h:190
    assert_eq!(STA_KEY_IDX_INVALID, 0xff); // fw/api/sta.h:191
    assert_eq!(ADD_STA_SUCCESS, 1); // fw/api/sta.h:413
    assert_eq!(COMMON_LEN, 1 + 1 + 2 + 32 + 16);
    assert_eq!(CMD_V1_LEN, COMMON_LEN + 1 + 1 + 2 * 5);
    assert_eq!(CMD_V2_LEN, COMMON_LEN + 8 + 8 + 8);
    // The key-id field is two bits wide and the cipher field three: no overlap with each other.
    assert_eq!(STA_KEY_FLG_KEYID_MSK & STA_KEY_FLG_EN_MSK, 0);
    assert_eq!(STA_KEY_FLG_KEYID_MSK, 3 << STA_KEY_FLG_KEYID_POS);
}

const KEY: [u8; 16] = [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf];

#[test]
fn pairwise_ccmp_v1_bytes() {
    let c = ccmp(1, 0, 0, 0, false, &KEY, 0, 0).unwrap();
    assert_eq!(c.len, 64);
    let mut want = [0u8; 64];
    want[0] = 0; // sta_id
    want[1] = 0; // key_offset
    want[2] = 0x0a; // WEP_KEY_MAP | CCM, little endian
    want[3] = 0x00;
    want[4..20].copy_from_slice(&KEY);
    assert_eq!(c.as_slice(), &want[..]);
}

#[test]
fn group_ccmp_v1_flags_carry_key_id_and_multicast() {
    let c = ccmp(1, 3, 5, 1, true, &KEY, 0, 0).unwrap();
    assert_eq!(c.bytes[0], 3);
    assert_eq!(c.bytes[1], 5);
    // keyid 1 << 8 | WEP_KEY_MAP 0x8 | CCM 0x2 | MULTICAST 0x4000
    assert_eq!(u16::from_le_bytes([c.bytes[2], c.bytes[3]]), 0x410a);
    // key id 3 fills the mask
    let c = ccmp(1, 3, 5, 3, true, &KEY, 0, 0).unwrap();
    assert_eq!(u16::from_le_bytes([c.bytes[2], c.bytes[3]]), 0x4000 | 0x300 | 0xa);
}

#[test]
fn rx_sequence_has_the_hole_below_version_three() {
    // RSC wire bytes 01 02 03 04 05 06 (LSB first) = PN 0x060504030201.
    let pn = 0x0605_0403_0201u64;
    // MSB-first PN bytes 06 05 04 03 02 01; hole of two after the first two (sta.c:3628-3645).
    assert_eq!(rx_seq(pn, 1)[..8], [0x06, 0x05, 0, 0, 0x04, 0x03, 0x02, 0x01]);
    assert_eq!(rx_seq(pn, 2)[..8], [0x06, 0x05, 0, 0, 0x04, 0x03, 0x02, 0x01]);
    assert_eq!(rx_seq(pn, 3)[..8], [0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0, 0]);
    assert_eq!(rx_seq(pn, 1)[8..], [0u8; 8]);
    // Only the low 48 bits are a packet number.
    assert_eq!(rx_seq(pn | (0xffff << 48), 1), rx_seq(pn, 1));
}

#[test]
fn the_rx_sequence_lands_at_offset_36_of_the_command() {
    let c = ccmp(1, 0, 0, 0, true, &KEY, 0x0605_0403_0201, 0).unwrap();
    assert_eq!(c.bytes[36..44], [0x06, 0x05, 0, 0, 0x04, 0x03, 0x02, 0x01]);
    assert_eq!(c.bytes[20..36], [0u8; 16]); // the rest of key[32]
    assert_eq!(c.bytes[44..64], [0u8; 20]); // remaining seq, tkip fields
}

#[test]
fn version_two_and_three_append_the_mic_keys_and_transmit_counter() {
    for ver in [2u8, 3] {
        let c = ccmp(ver, 1, 2, 0, false, &KEY, 0, 0x0102_0304_0506_0708).unwrap();
        assert_eq!(c.len, 76);
        assert_eq!(c.bytes[52..68], [0u8; 16]); // rx_mic_key, tx_mic_key: TKIP only
        assert_eq!(c.bytes[68..76], 0x0102_0304_0506_0708u64.to_le_bytes());
    }
    // Version 1 has no transmit counter field.
    let c = ccmp(1, 1, 2, 0, false, &KEY, 0, 0x0102_0304_0506_0708).unwrap();
    assert_eq!(c.len, 64);
    assert_eq!(c.bytes[64..], [0u8; 12]);
}

#[test]
fn encode_refusals_name_the_value() {
    assert_eq!(ccmp(1, 0, 0, 4, false, &KEY, 0, 0), Err(EncodeRefusal::KeyId(4)));
    assert_eq!(ccmp(1, 0, 16, 0, false, &KEY, 0, 0), Err(EncodeRefusal::KeyOffset(16)));
    assert!(ccmp(1, 0, 15, 0, false, &KEY, 0, 0).is_ok());
    assert_eq!(ccmp(0, 0, 0, 0, false, &KEY, 0, 0), Err(EncodeRefusal::CmdVersion(0)));
    assert_eq!(ccmp(4, 0, 0, 0, false, &KEY, 0, 0), Err(EncodeRefusal::CmdVersion(4)));
    assert_eq!(remove(0, 16, 0, false, false), Err(EncodeRefusal::KeyOffset(16)));
    assert_eq!(remove(0, 0, 4, false, false), Err(EncodeRefusal::KeyId(4)));
}

#[test]
fn remove_marks_the_key_not_valid_with_no_encryption() {
    let c = remove(2, 7, 0, false, false).unwrap();
    assert_eq!(c.len, 64);
    assert_eq!(c.bytes[0], 2);
    assert_eq!(c.bytes[1], 7);
    assert_eq!(u16::from_le_bytes([c.bytes[2], c.bytes[3]]), 0x0808); // WEP_KEY_MAP | NOT_VALID
    assert_eq!(c.bytes[4..64], [0u8; 60]);
    let c = remove(2, 7, 2, true, true).unwrap();
    assert_eq!(c.len, 76);
    assert_eq!(u16::from_le_bytes([c.bytes[2], c.bytes[3]]), 0x4000 | 0x200 | 0x0808);
    // No encryption algorithm bits are set on a removal.
    assert_eq!(c.bytes[2] & 7, 0);
}

#[test]
fn status_word() {
    assert_eq!(status(&1u32.to_le_bytes()), Ok(()));
    assert_eq!(status(&2u32.to_le_bytes()), Err(StatusRefusal::NotSuccess(2)));
    assert_eq!(status(&0u32.to_le_bytes()), Err(StatusRefusal::NotSuccess(0)));
    assert_eq!(status(&[1, 0, 0]), Err(StatusRefusal::Length(3)));
    assert_eq!(status(&[1, 0, 0, 0, 0]), Err(StatusRefusal::Length(5)));
}

#[test]
fn slots_hand_out_the_longest_freed_slot() {
    let mut s = KeySlots::new();
    for want in 0..16u8 {
        assert_eq!(s.alloc(), Ok(want));
    }
    assert_eq!(s.alloc(), Err(SlotRefusal::Full));
    s.free(3).unwrap();
    s.free(9).unwrap();
    // 3 was freed first so its counter is 1 higher than 9's: it is the one handed out.
    assert_eq!(s.alloc(), Ok(3));
    s.free(3).unwrap();
    // Now 9 (freed two events ago) beats 3 (just freed).
    assert_eq!(s.alloc(), Ok(9));
    assert!(s.in_use(9));
    assert!(!s.in_use(3));
}

#[test]
fn slot_ties_pick_the_lowest_index_and_bad_frees_are_named() {
    let mut s = KeySlots::new();
    assert_eq!(s.alloc(), Ok(0));
    assert_eq!(s.alloc(), Ok(1));
    s.free(1).unwrap();
    s.free(0).unwrap();
    // Slots 2..15 were never used and aged twice (counter 2); slot 1 was freed one event ago
    // (1) and slot 0 just now (0). Among the tied maxima the first, slot 2, wins (sta.c:3471 uses
    // a strict greater-than).
    assert_eq!(s.alloc(), Ok(2));
    assert_eq!(s.alloc(), Ok(3));
    assert_eq!(s.free(0), Err(SlotRefusal::NotInUse(0))); // already free
    assert_eq!(s.free(5), Err(SlotRefusal::NotInUse(5))); // never allocated
    assert_eq!(s.free(16), Err(SlotRefusal::NotInUse(16))); // not a slot
}
