// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for PCI header fields and command/status bits.
//!
//! Ported from `include/uapi/linux/pci_regs.h`, `drivers/pci/pci.c`, and
//! `include/linux/pci.h`. Copyright Drew Eckhardt, Martin Mares, and Linux PCI authors.

use pci_config_core::regs::{self, command, status, Register};

#[test]
fn common_header_names_offsets_and_count_match_linux() {
    // pci_regs.h:38-85,122,125-126. Written literally, not made from COMMON_REGISTERS.
    let expected = [
        Register {
            name: "VENDOR_ID",
            offset: 0x00,
        },
        Register {
            name: "DEVICE_ID",
            offset: 0x02,
        },
        Register {
            name: "COMMAND",
            offset: 0x04,
        },
        Register {
            name: "STATUS",
            offset: 0x06,
        },
        Register {
            name: "CLASS_REVISION",
            offset: 0x08,
        },
        Register {
            name: "REVISION_ID",
            offset: 0x08,
        },
        Register {
            name: "CLASS_PROG",
            offset: 0x09,
        },
        Register {
            name: "CLASS_DEVICE",
            offset: 0x0a,
        },
        Register {
            name: "CACHE_LINE_SIZE",
            offset: 0x0c,
        },
        Register {
            name: "LATENCY_TIMER",
            offset: 0x0d,
        },
        Register {
            name: "HEADER_TYPE",
            offset: 0x0e,
        },
        Register {
            name: "BIST",
            offset: 0x0f,
        },
        Register {
            name: "CAPABILITY_LIST",
            offset: 0x34,
        },
        Register {
            name: "INTERRUPT_LINE",
            offset: 0x3c,
        },
        Register {
            name: "INTERRUPT_PIN",
            offset: 0x3d,
        },
    ];
    assert_eq!(regs::COMMON_REGISTERS.len(), 15);
    assert_eq!(regs::COMMON_REGISTERS, expected);
}

#[test]
fn type_zero_names_offsets_and_count_match_linux() {
    // pci_regs.h:96-128.
    let expected = [
        Register {
            name: "BASE_ADDRESS_0",
            offset: 0x10,
        },
        Register {
            name: "BASE_ADDRESS_1",
            offset: 0x14,
        },
        Register {
            name: "BASE_ADDRESS_2",
            offset: 0x18,
        },
        Register {
            name: "BASE_ADDRESS_3",
            offset: 0x1c,
        },
        Register {
            name: "BASE_ADDRESS_4",
            offset: 0x20,
        },
        Register {
            name: "BASE_ADDRESS_5",
            offset: 0x24,
        },
        Register {
            name: "CARDBUS_CIS",
            offset: 0x28,
        },
        Register {
            name: "SUBSYSTEM_VENDOR_ID",
            offset: 0x2c,
        },
        Register {
            name: "SUBSYSTEM_ID",
            offset: 0x2e,
        },
        Register {
            name: "ROM_ADDRESS",
            offset: 0x30,
        },
        Register {
            name: "MIN_GNT",
            offset: 0x3e,
        },
        Register {
            name: "MAX_LAT",
            offset: 0x3f,
        },
    ];
    assert_eq!(regs::TYPE0_REGISTERS.len(), 12);
    assert_eq!(regs::TYPE0_REGISTERS, expected);
    assert_eq!(regs::STD_NUM_BARS, 6, "pci_regs.h:37");
}

