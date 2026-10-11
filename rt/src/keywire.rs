//! Published with the architect's ruling (Astra, 2026-10-11): the PHYSICAL KEY EVENT frame. MIT OR Apache-2.0.
//!
//! A keyboard `.device` reports which physical key changed, not what text it types: layout, modifiers, CapsLock, text
//! generation and any top-row action policy belong to the input layer that reads the endpoint. The frame rides the same
//! endpoint the USB HID keyboard path posts to (`KEY_EP`, number 12), told apart from that path's UTF-8 text by its lead
//! byte: [`MARKER`] (0x90) lies in the UTF-8 continuation range, so it can never begin a text sequence.
//!
//! FRAME (one `SEND`, exactly [`LEN`] bytes):  `[ MARKER, usage, state ]`
//!   usage  a USB HID Keyboard/Keypad page (0x07) usage, 1..=0xE7; the modifiers are 0xE0 LeftCtrl · 0xE1 LeftShift ·
//!          0xE2 LeftAlt · 0xE3 LeftMeta · 0xE4 RightCtrl · 0xE5 RightShift · 0xE6 RightAlt · 0xE7 RightMeta
//!   state  0 RELEASE · 1 PRESS · 2 REPEAT (typematic: the key is still down) · 3 ALL_UP (every key is up; `usage` is 0 —
//!          a resynchronisation, sent after the device dropped a frame, so a lost release cannot leave a key held)

/// The endpoint a keyboard device posts to.
pub const KEY_EP: u16 = 12;
/// The frame's lead byte.
pub const MARKER: u8 = 0x90;
/// The frame's length.
pub const LEN: usize = 3;

pub const RELEASE: u8 = 0;
pub const PRESS: u8 = 1;
pub const REPEAT: u8 = 2;
pub const ALL_UP: u8 = 3;

/// One decoded frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyFrame {
    pub usage: u8,
    pub state: u8,
}

pub const fn encode(usage: u8, state: u8) -> [u8; LEN] {
    [MARKER, usage, state]
}

/// The frame in `msg`, or None when it is not one (wrong length, wrong lead byte, a state past [`ALL_UP`]).
pub fn decode(msg: &[u8]) -> Option<KeyFrame> {
    match *msg {
        [MARKER, usage, state] if state <= ALL_UP => Some(KeyFrame { usage, state }),
        _ => None,
    }
}
