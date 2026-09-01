// SPDX-License-Identifier: GPL-2.0-only
//! Layout vectors ported from Linux `drivers/usb/host/xhci-mem.c`, `xhci-caps.h`, and `xhci.h`.
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use xhci_mem_core::layout::{
    container_size, context_size, endpoint_offset, input_control_offset, slot_offset, ContainerKind,
    LayoutError, CONTAINER_KIND_NAMES, EP_CTX_PER_DEV, HCC_64BYTE_CONTEXT,
};

/// xhci-caps.h:61-62. CSZ is bit 2 and doubles EVERY context stride from 32 to 64.
#[test]
fn csz_selects_exactly_thirty_two_or_sixty_four_bytes() {
    assert_eq!(HCC_64BYTE_CONTEXT, 0x4);
    assert_eq!(context_size(0), 32);
    assert_eq!(context_size(0x4), 64);
    assert_eq!(context_size(0xffff_ffff), 64);
    assert_eq!(context_size(0xffff_fffb), 32, "neighbouring capability bits are irrelevant");
}

/// xhci-mem.c:466-468: 32 contexts plus one input-control stride for input contexts.
#[test]
fn container_sizes_pin_the_csz_doubling() {
    assert_eq!(container_size(0, ContainerKind::Device), 1024);
    assert_eq!(container_size(0, ContainerKind::Input), 1056);
    assert_eq!(container_size(0x4, ContainerKind::Device), 2048);
    assert_eq!(container_size(0x4, ContainerKind::Input), 2112);
}

/// xhci-mem.c:516-522. Only an input container begins with an input-control context.
#[test]
fn input_control_exists_only_in_an_input_container() {
    assert_eq!(input_control_offset(ContainerKind::Input), Some(0));
    assert_eq!(input_control_offset(ContainerKind::Device), None);
}

/// xhci-mem.c:525-532. Device slot is first; input slot follows its control context.
#[test]
fn slot_offset_moves_by_one_full_csz_stride_in_input_contexts() {
    assert_eq!(slot_offset(0, ContainerKind::Device), 0);
    assert_eq!(slot_offset(0, ContainerKind::Input), 32);
    assert_eq!(slot_offset(0x4, ContainerKind::Device), 0);
    assert_eq!(slot_offset(0x4, ContainerKind::Input), 64);
}

/// xhci-mem.c:535-545. This is the classic silent-corruption vector: on CSZ=1 endpoint zero is at
/// 64/128, not 32/64, and endpoint 30 must reach the last full stride.
#[test]
fn endpoint_offsets_double_every_stride_not_just_the_allocation() {
    assert_eq!(endpoint_offset(0, ContainerKind::Device, 0), Ok(32));
    assert_eq!(endpoint_offset(0, ContainerKind::Input, 0), Ok(64));
    assert_eq!(endpoint_offset(0, ContainerKind::Device, 30), Ok(992));
    assert_eq!(endpoint_offset(0, ContainerKind::Input, 30), Ok(1024));

    assert_eq!(endpoint_offset(0x4, ContainerKind::Device, 0), Ok(64));
    assert_eq!(endpoint_offset(0x4, ContainerKind::Input, 0), Ok(128));
    assert_eq!(endpoint_offset(0x4, ContainerKind::Device, 1), Ok(128));
    assert_eq!(endpoint_offset(0x4, ContainerKind::Input, 30), Ok(2048));
    assert_eq!(endpoint_offset(0x4, ContainerKind::Input, 31), Err(
        LayoutError::EndpointIndexOutOfRange { index: 31, maximum: 30 }
    ));
}

/// xhci.h:324-325 and :732. Literal names and count are independent of production tables.
#[test]
fn linux_context_kind_and_endpoint_counts_are_pinned() {
    assert_eq!(CONTAINER_KIND_NAMES.len(), 2);
    assert_eq!(CONTAINER_KIND_NAMES, ["DEVICE", "INPUT"]);
    assert_eq!(EP_CTX_PER_DEV, 31);
}
