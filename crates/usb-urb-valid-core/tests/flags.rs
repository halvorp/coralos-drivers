// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for Linux URB flags and `drivers/usb/core/urb.c:511-533` policy.
//!
//! Copyright (C) the Linux USB core and Linux USB API authors.

use usb_urb_valid_core::{
    flags::{
        apply_policy, prepare_for_submit, DIR_IN, DIR_MASK, DIR_OUT, DMA_MAP_PAGE, DMA_MAP_SG,
        DMA_MAP_SINGLE, DMA_SG_COMBINED, FREE_BUFFER, INTERNAL_SUBMIT_MASK, ISO_ASAP, MAP_LOCAL,
        NO_INTERRUPT, NO_TRANSFER_DMA_MAP, SETUP_MAP_LOCAL, SETUP_MAP_SINGLE, SHORT_NOT_OK,
        ZERO_PACKET,
    },
    Direction, TransferType,
};

#[test]
fn every_linux_flag_literal_is_pinned() {
    assert_eq!(SHORT_NOT_OK, 0x0001); // include/linux/usb.h:1386
    assert_eq!(ISO_ASAP, 0x0002); // include/linux/usb.h:1387
    assert_eq!(NO_TRANSFER_DMA_MAP, 0x0004); // include/linux/usb.h:1389
    assert_eq!(ZERO_PACKET, 0x0040); // include/linux/usb.h:1390
    assert_eq!(NO_INTERRUPT, 0x0080); // include/linux/usb.h:1391
    assert_eq!(FREE_BUFFER, 0x0100); // include/linux/usb.h:1393
    assert_eq!(DIR_IN, 0x0200); // include/linux/usb.h:1396
    assert_eq!(DIR_OUT, 0); // include/linux/usb.h:1397
    assert_eq!(DIR_MASK, DIR_IN); // include/linux/usb.h:1398
    assert_eq!(DMA_MAP_SINGLE, 0x0001_0000); // include/linux/usb.h:1400
    assert_eq!(DMA_MAP_PAGE, 0x0002_0000); // include/linux/usb.h:1401
    assert_eq!(DMA_MAP_SG, 0x0004_0000); // include/linux/usb.h:1402
    assert_eq!(MAP_LOCAL, 0x0008_0000); // include/linux/usb.h:1403
    assert_eq!(SETUP_MAP_SINGLE, 0x0010_0000); // include/linux/usb.h:1404
    assert_eq!(SETUP_MAP_LOCAL, 0x0020_0000); // include/linux/usb.h:1405
    assert_eq!(DMA_SG_COMBINED, 0x0040_0000); // include/linux/usb.h:1406
    assert_eq!(
        INTERNAL_SUBMIT_MASK,
        DIR_MASK
            | DMA_MAP_SINGLE
            | DMA_MAP_PAGE
            | DMA_MAP_SG
            | MAP_LOCAL
            | SETUP_MAP_SINGLE
            | SETUP_MAP_LOCAL
            | DMA_SG_COMBINED
    ); // drivers/usb/core/urb.c:425-428
}

#[test]
fn zero_valued_dir_out_is_pinned_by_direction_selection_behavior() {
    assert_ne!(DIR_OUT, DIR_IN);
    let out = prepare_for_submit(
        DIR_IN | DMA_MAP_SINGLE | NO_TRANSFER_DMA_MAP,
        Direction::Out,
    );
    assert_eq!(out.flags, 0x0004); // urb.c:425-429: common public bit remains, OUT is zero
    assert_eq!(out.removed_internal, 0x0001_0200);
    let input = prepare_for_submit(DMA_MAP_PAGE | NO_TRANSFER_DMA_MAP, Direction::In);
    assert_eq!(input.flags, 0x0204); // include/linux/usb.h:1389,1396
    assert_eq!(input.removed_internal, 0x0002_0000);
}

#[test]
fn flag_policy_drives_every_transfer_type_and_direction() {
    let semantic = SHORT_NOT_OK | ISO_ASAP | NO_TRANSFER_DMA_MAP | ZERO_PACKET;
    let vectors = [
        (TransferType::Control, Direction::Out, 0x0004, 0x0043),
        (TransferType::Control, Direction::In, 0x0005, 0x0042),
        (TransferType::Bulk, Direction::Out, 0x0044, 0x0003),
        (TransferType::Bulk, Direction::In, 0x0005, 0x0042),
        (TransferType::Interrupt, Direction::Out, 0x0044, 0x0003),
        (TransferType::Interrupt, Direction::In, 0x0005, 0x0042),
        (TransferType::Isochronous, Direction::Out, 0x0006, 0x0041),
        (TransferType::Isochronous, Direction::In, 0x0006, 0x0041),
    ]; // drivers/usb/core/urb.c:511-528
    for (kind, direction, accepted, refused) in vectors {
        let got = apply_policy(kind, direction, semantic);
        assert_eq!(got.accepted, accepted, "{kind:?} {direction:?}");
        assert_eq!(got.refused, refused, "{kind:?} {direction:?}");
    }
}

#[test]
fn common_flags_survive_for_every_type_and_unknown_flags_are_named_as_refused_bits() {
    let common = NO_TRANSFER_DMA_MAP | NO_INTERRUPT | FREE_BUFFER;
    for kind in [
        TransferType::Control,
        TransferType::Isochronous,
        TransferType::Bulk,
        TransferType::Interrupt,
    ] {
        let got = apply_policy(kind, Direction::Out, common | 0x8000_0000);
        assert_eq!(got.accepted & common, 0x0184); // usb.h:1389,1391,1393
        assert_eq!(got.refused, 0x8000_0000);
    }
}
