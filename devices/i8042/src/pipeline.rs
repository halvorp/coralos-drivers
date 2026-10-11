// SPDX-License-Identifier: GPL-2.0-only
//! Keyboard bytes -> physical key events. The decoder (`atkbd-scancode-core`) turns bytes into Linux keycodes with
//! make/break; `hid-input-keyboard-core` maps each keycode to the USB HID usage the input path speaks; this state turns
//! make/break into PRESS / REPEAT / RELEASE by remembering which usages are down. No modifier, layout or text decision is
//! made here: a Shift key is just usage 0xE1.
use atkbd_scancode_core::decode::{DecodeResult, Decoder, KeyState};
use coralos_rt::keywire::{ALL_UP, PRESS, RELEASE, REPEAT};
use hid_input_keyboard_core::keycode_to_usage;

/// One event for the wire: (usage, state).
pub type Event = (u8, u8);

/// What the pipeline has seen that was NOT a key event. Counted, and reported by the device at intervals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// A key the HID table has no usage for (or the decoder named unknown / null).
    pub unmapped: u32,
    /// A release for a key that was not down.
    pub stray_release: u32,
    /// A decode refusal (an impossible table lookup).
    pub decode_errors: u32,
    /// A keyboard reset / error byte: every key was declared up.
    pub resets: u32,
}

pub struct Pipeline {
    dec: Decoder,
    down: [u8; 32],
    pub counts: Counts,
}

impl Pipeline {
    pub const fn new(dec: Decoder) -> Self {
        Pipeline { dec, down: [0; 32], counts: Counts { unmapped: 0, stray_release: 0, decode_errors: 0, resets: 0 } }
    }

    fn is_down(&self, usage: u8) -> bool {
        self.down[(usage >> 3) as usize] & (1 << (usage & 7)) != 0
    }
    fn set_down(&mut self, usage: u8, down: bool) {
        let (i, m) = ((usage >> 3) as usize, 1u8 << (usage & 7));
        if down {
            self.down[i] |= m;
        } else {
            self.down[i] &= !m;
        }
    }
    pub fn any_down(&self) -> bool {
        self.down.iter().any(|&b| b != 0)
    }
    /// Every usage currently down, in ascending order, for a resynchronisation after a dropped frame.
    pub fn for_each_down(&self, mut f: impl FnMut(u8)) {
        for u in 0..=255u8 {
            if self.is_down(u) {
                f(u);
            }
        }
    }

    /// Feed one byte from the keyboard; `out` gets the event it completes, if any.
    pub fn feed(&mut self, byte: u8, out: &mut dyn FnMut(Event)) {
        match self.dec.feed(byte) {
            Ok(DecodeResult::Key(k)) => {
                let Some(usage) = keycode_to_usage(k.keycode) else {
                    self.counts.unmapped += 1;
                    return;
                };
                match k.state {
                    KeyState::Make => {
                        let state = if self.is_down(usage) { REPEAT } else { PRESS };
                        self.set_down(usage, true);
                        out((usage, state));
                    }
                    KeyState::Break => {
                        if self.is_down(usage) {
                            self.set_down(usage, false);
                            out((usage, RELEASE));
                        } else {
                            self.counts.stray_release += 1;
                        }
                    }
                }
            }
            // The keyboard says it reset itself (BAT) or overran (0xFF): whatever was down is no longer known to be.
            Ok(DecodeResult::BasicAssuranceTest) | Ok(DecodeResult::TooManyKeys) => {
                self.counts.resets += 1;
                if self.any_down() {
                    self.down = [0; 32];
                    out((0, ALL_UP));
                }
            }
            Ok(DecodeResult::Unknown { .. }) | Ok(DecodeResult::Null { .. }) => self.counts.unmapped += 1,
            Ok(DecodeResult::Prefix) | Ok(DecodeResult::ProtocolResponse { .. }) => {}
            Err(_) => self.counts.decode_errors += 1,
        }
    }
}
