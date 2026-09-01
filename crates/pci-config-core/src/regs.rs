// SPDX-License-Identifier: GPL-2.0-only
//! Standard PCI header offsets and command/status bits.
//!
//! Ported from Linux `include/uapi/linux/pci_regs.h` and `drivers/pci/pci.c`.
//! Copyright 1994 Drew Eckhardt. Copyright 1997--1999 Martin Mares and the
//! Linux PCI authors.

/// A named register offset, used to pin the complete public layout corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Register {
    pub name: &'static str,
    pub offset: u16,
}

/// Common fields shared by type 0 and type 1 headers.
pub const COMMON_REGISTERS: &[Register] = &[
    Register {
        name: "VENDOR_ID",
        offset: 0x00,
    }, // pci_regs.h:38
    Register {
        name: "DEVICE_ID",
        offset: 0x02,
    }, // pci_regs.h:39
    Register {
        name: "COMMAND",
        offset: 0x04,
    }, // pci_regs.h:40
    Register {
        name: "STATUS",
        offset: 0x06,
    }, // pci_regs.h:53
    Register {
        name: "CLASS_REVISION",
        offset: 0x08,
    }, // pci_regs.h:71
    Register {
        name: "REVISION_ID",
        offset: 0x08,
    }, // pci_regs.h:72
    Register {
        name: "CLASS_PROG",
        offset: 0x09,
    }, // pci_regs.h:73
    Register {
        name: "CLASS_DEVICE",
        offset: 0x0a,
    }, // pci_regs.h:74
    Register {
        name: "CACHE_LINE_SIZE",
        offset: 0x0c,
    }, // pci_regs.h:76
    Register {
        name: "LATENCY_TIMER",
        offset: 0x0d,
    }, // pci_regs.h:77
    Register {
        name: "HEADER_TYPE",
        offset: 0x0e,
    }, // pci_regs.h:78
    Register {
        name: "BIST",
        offset: 0x0f,
    }, // pci_regs.h:85
    Register {
        name: "CAPABILITY_LIST",
        offset: 0x34,
    }, // pci_regs.h:122
    Register {
        name: "INTERRUPT_LINE",
        offset: 0x3c,
    }, // pci_regs.h:125
    Register {
        name: "INTERRUPT_PIN",
        offset: 0x3d,
    }, // pci_regs.h:126
];

/// Fields specific to a type 0 (normal endpoint) header.
pub const TYPE0_REGISTERS: &[Register] = &[
    Register {
        name: "BASE_ADDRESS_0",
        offset: 0x10,
    }, // pci_regs.h:96
    Register {
        name: "BASE_ADDRESS_1",
        offset: 0x14,
    }, // pci_regs.h:97
    Register {
        name: "BASE_ADDRESS_2",
        offset: 0x18,
    }, // pci_regs.h:98
    Register {
        name: "BASE_ADDRESS_3",
        offset: 0x1c,
    }, // pci_regs.h:99
    Register {
        name: "BASE_ADDRESS_4",
        offset: 0x20,
    }, // pci_regs.h:100
    Register {
        name: "BASE_ADDRESS_5",
        offset: 0x24,
    }, // pci_regs.h:101
    Register {
        name: "CARDBUS_CIS",
        offset: 0x28,
    }, // pci_regs.h:115
    Register {
        name: "SUBSYSTEM_VENDOR_ID",
        offset: 0x2c,
    }, // pci_regs.h:116
    Register {
        name: "SUBSYSTEM_ID",
        offset: 0x2e,
    }, // pci_regs.h:117
    Register {
        name: "ROM_ADDRESS",
        offset: 0x30,
    }, // pci_regs.h:118
    Register {
        name: "MIN_GNT",
        offset: 0x3e,
    }, // pci_regs.h:127
    Register {
        name: "MAX_LAT",
        offset: 0x3f,
    }, // pci_regs.h:128
];

