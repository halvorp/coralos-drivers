// SPDX-License-Identifier: GPL-2.0-only
//! CRCR field construction and abort decisions, ported from Linux
//! `drivers/usb/host/xhci.c:496-:512`, `drivers/usb/host/xhci.h:187-:197`, and
//! `drivers/usb/host/xhci-ring.c:493-:535, :1763-:1777`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

pub const CYCLE: u64 = 1 << 0; // xhci.h:189
pub const PAUSE: u64 = 1 << 1; // xhci.h:191
pub const ABORT: u64 = 1 << 2; // xhci.h:193
pub const RUNNING: u64 = 1 << 3; // xhci.h:195
pub const POINTER_MASK: u64 = 0xffff_ffff_ffff_ffc0; // xhci.h:197
pub const ABORT_TIMEOUT_US: u64 = 5_000_000; // xhci-ring.c:521-523
pub const ABORT_EVENT_TIMEOUT_MS: u32 = 2_000; // xhci-ring.c:531-538

/// Names of every CRCR control field Linux defines.
pub const CRCR_FIELD_NAMES: [&str; 5] = ["CYCLE", "PAUSE", "ABORT", "RUNNING", "POINTER"];
// xhci.h:187-197

/// Construct CRCR's command-ring pointer and producer cycle while preserving its non-pointer bits,
/// exactly as `xhci_set_cmd_ring_deq` does (xhci.c:496-:512).
pub const fn program_pointer(current_crcr: u64, dequeue_dma: u64, cycle_state: bool) -> u64 {
    let mut crcr = (current_crcr & !POINTER_MASK) | (dequeue_dma & POINTER_MASK);
    crcr &= !CYCLE;
    if cycle_state {
        crcr |= CYCLE;
    }
    crcr
}

/// Build the 64-bit CRCR abort write. Linux writes a valid next-command pointer together with CA
/// because some controllers require all 64 bits (`xhci_abort_cmd_ring`, xhci-ring.c:497-:515).
pub const fn abort_word(next_command_dma: u64) -> u64 {
    (next_command_dma & POINTER_MASK) | ABORT
}

/// Whether CRCR reports the command ring running (xhci-ring.c:521-:523, :1769-:1770).
pub const fn is_running(crcr: u64) -> bool {
    crcr & RUNNING != 0
}

/// Whether both Linux's software state and CRCR permit starting an abort. Testing CRCR alone can
/// race a software-side transition that deliberately blocks new command doorbells
/// (xhci-ring.c:1763-:1777).
pub const fn abort_permitted(software_running: bool, crcr: u64) -> bool {
    software_running && is_running(crcr)
}
