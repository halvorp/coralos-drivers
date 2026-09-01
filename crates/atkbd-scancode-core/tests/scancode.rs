// SPDX-License-Identifier: GPL-2.0-only
//! Literal Linux vectors for `drivers/input/keyboard/atkbd.c` scancode encoding.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux atkbd authors.

use atkbd_scancode_core::scancode::*;

#[test]
fn protocol_response_count_names_and_values_are_pinned() {
    assert_eq!(XLATE_BYTES.len(), 6); // atkbd.c:340-343
    let actual: Vec<(&str, u8)> = XLATE_BYTES.iter().map(|entry| (entry.name, entry.code)).collect();
    assert_eq!(actual, [
        ("BAT", 0xaa), ("ERR", 0xff), ("ACK", 0xfa),
        ("NAK", 0xfe), ("HANJA", 0xf1), ("HANGEUL", 0xf2),
    ]); // atkbd.c:341-342
}

#[test]
fn all_protocol_literals_are_pinned() {
    assert_eq!([
        RET_ACK, RET_NAK, RET_BAT, RET_EMUL0, RET_EMUL1,
        RET_RELEASE, RET_HANJA, RET_HANGEUL, RET_ERR,
    ], [0xfa, 0xfe, 0xaa, 0xe0, 0xe1, 0xf0, 0xf1, 0xf2, 0xff]); // atkbd.c:152-160
}

#[test]
fn compatibility_encoding_covers_set_two_set_three_and_refusals() {
    assert_eq!(compat_scancode(2, 0, 0x83), Ok(0x103)); // atkbd.c:393
    assert_eq!(compat_scancode(2, 1, 0x75), Ok(0x0f5)); // atkbd.c:394-395
    assert_eq!(compat_scancode(3, 1, 0x75), Ok(0x175)); // atkbd.c:389-391
    assert_eq!(compat_scancode(3, 0, 0x75), Ok(0x075));
    assert_eq!(compat_scancode(1, 0, 0), Err(ScancodeError::UnsupportedScancodeSet { set: 1, supported: [2, 3] }));
    assert_eq!(compat_scancode(2, 3, 0), Err(ScancodeError::EmulationDepthOutOfRange { emul: 3, maximum: 2 }));
}

#[test]
fn translated_response_tracking_distinguishes_make_from_protocol_byte() {
    assert!(needs_xlate(0, 0x1c)); // ordinary byte, atkbd.c:360
    assert!(!needs_xlate(0, RET_ACK)); // protocol response with no make pending
    let bits = calculate_xl_bits(0, 0x7a); // low-seven-bit partner of 0xfa, atkbd.c:372-378
    assert_eq!(bits, 0x04); // ACK is xl_table index 2, atkbd.c:341
    assert!(needs_xlate(bits, RET_ACK));
    assert_eq!(calculate_xl_bits(bits, RET_ACK), 0);
    assert!(!needs_xlate(0xff, RET_EMUL0));
    assert!(!needs_xlate(0xff, RET_EMUL1));
}
