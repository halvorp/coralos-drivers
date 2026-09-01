// SPDX-License-Identifier: GPL-2.0-only
//! Stateful AT/PS2 make/break and E0/E1 sequence decoding.
//!
//! Ported from Linux `drivers/input/keyboard/atkbd.c`, especially `atkbd_receive_byte`.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux atkbd authors.

use crate::scancode::{
    calculate_xl_bits, compat_scancode, needs_xlate, ScancodeError, RET_ACK, RET_BAT, RET_EMUL0,
    RET_EMUL1, RET_ERR, RET_HANGEUL, RET_HANJA, RET_NAK, RET_RELEASE,
};
use crate::tables::{raw_keycode, translated_keycode, LookupError, KEY_NULL, KEY_UNKNOWN};

/// Decoder configuration fixed when Linux selects the keyboard's scancode set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub set: u8,
    pub translated: bool,
    pub scroll: bool,
}

/// Named refusal from decoder construction or table access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    UnsupportedScancodeSet { set: u8, supported: [u8; 2] },
    Scancode(ScancodeError),
    Keycode(LookupError),
}

/// Whether a decoded key byte is a make or break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Make,
    Break,
}

/// One fully decoded keyboard key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub scancode: u16,
    pub keycode: u16,
    pub state: KeyState,
}

/// Result of consuming one PS/2 byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeResult {
    Prefix,
    Key(KeyEvent),
    Unknown { scancode: u16, state: KeyState },
    Null { scancode: u16, state: KeyState },
    BasicAssuranceTest,
    ProtocolResponse { name: &'static str, byte: u8 },
    TooManyKeys,
}

/// Linux atkbd's interrupt-side sequence state, with no hardware or input I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoder {
    config: Config,
    emul: u8,
    release: bool,
    xl_bits: u8,
}

impl Decoder {
    /// Construct a decoder for Linux-supported set 2 or set 3 operation.
    pub fn new(config: Config) -> Result<Self, DecodeError> {
        if config.set != 2 && config.set != 3 {
            return Err(DecodeError::UnsupportedScancodeSet { set: config.set, supported: [2, 3] });
        }
        Ok(Self { config, emul: 0, release: false, xl_bits: 0 })
    }

    /// Reset partial E0/E1, release, and translated-response state after reconnect.
    pub fn reset(&mut self) {
        self.emul = 0; // atkbd.c:1400
        self.release = false;
        self.xl_bits = 0; // atkbd.c:1399
    }

    /// Consume one raw keyboard byte using Linux's receive-state ordering.
    pub fn feed(&mut self, data: u8) -> Result<DecodeResult, DecodeError> {
        let mut code = data;
        if self.config.translated {
            if self.emul != 0 || needs_xlate(self.xl_bits, code) {
                self.release = code >> 7 != 0; // atkbd.c:464-465
                code &= 0x7f; // atkbd.c:466
            }
            if self.emul == 0 { self.xl_bits = calculate_xl_bits(self.xl_bits, data); } // atkbd.c:469-470
        }

        match code {
            RET_BAT => return Ok(DecodeResult::BasicAssuranceTest), // atkbd.c:473-477
            RET_EMUL0 => { self.emul = 1; return Ok(DecodeResult::Prefix); } // atkbd.c:478-480
            RET_EMUL1 => { self.emul = 2; return Ok(DecodeResult::Prefix); } // atkbd.c:481-483
            RET_RELEASE => { self.release = true; return Ok(DecodeResult::Prefix); } // atkbd.c:484-486
            RET_ACK => return Ok(DecodeResult::ProtocolResponse { name: "ACK", byte: RET_ACK }), // atkbd.c:487
            RET_NAK => return Ok(DecodeResult::ProtocolResponse { name: "NAK", byte: RET_NAK }), // atkbd.c:488
            RET_ERR => return Ok(DecodeResult::TooManyKeys), // atkbd.c:495-498
            _ => {}
        }

        let scancode = compat_scancode(self.config.set, self.emul, code)
            .map_err(DecodeError::Scancode)?; // atkbd.c:502
        if self.emul != 0 {
            self.emul -= 1;
            if self.emul != 0 { return Ok(DecodeResult::Prefix); } // atkbd.c:504-505
        }
        let state = if self.release { KeyState::Break } else { KeyState::Make };
        let keycode = if self.config.translated {
            translated_keycode(scancode, self.config.scroll)
        } else {
            raw_keycode(self.config.set, scancode, self.config.scroll)
        }.map_err(DecodeError::Keycode)?;
        self.release = false; // atkbd.c:580

        Ok(match keycode {
            KEY_NULL => DecodeResult::Null { scancode, state }, // atkbd.c:513-515
            KEY_UNKNOWN => DecodeResult::Unknown { scancode, state }, // atkbd.c:516-525
            _ => DecodeResult::Key(KeyEvent { scancode, keycode, state }),
        })
    }

    /// Current E0/E1 prefix depth, exposed for deterministic state-machine driving.
    pub fn emulation_depth(&self) -> u8 { self.emul }

    /// Current pending break state.
    pub fn release_pending(&self) -> bool { self.release }

    /// Current translated response-pair bits.
    pub fn xlate_bits(&self) -> u8 { self.xl_bits }
}

/// Linux forces these two Korean keys to release immediately after make.
pub fn is_forced_release_protocol_key(byte: u8) -> bool {
    byte == RET_HANGEUL || byte == RET_HANJA // atkbd.c:1159-1170
}
