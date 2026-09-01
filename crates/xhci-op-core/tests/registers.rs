// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for operational register fields and word construction.
//!
//! Ported from Linux `drivers/usb/host/xhci.c`, `drivers/usb/host/xhci.h`,
//! `drivers/usb/host/xhci-caps.h`, and `drivers/usb/host/xhci-ext-caps.h`.
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

use xhci_op_core::registers::{self, command, offset, status};

#[test]
fn all_eight_operational_register_names_and_offsets_match_linux() {
    // xhci.h:92-118. Literal list: never generated from production constants.
    assert_eq!(registers::OP_REGISTER_NAMES.len(), 8);
    assert_eq!(
        registers::OP_REGISTER_NAMES,
        [
            "USBCMD",
            "USBSTS",
            "PAGESIZE",
            "DNCTRL",
            "CRCR",
            "DCBAAP",
            "CONFIG",
            "PORT_REGS"
        ]
    );
    assert_eq!(
        [
            offset::USBCMD,
            offset::USBSTS,
            offset::PAGESIZE,
            offset::DNCTRL,
            offset::CRCR,
            offset::DCBAAP,
            offset::CONFIG,
            offset::PORT_REGS
        ],
        [0x00, 0x04, 0x08, 0x14, 0x18, 0x30, 0x38, 0x400]
    );
}

#[test]
fn all_ten_usbcmd_fields_match_linux() {
    // xhci.h:121-148; aliases have their literals in xhci-ext-caps.h:74-80.
    assert_eq!(registers::USBCMD_FIELD_NAMES.len(), 10);
    assert_eq!(
        registers::USBCMD_FIELD_NAMES,
        [
            "RUN",
            "RESET",
            "EVENT_INTERRUPT_ENABLE",
            "HOST_SYSTEM_ERROR_INTERRUPT_ENABLE",
            "LIGHT_RESET",
            "SAVE_STATE",
            "RESTORE_STATE",
            "ENABLE_WRAP_EVENT",
            "MFINDEX_POWER_MANAGEMENT",
            "EXTENDED_TBC_ENABLE",
        ]
    );
    assert_eq!(
        [
            command::RUN,
            command::RESET,
            command::EVENT_INTERRUPT_ENABLE,
            command::HOST_SYSTEM_ERROR_INTERRUPT_ENABLE,
            command::LIGHT_RESET,
            command::SAVE_STATE,
            command::RESTORE_STATE,
            command::ENABLE_WRAP_EVENT,
            command::MFINDEX_POWER_MANAGEMENT,
            command::EXTENDED_TBC_ENABLE,
        ],
        [
            1 << 0,
            1 << 1,
            1 << 2,
            1 << 3,
            1 << 7,
            1 << 8,
            1 << 9,
            1 << 10,
            1 << 11,
            1 << 14
        ]
    );
    assert_eq!(command::INTERRUPTS, 0x40c); // xhci-ext-caps.h:82
}

#[test]
fn all_nine_usbsts_fields_match_linux() {
    // xhci.h:154-173; aliases have their literals in xhci-ext-caps.h:14,85.
    assert_eq!(registers::USBSTS_FIELD_NAMES.len(), 9);
    assert_eq!(
        registers::USBSTS_FIELD_NAMES,
        [
            "HALTED",
            "HOST_SYSTEM_ERROR",
            "EVENT_INTERRUPT",
            "PORT_CHANGE",
            "SAVING_STATE",
            "RESTORING_STATE",
            "SAVE_RESTORE_ERROR",
            "CONTROLLER_NOT_READY",
            "HOST_CONTROLLER_ERROR",
        ]
    );
    assert_eq!(
        [
            status::HALTED,
            status::HOST_SYSTEM_ERROR,
            status::EVENT_INTERRUPT,
            status::PORT_CHANGE,
            status::SAVING_STATE,
            status::RESTORING_STATE,
            status::SAVE_RESTORE_ERROR,
            status::CONTROLLER_NOT_READY,
            status::HOST_CONTROLLER_ERROR,
        ],
        [
            1 << 0,
            1 << 2,
            1 << 3,
            1 << 4,
            1 << 8,
            1 << 9,
            1 << 10,
            1 << 11,
            1 << 12
        ]
    );
}

#[test]
fn quiesce_preserves_run_only_when_halt_was_already_observed() {
    // xhci.c:107-117: IRQ bits always clear; CMD_RUN clears only when STS_HALT is absent.
    assert_eq!(registers::quiesce_command(0xffff_ffff, 0), 0xffff_fbf2);
    assert_eq!(registers::quiesce_command(0xffff_ffff, 1 << 0), 0xffff_fbf3);
}

#[test]
fn start_reset_and_interrupt_words_preserve_unrelated_bits() {
    // xhci.c:157,209,609.
    assert_eq!(registers::start_command(0x8000_0100), 0x8000_0101);
    assert_eq!(registers::reset_command(0x8000_0100), 0x8000_0102);
    assert_eq!(registers::enable_event_interrupt(0x8000_0100), 0x8000_0104);
}

#[test]
fn config_and_dcbaap_program_linux_values() {
    // xhci-caps.h:19; xhci.c:493-497,570-571; xhci.h:36,203,205.
    assert_eq!(registers::CONFIG_MAX_SLOTS_MASK, 0xff);
    assert_eq!(registers::CONFIG_U3_ENTRY_ENABLE, 1 << 8);
    assert_eq!(registers::CONFIG_INFORMATION_ENABLE, 1 << 9);
    assert_eq!(registers::MAX_HC_SLOTS, 256);
    assert_eq!(
        registers::program_config_slots(0xa5a5_03aa, 0x44),
        0xa5a5_0344
    );
    assert_eq!(
        registers::program_dcbaap(0x1234_5678_9abc_d000),
        0x1234_5678_9abc_d000
    );
}

#[test]
fn status_decode_vectors_cover_each_public_decoder() {
    // xhci.c:133-135,222,235.
    assert!(registers::is_halted(0x1));
    assert!(!registers::is_halted(0x1000));
    assert!(registers::reset_in_progress(0x2));
    assert!(!registers::reset_in_progress(0x4));
    assert!(registers::controller_not_ready(0x800));
    assert!(!registers::controller_not_ready(0x400));
}
