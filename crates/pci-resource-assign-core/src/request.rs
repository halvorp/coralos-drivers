// SPDX-License-Identifier: GPL-2.0-only
//! BAR requests, Linux resource flags, allocation buckets, and sort keys.
//!
//! Ported from Linux `drivers/pci/setup-res.c`, `drivers/pci/setup-bus.c`,
//! `kernel/resource.c`, and `include/linux/ioport.h`.
//!
//! Copyright Dave Rusling, David Mosberger, David Miller, Andrea Arcangeli,
//! Ivan Kokshaysky, Linus Torvalds, and the Linux PCI authors.

use pci_config_core::bar::{Bar, BarKind};

/// PCI/ISA I/O port resource. (`include/linux/ioport.h:40`)
pub const IORESOURCE_IO: u32 = 0x0000_0100;
/// Memory resource. (`include/linux/ioport.h:41`)
pub const IORESOURCE_MEM: u32 = 0x0000_0200;
/// Resource has no read side effects. (`include/linux/ioport.h:47`)
pub const IORESOURCE_PREFETCH: u32 = 0x0000_2000;
/// Size indicates alignment. (`include/linux/ioport.h:53`)
pub const IORESOURCE_SIZEALIGN: u32 = 0x0004_0000;
/// Start field indicates alignment. (`include/linux/ioport.h:54`)
pub const IORESOURCE_STARTALIGN: u32 = 0x0008_0000;
/// Resource is addressable by a 64-bit memory BAR. (`include/linux/ioport.h:56`)
pub const IORESOURCE_MEM_64: u32 = 0x0010_0000;

/// Number of assignment buckets represented by [`BUCKET_NAMES`].
/// (`drivers/pci/setup-res.c:267-303`)
pub const BUCKET_COUNT: usize = 4;
/// Frozen names for every assignment bucket.
pub const BUCKET_NAMES: [&str; BUCKET_COUNT] = [
    "I/O",          // setup-res.c:267
    "non-prefetch", // setup-res.c:295-303
    "prefetch-32",  // setup-res.c:269-280
    "prefetch-64",  // setup-res.c:269-293
];

/// The resource family used to select a compatible bridge window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Io,
    NonPrefetchableMemory,
    PrefetchableMemory32,
    PrefetchableMemory64,
}

impl Bucket {
    /// Stable index into [`BUCKET_NAMES`].
    pub const fn index(self) -> usize {
        match self {
            Self::Io => 0,
            Self::NonPrefetchableMemory => 1,
            Self::PrefetchableMemory32 => 2,
            Self::PrefetchableMemory64 => 3,
        }
    }

    /// Human-readable Linux resource-family name.
    pub const fn name(self) -> &'static str {
        BUCKET_NAMES[self.index()]
    }
}

/// A logical BAR's request after sizing-probe decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarRequest {
    pub index: u8,
    pub slots: u8,
    pub size: u64,
    pub alignment: u64,
    pub flags: u32,
    pub bucket: Bucket,
}

/// Named refusals while converting or ordering BAR requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    SizeIsZero { index: u8, size: u64 },
    SizeIsNotPowerOfTwo { index: u8, size: u64 },
    InvalidSlotCount { index: u8, slots: u8, expected: u8 },
}

/// Turn a decoded BAR and its decoded probe size into an assignment request.
///
/// Ordinary PCI BAR resources carry `IORESOURCE_SIZEALIGN`, so Linux's
/// `resource_alignment()` returns the resource size (`kernel/resource.c:1242-1245`).
/// This is load-bearing: the BAR decodes only address bits above that size.
pub fn request_from_bar(bar: Bar, size: u64) -> Result<BarRequest, RequestError> {
    if size == 0 {
        return Err(RequestError::SizeIsZero {
            index: bar.index,
            size,
        });
    }
    if !size.is_power_of_two() {
        return Err(RequestError::SizeIsNotPowerOfTwo {
            index: bar.index,
            size,
        });
    }

    let (expected_slots, type_flags, bucket) = match bar.kind {
        BarKind::Io => (1, IORESOURCE_IO, Bucket::Io),
        BarKind::Memory32 if bar.prefetchable => (
            1,
            IORESOURCE_MEM | IORESOURCE_PREFETCH,
            Bucket::PrefetchableMemory32,
        ),
        BarKind::Memory32 => (1, IORESOURCE_MEM, Bucket::NonPrefetchableMemory),
        BarKind::Memory64 if bar.prefetchable => (
            2,
            IORESOURCE_MEM | IORESOURCE_PREFETCH | IORESOURCE_MEM_64,
            Bucket::PrefetchableMemory64,
        ),
        BarKind::Memory64 => (
            2,
            IORESOURCE_MEM | IORESOURCE_MEM_64,
            Bucket::NonPrefetchableMemory,
        ),
    };
    if bar.slots != expected_slots {
        return Err(RequestError::InvalidSlotCount {
            index: bar.index,
            slots: bar.slots,
            expected: expected_slots,
        });
    }

    Ok(BarRequest {
        index: bar.index,
        slots: bar.slots,
        size,
        alignment: size,
        flags: type_flags | IORESOURCE_SIZEALIGN,
        bucket,
    })
}

/// Sort requests in-place by decreasing alignment, preserving input order for ties.
///
/// This is the insertion order of `pdev_sort_resources()`
/// (`drivers/pci/setup-bus.c:328-376`): resources with greater alignment are
/// inserted before smaller ones; equal alignments retain discovery order.
pub fn sort_requests(requests: &mut [BarRequest]) {
    for i in 1..requests.len() {
        let mut j = i;
        while j > 0 && requests[j].alignment > requests[j - 1].alignment {
            requests.swap(j, j - 1);
            j -= 1;
        }
    }
}
