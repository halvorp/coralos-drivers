// SPDX-License-Identifier: GPL-2.0-only
//! The endpoints the device speaks on, as numbers. These are PROPOSED registry entries (2026-10-13; delete-when: the loader's registry assigns them): the CoralOS loader's endpoint registry
//! (`cap_ids`) owns the numbers, `DYN_EP_BASE` is 67 today, so the six below take 67..=72 and the registry's base moves to 73. Nothing but
//! [`crate::rt`] reads them, so a renumbering is a six-line change here and the matching row.
//!
//! * `AVS_REQ` / `AVS_REPLY` - Aura.engine's session with the device (the device RECVs requests, SENDs replies and events).
//! * `AVS_FW_REQ` / `AVS_FW_REPLY` - the firmware channel to the CoralOS-side broker (the device SENDs requests, RECVs replies).
//! * `AVS_JACK` / `AVS_JACK_ACK` - the pad interrupt owner's delivery of the codec's interrupt (RECV) and the device's "serviced, re-arm" (SEND).
//!
//! The jack pair exists because one `WAIT_ANY_IRQ` waits on ONE interrupt line (the cAVS one) plus endpoints; the codec's interrupt is a
//! pad of a controller other devices share, so its owner turns the pad event into a message.
pub const AVS_REQ: u16 = 67;
pub const AVS_REPLY: u16 = 68;
pub const AVS_FW_REQ: u16 = 69;
pub const AVS_FW_REPLY: u16 = 70;
pub const AVS_JACK: u16 = 71;
pub const AVS_JACK_ACK: u16 = 72;

/// The frame the device sends on `AVS_JACK_ACK` after it serviced the codec's interrupt: the magic and the pad's community and index.
pub const JACK_ACK_MAGIC: [u8; 4] = *b"AVSJ";

/// Exit codes (the log line before an exit names the step).
pub mod exit_code {
    pub const PANIC: u64 = 101;
    pub const NO_WINDOW: u64 = 102;
}
