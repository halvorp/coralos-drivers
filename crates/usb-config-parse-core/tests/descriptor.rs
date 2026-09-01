// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for fixed USB descriptors and every fixed-parser length refusal.
//!
//! Ported from Linux `drivers/usb/core/config.c` and `include/uapi/linux/usb/ch9.h`.
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! core and Chapter 9 header authors and contributors.

use usb_config_parse_core::descriptor::{
    parse_configuration, parse_endpoint, parse_header, parse_interface, ParseError, RefusalSite,
    CONFIGURATION_SIZE, CONFIGURATION_TYPE, ENDPOINT_AUDIO_SIZE, ENDPOINT_SIZE, ENDPOINT_TYPE,
    HEADER_SIZE, INTERFACE_SIZE, INTERFACE_TYPE, PARSED_DESCRIPTOR_TYPES,
};

/// Linux `include/uapi/linux/usb/ch9.h:238-241,366,410,430-431` literals, written independently.
#[test]
fn descriptor_literals_match_linux() {
    assert_eq!(HEADER_SIZE, 2); // ch9.h:276-279
    assert_eq!(CONFIGURATION_SIZE, 9); // ch9.h:366
    assert_eq!(INTERFACE_SIZE, 9); // ch9.h:410
    assert_eq!(ENDPOINT_SIZE, 7); // ch9.h:430
    assert_eq!(ENDPOINT_AUDIO_SIZE, 9); // ch9.h:431
    assert_eq!(CONFIGURATION_TYPE, 0x02); // ch9.h:238
    assert_eq!(INTERFACE_TYPE, 0x04); // ch9.h:240
    assert_eq!(ENDPOINT_TYPE, 0x05); // ch9.h:241

    // Exactly the three descriptor kinds this crate interprets; never derived from production.
    let linux_names = ["CONFIGURATION", "INTERFACE", "ENDPOINT"];
    let linux_values = [0x02, 0x04, 0x05];
    assert_eq!(PARSED_DESCRIPTOR_TYPES.len(), 3);
    assert_eq!(
        PARSED_DESCRIPTOR_TYPES
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        linux_names
    );
    assert_eq!(
        PARSED_DESCRIPTOR_TYPES
            .iter()
            .map(|(_, value)| *value)
            .collect::<Vec<_>>(),
        linux_values
    );
}

/// `config.c:706-718`: before reading either header byte, two bytes must remain.
#[test]
fn header_refuses_each_truncated_prefix_by_name() {
    for bytes in [&[][..], &[2][..]] {
        assert_eq!(
            parse_header(bytes),
            Err(ParseError::Truncated {
                site: RefusalSite::Header,
                available: bytes.len(),
                required: 2,
            })
        );
    }
    assert!(parse_header(&[2, 0x99]).is_ok());
}

/// `config.c:713-718`: `bLength` below two or beyond the remaining bytes stops the walk.
#[test]
fn header_refuses_blength_bounds_by_name() {
    assert_eq!(
        parse_header(&[1, 0x99]),
        Err(ParseError::LengthBelowMinimum {
            site: RefusalSite::Header,
            length: 1,
            minimum: 2,
        })
    );
    assert_eq!(
        parse_header(&[3, 0x99]),
        Err(ParseError::LengthExceedsBuffer {
            site: RefusalSite::Header,
            length: 3,
            available: 2,
        })
    );
}

#[test]
fn configuration_vector_uses_little_endian_total_length() {
    // usb_config_descriptor layout, ch9.h:352-366.
    let d = parse_configuration(&[9, 2, 0x20, 0x01, 2, 7, 3, 0xc0, 50]).unwrap();
    assert_eq!(d.total_length, 0x0120);
    assert_eq!(d.num_interfaces, 2);
    assert_eq!(d.configuration_value, 7);
    assert_eq!(d.configuration_string, 3);
    assert_eq!(d.attributes, 0xc0);
    assert_eq!(d.max_power, 50);
}

/// Every configuration length check gets its own hostile truncation (`config.c:675-685`).
#[test]
fn configuration_refuses_every_truncation_and_bad_length() {
    assert_eq!(
        parse_configuration(&[9]),
        Err(ParseError::Truncated {
            site: RefusalSite::Configuration,
            available: 1,
            required: 2,
        })
    );
    assert_eq!(
        parse_configuration(&[8, 2, 0, 0, 0, 0, 0, 0]),
        Err(ParseError::LengthBelowMinimum {
            site: RefusalSite::Configuration,
            length: 8,
            minimum: 9,
        })
    );
    assert_eq!(
        parse_configuration(&[9, 2, 0, 0, 0, 0, 0, 0]),
        Err(ParseError::LengthExceedsBuffer {
            site: RefusalSite::Configuration,
            length: 9,
            available: 8,
        })
    );
    assert_eq!(
        parse_configuration(&[9, 4, 0, 0, 0, 0, 0, 0, 0]),
        Err(ParseError::UnexpectedType {
            site: RefusalSite::Configuration,
            actual: 4,
            expected: 2,
        })
    );
}

