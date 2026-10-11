// SPDX-License-Identifier: GPL-2.0-only
//! The HID keyboard page (0x07) → Linux input keycode table, and its inverse.
//!
//! Ported mechanically from Linux `drivers/hid/hid-input.c` (`hid_keyboard[256]` and the
//! `HID_UP_KEYBOARD` arm of `hidinput_configure_usage`). That file is GPL-2.0-or-later; this port is
//! distributed under GPL-2.0-only, as the "or later" option permits.
//! Copyright (c) 2000-2001 Vojtech Pavlik; Copyright (c) 2006-2010 Jiri Kosina.
//!
//! The inverse ([`keycode_to_usage`]) is what lets a keyboard that speaks Linux keycodes (an AT keyboard
//! decoded by `atkbd-scancode-core`) enter a host whose key path speaks HID usages: the FIRST usage that
//! maps to a keycode wins, so the documented twins (0x31/0x32 both `KEY_BACKSLASH`; 0x4c/0x9c/0xd8 all
//! `KEY_DELETE`; the media tail past 0xe7 repeating 0x7f-0x81) resolve to the lowest usage.

#![no_std]
#![forbid(unsafe_code)]

/// `KEY_UNKNOWN`, the table's `unk`.
pub const KEY_UNKNOWN: u8 = 240; // input-event-codes.h:325
/// The keyboard usage page.
pub const HID_UP_KEYBOARD: u32 = 0x0007_0000; // hid.h:152
/// The table's length: usages at or past it map to `KEY_UNKNOWN`.
pub const TABLE_LEN: usize = 256; // hid-input.c:27

const UNK: u8 = KEY_UNKNOWN;

/// `hid_keyboard[256]`: index = keyboard-page usage, value = Linux keycode (0 = ignored).
pub const HID_KEYBOARD: [u8; TABLE_LEN] = [
      0,   0,   0,   0,  30,  48,  46,  32,  18,  33,  34,  35,  23,  36,  37,  38, // hid-input.c:28
     50,  49,  24,  25,  16,  19,  31,  20,  22,  47,  17,  45,  21,  44,   2,   3, // hid-input.c:29
      4,   5,   6,   7,   8,   9,  10,  11,  28,   1,  14,  15,  57,  12,  13,  26, // hid-input.c:30
     27,  43,  43,  39,  40,  41,  51,  52,  53,  58,  59,  60,  61,  62,  63,  64, // hid-input.c:31
     65,  66,  67,  68,  87,  88,  99,  70, 119, 110, 102, 104, 111, 107, 109, 106, // hid-input.c:32
    105, 108, 103,  69,  98,  55,  74,  78,  96,  79,  80,  81,  75,  76,  77,  71, // hid-input.c:33
     72,  73,  82,  83,  86, 127, 116, 117, 183, 184, 185, 186, 187, 188, 189, 190, // hid-input.c:34
    191, 192, 193, 194, 134, 138, 130, 132, 128, 129, 131, 137, 133, 135, 136, 113, // hid-input.c:35
    115, 114, UNK, UNK, UNK, 121, UNK,  89,  93, 124,  92,  94,  95, UNK, UNK, UNK, // hid-input.c:36
    122, 123,  90,  91,  85, UNK, UNK, UNK, UNK, UNK, UNK, UNK, 111, UNK, UNK, UNK, // hid-input.c:37
    UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, // hid-input.c:38
    UNK, UNK, UNK, UNK, UNK, UNK, 179, 180, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, // hid-input.c:39
    UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, // hid-input.c:40
    UNK, UNK, UNK, UNK, UNK, UNK, UNK, UNK, 111, UNK, UNK, UNK, UNK, UNK, UNK, UNK, // hid-input.c:41
     29,  42,  56, 125,  97,  54, 100, 126, 164, 166, 165, 163, 161, 115, 114, 113, // hid-input.c:42
    150, 158, 159, 128, 136, 177, 178, 176, 142, 152, 173, 140, UNK, UNK, UNK, UNK, // hid-input.c:43
];

/// The keycode for a keyboard-page usage, as `hidinput_configure_usage` maps it (hid-input.c:752-756):
/// `None` for a usage the table leaves 0 ("goto ignore"), `KEY_UNKNOWN` past the table.
pub fn usage_to_keycode(usage: u16) -> Option<u16> {
    if (usage as usize) < TABLE_LEN {
        match HID_KEYBOARD[usage as usize] {
            0 => None,
            k => Some(k as u16),
        }
    } else {
        Some(KEY_UNKNOWN as u16)
    }
}

/// The lowest keyboard-page usage whose keycode is `keycode`; `None` for 0, `KEY_UNKNOWN`, or a keycode no
/// usage maps to.
pub fn keycode_to_usage(keycode: u16) -> Option<u8> {
    if keycode == 0 || keycode == KEY_UNKNOWN as u16 || keycode > u8::MAX as u16 {
        return None;
    }
    HID_KEYBOARD.iter().position(|&k| k as u16 == keycode).map(|u| u as u8)
}
