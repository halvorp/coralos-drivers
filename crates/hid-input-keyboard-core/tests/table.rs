// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for `drivers/hid/hid-input.c` `hid_keyboard[]` and its inverse.
//! Copyright (c) 2000-2001 Vojtech Pavlik; Copyright (c) 2006-2010 Jiri Kosina.

use hid_input_keyboard_core::*;

#[test]
fn constants_are_the_linux_literals() {
    assert_eq!(KEY_UNKNOWN, 240); // input-event-codes.h:325
    assert_eq!(HID_UP_KEYBOARD, 0x0007_0000); // hid.h:152
    assert_eq!(TABLE_LEN, 256); // hid-input.c:27
    assert_eq!(HID_KEYBOARD.len(), 256);
}

/// Every row's first and last entry, written out from hid-input.c:28-43 — a row shifted by one entry or a
/// dropped row moves at least one of these.
#[test]
fn every_row_starts_and_ends_where_linux_does() {
    let ends: [(u8, u8); 16] = [
        (0, 38), (50, 3), (4, 26), (27, 64), (65, 106), (105, 71), (72, 190), (191, 113),
        (115, 240), (122, 240), (240, 240), (240, 240), (240, 240), (240, 240), (29, 113), (150, 240),
    ];
    for (row, &(first, last)) in ends.iter().enumerate() {
        assert_eq!(HID_KEYBOARD[row * 16], first, "row {row:#x} first");
        assert_eq!(HID_KEYBOARD[row * 16 + 15], last, "row {row:#x} last");
    }
}

/// The keys a keyboard user meets: (usage, Linux keycode) from hid-input.c:28-42 and the keycode names
/// in input-event-codes.h.
#[test]
fn the_usages_a_user_meets_map_to_their_keycodes() {
    let vectors: [(u16, u16); 24] = [
        (0x04, 30),  // a -> KEY_A
        (0x1d, 44),  // z -> KEY_Z
        (0x1e, 2),   // 1 -> KEY_1
        (0x27, 11),  // 0 -> KEY_0
        (0x28, 28),  // Enter -> KEY_ENTER
        (0x29, 1),   // Escape -> KEY_ESC
        (0x2a, 14),  // Backspace -> KEY_BACKSPACE
        (0x2b, 15),  // Tab -> KEY_TAB
        (0x2c, 57),  // Space -> KEY_SPACE
        (0x3a, 59),  // F1 -> KEY_F1
        (0x43, 68),  // F10 -> KEY_F10
        (0x45, 88),  // F12 -> KEY_F12
        (0x4a, 102), // Home -> KEY_HOME
        (0x4b, 104), // Page Up -> KEY_PAGEUP
        (0x4c, 111), // Delete -> KEY_DELETE
        (0x4d, 107), // End -> KEY_END
        (0x4e, 109), // Page Down -> KEY_PAGEDOWN
        (0x4f, 106), // Right -> KEY_RIGHT
        (0x52, 103), // Up -> KEY_UP
        (0x7f, 113), // Mute -> KEY_MUTE
        (0xe0, 29),  // Left Control -> KEY_LEFTCTRL
        (0xe3, 125), // Left GUI -> KEY_LEFTMETA
        (0xe6, 100), // Right Alt -> KEY_RIGHTALT
        (0xe7, 126), // Right GUI -> KEY_RIGHTMETA
    ];
    for (u, k) in vectors {
        assert_eq!(usage_to_keycode(u), Some(k), "usage {u:#x}");
    }
}

#[test]
fn ignored_unknown_and_out_of_table_usages() {
    for u in 0..4u16 {
        assert_eq!(usage_to_keycode(u), None, "usage {u:#x} is 0: goto ignore (hid-input.c:753)");
    }
    assert_eq!(usage_to_keycode(0x82), Some(240)); // unk
    assert_eq!(usage_to_keycode(0x100), Some(240)); // map_key(KEY_UNKNOWN), hid-input.c:756
}

