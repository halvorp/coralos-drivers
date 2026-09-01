// SPDX-License-Identifier: GPL-2.0-only
//! Literal Linux vectors for `drivers/input/keyboard/atkbd.c` keycode tables.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux atkbd authors.

use atkbd_scancode_core::tables::*;

// Written out literally by INDEX: each position is the scancode and each value its keycode.
// This deliberately does not derive expectations from either production table. atkbd.c:78-107.
const LINUX_SET2_BY_INDEX: [u16; 512] = [
      0,  67,  65,  63,  61,  59,  60,  88, 183,  68,  66,  64,  62,  15,  41, 117,
    184,  56,  42,  93,  29,  16,   2,   0, 185,   0,  44,  31,  30,  17,   3,   0,
    186,  46,  45,  32,  18,   5,   4,  95, 187,  57,  47,  33,  20,  19,   6, 183,
    188,  49,  48,  35,  34,  21,   7, 184, 189,   0,  50,  36,  22,   8,   9, 185,
    190,  51,  37,  23,  24,  11,  10,   0, 191,  52,  53,  38,  39,  25,  12,   0,
    192,  89,  40,   0,  26,  13,   0, 193,  58,  54,  28,  27,   0,  43,   0, 194,
      0,  86,  91,  90,  92,   0,  14,  94,   0,  79, 124,  75,  71, 121,   0,   0,
     82,  83,  80,  76,  77,  72,   1,  69,  87,  78,  81,  74,  55,  73,  70,  99,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
    217, 100, 255,   0,  97, 165,   0,   0, 156,   0,   0,   0,   0,   0,   0, 125,
    173, 114,   0, 113,   0,   0,   0, 126, 128,   0,   0, 140,   0,   0,   0, 127,
    159,   0, 115,   0, 164,   0,   0, 116, 158,   0, 172, 166,   0,   0,   0, 142,
    157,   0,   0,   0,   0,   0,   0,   0, 155,   0,  98,   0,   0, 163,   0,   0,
    226,   0,   0,   0,   0,   0,   0,   0,   0, 255,  96,   0,   0,   0, 143,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0, 107,   0, 105, 102,   0,   0, 112,
    110, 111, 108, 112, 106, 103,   0, 119,   0, 118, 109,   0,  99, 104, 119,   0,
      0,   0,   0,  65,  99,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
];

// Written out literally by INDEX, including every zero. atkbd.c:109-123.
const LINUX_SET3_BY_INDEX: [u16; 512] = [
      0,   0,   0,   0,   0,   0,   0,  59,   1, 138, 128, 129, 130,  15,  41,  60,
    131,  29,  42,  86,  58,  16,   2,  61, 133,  56,  44,  31,  30,  17,   3,  62,
    134,  46,  45,  32,  18,   5,   4,  63, 135,  57,  47,  33,  20,  19,   6,  64,
    136,  49,  48,  35,  34,  21,   7,  65, 137, 100,  50,  36,  22,   8,   9,  66,
    125,  51,  37,  23,  24,  11,  10,  67, 126,  52,  53,  38,  39,  25,  12,  68,
    113, 114,  40,  43,  26,  13,  87,  99,  97,  54,  28,  27,  43,  43,  88,  70,
    108, 105, 119, 103, 111, 107,  14, 110,   0,  79, 106,  75,  71, 109, 102, 104,
     82,  83,  80,  76,  77,  72,  69,  98,   0,  96,  81,   0,  78,  73,  55, 183,
    184, 185, 186, 187,  74,  94,  92,  93,   0,   0,   0, 125, 126, 127, 112,   0,
      0, 139, 172, 163, 165, 115, 152, 172, 166, 140, 160, 154, 113, 114, 167, 168,
    148, 149, 147, 140,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
];

