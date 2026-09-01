// SPDX-License-Identifier: GPL-2.0-only
//! First-fit, naturally aligned assignment into compatible PCI windows.
//!
//! Ported from Linux `drivers/pci/setup-res.c`, `drivers/pci/bus.c`, and
//! `kernel/resource.c`.
//!
//! Copyright Dave Rusling, David Mosberger, David Miller, Andrea Arcangeli,
//! Ivan Kokshaysky, Linus Torvalds, and the Linux PCI authors.

use crate::request::{BarRequest, Bucket, IORESOURCE_MEM_64, IORESOURCE_PREFETCH};

/// A half-open address interval `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: u64,
    pub end: u64,
}

/// A bridge or host resource window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub range: Range,
    pub bucket: Bucket,
}

/// A successful placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assignment {
    pub index: u8,
    pub slots: u8,
    pub base: u64,
    pub size: u64,
    pub window: usize,
}

/// Named assignment refusals. Every variant carries the value and relevant bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignError {
    SizeIsZero {
        index: u8,
        size: u64,
    },
    AlignmentIsZero {
        index: u8,
        alignment: u64,
    },
    AlignmentIsNotPowerOfTwo {
        index: u8,
        alignment: u64,
    },
    AlignmentSmallerThanSize {
        index: u8,
        alignment: u64,
        size: u64,
    },
    InvalidWindow {
        window: usize,
        start: u64,
        end: u64,
    },
    UsedRangeOutsideWindow {
        used: usize,
        window: usize,
        start: u64,
        end: u64,
        window_start: u64,
        window_end: u64,
    },
    NoCompatibleWindow {
        index: u8,
        bucket: Bucket,
        window_count: usize,
    },
    WindowCannotSatisfy {
        index: u8,
        size: u64,
        alignment: u64,
        bucket: Bucket,
        window_count: usize,
    },
}

/// Return whether Linux may allocate `request` from `window`.
///
/// The exact-prefetch, 64-prefetch-in-32-prefetch, and non-prefetch fallback
/// order comes from `drivers/pci/setup-res.c:269-303`. I/O never crosses into
/// memory. A 32-bit non-prefetch resource never enters a prefetch window
/// (`drivers/pci/bus.c:212-220`).
pub const fn window_compatible(request: BarRequest, window: Window) -> bool {
    match request.bucket {
        Bucket::Io => matches!(window.bucket, Bucket::Io),
        Bucket::NonPrefetchableMemory => {
            matches!(window.bucket, Bucket::NonPrefetchableMemory)
        }
        Bucket::PrefetchableMemory32 => matches!(
            window.bucket,
            Bucket::PrefetchableMemory32 | Bucket::NonPrefetchableMemory
        ),
        Bucket::PrefetchableMemory64 => matches!(
            window.bucket,
            Bucket::PrefetchableMemory64
                | Bucket::PrefetchableMemory32
                | Bucket::NonPrefetchableMemory
        ),
    }
}

/// Return the Linux search rank for a compatible window; lower is tried first.
///
/// `None` means incompatible. This exposes and pins the fallback policy without
/// performing any allocation.
pub const fn window_rank(request: BarRequest, window: Window) -> Option<u8> {
    match (request.bucket, window.bucket) {
        (Bucket::Io, Bucket::Io) => Some(0),
        (Bucket::NonPrefetchableMemory, Bucket::NonPrefetchableMemory) => Some(0),
        (Bucket::PrefetchableMemory32, Bucket::PrefetchableMemory32) => Some(0),
        (Bucket::PrefetchableMemory32, Bucket::NonPrefetchableMemory) => Some(1),
        (Bucket::PrefetchableMemory64, Bucket::PrefetchableMemory64) => Some(0),
        (Bucket::PrefetchableMemory64, Bucket::PrefetchableMemory32) => Some(1),
        (Bucket::PrefetchableMemory64, Bucket::NonPrefetchableMemory) => Some(2),
        _ => None,
    }
}

/// Return whether `base` obeys the BAR's natural size alignment.
pub const fn is_naturally_aligned(base: u64, size: u64) -> bool {
    size != 0 && base % size == 0
}