#[test]
fn type_one_names_offsets_and_count_match_linux() {
    // pci_regs.h:96-97,131-166.
    let expected = [
        Register {
            name: "BASE_ADDRESS_0",
            offset: 0x10,
        },
        Register {
            name: "BASE_ADDRESS_1",
            offset: 0x14,
        },
        Register {
            name: "PRIMARY_BUS",
            offset: 0x18,
        },
        Register {
            name: "SECONDARY_BUS",
            offset: 0x19,
        },
        Register {
            name: "SUBORDINATE_BUS",
            offset: 0x1a,
        },
        Register {
            name: "SEC_LATENCY_TIMER",
            offset: 0x1b,
        },
        Register {
            name: "IO_BASE",
            offset: 0x1c,
        },
        Register {
            name: "IO_LIMIT",
            offset: 0x1d,
        },
        Register {
            name: "SEC_STATUS",
            offset: 0x1e,
        },
        Register {
            name: "MEMORY_BASE",
            offset: 0x20,
        },
        Register {
            name: "MEMORY_LIMIT",
            offset: 0x22,
        },
        Register {
            name: "PREF_MEMORY_BASE",
            offset: 0x24,
        },
        Register {
            name: "PREF_MEMORY_LIMIT",
            offset: 0x26,
        },
        Register {
            name: "PREF_BASE_UPPER32",
            offset: 0x28,
        },
        Register {
            name: "PREF_LIMIT_UPPER32",
            offset: 0x2c,
        },
        Register {
            name: "IO_BASE_UPPER16",
            offset: 0x30,
        },
        Register {
            name: "IO_LIMIT_UPPER16",
            offset: 0x32,
        },
        Register {
            name: "ROM_ADDRESS1",
            offset: 0x38,
        },
        Register {
            name: "BRIDGE_CONTROL",
            offset: 0x3e,
        },
    ];
    assert_eq!(regs::TYPE1_REGISTERS.len(), 19);
    assert_eq!(regs::TYPE1_REGISTERS, expected);
}

#[test]
fn command_and_status_literals_match_linux() {
    // pci_regs.h:41-51: eleven command bits.
    let commands = [
        ("IO", command::IO, 0x001),
        ("MEMORY", command::MEMORY, 0x002),
        ("MASTER", command::MASTER, 0x004),
        ("SPECIAL", command::SPECIAL, 0x008),
        ("INVALIDATE", command::INVALIDATE, 0x010),
        ("VGA_PALETTE", command::VGA_PALETTE, 0x020),
        ("PARITY", command::PARITY, 0x040),
        ("WAIT", command::WAIT, 0x080),
        ("SERR", command::SERR, 0x100),
        ("FAST_BACK", command::FAST_BACK, 0x200),
        ("INTX_DISABLE", command::INTX_DISABLE, 0x400),
    ];
    assert_eq!(commands.len(), 11);
    for (name, got, expected) in commands {
        assert_eq!(got, expected, "{name}");
    }
    assert_eq!(command::DECODE_ENABLE, 0x003, "probe.c:167");

    // pci_regs.h:54-69: sixteen named status values including three DEVSEL values.
    let statuses = [
        ("IMM_READY", status::IMM_READY, 0x0001),
        ("INTERRUPT", status::INTERRUPT, 0x0008),
        ("CAP_LIST", status::CAP_LIST, 0x0010),
        ("66MHZ", status::MHZ_66, 0x0020),
        ("UDF", status::UDF, 0x0040),
        ("FAST_BACK", status::FAST_BACK, 0x0080),
        ("PARITY", status::PARITY, 0x0100),
        ("DEVSEL_MASK", status::DEVSEL_MASK, 0x0600),
        ("DEVSEL_FAST", status::DEVSEL_FAST, 0x0000),
        ("DEVSEL_MEDIUM", status::DEVSEL_MEDIUM, 0x0200),
        ("DEVSEL_SLOW", status::DEVSEL_SLOW, 0x0400),
        ("SIG_TARGET_ABORT", status::SIG_TARGET_ABORT, 0x0800),
        ("REC_TARGET_ABORT", status::REC_TARGET_ABORT, 0x1000),
        ("REC_MASTER_ABORT", status::REC_MASTER_ABORT, 0x2000),
        ("SIG_SYSTEM_ERROR", status::SIG_SYSTEM_ERROR, 0x4000),
        ("DETECTED_PARITY", status::DETECTED_PARITY, 0x8000),
    ];
    assert_eq!(statuses.len(), 16);
    for (name, got, expected) in statuses {
        assert_eq!(got, expected, "{name}");
    }
}

#[test]
fn public_register_decoders_have_linux_vectors() {
    assert_eq!(
        regs::header_layout(0x80),
        0,
        "pci_regs.h:79-83; pci.c:504-506"
    );
    assert_eq!(regs::header_layout(0x81), 1);
    assert_eq!(
        regs::status_errors(0xffff),
        0xf900,
        "pci.h:46-51; pci.c:213-219"
    );
    assert_eq!(
        regs::status_errors(status::CAP_LIST | status::REC_MASTER_ABORT),
        0x2000
    );
    assert_eq!(regs::CFG_SPACE_SIZE, 256, "pci_regs.h:29");
    assert_eq!(regs::STD_HEADER_SIZE, 64, "pci_regs.h:36");
}