/// Fields specific to a type 1 (PCI-to-PCI bridge) header.
pub const TYPE1_REGISTERS: &[Register] = &[
    Register {
        name: "BASE_ADDRESS_0",
        offset: 0x10,
    }, // pci_regs.h:96
    Register {
        name: "BASE_ADDRESS_1",
        offset: 0x14,
    }, // pci_regs.h:97
    Register {
        name: "PRIMARY_BUS",
        offset: 0x18,
    }, // pci_regs.h:131
    Register {
        name: "SECONDARY_BUS",
        offset: 0x19,
    }, // pci_regs.h:132
    Register {
        name: "SUBORDINATE_BUS",
        offset: 0x1a,
    }, // pci_regs.h:133
    Register {
        name: "SEC_LATENCY_TIMER",
        offset: 0x1b,
    }, // pci_regs.h:134
    Register {
        name: "IO_BASE",
        offset: 0x1c,
    }, // pci_regs.h:140
    Register {
        name: "IO_LIMIT",
        offset: 0x1d,
    }, // pci_regs.h:141
    Register {
        name: "SEC_STATUS",
        offset: 0x1e,
    }, // pci_regs.h:147
    Register {
        name: "MEMORY_BASE",
        offset: 0x20,
    }, // pci_regs.h:148
    Register {
        name: "MEMORY_LIMIT",
        offset: 0x22,
    }, // pci_regs.h:149
    Register {
        name: "PREF_MEMORY_BASE",
        offset: 0x24,
    }, // pci_regs.h:152
    Register {
        name: "PREF_MEMORY_LIMIT",
        offset: 0x26,
    }, // pci_regs.h:153
    Register {
        name: "PREF_BASE_UPPER32",
        offset: 0x28,
    }, // pci_regs.h:158
    Register {
        name: "PREF_LIMIT_UPPER32",
        offset: 0x2c,
    }, // pci_regs.h:159
    Register {
        name: "IO_BASE_UPPER16",
        offset: 0x30,
    }, // pci_regs.h:160
    Register {
        name: "IO_LIMIT_UPPER16",
        offset: 0x32,
    }, // pci_regs.h:161
    Register {
        name: "ROM_ADDRESS1",
        offset: 0x38,
    }, // pci_regs.h:164
    Register {
        name: "BRIDGE_CONTROL",
        offset: 0x3e,
    }, // pci_regs.h:166
];

pub const CFG_SPACE_SIZE: usize = 256; // pci_regs.h:29
pub const STD_HEADER_SIZE: u8 = 64; // pci_regs.h:36
pub const STD_NUM_BARS: usize = 6; // pci_regs.h:37
pub const HEADER_TYPE_MASK: u8 = 0x7f; // pci_regs.h:79
pub const HEADER_TYPE_NORMAL: u8 = 0; // pci_regs.h:80
pub const HEADER_TYPE_BRIDGE: u8 = 1; // pci_regs.h:81
pub const HEADER_TYPE_CARDBUS: u8 = 2; // pci_regs.h:82
pub const HEADER_TYPE_MFD: u8 = 0x80; // pci_regs.h:83

pub mod command {
    pub const IO: u16 = 0x001; // pci_regs.h:41
    pub const MEMORY: u16 = 0x002; // pci_regs.h:42
    pub const MASTER: u16 = 0x004; // pci_regs.h:43
    pub const SPECIAL: u16 = 0x008; // pci_regs.h:44
    pub const INVALIDATE: u16 = 0x010; // pci_regs.h:45
    pub const VGA_PALETTE: u16 = 0x020; // pci_regs.h:46
    pub const PARITY: u16 = 0x040; // pci_regs.h:47
    pub const WAIT: u16 = 0x080; // pci_regs.h:48
    pub const SERR: u16 = 0x100; // pci_regs.h:49
    pub const FAST_BACK: u16 = 0x200; // pci_regs.h:50
    pub const INTX_DISABLE: u16 = 0x400; // pci_regs.h:51
    pub const DECODE_ENABLE: u16 = MEMORY | IO; // probe.c:167
}

pub mod status {
    pub const IMM_READY: u16 = 0x0001; // pci_regs.h:54
    pub const INTERRUPT: u16 = 0x0008; // pci_regs.h:55
    pub const CAP_LIST: u16 = 0x0010; // pci_regs.h:56
    pub const MHZ_66: u16 = 0x0020; // pci_regs.h:57
    pub const UDF: u16 = 0x0040; // pci_regs.h:58
    pub const FAST_BACK: u16 = 0x0080; // pci_regs.h:59
    pub const PARITY: u16 = 0x0100; // pci_regs.h:60
    pub const DEVSEL_MASK: u16 = 0x0600; // pci_regs.h:61
    pub const DEVSEL_FAST: u16 = 0x0000; // pci_regs.h:62
    pub const DEVSEL_MEDIUM: u16 = 0x0200; // pci_regs.h:63
    pub const DEVSEL_SLOW: u16 = 0x0400; // pci_regs.h:64
    pub const SIG_TARGET_ABORT: u16 = 0x0800; // pci_regs.h:65
    pub const REC_TARGET_ABORT: u16 = 0x1000; // pci_regs.h:66
    pub const REC_MASTER_ABORT: u16 = 0x2000; // pci_regs.h:67
    pub const SIG_SYSTEM_ERROR: u16 = 0x4000; // pci_regs.h:68
    pub const DETECTED_PARITY: u16 = 0x8000; // pci_regs.h:69
    pub const ERROR_BITS: u16 = 0xf900; // pci.h:46-51
}

/// Strip the multifunction flag and return the header layout number.
pub const fn header_layout(header_type: u8) -> u8 {
    header_type & HEADER_TYPE_MASK // pci.c:504-506
}

/// Return only the status errors Linux clears in `pci_status_get_and_clear_errors`.
pub const fn status_errors(status: u16) -> u16 {
    status & status::ERROR_BITS // pci.c:213-219
}
