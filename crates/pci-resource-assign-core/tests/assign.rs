// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for compatible-window selection and naturally aligned first-fit assignment.
//!
//! Ported from Linux `drivers/pci/setup-res.c`, `drivers/pci/bus.c`, and
//! `kernel/resource.c`.
//!
//! Copyright Dave Rusling, David Mosberger, David Miller, Andrea Arcangeli,
//! Ivan Kokshaysky, Linus Torvalds, and the Linux PCI authors.

use pci_config_core::bar::{Bar, BarKind};
use pci_resource_assign_core::assign::{
    assign_one, is_naturally_aligned, is_prefetchable_64, window_compatible, window_rank,
    AssignError, Assignment, Range, Window,
};
use pci_resource_assign_core::request::{request_from_bar, BarRequest, Bucket};

fn request(index: u8, kind: BarKind, prefetchable: bool, size: u64) -> BarRequest {
    request_from_bar(
        Bar {
            index,
            offset: 0x10 + index * 4,
            kind,
            address: 0,
            prefetchable,
            slots: if kind == BarKind::Memory64 { 2 } else { 1 },
        },
        size,
    )
    .unwrap()
}

fn window(start: u64, end: u64, bucket: Bucket) -> Window {
    Window {
        range: Range { start, end },
        bucket,
    }
}

/// setup-res.c:269-303 and bus.c:212-220. Drive every request/window bucket pairing. This guards
/// all siblings of the enum-like family rather than only the family member a placement happens to
/// exercise.
#[test]
fn every_io_mem_prefetch_bucket_pair_has_the_linux_compatibility() {
    let requests = [
        request(0, BarKind::Io, false, 0x100),
        request(1, BarKind::Memory32, false, 0x1000),
        request(2, BarKind::Memory32, true, 0x1000),
        request(3, BarKind::Memory64, true, 0x1000),
    ];
    let windows = [
        window(0x1000, 0x2000, Bucket::Io),
        window(0x1000_0000, 0x1001_0000, Bucket::NonPrefetchableMemory),
        window(0x2000_0000, 0x2001_0000, Bucket::PrefetchableMemory32),
        window(0x1_0000_0000, 0x1_0001_0000, Bucket::PrefetchableMemory64),
    ];
    let expected = [
        [true, false, false, false],
        [false, true, false, false],
        [false, true, true, false],
        [false, true, true, true],
    ];
    assert_eq!(requests.len(), 4);
    assert_eq!(windows.len(), 4);
    for request_index in 0..4 {
        for window_index in 0..4 {
            assert_eq!(
                window_compatible(requests[request_index], windows[window_index]),
                expected[request_index][window_index],
                "request {request_index}, window {window_index}"
            );
        }
    }
}

/// setup-res.c:276-303: exact prefetch match first; a 64-bit prefetch BAR may then use a 32-bit
/// prefetch window, then non-prefetch. A 32-bit prefetch BAR skips the 64-bit-prefetch window.
#[test]
fn every_fallback_rank_matches_linux() {
    let p32 = request(0, BarKind::Memory32, true, 0x1000);
    let p64 = request(1, BarKind::Memory64, true, 0x1000);
    let io = request(2, BarKind::Io, false, 0x100);
    let nonpref = request(3, BarKind::Memory32, false, 0x1000);

    assert_eq!(
        window_rank(p32, window(0, 1, Bucket::PrefetchableMemory32)),
        Some(0)
    );
    assert_eq!(
        window_rank(p32, window(0, 1, Bucket::NonPrefetchableMemory)),
        Some(1)
    );
    assert_eq!(
        window_rank(p32, window(0, 1, Bucket::PrefetchableMemory64)),
        None
    );
    assert_eq!(
        window_rank(p64, window(0, 1, Bucket::PrefetchableMemory64)),
        Some(0)
    );
    assert_eq!(
        window_rank(p64, window(0, 1, Bucket::PrefetchableMemory32)),
        Some(1)
    );
    assert_eq!(
        window_rank(p64, window(0, 1, Bucket::NonPrefetchableMemory)),
        Some(2)
    );
    assert_eq!(window_rank(io, window(0, 1, Bucket::Io)), Some(0));
    assert_eq!(
        window_rank(nonpref, window(0, 1, Bucket::NonPrefetchableMemory)),
        Some(0)
    );
    assert!(is_prefetchable_64(p64));
    assert!(!is_prefetchable_64(p32));
}

