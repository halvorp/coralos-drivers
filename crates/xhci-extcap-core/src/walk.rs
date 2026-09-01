// SPDX-License-Identifier: GPL-2.0-only
//! Pure xHCI extended-capability list walking.
//!
//! Ported from Linux `drivers/usb/host/xhci-ext-caps.h`, especially
//! `xhci_find_next_ext_cap()` at lines 130-156.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use crate::caps::{ext_cap_id, ext_cap_next, hcc_ext_caps, HCC_PARAMS_OFFSET, MAX_EXT_CAPS};

/// One already-read capability header and its byte offset from the controller register base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityWord {
    pub offset: u32,
    pub header: u32,
}

/// A requested capability ID. Linux uses zero to mean "the next capability" (:122-123).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityMatch {
    Any,
    Id(u8),
}

impl CapabilityMatch {
    const fn matches(self, header: u32) -> bool {
        match self {
            Self::Any => true,
            Self::Id(id) => ext_cap_id(header) == id,
        }
    }
}

/// Named end/refusal outcomes from a capability walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkRefusal {
    /// Linux treats an all-ones register read as an inaccessible controller (:139-140, :147-148).
    InaccessibleRegister { offset: u32 },
    /// HCCPARAMS1 contains no first extended-capability pointer (:141-143).
    NoFirstCapability { hcc_params1: u32 },
    /// The current capability's NEXT field is zero, which means END OF LIST, never a zero step.
    NextPointerIsZeroEndOfList { offset: u32 },
    /// The provided pure input has no header at the offset selected by the hardware list.
    HeaderNotProvided { offset: u32 },
    /// The byte offset calculation cannot be represented.
    OffsetOverflow { offset: u32, next_dwords: u8 },
    /// Linux limits extended-capability inspection to 50 entries.
    MaximumCapabilitiesExceeded { maximum: usize },
}

/// Convert HCCPARAMS1's DWORD xECP to the first extended capability's byte offset.
pub const fn first_capability_offset(hcc_params1: u32) -> Result<u32, WalkRefusal> {
    if hcc_params1 == u32::MAX {
        return Err(WalkRefusal::InaccessibleRegister {
            offset: HCC_PARAMS_OFFSET,
        }); // xhci-ext-caps.h:138-140
    }
    let dwords = hcc_ext_caps(hcc_params1);
    if dwords == 0 {
        return Err(WalkRefusal::NoFirstCapability { hcc_params1 }); // xhci-ext-caps.h:141-143
    }
    Ok((dwords as u32) << 2) // xhci-ext-caps.h:141
}

/// Advance from one capability header using its DWORD NEXT field.
pub const fn next_capability_offset(offset: u32, header: u32) -> Result<u32, WalkRefusal> {
    let next_dwords = ext_cap_next(header); // xhci-ext-caps.h:152
    if next_dwords == 0 {
        return Err(WalkRefusal::NextPointerIsZeroEndOfList { offset }); // xhci-ext-caps.h:154-156
    }
    let step = (next_dwords as u32) << 2; // xhci-ext-caps.h:153
    match offset.checked_add(step) {
        Some(next) => Ok(next),
        None => Err(WalkRefusal::OffsetOverflow {
            offset,
            next_dwords,
        }),
    }
}

fn header_at(words: &[CapabilityWord], offset: u32) -> Result<u32, WalkRefusal> {
    let Some(word) = words.iter().find(|word| word.offset == offset) else {
        return Err(WalkRefusal::HeaderNotProvided { offset });
    };
    if word.header == u32::MAX {
        return Err(WalkRefusal::InaccessibleRegister { offset }); // xhci-ext-caps.h:146-148
    }
    Ok(word.header)
}

/// Find the next matching capability after `start`, mirroring `xhci_find_next_ext_cap()` without
/// MMIO. `start == 0` or `HCC_PARAMS_OFFSET` starts from HCCPARAMS1; otherwise the current
/// capability at `start` is skipped before matching, allowing repeated Protocol capabilities.
pub fn find_next_ext_cap(
    hcc_params1: u32,
    words: &[CapabilityWord],
    start: u32,
    wanted: CapabilityMatch,
) -> Result<u32, WalkRefusal> {
    let from_hcc = start == 0 || start == HCC_PARAMS_OFFSET; // xhci-ext-caps.h:137
    let mut offset = if from_hcc {
        first_capability_offset(hcc_params1)?
    } else {
        start
    };

    for inspected in 0..MAX_EXT_CAPS {
        let header = header_at(words, offset)?;
        if offset != start && wanted.matches(header) {
            return Ok(offset); // xhci-ext-caps.h:149-150
        }
        match next_capability_offset(offset, header) {
            Ok(next) => offset = next,
            Err(WalkRefusal::NextPointerIsZeroEndOfList { .. }) => {
                return Err(WalkRefusal::NextPointerIsZeroEndOfList { offset });
            }
            Err(other) => return Err(other),
        }
        if inspected + 1 == MAX_EXT_CAPS {
            return Err(WalkRefusal::MaximumCapabilitiesExceeded {
                maximum: MAX_EXT_CAPS,
            });
        }
    }

    Err(WalkRefusal::MaximumCapabilitiesExceeded {
        maximum: MAX_EXT_CAPS,
    })
}
