// SPDX-License-Identifier: GPL-2.0-only
//! Item decoding vectors from Linux `drivers/hid/hid-core.c` and
//! `include/linux/hid.h`; original copyright holders are named by the crate.

use hid_report_core::item::{
    fetch_item, FetchError, Format, ItemType, FORMAT_NAMES, ITEM_TAG_LONG, TYPE_NAMES,
};

#[test]
fn linux_item_format_and_type_lists_are_frozen() {
    // include/linux/hid.h:57-58 and :70-73. Literal, not derived from production.
    assert_eq!(FORMAT_NAMES.len(), 2);
    assert_eq!(FORMAT_NAMES, ["SHORT", "LONG"]);
    assert_eq!(TYPE_NAMES.len(), 4);
    assert_eq!(TYPE_NAMES, ["MAIN", "GLOBAL", "LOCAL", "RESERVED"]);
    assert_eq!(ITEM_TAG_LONG, 15); // include/linux/hid.h:64
}

#[test]
fn all_short_size_codes_and_little_endian_values_decode() {
    // hid-core.c:809: 0,1,2,3 -> 0,1,2,4; :818-827 little endian.
    let (zero, used) = fetch_item(&[0x80]).unwrap();
    assert_eq!((zero.size, zero.data, used), (0, 0, 1));
    let (one, used) = fetch_item(&[0x81, 0xa5]).unwrap();
    assert_eq!((one.size, one.data, used), (1, 0xa5, 2));
    let (two, used) = fetch_item(&[0x82, 0x34, 0x12]).unwrap();
    assert_eq!((two.size, two.data, used), (2, 0x1234, 3));
    let (four, used) = fetch_item(&[0x83, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert_eq!((four.size, four.data, used), (4, 0x1234_5678, 5));
    assert_eq!(four.format, Format::Short);
    assert_eq!(four.item_type, ItemType::Main);
    assert_eq!(four.tag, 8);
}

#[test]
fn type_bits_and_signed_data_decode_independently() {
    // hid-core.c:787-788; item_sdata :387-395.
    assert_eq!(fetch_item(&[0x04]).unwrap().0.item_type, ItemType::Global);
    assert_eq!(fetch_item(&[0x08]).unwrap().0.item_type, ItemType::Local);
    assert_eq!(fetch_item(&[0x0c]).unwrap().0.item_type, ItemType::Reserved);
    assert_eq!(fetch_item(&[0x15, 0xff]).unwrap().0.signed_data(), -1);
    assert_eq!(
        fetch_item(&[0x16, 0x00, 0x80]).unwrap().0.signed_data(),
        -32768
    );
    assert_eq!(
        fetch_item(&[0x17, 0, 0, 0, 0x80]).unwrap().0.signed_data(),
        i32::MIN
    );
    assert_eq!(fetch_item(&[0x14]).unwrap().0.signed_data(), 0);
}

#[test]
fn long_item_header_tag_payload_and_consumption_decode() {
    // hid-core.c:790-805: 0xfe, size, long tag, payload.
    let bytes = [0xfe, 3, 0x42, 0xaa, 0xbb, 0xcc, 0x81];
    let (item, used) = fetch_item(&bytes).unwrap();
    assert_eq!(item.format, Format::Long);
    assert_eq!(item.item_type, ItemType::Reserved);
    assert_eq!((item.tag, item.size, item.data), (0x42, 3, 0));
    assert_eq!(item.long_data, &[0xaa, 0xbb, 0xcc]);
    assert_eq!(used, 6, "the following 0x81 belongs to the next item");
}

#[test]
fn every_truncation_is_bounded_and_named() {
    assert_eq!(fetch_item(&[]), Err(FetchError::EmptyInput));
    assert_eq!(
        fetch_item(&[0xfe]),
        Err(FetchError::LongHeaderTruncated {
            available: 0,
            required: 2
        })
    );
    assert_eq!(
        fetch_item(&[0xfe, 3]),
        Err(FetchError::LongHeaderTruncated {
            available: 1,
            required: 2
        })
    );
    assert_eq!(
        fetch_item(&[0xfe, 3, 0x42, 0xaa]),
        Err(FetchError::LongPayloadTruncated {
            declared: 3,
            available: 1
        })
    );
    assert_eq!(
        fetch_item(&[0x82, 0xaa]),
        Err(FetchError::ShortPayloadTruncated {
            declared: 2,
            available: 1
        })
    );
    assert_eq!(
        fetch_item(&[0x83, 0, 0, 0]),
        Err(FetchError::ShortPayloadTruncated {
            declared: 4,
            available: 3
        })
    );
}
