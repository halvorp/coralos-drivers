// SPDX-License-Identifier: GPL-2.0-only
//! The endpoints the device speaks on, as numbers. These are PROPOSED registry entries (2026-10-13; delete-when: the loader's registry assigns
//! them): `DYN_EP_BASE` is 67 today, so the five below take 67..=71 and the registry's base moves to 72. Nothing but [`crate::rt`] reads them.
//!
//! * `AVS_LISTEN` - the LISTEN endpoint Aura.engine `connect()`s to. Each connect is a kernel-minted (request, event) session the device serves
//!   (rt `session`), one at a time; the device RECVs requests on the session's request endpoint, SENDs replies and events on its event
//!   endpoint, and gives the pair back with SESSION_RELEASE when the client is gone.
//! * `AVS_FW_REQ` / `AVS_FW_REPLY` - the firmware channel to the CoralOS-side broker (the device SENDs requests, RECVs replies).
//! * `AVS_JACK` / `AVS_JACK_ACK` - the pad interrupt owner's delivery of the codec's interrupt (RECV) and the device's "serviced, re-arm" (SEND).
//!
//! The jack pair exists because one `WAIT_ANY_IRQ` waits on ONE interrupt line (the cAVS one) plus endpoints; the codec's interrupt is a
//! pad of a controller other devices share, so its owner turns the pad event into a message.
pub const AVS_LISTEN: u16 = 67;
pub const AVS_FW_REQ: u16 = 68;
pub const AVS_FW_REPLY: u16 = 69;
pub const AVS_JACK: u16 = 70;
pub const AVS_JACK_ACK: u16 = 71;

/// The frame the device sends on `AVS_JACK_ACK` after it serviced the codec's interrupt.
pub const JACK_ACK_MAGIC: [u8; 4] = *b"AVSJ";

/// Exit codes (the log line before an exit names the step).
pub mod exit_code {
    pub const PANIC: u64 = 101;
    pub const NO_WINDOW: u64 = 102;
}