#[test]
fn interface_vector_decodes_all_fixed_fields() {
    // usb_interface_descriptor layout, ch9.h:397-410.
    let d = parse_interface(&[9, 4, 3, 2, 4, 0xff, 0x42, 0x17, 6]).unwrap();
    assert_eq!(d.number, 3);
    assert_eq!(d.alternate_setting, 2);
    assert_eq!(d.num_endpoints, 4);
    assert_eq!(d.class, 0xff);
    assert_eq!(d.subclass, 0x42);
    assert_eq!(d.protocol, 0x17);
    assert_eq!(d.interface_string, 6);
}

/// Interface checks from `config.c:565-570,721-730`, including each truncated boundary.
#[test]
fn interface_refuses_every_truncation_length_and_type_error() {
    assert_eq!(
        parse_interface(&[]),
        Err(ParseError::Truncated {
            site: RefusalSite::Interface,
            available: 0,
            required: 2,
        })
    );
    assert_eq!(
        parse_interface(&[8, 4, 0, 0, 0, 0, 0, 0]),
        Err(ParseError::LengthBelowMinimum {
            site: RefusalSite::Interface,
            length: 8,
            minimum: 9,
        })
    );
    assert_eq!(
        parse_interface(&[9, 4, 0, 0, 0, 0, 0, 0]),
        Err(ParseError::LengthExceedsBuffer {
            site: RefusalSite::Interface,
            length: 9,
            available: 8,
        })
    );
    assert_eq!(
        parse_interface(&[9, 5, 0, 0, 0, 0, 0, 0, 0]),
        Err(ParseError::UnexpectedType {
            site: RefusalSite::Interface,
            actual: 5,
            expected: 4,
        })
    );
}

#[test]
fn endpoint_vectors_cover_standard_audio_and_reserved_address_bits() {
    // usb_endpoint_descriptor layout, ch9.h:414-431.
    let d = parse_endpoint(&[7, 5, 0xf2, 0x25, 0x00, 0x14, 9]).unwrap();
    assert_eq!(
        d.address, 0x82,
        "config.c:333-340 clears reserved address bits"
    );
    assert_eq!(d.attributes, 0x25);
    assert_eq!(d.max_packet_size, 0x1400);
    assert_eq!(d.interval, 9);
    assert_eq!(d.audio_extension, None);

    let audio = parse_endpoint(&[9, 5, 0x03, 1, 0x00, 0x04, 1, 7, 0x83]).unwrap();
    assert_eq!(audio.audio_extension, Some([7, 0x83])); // ch9.h:426-431
}

/// Endpoint checks from `config.c:300-321`; each dereference boundary is fed a short buffer.
#[test]
fn endpoint_refuses_every_truncation_length_type_and_zero_error() {
    assert_eq!(
        parse_endpoint(&[7]),
        Err(ParseError::Truncated {
            site: RefusalSite::Endpoint,
            available: 1,
            required: 2,
        })
    );
    assert_eq!(
        parse_endpoint(&[6, 5, 1, 0, 0, 0]),
        Err(ParseError::LengthBelowMinimum {
            site: RefusalSite::Endpoint,
            length: 6,
            minimum: 7,
        })
    );
    assert_eq!(
        parse_endpoint(&[7, 5, 1, 0, 0, 0]),
        Err(ParseError::LengthExceedsBuffer {
            site: RefusalSite::Endpoint,
            length: 7,
            available: 6,
        })
    );
    assert_eq!(
        parse_endpoint(&[9, 5, 1, 0, 0, 0, 1, 2]),
        Err(ParseError::LengthExceedsBuffer {
            site: RefusalSite::Endpoint,
            length: 9,
            available: 8,
        }),
        "audio extension may not read bytes 7/8 until all nine advertised bytes exist"
    );
    assert_eq!(
        parse_endpoint(&[7, 4, 1, 0, 0, 0, 0]),
        Err(ParseError::UnexpectedType {
            site: RefusalSite::Endpoint,
            actual: 4,
            expected: 5,
        })
    );
    assert_eq!(
        parse_endpoint(&[7, 5, 0x80, 0, 8, 0, 0]),
        Err(ParseError::EndpointZero { address: 0x80 })
    );
}

#[test]
fn refusal_display_names_the_operation_value_and_bound() {
    let e = parse_endpoint(&[7, 5]).unwrap_err();
    assert_eq!(e.name(), "endpoint descriptor");
    assert_eq!(
        e.to_string(),
        "endpoint descriptor refused bLength 7 exceeding available 2"
    );
}