/// Assign one request using Linux's ranked-window, first-fit arithmetic.
///
/// Linux aligns the start of each free interval before testing containment
/// (`kernel/resource.c:751-771`). The caller supplies already occupied ranges;
/// no MMIO or resource-tree mutation occurs here.
pub fn assign_one(
    request: BarRequest,
    windows: &[Window],
    used: &[Assignment],
) -> Result<Assignment, AssignError> {
    validate_request(request)?;
    for (i, window) in windows.iter().copied().enumerate() {
        if window.range.start >= window.range.end {
            return Err(AssignError::InvalidWindow {
                window: i,
                start: window.range.start,
                end: window.range.end,
            });
        }
    }
    validate_used(windows, used)?;

    let mut compatible = false;
    for wanted_rank in 0..=2 {
        // A MEM_64 resource tries the region above 4 GiB before the full 64-bit region
        // (drivers/pci/bus.c:270-282). Non-MEM_64 resources are clipped to 32 bits (:286-288).
        let region_passes = if request.flags & IORESOURCE_MEM_64 != 0 {
            2
        } else {
            1
        };
        for region_pass in 0..region_passes {
            let (region_start, region_end) = if request.flags & IORESOURCE_MEM_64 == 0 {
                (0, 0x1_0000_0000)
            } else if region_pass == 0 {
                (0x1_0000_0000, u64::MAX)
            } else {
                (0, u64::MAX)
            };
            for (window_index, window) in windows.iter().copied().enumerate() {
                if window_rank(request, window) != Some(wanted_rank) {
                    continue;
                }
                compatible = true;
                if let Some(base) = first_fit(
                    request,
                    window_index,
                    window,
                    used,
                    region_start,
                    region_end,
                ) {
                    debug_assert!(is_naturally_aligned(base, request.size));
                    return Ok(Assignment {
                        index: request.index,
                        slots: request.slots,
                        base,
                        size: request.size,
                        window: window_index,
                    });
                }
            }
        }
    }

    if !compatible {
        Err(AssignError::NoCompatibleWindow {
            index: request.index,
            bucket: request.bucket,
            window_count: windows.len(),
        })
    } else {
        Err(AssignError::WindowCannotSatisfy {
            index: request.index,
            size: request.size,
            alignment: request.alignment,
            bucket: request.bucket,
            window_count: windows.len(),
        })
    }
}

fn validate_request(request: BarRequest) -> Result<(), AssignError> {
    if request.size == 0 {
        return Err(AssignError::SizeIsZero {
            index: request.index,
            size: request.size,
        });
    }
    if request.alignment == 0 {
        return Err(AssignError::AlignmentIsZero {
            index: request.index,
            alignment: request.alignment,
        });
    }
    if !request.alignment.is_power_of_two() {
        return Err(AssignError::AlignmentIsNotPowerOfTwo {
            index: request.index,
            alignment: request.alignment,
        });
    }
    if request.alignment < request.size {
        return Err(AssignError::AlignmentSmallerThanSize {
            index: request.index,
            alignment: request.alignment,
            size: request.size,
        });
    }
    Ok(())
}

fn validate_used(windows: &[Window], used: &[Assignment]) -> Result<(), AssignError> {
    for (i, assignment) in used.iter().copied().enumerate() {
        let Some(window) = windows.get(assignment.window).copied() else {
            return Err(AssignError::UsedRangeOutsideWindow {
                used: i,
                window: assignment.window,
                start: assignment.base,
                end: assignment.base.saturating_add(assignment.size),
                window_start: 0,
                window_end: 0,
            });
        };
        let end = assignment
            .base
            .checked_add(assignment.size)
            .unwrap_or(u64::MAX);
        if assignment.size == 0 || assignment.base < window.range.start || end > window.range.end {
            return Err(AssignError::UsedRangeOutsideWindow {
                used: i,
                window: assignment.window,
                start: assignment.base,
                end,
                window_start: window.range.start,
                window_end: window.range.end,
            });
        }
    }
    Ok(())
}

fn first_fit(
    request: BarRequest,
    window_index: usize,
    window: Window,
    used: &[Assignment],
    region_start: u64,
    region_end: u64,
) -> Option<u64> {
    let clipped_start = window.range.start.max(region_start);
    let clipped_end = window.range.end.min(region_end);
    let mut cursor = clipped_start;
    loop {
        let base = align_up(cursor, request.alignment)?;
        let end = base.checked_add(request.size)?;
        if end > clipped_end {
            return None;
        }

        let mut blocking_end: Option<u64> = None;
        for assignment in used {
            if assignment.window != window_index {
                continue;
            }
            let occupied_end = assignment.base.checked_add(assignment.size)?;
            if base < occupied_end && assignment.base < end {
                blocking_end = Some(match blocking_end {
                    Some(previous) => previous.min(occupied_end),
                    None => occupied_end,
                });
            }
        }
        match blocking_end {
            Some(next) if next > cursor => cursor = next,
            Some(_) => return None,
            None => return Some(base),
        }
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let mask = alignment - 1;
    value.checked_add(mask).map(|sum| sum & !mask)
}

/// Whether a request carries Linux's 64-bit prefetch pair of modifiers.
pub const fn is_prefetchable_64(request: BarRequest) -> bool {
    request.flags & (IORESOURCE_PREFETCH | IORESOURCE_MEM_64)
        == (IORESOURCE_PREFETCH | IORESOURCE_MEM_64)
}
