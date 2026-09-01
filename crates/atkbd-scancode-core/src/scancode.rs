// SPDX-License-Identifier: GPL-2.0-only
//! AT keyboard protocol literals and Linux-compatible scancode encoding.
//!
//! Ported from Linux `drivers/input/keyboard/atkbd.c`.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux atkbd authors.

pub const RET_ACK: u8 = 0xfa; // atkbd.c:152
pub const RET_NAK: u8 = 0xfe; // atkbd.c:153
pub const RET_BAT: u8 = 0xaa; // atkbd.c:154
pub const RET_EMUL0: u8 = 0xe0; // atkbd.c:155
pub const RET_EMUL1: u8 = 0xe1; // atkbd.c:156
pub const RET_RELEASE: u8 = 0xf0; // atkbd.c:157
pub const RET_HANJA: u8 = 0xf1; // atkbd.c:158
pub const RET_HANGEUL: u8 = 0xf2; // atkbd.c:159
pub const RET_ERR: u8 = 0xff; // atkbd.c:160

/// One protocol byte that translated mode must distinguish from its high-bit partner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XlateByte {
    pub name: &'static str,
    pub code: u8,
}

/// Linux's six translated-mode response bytes, in bit order within `xl_bit`.
pub const XLATE_BYTES: [XlateByte; 6] = [ // atkbd.c:340-343
    XlateByte { name: "BAT", code: RET_BAT }, // atkbd.c:341
    XlateByte { name: "ERR", code: RET_ERR }, // atkbd.c:341
    XlateByte { name: "ACK", code: RET_ACK }, // atkbd.c:341
    XlateByte { name: "NAK", code: RET_NAK }, // atkbd.c:342
    XlateByte { name: "HANJA", code: RET_HANJA }, // atkbd.c:342
    XlateByte { name: "HANGEUL", code: RET_HANGEUL }, // atkbd.c:342
];

/// Named refusal from compatibility scancode encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScancodeError {
    UnsupportedScancodeSet { set: u8, supported: [u8; 2] },
    EmulationDepthOutOfRange { emul: u8, maximum: u8 },
}

/// Encode the byte, E0 state, and high bit as Linux `atkbd_compat_scancode` does.
pub fn compat_scancode(set: u8, emul: u8, code: u8) -> Result<u16, ScancodeError> {
    if emul > 2 {
        return Err(ScancodeError::EmulationDepthOutOfRange { emul, maximum: 2 });
    }
    let mut code = code as u16;
    match set {
        3 => {
            if emul == 1 { code |= 0x100; } // atkbd.c:389-391
        }
        2 => {
            code = (code & 0x7f) | ((code & 0x80) << 1); // atkbd.c:393
            if emul == 1 { code |= 0x80; } // atkbd.c:394-395
        }
        _ => return Err(ScancodeError::UnsupportedScancodeSet { set, supported: [2, 3] }),
    }
    Ok(code)
}

/// Whether translated mode should extract the byte's high bit as a break flag.
pub fn needs_xlate(xl_bits: u8, code: u8) -> bool {
    if code == RET_EMUL0 || code == RET_EMUL1 { return false; } // atkbd.c:353-354
    if let Some(index) = XLATE_BYTES.iter().position(|entry| entry.code == code) {
        return xl_bits & (1 << index) != 0; // atkbd.c:356-358
    }
    true // atkbd.c:360
}

/// Update translated-mode response tracking from a make/break byte pair.
pub fn calculate_xl_bits(mut xl_bits: u8, code: u8) -> u8 {
    if let Some(index) = XLATE_BYTES
        .iter()
        .position(|entry| ((code ^ entry.code) & 0x7f) == 0)
    {
        if code & 0x80 != 0 { xl_bits &= !(1 << index); } // atkbd.c:374-375
        else { xl_bits |= 1 << index; } // atkbd.c:376-377
    }
    xl_bits
}
