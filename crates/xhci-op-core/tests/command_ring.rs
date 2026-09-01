// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for CRCR construction and command abort semantics.
//!
//! Ported from Linux `drivers/usb/host/xhci.c`, `drivers/usb/host/xhci.h`, and
//! `drivers/usb/host/xhci-ring.c`.
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

use xhci_op_core::command_ring::*;

#[test]
fn all_five_crcr_fields_and_names_match_linux() {
    // xhci.h:187-197. Literal names are frozen independently of the production table.
    assert_eq!(CRCR_FIELD_NAMES.len(), 5);
    assert_eq!(
        CRCR_FIELD_NAMES,
        ["CYCLE", "PAUSE", "ABORT", "RUNNING", "POINTER"]
    );
    assert_eq!(
        [CYCLE, PAUSE, ABORT, RUNNING],
        [1 << 0, 1 << 1, 1 << 2, 1 << 3]
    );
    assert_eq!(POINTER_MASK, 0xffff_ffff_ffff_ffc0);
}

#[test]
fn pointer_programming_masks_both_address_and_cycle() {
    // xhci.c:502-510. Low non-pointer bits survive except cycle, which is replaced.
    assert_eq!(
        program_pointer(0xaaaa_0000_0000_000e, 0x1234_5678_9abc_def7, true),
        0x1234_5678_9abc_decf
    );
    assert_eq!(
        program_pointer(0xaaaa_0000_0000_000f, 0x1234_5678_9abc_def7, false),
        0x1234_5678_9abc_dece
    );
}

#[test]
fn abort_write_has_a_valid_full_pointer_and_abort_only() {
    // xhci-ring.c:497-515: all 64 bits are written, pointer-aligned, with CA.
    assert_eq!(abort_word(0x1234_5678_9abc_def7), 0x1234_5678_9abc_dec4);
}

#[test]
fn running_and_abort_permission_require_hardware_and_software_agreement() {
    // xhci-ring.c:1769-1773.
    assert!(is_running(1 << 3));
    assert!(!is_running(1 << 2));
    assert!(abort_permitted(true, 1 << 3));
    assert!(!abort_permitted(false, 1 << 3));
    assert!(!abort_permitted(true, 0));
}

#[test]
fn abort_wait_budgets_match_linux_literals() {
    // xhci-ring.c:521-523 and :531-538.
    assert_eq!(ABORT_TIMEOUT_US, 5_000_000);
    assert_eq!(ABORT_EVENT_TIMEOUT_MS, 2_000);
}
