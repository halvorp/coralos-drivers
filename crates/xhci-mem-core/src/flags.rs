// SPDX-License-Identifier: GPL-2.0-only
//! Configure-endpoint add/drop flags, ported from Linux `drivers/usb/host/xhci.h`,
//! `drivers/usb/host/xhci.c`, and their bandwidth use in `drivers/usb/host/xhci-mem.c`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use crate::layout::EP_CTX_PER_DEV;

pub const SLOT_FLAG: u32 = 1 << 0; // xhci.h:366
pub const EP0_FLAG: u32 = 1 << 1; // xhci.h:367
pub const CONTROL_FLAG_NAMES: [&str; 2] = ["SLOT", "EP0"]; // xhci.h:366-367

/// Input-control context add/drop words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigureFlags {
    pub add: u32,
    pub drop: u32,
}

/// Flag for Linux endpoint index 0 through 30.
pub const fn endpoint_flag(ep_index: usize) -> Result<u32, FlagError> {
    if ep_index >= EP_CTX_PER_DEV {
        return Err(FlagError::EndpointIndexOutOfRange { index: ep_index, maximum: EP_CTX_PER_DEV - 1 });
    }
    Ok(1u32 << (ep_index + 1)) // xhci.c:1483-1485; xhci.h:519-522
}

/// Whether an endpoint context is in the add word.
pub const fn endpoint_is_added(flags: ConfigureFlags, ep_index: usize) -> Result<bool, FlagError> {
    match endpoint_flag(ep_index) {
        Ok(flag) => Ok(flags.add & flag != 0), // xhci.h:519-520
        Err(error) => Err(error),
    }
}

/// Whether an endpoint context is in the drop word.
pub const fn endpoint_is_dropped(flags: ConfigureFlags, ep_index: usize) -> Result<bool, FlagError> {
    match endpoint_flag(ep_index) {
        Ok(flag) => Ok(flags.drop & flag != 0), // xhci.h:521-522
        Err(error) => Err(error),
    }
}

/// Apply Linux's drop-endpoint transition.
///
/// Dropping sets Dn and clears An. Slot and EP0 are refused explicitly.
pub const fn request_drop(mut flags: ConfigureFlags, ep_index: usize) -> Result<ConfigureFlags, FlagError> {
    let flag = match endpoint_flag(ep_index) { Ok(flag) => flag, Err(error) => return Err(error) };
    if flag == EP0_FLAG {
        return Err(FlagError::CannotDropDefaultControlEndpoint { ep_index }); // xhci.c:1922-1927
    }
    flags.drop |= flag; // xhci.c:1953
    flags.add &= !flag; // xhci.c:1956
    Ok(flags)
}

/// Apply Linux's add-endpoint transition.
///
/// An endpoint already in use must first have Dn set. Re-adding after a drop deliberately leaves
/// Dn set, making the command a change (drop and add).
pub const fn request_add(
    mut flags: ConfigureFlags,
    ep_index: usize,
    endpoint_already_in_use: bool,
) -> Result<ConfigureFlags, FlagError> {
    let flag = match endpoint_flag(ep_index) { Ok(flag) => flag, Err(error) => return Err(error) };
    if flag == EP0_FLAG {
        return Err(FlagError::CannotAddDefaultControlEndpoint { ep_index }); // xhci.c:2008-2016
    }
    if endpoint_already_in_use && flags.drop & flag == 0 {
        return Err(FlagError::EndpointInUseWithoutDrop { ep_index }); // xhci.c:2029-2038
    }
    if flags.add & flag != 0 {
        return Err(FlagError::EndpointAlreadyMarkedAdded { ep_index }); // xhci.c:2043-2048
    }
    flags.add |= flag; // xhci.c:2060
    Ok(flags) // drop is intentionally unchanged; xhci.c:2063-2069
}

/// Normalize words immediately before a Configure Endpoint command.
///
/// Section 4.6.6 requires A0=1 and A1=D0=D1=0.
pub const fn prepare_configure(mut flags: ConfigureFlags) -> ConfigureFlags {
    flags.add |= SLOT_FLAG;
    flags.add &= !EP0_FLAG;
    flags.drop &= !(SLOT_FLAG | EP0_FLAG); // xhci.c:3110-3117
    flags
}

/// Number of genuinely new endpoints, excluding slot, EP0, and changed endpoints.
pub const fn count_new_endpoints(flags: ConfigureFlags) -> u32 {
    let add = flags.add >> 2;
    let drop = flags.drop >> 2; // xhci.c:2230-2231
    add.count_ones() - (add & drop).count_ones() // xhci.c:2233-2238
}

/// Number of genuinely dropped endpoints, excluding slot, EP0, and changed endpoints.
pub const fn count_dropped_endpoints(flags: ConfigureFlags) -> u32 {
    let add = flags.add >> 2;
    let drop = flags.drop >> 2; // xhci.c:2247-2248
    drop.count_ones() - (add & drop).count_ones() // xhci.c:2250-2252
}

/// Configure-flag refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagError {
    EndpointIndexOutOfRange { index: usize, maximum: usize },
    CannotDropDefaultControlEndpoint { ep_index: usize },
    CannotAddDefaultControlEndpoint { ep_index: usize },
    EndpointInUseWithoutDrop { ep_index: usize },
    EndpointAlreadyMarkedAdded { ep_index: usize },
}