/// kernel/resource.c:751-771 aligns the beginning of EACH free interval before checking whether the
/// requested size fits. The first free byte is 0x1800, but a 4 KiB BAR must start at 0x2000.
#[test]
fn first_fit_rounds_up_to_natural_bar_alignment() {
    let req = request(0, BarKind::Memory32, false, 0x1000);
    let windows = [window(0x1800, 0x5000, Bucket::NonPrefetchableMemory)];
    assert_eq!(
        assign_one(req, &windows, &[]),
        Ok(Assignment {
            index: 0,
            slots: 1,
            base: 0x2000,
            size: 0x1000,
            window: 0
        })
    );
}

/// The alignment rule must hold for the entire BAR-size family, not just a convenient 4 KiB case.
/// These expected bases are literal hand-worked ALIGN(window.start, size) results.
#[test]
fn every_assigned_base_is_zero_modulo_every_bar_size_in_the_family() {
    let sizes_and_bases = [
        (0x10u64, 0x1240u64),
        (0x20, 0x1240),
        (0x40, 0x1240),
        (0x80, 0x1280),
        (0x100, 0x1300),
        (0x200, 0x1400),
        (0x400, 0x1400),
        (0x800, 0x1800),
        (0x1000, 0x2000),
        (0x2000, 0x2000),
        (0x4000, 0x4000),
        (0x8000, 0x8000),
        (0x1_0000, 0x1_0000),
        (0x20_0000, 0x20_0000),
        (0x1_0000_0000, 0x1_0000_0000),
    ];
    assert_eq!(sizes_and_bases.len(), 15);
    for (index, (size, expected_base)) in sizes_and_bases.into_iter().enumerate() {
        let req = request(index as u8, BarKind::Memory64, false, size);
        let windows = [window(
            0x1231,
            expected_base.checked_add(size).unwrap(),
            Bucket::NonPrefetchableMemory,
        )];
        let assigned = assign_one(req, &windows, &[]).unwrap();
        assert_eq!(assigned.base, expected_base, "size {size:#x}");
        assert_eq!(assigned.base % size, 0, "size {size:#x}");
        assert!(is_naturally_aligned(assigned.base, size));
    }
    assert!(!is_naturally_aligned(0x1800, 0x1000));
    assert!(!is_naturally_aligned(0, 0));
}

/// Linux's sorted assignment puts the 8 KiB request before the 4 KiB request. Show why: in this
/// 12 KiB window, small-first strands the 8 KiB BAR, while large-first places both naturally.
#[test]
fn large_first_order_avoids_alignment_fragmentation() {
    let windows = [window(0, 0x3000, Bucket::NonPrefetchableMemory)];
    let large = request(0, BarKind::Memory32, false, 0x2000);
    let small = request(1, BarKind::Memory32, false, 0x1000);
    let a0 = assign_one(large, &windows, &[]).unwrap();
    let a1 = assign_one(small, &windows, &[a0]).unwrap();
    assert_eq!((a0.base, a1.base), (0, 0x2000));

    let small_first = assign_one(small, &windows, &[]).unwrap();
    assert_eq!(
        assign_one(large, &windows, &[small_first]),
        Err(AssignError::WindowCannotSatisfy {
            index: 0,
            size: 0x2000,
            alignment: 0x2000,
            bucket: Bucket::NonPrefetchableMemory,
            window_count: 1,
        })
    );
}

/// A 64-bit BAR remains one logical assignment but consumes both adjacent config slots.
#[test]
fn a_64_bit_bar_pair_is_assigned_once_and_preserves_two_slots() {
    let req = request(4, BarKind::Memory64, true, 0x2_0000_0000);
    let windows = [window(
        0x10_0000_0000,
        0x14_0000_0000,
        Bucket::PrefetchableMemory64,
    )];
    assert_eq!(
        assign_one(req, &windows, &[]),
        Ok(Assignment {
            index: 4,
            slots: 2,
            base: 0x10_0000_0000,
            size: 0x2_0000_0000,
            window: 0,
        })
    );
}

/// setup-res.c:283-303: if exact 64-bit prefetch space fails, try 32-bit prefetch and then
/// non-prefetch. Rank wins over input window order.
#[test]
fn exact_bucket_is_tried_before_fallback_regardless_of_window_order() {
    let req = request(0, BarKind::Memory64, true, 0x1000);
    let windows = [
        window(0x1000_0000, 0x1000_4000, Bucket::NonPrefetchableMemory),
        window(0x2000_0000, 0x2000_4000, Bucket::PrefetchableMemory32),
        window(0x1_0000_0000, 0x1_0000_4000, Bucket::PrefetchableMemory64),
    ];
    let assignment = assign_one(req, &windows, &[]).unwrap();
    assert_eq!((assignment.window, assignment.base), (2, 0x1_0000_0000));
}

