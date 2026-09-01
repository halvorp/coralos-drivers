// SPDX-License-Identifier: GPL-2.0-only
//! Context-size selection and byte offsets, ported from Linux
//! `drivers/usb/host/xhci-mem.c`, `drivers/usb/host/xhci-caps.h`, and
//! `drivers/usb/host/xhci.h`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

/// HCCPARAMS1 Context Size capability bit.
pub const HCC_64BYTE_CONTEXT: u32 = 1 << 2; // xhci-caps.h:61
/// Number of endpoint contexts in one device context.
pub const EP_CTX_PER_DEV: usize = 31; // xhci.h:732
/// Names of the two container context kinds Linux accepts.
pub const CONTAINER_KIND_NAMES: [&str; 2] = ["DEVICE", "INPUT"]; // xhci.h:324-325

/// Linux container-context kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    /// Hardware output/device context.
    Device,
    /// Command input context, prefixed by an input-control context.
    Input,
}

/// Size of one context stride from HCCPARAMS1 CSZ.
///
/// The 64-byte result is load-bearing: using 32 on a CSZ controller aliases each endpoint with the
/// reserved back half of the preceding context.
pub const fn context_size(hcc_params: u32) -> usize {
    if hcc_params & HCC_64BYTE_CONTEXT != 0 { 64 } else { 32 } // xhci-caps.h:62
}

/// Bytes Linux allocates for a device or input container.
pub const fn container_size(hcc_params: u32, kind: ContainerKind) -> usize {
    let device_bytes = if hcc_params & HCC_64BYTE_CONTEXT != 0 { 2048 } else { 1024 }; // xhci-mem.c:466
    match kind {
        ContainerKind::Device => device_bytes,
        ContainerKind::Input => device_bytes + context_size(hcc_params), // xhci-mem.c:467-468
    }
}

/// Byte offset of the input-control context.
pub const fn input_control_offset(kind: ContainerKind) -> Option<usize> {
    match kind {
        ContainerKind::Input => Some(0), // xhci-mem.c:516-522
        ContainerKind::Device => None,   // xhci-mem.c:519-520
    }
}

/// Byte offset of the slot context.
pub const fn slot_offset(hcc_params: u32, kind: ContainerKind) -> usize {
    match kind {
        ContainerKind::Device => 0,                    // xhci-mem.c:528-529
        ContainerKind::Input => context_size(hcc_params), // xhci-mem.c:531-532
    }
}

/// Byte offset of endpoint context `ep_index` (Linux index 0 through 30).
pub const fn endpoint_offset(
    hcc_params: u32,
    kind: ContainerKind,
    ep_index: usize,
) -> Result<usize, LayoutError> {
    if ep_index >= EP_CTX_PER_DEV {
        return Err(LayoutError::EndpointIndexOutOfRange {
            index: ep_index,
            maximum: EP_CTX_PER_DEV - 1,
        });
    }
    let prefix = match kind {
        ContainerKind::Device => 1, // slot context; xhci-mem.c:541
        ContainerKind::Input => 2,  // input-control + slot contexts; xhci-mem.c:542-543
    };
    Ok((ep_index + prefix) * context_size(hcc_params)) // xhci-mem.c:544-545
}

/// A context-address calculation refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutError {
    /// The requested Linux endpoint index exceeds the 31-context array.
    EndpointIndexOutOfRange { index: usize, maximum: usize },
}