#[test]
fn the_inverse_picks_the_lowest_usage_of_every_twin() {
    assert_eq!(keycode_to_usage(43), Some(0x31)); // KEY_BACKSLASH: 0x31 and 0x32
    assert_eq!(keycode_to_usage(111), Some(0x4c)); // KEY_DELETE: 0x4c, 0x9c, 0xd8
    assert_eq!(keycode_to_usage(113), Some(0x7f)); // KEY_MUTE: 0x7f and 0xef
    assert_eq!(keycode_to_usage(115), Some(0x80)); // KEY_VOLUMEUP: 0x80 and 0xed
    assert_eq!(keycode_to_usage(114), Some(0x81)); // KEY_VOLUMEDOWN: 0x81 and 0xee
    assert_eq!(keycode_to_usage(158), Some(0xf1)); // KEY_BACK: only the media tail
    assert_eq!(keycode_to_usage(173), Some(0xfa)); // KEY_REFRESH: only the media tail
    assert_eq!(keycode_to_usage(125), Some(0xe3)); // KEY_LEFTMETA
    assert_eq!(keycode_to_usage(0), None);
    assert_eq!(keycode_to_usage(240), None);
    assert_eq!(keycode_to_usage(224), None); // KEY_BRIGHTNESSDOWN: no keyboard-page usage
    assert_eq!(keycode_to_usage(0x174), None); // KEY_FULL_SCREEN: past u8
}

#[test]
fn the_inverse_round_trips_every_first_usage() {
    let mut checked = 0;
    let mut twins = Vec::new();
    for u in 0..256usize {
        let k = HID_KEYBOARD[u];
        if k == 0 || k == KEY_UNKNOWN {
            continue;
        }
        let back = keycode_to_usage(k as u16).unwrap() as usize;
        assert!(back <= u && HID_KEYBOARD[back] == k, "usage {u:#x}");
        if back == u {
            checked += 1;
        } else {
            twins.push(u);
        }
    }
    // 170 mapped entries: 162 distinct keycodes, and these 8 usages repeat a lower usage's keycode
    // (counted by hand from hid-input.c:28-43).
    assert_eq!(checked, 162);
    assert_eq!(twins, [0x32, 0x9c, 0xd8, 0xed, 0xee, 0xef, 0xf3, 0xf4]);
}

/// The whole table, copied from hid-input.c:28-43 into this test with `unk` written as 240 — an independent
/// copy, so a changed, dropped or shifted entry in the production table fails here.
#[test]
fn the_whole_table_matches_linux() {
    let want: [u8; 256] = [
        0, 0, 0, 0, 30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38,
        50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44, 2, 3,
        4, 5, 6, 7, 8, 9, 10, 11, 28, 1, 14, 15, 57, 12, 13, 26,
        27, 43, 43, 39, 40, 41, 51, 52, 53, 58, 59, 60, 61, 62, 63, 64,
        65, 66, 67, 68, 87, 88, 99, 70, 119, 110, 102, 104, 111, 107, 109, 106,
        105, 108, 103, 69, 98, 55, 74, 78, 96, 79, 80, 81, 75, 76, 77, 71,
        72, 73, 82, 83, 86, 127, 116, 117, 183, 184, 185, 186, 187, 188, 189, 190,
        191, 192, 193, 194, 134, 138, 130, 132, 128, 129, 131, 137, 133, 135, 136, 113,
        115, 114, 240, 240, 240, 121, 240, 89, 93, 124, 92, 94, 95, 240, 240, 240,
        122, 123, 90, 91, 85, 240, 240, 240, 240, 240, 240, 240, 111, 240, 240, 240,
        240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240,
        240, 240, 240, 240, 240, 240, 179, 180, 240, 240, 240, 240, 240, 240, 240, 240,
        240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240,
        240, 240, 240, 240, 240, 240, 240, 240, 111, 240, 240, 240, 240, 240, 240, 240,
        29, 42, 56, 125, 97, 54, 100, 126, 164, 166, 165, 163, 161, 115, 114, 113,
        150, 158, 159, 128, 136, 177, 178, 176, 142, 152, 173, 140, 240, 240, 240, 240,
    ];
    assert_eq!(HID_KEYBOARD, want);
}
