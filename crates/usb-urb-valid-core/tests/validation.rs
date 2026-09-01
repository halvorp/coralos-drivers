// SPDX-License-Identifier: GPL-2.0-only
//! Frozen sanity and bounds vectors from Linux `drivers/usb/core/urb.c:399-499`.
//!
//! Copyright (C) the Linux USB core authors.

use usb_urb_valid_core::{
    validation::{
        maximum_packet_length, validate, ControlSetup, UrbDescriptor, ValidatedUrb,
        ValidationError, MAX_TRANSFER_BUFFER_LENGTH, USB_DIR_IN,
    },
    Direction, Speed, TransferType,
};

fn descriptor<'a>(kind: TransferType, packets: &'a [i32]) -> UrbDescriptor<'a> {
    UrbDescriptor {
        transfer_type: kind,
        pipe_transfer_type: kind,
        speed: Speed::Full,
        endpoint_direction: Direction::Out,
        max_packet_size: 64,
        high_speed_transactions: 1,
        superspeed_burst: 1,
        superspeed_mult: 1,
        superspeed_plus_bytes_per_interval: None,
        high_speed_double_bytes_per_interval: None,
        setup: None,
        transfer_buffer_length: 0,
        number_of_packets: packets.len() as i32,
        iso_packet_lengths: packets,
        scatter_gather_lengths: &[],
        no_sg_constraint: false,
    }
}

#[test]
fn all_four_transfer_types_drive_their_real_validation_paths() {
    let control = UrbDescriptor {
        setup: Some(ControlSetup {
            request_type: USB_DIR_IN,
            length: 8,
        }),
        transfer_buffer_length: 8,
        endpoint_direction: Direction::In,
        ..descriptor(TransferType::Control, &[])
    };
    assert_eq!(
        validate(&control, false),
        Ok(ValidatedUrb {
            direction: Direction::In,
            maximum_packet_length: 64,
            endpoint_direction_mismatch: false,
            pipe_type_mismatch: false,
        })
    );
    assert!(validate(&descriptor(TransferType::Bulk, &[]), true).is_ok());
    assert!(validate(&descriptor(TransferType::Interrupt, &[]), true).is_ok());
    assert!(validate(&descriptor(TransferType::Isochronous, &[64]), true).is_ok());
}

#[test]
fn pipe_type_mismatch_is_reported_without_rejecting_like_linux_warning() {
    let mismatch = UrbDescriptor {
        pipe_transfer_type: TransferType::Interrupt,
        ..descriptor(TransferType::Bulk, &[])
    };
    let got = validate(&mismatch, true).unwrap();
    assert!(got.pipe_type_mismatch); // urb.c:506-509 warns but continues
}

#[test]
fn control_requires_setup_and_exact_setup_length() {
    let missing = descriptor(TransferType::Control, &[]);
    assert_eq!(
        validate(&missing, false),
        Err(ValidationError::ControlSetupMissing {
            transfer_type: TransferType::Control
        })
    ); // urb.c:403-408

    let mismatch = UrbDescriptor {
        setup: Some(ControlSetup {
            request_type: USB_DIR_IN,
            length: 8,
        }),
        transfer_buffer_length: 7,
        ..missing
    };
    assert_eq!(
        validate(&mismatch, false),
        Err(ValidationError::ControlLengthMismatch {
            setup_length: 8,
            transfer_buffer_length: 7,
        })
    ); // urb.c:414-418
}

#[test]
fn control_direction_comes_from_setup_and_zero_length_is_out() {
    let in_transfer = UrbDescriptor {
        setup: Some(ControlSetup {
            request_type: 0x80,
            length: 1,
        }),
        transfer_buffer_length: 1,
        endpoint_direction: Direction::Out,
        ..descriptor(TransferType::Control, &[])
    };
    let got = validate(&in_transfer, false).unwrap();
    assert_eq!(got.direction, Direction::In);
    assert!(got.endpoint_direction_mismatch); // urb.c:409-413 warns, does not reject

    let no_data = UrbDescriptor {
        setup: Some(ControlSetup {
            request_type: 0x80,
            length: 0,
        }),
        transfer_buffer_length: 0,
        endpoint_direction: Direction::Out,
        ..descriptor(TransferType::Control, &[])
    };
    assert_eq!(validate(&no_data, false).unwrap().direction, Direction::Out); // urb.c:409-410
}

#[test]
fn only_control_is_allowed_before_configuration() {
    for kind in [
        TransferType::Bulk,
        TransferType::Interrupt,
        TransferType::Isochronous,
    ] {
        assert_eq!(
            validate(&descriptor(kind, &[1]), false),
            Err(ValidationError::DeviceNotConfigured {
                transfer_type: kind
            })
        ); // urb.c:432-434
    }
}

#[test]
fn zero_maxpacket_names_the_type_and_minimum() {
    let zero = UrbDescriptor {
        max_packet_size: 0,
        ..descriptor(TransferType::Bulk, &[])
    };
    assert_eq!(
        validate(&zero, true),
        Err(ValidationError::EndpointMaxPacketZero {
            transfer_type: TransferType::Bulk,
            minimum: 1,
        })
    ); // urb.c:436-443
}

