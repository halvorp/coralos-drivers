// SPDX-License-Identifier: GPL-2.0-only
//! The endpoints the device speaks on, as numbers. These are the CoralOS registry's entries (D16): the first five free static endpoints AFTER
//! `EP_FS_VAULT` (69), so 70..=74, and the registry's dynamic base is 75. They were proposed as 67..=71 (D14) and renumbered whenever the registry moved
//! under them; a number here that disagrees with the loader's grants is a device that talks to nobody. Nothing but [`crate::rt`] reads them.
//!
//! * `AVS_LISTEN` - the LISTEN endpoint Aura.engine `connect()`s to. Each connect is a kernel-minted (request, event) session the device serves
//!   (rt `session`), one at a time; the device RECVs requests on the session's request endpoint, SENDs replies and events on its event
//!   endpoint, and gives the pair back with SESSION_RELEASE when the client is gone.
//! * `AVS_FW_REQ` / `AVS_FW_REPLY` - the firmware channel to the CoralOS-side broker (the device SENDs requests, RECVs replies).
//! * `AVS_JACK` / `AVS_JACK_ACK` - the pad interrupt owner's delivery of the codec's interrupt (RECV) and the device's "serviced, re-arm" (SEND).
//!
//! The jack pair exists because one `WAIT_ANY_IRQ` waits on ONE interrupt line (the cAVS one) plus endpoints; the codec's interrupt is a
//! pad of a controller other devices share, so its owner turns the pad event into a message.
pub const AVS_LISTEN: u16 = 70;
pub const AVS_FW_REQ: u16 = 71;
pub const AVS_FW_REPLY: u16 = 72;
pub const AVS_JACK: u16 = 73;
pub const AVS_JACK_ACK: u16 = 74;

/// The frame the device sends on `AVS_JACK_ACK` after it serviced the codec's interrupt.
pub const JACK_ACK_MAGIC: [u8; 4] = *b"AVSJ";

/// Exit codes (the log line before an exit names the step).
pub mod exit_code {
    pub const PANIC: u64 = 101;
    pub const NO_WINDOW: u64 = 102;
}
