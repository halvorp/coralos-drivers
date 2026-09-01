// SPDX-License-Identifier: GPL-2.0-only
//! Frozen AT keyboard keycode maps from Linux `drivers/input/keyboard/atkbd.c`, with Korean
//! keycode literals from `include/uapi/linux/input-event-codes.h`.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux atkbd authors.

/// Number of entries in every keyboard keycode map.
pub const KEYMAP_SIZE: usize = 512; // atkbd.c:76
/// Linux input keycode used for an unmapped scancode.
pub const KEY_UNKNOWN: u16 = 0; // atkbd.c:162
/// Linux atkbd sentinel: consume the scancode but emit no key event.
pub const KEY_NULL: u16 = 255; // atkbd.c:163
/// Linux input keycode assigned to the protocol Hangeul byte.
pub const KEY_HANGEUL: u16 = 122; // input-event-codes.h:199
/// Linux input keycode assigned to the protocol Hanja byte.
pub const KEY_HANJA: u16 = 123; // input-event-codes.h:201

/// Default raw set 2 scancode-to-keycode map. Omitted C initializers are explicit zeroes here.
pub const SET2_KEYCODES: [u16; KEYMAP_SIZE] = [ // atkbd.c:78-107
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

/// Default raw set 3 scancode-to-keycode map. Omitted C initializers are explicit zeroes here.
pub const SET3_KEYCODES: [u16; KEYMAP_SIZE] = [ // atkbd.c:109-123
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

/// Set-1-to-set-2 conversion used when the controller translates scancodes.
pub const UNXLATE: [u8; 128] = [ // atkbd.c:125-134
      0, 118,  22,  30,  38,  37,  46,  54,  61,  62,  70,  69,  78,  85, 102,  13,
     21,  29,  36,  45,  44,  53,  60,  67,  68,  77,  84,  91,  90,  20,  28,  27,
     35,  43,  52,  51,  59,  66,  75,  76,  82,  14,  18,  93,  26,  34,  33,  42,
     50,  49,  58,  65,  73,  74,  89, 124,  17,  41,  88,   5,   6,   4,  12,   3,
     11,   2,  10,   1,   9, 119, 126, 108, 117, 125, 123, 107, 115, 116, 121, 105,
    114, 122, 112, 113, 127,  96,  97, 120,   7,  15,  23,  31,  39,  47,  55,  63,
     71,  79,  86,  94,   8,  16,  24,  32,  40,  48,  56,  64,  72,  80,  87, 111,
     19,  25,  57,  81,  83,  92,  95,  98,  99, 100, 101, 103, 104, 106, 109, 110,
];

/// One selectable Linux scancode table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScancodeTable {
    pub set: u8,
    pub name: &'static str,
    pub keycodes: &'static [u16; KEYMAP_SIZE],
}

/// Both scancode sets Linux atkbd offers by name.
pub const SCANCODE_TABLES: [ScancodeTable; 2] = [ // atkbd.c:36-38,78-123
    ScancodeTable { set: 2, name: "set2", keycodes: &SET2_KEYCODES }, // atkbd.c:78
    ScancodeTable { set: 3, name: "set3", keycodes: &SET3_KEYCODES }, // atkbd.c:109
];

/// One special set-2 scroll mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollKey {
    pub name: &'static str,
    pub keycode: u16,
    pub set2_scancode: u8,
}

pub const SCR_1: u16 = 0xfffe; // atkbd.c:165
pub const SCR_2: u16 = 0xfffd; // atkbd.c:166
pub const SCR_4: u16 = 0xfffc; // atkbd.c:167
pub const SCR_8: u16 = 0xfffb; // atkbd.c:168
pub const SCR_CLICK: u16 = 0xfffa; // atkbd.c:169
pub const SCR_LEFT: u16 = 0xfff9; // atkbd.c:170
pub const SCR_RIGHT: u16 = 0xfff8; // atkbd.c:171

/// The seven optional scroll-wheel substitutions, in Linux order.
pub const SCROLL_KEYS: [ScrollKey; 7] = [ // atkbd.c:185-196
    ScrollKey { name: "SCR_1", keycode: SCR_1, set2_scancode: 0xc5 }, // atkbd.c:189
    ScrollKey { name: "SCR_2", keycode: SCR_2, set2_scancode: 0x9d }, // atkbd.c:190
    ScrollKey { name: "SCR_4", keycode: SCR_4, set2_scancode: 0xa4 }, // atkbd.c:191
    ScrollKey { name: "SCR_8", keycode: SCR_8, set2_scancode: 0x9b }, // atkbd.c:192
    ScrollKey { name: "SCR_CLICK", keycode: SCR_CLICK, set2_scancode: 0xe0 }, // atkbd.c:193
    ScrollKey { name: "SCR_LEFT", keycode: SCR_LEFT, set2_scancode: 0xcb }, // atkbd.c:194
    ScrollKey { name: "SCR_RIGHT", keycode: SCR_RIGHT, set2_scancode: 0xd2 }, // atkbd.c:195
];

/// Named refusal from a keycode table lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupError {
    UnsupportedScancodeSet { set: u8, supported: [u8; 2] },
    ScancodeOutOfRange { scancode: u16, maximum: u16 },
}

/// Look up a raw set 2 or set 3 compatibility scancode.
pub fn raw_keycode(set: u8, scancode: u16, scroll: bool) -> Result<u16, LookupError> {
    if scancode as usize >= KEYMAP_SIZE {
        return Err(LookupError::ScancodeOutOfRange { scancode, maximum: 511 });
    }
    let mut keycode = match set {
        2 => match scancode {
            0x172 => KEY_HANGEUL, // atkbd.c:1162-1164
            0x171 => KEY_HANJA, // atkbd.c:1166-1168
            _ => SET2_KEYCODES[scancode as usize],
        },
        3 => match scancode {
            0x0f2 => KEY_HANGEUL, // atkbd.c:1162-1164
            0x0f1 => KEY_HANJA, // atkbd.c:1166-1168
            _ => SET3_KEYCODES[scancode as usize],
        },
        _ => return Err(LookupError::UnsupportedScancodeSet { set, supported: [2, 3] }),
    };
    if set == 2 && scroll {
        if let Some(entry) = SCROLL_KEYS.iter().find(|entry| entry.set2_scancode as u16 == scancode) {
            keycode = entry.keycode;
        }
    }
    Ok(keycode)
}

/// Look up a compatibility scancode in the translated set-2 map.
pub fn translated_keycode(scancode: u16, scroll: bool) -> Result<u16, LookupError> {
    if scancode > 0xff {
        return Err(LookupError::ScancodeOutOfRange { scancode, maximum: 255 });
    }
    let set1 = scancode as u8;
    let mut set2 = UNXLATE[(set1 & 0x7f) as usize] as u16;
    if set1 & 0x80 != 0 { set2 |= 0x80; }
    raw_keycode(2, set2, scroll)
}