#[test]
fn maximum_packet_length_covers_speed_companion_rules() {
    let high = UrbDescriptor {
        speed: Speed::High,
        max_packet_size: 1024,
        high_speed_transactions: 3,
        ..descriptor(TransferType::Isochronous, &[3072])
    };
    assert_eq!(maximum_packet_length(&high), Ok(3072)); // urb.c:471-476

    let double = UrbDescriptor {
        high_speed_double_bytes_per_interval: Some(6144),
        ..high
    };
    assert_eq!(maximum_packet_length(&double), Ok(6144)); // urb.c:473-474

    let super_speed = UrbDescriptor {
        speed: Speed::Super,
        max_packet_size: 1024,
        superspeed_burst: 16,
        superspeed_mult: 3,
        ..descriptor(TransferType::Isochronous, &[])
    };
    assert_eq!(maximum_packet_length(&super_speed), Ok(49152)); // urb.c:453-460

    let super_plus = UrbDescriptor {
        speed: Speed::SuperPlus,
        superspeed_plus_bytes_per_interval: Some(65536),
        ..super_speed
    };
    assert_eq!(maximum_packet_length(&super_plus), Ok(65536)); // urb.c:463-469

    let bulk = UrbDescriptor {
        speed: Speed::Super,
        max_packet_size: 1024,
        superspeed_burst: 16,
        superspeed_mult: 3,
        ..descriptor(TransferType::Bulk, &[])
    };
    assert_eq!(
        maximum_packet_length(&bulk),
        Ok(1024),
        "multipliers are ISO-only"
    );
}

#[test]
fn isochronous_packet_count_and_each_length_are_bounded() {
    let empty = descriptor(TransferType::Isochronous, &[]);
    assert_eq!(
        validate(&empty, true),
        Err(ValidationError::IsoPacketCountNotPositive {
            number_of_packets: 0,
            minimum: 1,
        })
    ); // urb.c:479-480
    let minus_one = UrbDescriptor {
        number_of_packets: -1,
        ..empty
    };
    assert_eq!(
        validate(&minus_one, true),
        Err(ValidationError::IsoPacketCountNotPositive {
            number_of_packets: -1,
            minimum: 1,
        })
    );
    let missing = UrbDescriptor {
        number_of_packets: 2,
        iso_packet_lengths: &[64],
        ..empty
    };
    assert_eq!(
        validate(&missing, true),
        Err(ValidationError::IsoPacketDescriptorsMissing {
            number_of_packets: 2,
            descriptors_supplied: 1,
        })
    );

    let negative = descriptor(TransferType::Isochronous, &[64, -1]);
    assert_eq!(
        validate(&negative, true),
        Err(ValidationError::IsoPacketLengthNegative {
            packet_index: 1,
            length: -1,
            minimum: 0,
        })
    ); // urb.c:481-484

    let above = descriptor(TransferType::Isochronous, &[64, 65]);
    assert_eq!(
        validate(&above, true),
        Err(ValidationError::IsoPacketLengthAboveMaximum {
            packet_index: 1,
            length: 65,
            maximum: 64,
        })
    );

    assert!(validate(&descriptor(TransferType::Isochronous, &[0, 64]), true).is_ok());
}

#[test]
fn scatter_gather_entries_except_the_last_must_align_to_maxpacket() {
    let aligned = UrbDescriptor {
        scatter_gather_lengths: &[64, 128, 1],
        ..descriptor(TransferType::Bulk, &[])
    };
    assert!(validate(&aligned, true).is_ok()); // urb.c:488-494: last entry is exempt

    let bad = UrbDescriptor {
        scatter_gather_lengths: &[65, 64],
        ..descriptor(TransferType::Bulk, &[])
    };
    assert_eq!(
        validate(&bad, true),
        Err(ValidationError::ScatterGatherLengthNotPacketAligned {
            entry_index: 0,
            length: 65,
            packet_size: 64,
        })
    );

    let unconstrained = UrbDescriptor {
        no_sg_constraint: true,
        ..bad
    };
    assert!(validate(&unconstrained, true).is_ok());
}

#[test]
fn transfer_buffer_length_is_bounded_at_int_max_from_both_sides() {
    assert_eq!(MAX_TRANSFER_BUFFER_LENGTH, 2_147_483_647); // urb.c:497-499, INT_MAX
    let at = UrbDescriptor {
        transfer_buffer_length: 2_147_483_647,
        ..descriptor(TransferType::Bulk, &[])
    };
    assert!(validate(&at, true).is_ok());
    let above = UrbDescriptor {
        transfer_buffer_length: 2_147_483_648,
        ..descriptor(TransferType::Bulk, &[])
    };
    assert_eq!(
        validate(&above, true),
        Err(ValidationError::TransferBufferLengthAboveMaximum {
            length: 2_147_483_648,
            maximum: 2_147_483_647,
        })
    );
}

/// `USB_DIR_IN` is pinned BY VALUE, not merely used.
///
/// Every other test in this file passes `USB_DIR_IN` as the `request_type` it builds its input
/// from, and production then tests that same input against the same constant. Mutating the constant
/// therefore changes BOTH sides and cancels out — the suite stays green with a wrong direction bit.
/// A mutation sweep caught exactly this: USB_DIR_IN was the one constant in the crate no test could
/// detect a change to. A wrong direction bit sends a control read as a write, and the device
/// answers a question nobody asked.
#[test]
fn usb_dir_in_is_the_top_bit_of_bmrequesttype() {
    assert_eq!(USB_DIR_IN, 0x80, "include/uapi/linux/usb/ch9.h:46");
    assert_eq!(USB_DIR_IN.count_ones(), 1, "it is a single direction bit, not a mask");
    assert_eq!(USB_DIR_IN & 0x7f, 0, "the low seven bits belong to type and recipient");
}
