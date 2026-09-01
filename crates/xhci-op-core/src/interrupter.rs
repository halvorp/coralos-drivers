// SPDX-License-Identifier: GPL-2.0-only
//! Interrupter moderation and Event Ring Segment Table fields, ported from Linux
//! `drivers/usb/host/xhci.c:313-:366`, `drivers/usb/host/xhci.h:210-:272`,
//! `drivers/usb/host/xhci-mem.c:1790-:1816, :2321-:2343`, and
//! `drivers/usb/host/xhci-ring.c:3038-:3064`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

pub mod offset {
    pub const IMAN: u32 = 0x00; // xhci.h:224
    pub const IMOD: u32 = 0x04; // xhci.h:225
    pub const ERSTSZ: u32 = 0x08; // xhci.h:226
    pub const ERSTBA: u32 = 0x10; // xhci.h:228
    pub const ERDP: u32 = 0x18; // xhci.h:229
    pub const REGISTER_SET_BYTES: u32 = 0x20; // xhci.h:223-230
}

pub const IMAN_PENDING: u32 = 1 << 0; // xhci.h:235
pub const IMAN_ENABLE: u32 = 1 << 1; // xhci.h:237
pub const IMOD_INTERVAL_MASK: u32 = 0x0000_ffff; // xhci.h:248
pub const IMOD_COUNTER_MASK: u32 = 0xffff_0000; // xhci.h:250
pub const IMOD_QUANTUM_NS: u32 = 250; // xhci.c:358
pub const ERST_SIZE_MASK: u32 = 0x0000_ffff; // xhci.h:254
pub const ERST_BASE_ADDRESS_MASK: u64 = 0xffff_ffff_ffff_ffc0; // xhci.h:258
pub const ERDP_SEGMENT_INDEX_MASK: u64 = 0x7; // xhci.h:265
pub const ERDP_EVENT_HANDLER_BUSY: u64 = 1 << 3; // xhci.h:270
pub const ERDP_POINTER_MASK: u64 = 0xffff_ffff_ffff_fff0; // xhci.h:272
pub const DEFAULT_ERST_SEGMENTS: u16 = 2; // xhci.h:1413
pub const MAX_INTERRUPTERS: usize = 128; // xhci.h:46
pub const EVENT_RING_SEGMENT_TRBS: u32 = 256; // xhci.h:1259; used by xhci-mem.c:1813

/// One 16-byte Event Ring Segment Table entry (`struct xhci_erst_entry`, xhci.h:1385-:1391).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErstEntry {
    pub segment_address: u64,
    pub segment_size_trbs: u32,
    pub reserved: u32,
}

/// Names of every register in one interrupter register set.
pub const INTERRUPTER_REGISTER_NAMES: [&str; 5] = ["IMAN", "IMOD", "ERSTSZ", "ERSTBA", "ERDP"];
// xhci.h:210-230
/// Names of every distinct register-word field represented in this module.
pub const INTERRUPTER_FIELD_NAMES: [&str; 9] = [
    "IMAN_PENDING",
    "IMAN_ENABLE",
    "IMOD_INTERVAL",
    "IMOD_COUNTER",
    "ERST_SIZE",
    "ERST_BASE",
    "ERDP_SEGMENT_INDEX",
    "ERDP_EVENT_HANDLER_BUSY",
    "ERDP_POINTER",
]; // xhci.h:233-272

/// Construct Linux's ERST entry: DMA address, 256 TRBs, and a zero reserved word
/// (`xhci_alloc_erst`, xhci-mem.c:1807-:1814).
pub const fn erst_entry(segment_dma: u64) -> ErstEntry {
    ErstEntry {
        segment_address: segment_dma,
        segment_size_trbs: EVENT_RING_SEGMENT_TRBS,
        reserved: 0,
    }
}

/// Enable an interrupter: write-one-to-clear IP and set IE (`xhci_enable_interrupter`,
/// xhci.c:313-:325).
pub const fn enable_interrupter(iman: u32) -> u32 {
    (iman & !IMAN_PENDING) | IMAN_ENABLE
}

/// Disable an interrupter: do not accidentally acknowledge pending IP, and clear IE
/// (`xhci_disable_interrupter`, xhci.c:330-:343).
pub const fn disable_interrupter(iman: u32) -> u32 {
    iman & !(IMAN_PENDING | IMAN_ENABLE)
}

/// Result of Linux's saturating nanosecond-to-IMODI conversion. `saturated` makes the clamp
/// explicit rather than silently hiding that the requested interval was not representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModerationProgramming {
    pub word: u32,
    pub interval_units: u16,
    pub saturated: bool,
}

/// Replace IMODI from a nanosecond interval. Linux truncates to 250 ns units and saturates at
/// 0xffff while preserving the hardware-owned counter (`xhci_set_interrupter_moderation`,
/// xhci.c:350-:365). The returned metadata explicitly reports saturation.
pub const fn program_moderation(imod: u32, interval_ns: u32) -> ModerationProgramming {
    let requested_units = interval_ns / IMOD_QUANTUM_NS;
    let saturated = requested_units > IMOD_INTERVAL_MASK;
    let units = if saturated {
        IMOD_INTERVAL_MASK
    } else {
        requested_units
    };
    ModerationProgramming {
        word: (imod & !IMOD_INTERVAL_MASK) | units,
        interval_units: units as u16,
        saturated,
    }
}

/// Replace ERSTSZ.Size while preserving reserved bits (`xhci_add_interrupter`,
/// xhci-mem.c:2331-:2334).
pub const fn program_erst_size(erstsz: u32, segment_count: u16) -> u32 {
    (erstsz & !ERST_SIZE_MASK) | segment_count as u32
}

/// Replace ERSTBA.Base while preserving its low reserved bits (`xhci_add_interrupter`,
/// xhci-mem.c:2336-:2342).
pub const fn program_erst_base(erstba: u64, table_dma: u64) -> u64 {
    (erstba & !ERST_BASE_ADDRESS_MASK) | (table_dma & ERST_BASE_ADDRESS_MASK)
}

/// Build ERDP from DESI, dequeue pointer, and the write-one-to-clear EHB request
/// (`xhci_update_erst_dequeue`, xhci-ring.c:3058-:3064).
pub const fn program_erdp(segment_index: u8, dequeue_dma: u64, clear_ehb: bool) -> u64 {
    let mut erdp =
        (segment_index as u64 & ERDP_SEGMENT_INDEX_MASK) | (dequeue_dma & ERDP_POINTER_MASK);
    if clear_ehb {
        erdp |= ERDP_EVENT_HANDLER_BUSY;
    }
    erdp
}

/// ERDP's decoded dequeue address (`xhci_run`, xhci.c:662-:663).
pub const fn erdp_pointer(erdp: u64) -> u64 {
    erdp & ERDP_POINTER_MASK
}

/// Whether Linux may skip an ERDP write. The pointer may stay equal only when the caller is not
/// clearing EHB (`xhci_update_erst_dequeue`, xhci-ring.c:3045-:3055).
pub const fn erdp_update_needed(current_erdp: u64, dequeue_dma: u64, clear_ehb: bool) -> bool {
    (current_erdp & ERDP_POINTER_MASK) != (dequeue_dma & ERDP_POINTER_MASK) || clear_ehb
}
