// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for BAR requests and decreasing-alignment ordering.
//!
//! Ported from Linux `drivers/pci/setup-res.c`, `drivers/pci/setup-bus.c`,
//! `kernel/resource.c`, and `include/linux/ioport.h`.
//!
//! Copyright Dave Rusling, David Mosberger, David Miller, Andrea Arcangeli,
//! Ivan Kokshaysky, Linus Torvalds, and the Linux PCI authors.

use pci_config_core::bar::{Bar, BarKind};
use pci_resource_assign_core::request::{
    request_from_bar, sort_requests, BarRequest, Bucket, RequestError, BUCKET_COUNT, BUCKET_NAMES,
    IORESOURCE_IO, IORESOURCE_MEM, IORESOURCE_MEM_64, IORESOURCE_PREFETCH, IORESOURCE_SIZEALIGN,
    IORESOURCE_STARTALIGN,
};

fn bar(index: u8, kind: BarKind, prefetchable: bool, slots: u8) -> Bar {
    Bar {
        index,
        offset: 0x10 + index * 4,
        kind,
        address: 0,
        prefetchable,
        slots,
    }
}

/// include/linux/ioport.h:40-56. The actual side routes through every production constant; the
/// expected side is a frozen Linux literal rather than an expression copied from production.
#[test]
fn every_resource_flag_matches_linux() {
    let expected = [
        ("IO", 0x0000_0100u32),
        ("MEM", 0x0000_0200),
        ("PREFETCH", 0x0000_2000),
        ("SIZEALIGN", 0x0004_0000),
        ("STARTALIGN", 0x0008_0000),
        ("MEM_64", 0x0010_0000),
    ];
    let actual = [
        ("IO", IORESOURCE_IO),
        ("MEM", IORESOURCE_MEM),
        ("PREFETCH", IORESOURCE_PREFETCH),
        ("SIZEALIGN", IORESOURCE_SIZEALIGN),
        ("STARTALIGN", IORESOURCE_STARTALIGN),
        ("MEM_64", IORESOURCE_MEM_64),
    ];
    assert_eq!(actual.len(), 6);
    assert_eq!(actual, expected);
}

/// setup-res.c:267-303 has four logically distinct window families. Pin both the count and the
/// hand-written names; never derive this expectation from BUCKET_NAMES.
#[test]
fn bucket_count_names_indexes_and_selection_are_pinned() {
    assert_eq!(BUCKET_COUNT, 4);
    assert_eq!(BUCKET_NAMES.len(), 4);
    assert_eq!(
        BUCKET_NAMES,
        ["I/O", "non-prefetch", "prefetch-32", "prefetch-64"]
    );

    let buckets = [
        Bucket::Io,
        Bucket::NonPrefetchableMemory,
        Bucket::PrefetchableMemory32,
        Bucket::PrefetchableMemory64,
    ];
    let expected = [
        (0usize, "I/O"),
        (1, "non-prefetch"),
        (2, "prefetch-32"),
        (3, "prefetch-64"),
    ];
    for (bucket, literal) in buckets.into_iter().zip(expected) {
        assert_eq!((bucket.index(), bucket.name()), literal);
    }
}

/// kernel/resource.c:1242-1245 makes an ordinary BAR's alignment equal its size. setup-res.c:342-
/// 343 then passes that size and alignment together to allocation. Drive every BAR family,
/// including the two-slot 64-bit pair, instead of letting one convenient kind guard its siblings.
#[test]
fn every_bar_family_becomes_the_exact_linux_request() {
    let vectors = [
        (
            bar(0, BarKind::Io, false, 1),
            0x100,
            BarRequest {
                index: 0,
                slots: 1,
                size: 0x100,
                alignment: 0x100,
                flags: 0x0004_0100,
                bucket: Bucket::Io,
            },
        ),
        (
            bar(1, BarKind::Memory32, false, 1),
            0x1000,
            BarRequest {
                index: 1,
                slots: 1,
                size: 0x1000,
                alignment: 0x1000,
                flags: 0x0004_0200,
                bucket: Bucket::NonPrefetchableMemory,
            },
        ),
        (
            bar(2, BarKind::Memory32, true, 1),
            0x20_0000,
            BarRequest {
                index: 2,
                slots: 1,
                size: 0x20_0000,
                alignment: 0x20_0000,
                flags: 0x0004_2200,
                bucket: Bucket::PrefetchableMemory32,
            },
        ),
        (
            bar(3, BarKind::Memory64, false, 2),
            0x2_0000_0000,
            BarRequest {
                index: 3,
                slots: 2,
                size: 0x2_0000_0000,
                alignment: 0x2_0000_0000,
                flags: 0x0014_0200,
                bucket: Bucket::NonPrefetchableMemory,
            },
        ),
        (
            bar(4, BarKind::Memory64, true, 2),
            0x4_0000_0000,
            BarRequest {
                index: 4,
                slots: 2,
                size: 0x4_0000_0000,
                alignment: 0x4_0000_0000,
                flags: 0x0014_2200,
                bucket: Bucket::PrefetchableMemory64,
            },
        ),
    ];
    assert_eq!(vectors.len(), 5);
    for (input, literal_size, expected) in vectors {
        assert_eq!(request_from_bar(input, literal_size), Ok(expected));
    }
}

/// setup-bus.c:328-376 sorts by decreasing alignment and inserts equal alignments after existing
/// entries. The two 4 KiB entries prove the tie is stable.
#[test]
fn requests_are_sorted_large_aligned_first_and_ties_stay_stable() {
    let mut requests = [
        request_from_bar(bar(0, BarKind::Memory32, false, 1), 0x1000).unwrap(),
        request_from_bar(bar(1, BarKind::Memory32, false, 1), 0x20_0000).unwrap(),
        request_from_bar(bar(2, BarKind::Io, false, 1), 0x100).unwrap(),
        request_from_bar(bar(3, BarKind::Memory32, true, 1), 0x1000).unwrap(),
        request_from_bar(bar(4, BarKind::Memory64, true, 2), 0x1_0000_0000).unwrap(),
    ];
    sort_requests(&mut requests);
    assert_eq!(
        requests.map(|request| request.index),
        [4, 1, 0, 3, 2],
        "setup-bus.c:370-376"
    );
    assert_eq!(
        requests.map(|request| request.alignment),
        [0x1_0000_0000, 0x20_0000, 0x1000, 0x1000, 0x100,]
    );
}

#[test]
fn malformed_requests_name_the_value_and_bound_that_refused() {
    assert_eq!(
        request_from_bar(bar(5, BarKind::Memory32, false, 1), 0),
        Err(RequestError::SizeIsZero { index: 5, size: 0 })
    );
    assert_eq!(
        request_from_bar(bar(2, BarKind::Memory32, false, 1), 0x1800),
        Err(RequestError::SizeIsNotPowerOfTwo {
            index: 2,
            size: 0x1800
        })
    );
    assert_eq!(
        request_from_bar(bar(1, BarKind::Memory64, true, 1), 0x1000),
        Err(RequestError::InvalidSlotCount {
            index: 1,
            slots: 1,
            expected: 2
        })
    );
}