// Written out literally by translated set-1 index. atkbd.c:125-134.
const LINUX_UNXLATE_BY_INDEX: [u8; 128] = [
      0, 118,  22,  30,  38,  37,  46,  54,  61,  62,  70,  69,  78,  85, 102,  13,
     21,  29,  36,  45,  44,  53,  60,  67,  68,  77,  84,  91,  90,  20,  28,  27,
     35,  43,  52,  51,  59,  66,  75,  76,  82,  14,  18,  93,  26,  34,  33,  42,
     50,  49,  58,  65,  73,  74,  89, 124,  17,  41,  88,   5,   6,   4,  12,   3,
     11,   2,  10,   1,   9, 119, 126, 108, 117, 125, 123, 107, 115, 116, 121, 105,
    114, 122, 112, 113, 127,  96,  97, 120,   7,  15,  23,  31,  39,  47,  55,  63,
     71,  79,  86,  94,   8,  16,  24,  32,  40,  48,  56,  64,  72,  80,  87, 111,
     19,  25,  57,  81,  83,  92,  95,  98,  99, 100, 101, 103, 104, 106, 109, 110,
];

#[test]
fn linux_keymap_count_and_every_index_are_pinned() {
    assert_eq!(KEYMAP_SIZE, 512); // atkbd.c:76
    assert_eq!(SET2_KEYCODES, LINUX_SET2_BY_INDEX);
    assert_eq!(SET3_KEYCODES, LINUX_SET3_BY_INDEX);
    assert_eq!(UNXLATE, LINUX_UNXLATE_BY_INDEX);
}

#[test]
fn scancode_table_count_and_names_are_pinned() {
    assert_eq!(SCANCODE_TABLES.len(), 2); // atkbd.c:36-38
    let names: Vec<&str> = SCANCODE_TABLES.iter().map(|table| table.name).collect();
    assert_eq!(names, ["set2", "set3"]); // atkbd.c:78,109
    let sets: Vec<u8> = SCANCODE_TABLES.iter().map(|table| table.set).collect();
    assert_eq!(sets, [2, 3]);
}

#[test]
fn scroll_count_names_and_literals_are_pinned() {
    assert_eq!(SCROLL_KEYS.len(), 7); // atkbd.c:185-196
    let actual: Vec<(&str, u16, u8)> = SCROLL_KEYS.iter()
        .map(|entry| (entry.name, entry.keycode, entry.set2_scancode)).collect();
    assert_eq!(actual, [
        ("SCR_1", 0xfffe, 0xc5), ("SCR_2", 0xfffd, 0x9d),
        ("SCR_4", 0xfffc, 0xa4), ("SCR_8", 0xfffb, 0x9b),
        ("SCR_CLICK", 0xfffa, 0xe0), ("SCR_LEFT", 0xfff9, 0xcb),
        ("SCR_RIGHT", 0xfff8, 0xd2),
    ]); // atkbd.c:189-195
}

#[test]
fn raw_lookup_maps_and_names_refusals() {
    assert_eq!(raw_keycode(2, 0x1c, false), Ok(30)); // atkbd.c:88
    assert_eq!(raw_keycode(3, 0x08, false), Ok(1)); // atkbd.c:111
    assert_eq!(raw_keycode(2, 0xc5, true), Ok(0xfffe)); // atkbd.c:189
    assert_eq!(raw_keycode(2, 0x172, false), Ok(122)); // HANGEUL, atkbd.c:1162-1164; input-event-codes.h:199
    assert_eq!(raw_keycode(3, 0x0f1, false), Ok(123)); // HANJA, atkbd.c:1166-1168; input-event-codes.h:201
    assert_eq!(raw_keycode(4, 0, false), Err(LookupError::UnsupportedScancodeSet { set: 4, supported: [2, 3] }));
    assert_eq!(raw_keycode(2, 512, false), Err(LookupError::ScancodeOutOfRange { scancode: 512, maximum: 511 }));
}

#[test]
fn translated_lookup_uses_literal_unxlate_indices() {
    assert_eq!(translated_keycode(0x1e, false), Ok(30)); // UNXLATE[0x1e]=0x1c, atkbd.c:127; set2[0x1c]=30, atkbd.c:88
    assert_eq!(translated_keycode(0x9c, false), Ok(96)); // extended Enter: UNXLATE[0x1c]=0x5a, atkbd.c:127; set2[0xda]=96, atkbd.c:101
    assert_eq!(translated_keycode(256, false), Err(LookupError::ScancodeOutOfRange { scancode: 256, maximum: 255 }));
}