/// bus.c:270-288 clips 32-bit BARs below 4 GiB, while MEM_64 BARs try the high region first.
#[test]
fn address_width_clipping_and_high_first_64_bit_search_match_linux() {
    let p64 = request(0, BarKind::Memory64, true, 0x1000);
    let windows = [
        window(0x2000_0000, 0x2000_4000, Bucket::PrefetchableMemory64),
        window(0x1_0000_0000, 0x1_0000_4000, Bucket::PrefetchableMemory64),
    ];
    assert_eq!(assign_one(p64, &windows, &[]).unwrap().base, 0x1_0000_0000);

    let p32 = request(1, BarKind::Memory32, true, 0x1000);
    let above_4g = [window(
        0x1_0000_0000,
        0x1_0000_4000,
        Bucket::PrefetchableMemory32,
    )];
    assert_eq!(
        assign_one(p32, &above_4g, &[]),
        Err(AssignError::WindowCannotSatisfy {
            index: 1,
            size: 0x1000,
            alignment: 0x1000,
            bucket: Bucket::PrefetchableMemory32,
            window_count: 1,
        })
    );
}

/// A too-small interval ending at 0x27ff must refuse a 4 KiB BAR instead of silently placing it at
/// the misaligned 0x1800 address. setup-res.c:336-339 and :350-357 name bogus alignment/no space.
#[test]
fn unsatisfiable_alignment_is_a_named_refusal_not_a_misaligned_placement() {
    let req = request(5, BarKind::Memory32, false, 0x1000);
    let windows = [window(0x1800, 0x2800, Bucket::NonPrefetchableMemory)];
    assert_eq!(
        assign_one(req, &windows, &[]),
        Err(AssignError::WindowCannotSatisfy {
            index: 5,
            size: 0x1000,
            alignment: 0x1000,
            bucket: Bucket::NonPrefetchableMemory,
            window_count: 1,
        })
    );
}

#[test]
fn refusal_paths_name_the_request_value_and_window_bound() {
    let mut req = request(1, BarKind::Memory32, false, 0x1000);
    let good = [window(0x1000, 0x4000, Bucket::NonPrefetchableMemory)];
    req.size = 0;
    assert_eq!(
        assign_one(req, &good, &[]),
        Err(AssignError::SizeIsZero { index: 1, size: 0 })
    );
    req.size = 0x1000;
    req.alignment = 0;
    assert_eq!(
        assign_one(req, &good, &[]),
        Err(AssignError::AlignmentIsZero {
            index: 1,
            alignment: 0
        })
    );
    req.alignment = 0x1800;
    assert_eq!(
        assign_one(req, &good, &[]),
        Err(AssignError::AlignmentIsNotPowerOfTwo {
            index: 1,
            alignment: 0x1800
        })
    );
    req.alignment = 0x800;
    assert_eq!(
        assign_one(req, &good, &[]),
        Err(AssignError::AlignmentSmallerThanSize {
            index: 1,
            alignment: 0x800,
            size: 0x1000,
        })
    );

    let invalid = [window(0x4000, 0x3000, Bucket::NonPrefetchableMemory)];
    req.alignment = 0x1000;
    assert_eq!(
        assign_one(req, &invalid, &[]),
        Err(AssignError::InvalidWindow {
            window: 0,
            start: 0x4000,
            end: 0x3000
        })
    );

    let io_only = [window(0x1000, 0x4000, Bucket::Io)];
    assert_eq!(
        assign_one(req, &io_only, &[]),
        Err(AssignError::NoCompatibleWindow {
            index: 1,
            bucket: Bucket::NonPrefetchableMemory,
            window_count: 1,
        })
    );

    let outside = [Assignment {
        index: 0,
        slots: 1,
        base: 0x3000,
        size: 0x2000,
        window: 0,
    }];
    assert_eq!(
        assign_one(req, &good, &outside),
        Err(AssignError::UsedRangeOutsideWindow {
            used: 0,
            window: 0,
            start: 0x3000,
            end: 0x5000,
            window_start: 0x1000,
            window_end: 0x4000,
        })
    );
}
