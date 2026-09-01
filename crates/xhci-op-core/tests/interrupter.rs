// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for interrupter moderation and event-ring register fields.
//!
//! Ported from Linux `drivers/usb/host/xhci.c`, `drivers/usb/host/xhci.h`,
//! `drivers/usb/host/xhci-mem.c`, and `drivers/usb/host/xhci-ring.c`.
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

use xhci_op_core::interrupter::*;

#[test]
fn all_five_interrupter_registers_have_linux_offsets() {
    // xhci.h:210-230. Literal list is not generated from production.
    assert_eq!(INTERRUPTER_REGISTER_NAMES.len(), 5);
    assert_eq!(
        INTERRUPTER_REGISTER_NAMES,
        ["IMAN", "IMOD", "ERSTSZ", "ERSTBA", "ERDP"]
    );
    assert_eq!(
        [
            offset::IMAN,
            offset::IMOD,
            offset::ERSTSZ,
            offset::ERSTBA,
            offset::ERDP
        ],
        [0x00, 0x04, 0x08, 0x10, 0x18]
    );
    assert_eq!(offset::REGISTER_SET_BYTES, 0x20);
}

#[test]
fn all_nine_interrupter_fields_match_linux() {
    // xhci.h:233-272.
    assert_eq!(INTERRUPTER_FIELD_NAMES.len(), 9);
    assert_eq!(
        INTERRUPTER_FIELD_NAMES,
        [
            "IMAN_PENDING",
            "IMAN_ENABLE",
            "IMOD_INTERVAL",
            "IMOD_COUNTER",
            "ERST_SIZE",
            "ERST_BASE",
            "ERDP_SEGMENT_INDEX",
            "ERDP_EVENT_HANDLER_BUSY",
            "ERDP_POINTER",
        ]
    );
    assert_eq!(IMAN_PENDING, 1 << 0);
    assert_eq!(IMAN_ENABLE, 1 << 1);
    assert_eq!(IMOD_INTERVAL_MASK, 0xffff);
    assert_eq!(IMOD_COUNTER_MASK, 0xffff_0000);
    assert_eq!(ERST_SIZE_MASK, 0xffff);
    assert_eq!(ERST_BASE_ADDRESS_MASK, 0xffff_ffff_ffff_ffc0);
    assert_eq!(ERDP_SEGMENT_INDEX_MASK, 0x7);
    assert_eq!(ERDP_EVENT_HANDLER_BUSY, 1 << 3);
    assert_eq!(ERDP_POINTER_MASK, 0xffff_ffff_ffff_fff0);
}

#[test]
fn iman_enable_and_disable_do_not_accidentally_ack_pending() {
    // xhci.c:319-323,336-340. Both writes clear IP in the value before writing its RW1C bit.
    assert_eq!(enable_interrupter(0xffff_ffff), 0xffff_fffe);
    assert_eq!(disable_interrupter(0xffff_ffff), 0xffff_fffc);
}

#[test]
fn imod_uses_250ns_truncation_saturation_and_preserves_counter() {
    // xhci.c:350-365; xhci.h:240-250.
    assert_eq!(IMOD_QUANTUM_NS, 250);
    assert_eq!(
        program_moderation(0xabcd_1234, 1_000_000),
        ModerationProgramming {
            word: 0xabcd_0fa0,
            interval_units: 4_000,
            saturated: false
        }
    );
    assert_eq!(
        program_moderation(0xabcd_1234, 499),
        ModerationProgramming {
            word: 0xabcd_0001,
            interval_units: 1,
            saturated: false
        }
    );
    assert_eq!(
        program_moderation(0xabcd_1234, u32::MAX),
        ModerationProgramming {
            word: 0xabcd_ffff,
            interval_units: 0xffff,
            saturated: true
        }
    );
}

#[test]
fn erst_entries_and_registers_match_linux() {
    // xhci.h:1259,1385-1391; xhci-mem.c:1807-1814,2331-2342.
    assert_eq!(EVENT_RING_SEGMENT_TRBS, 256);
    assert_eq!(
        erst_entry(0x1234_5678_9abc_d000),
        ErstEntry {
            segment_address: 0x1234_5678_9abc_d000,
            segment_size_trbs: 256,
            reserved: 0,
        }
    );
    assert_eq!(program_erst_size(0xa5a5_beef, 2), 0xa5a5_0002);
    assert_eq!(
        program_erst_base(0xaaaa_0000_0000_003f, 0x1234_5678_9abc_def7),
        0x1234_5678_9abc_deff
    );
    assert_eq!(DEFAULT_ERST_SEGMENTS, 2); // xhci.h:1413
    assert_eq!(MAX_INTERRUPTERS, 128); // xhci.h:46
}

#[test]
fn erdp_contains_desi_pointer_and_optional_ehb_clear() {
    // xhci-ring.c:3058-3064.
    assert_eq!(
        program_erdp(5, 0x1234_5678_9abc_def7, false),
        0x1234_5678_9abc_def5
    );
    assert_eq!(
        program_erdp(5, 0x1234_5678_9abc_def7, true),
        0x1234_5678_9abc_defd
    );
    assert_eq!(erdp_pointer(0x1234_5678_9abc_defd), 0x1234_5678_9abc_def0);
}

#[test]
fn unchanged_erdp_is_skipped_only_when_ehb_need_not_be_cleared() {
    // xhci-ring.c:3054-3055.
    assert!(!erdp_update_needed(
        0x1234_5678_9abc_def3,
        0x1234_5678_9abc_def9,
        false
    ));
    assert!(erdp_update_needed(
        0x1234_5678_9abc_def3,
        0x1234_5678_9abc_def9,
        true
    ));
    assert!(erdp_update_needed(
        0x1234_5678_9abc_def3,
        0x1234_5678_9abc_dee9,
        false
    ));
}
