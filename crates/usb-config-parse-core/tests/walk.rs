// SPDX-License-Identifier: GPL-2.0-only
//! Hostile-stream vectors for Linux's descriptor walk in `drivers/usb/core/config.c`.
//!
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! core authors and contributors.

use usb_config_parse_core::descriptor::{ParseError, RefusalSite};
use usb_config_parse_core::walk::{find_next_descriptor, DescriptorIter};

#[test]
fn iterator_walks_configuration_interface_endpoint_offsets() {
    let bytes = [
        9, 2, 25, 0, 1, 1, 0, 0x80, 50, // configuration, ch9.h:352-366
        9, 4, 0, 0, 1, 0xff, 0, 0, 0, // interface, ch9.h:397-410
        7, 5, 0x81, 2, 64, 0, 0, // endpoint, ch9.h:414-430
    ];
    let got: Vec<(usize, u8, usize)> = DescriptorIter::new(&bytes)
        .map(|r| {
            let d = r.unwrap();
            (d.offset, d.header.descriptor_type, d.bytes.len())
        })
        .collect();
    assert_eq!(got, [(0, 2, 9), (9, 4, 9), (18, 5, 7)]);
}

/// `config.c:706-710`: a one-byte tail is named rather than read as a header.
#[test]
fn iterator_refuses_truncated_header_tail() {
    let mut it = DescriptorIter::new(&[2, 0x24, 0xaa]);
    assert!(it.next().unwrap().is_ok());
    assert_eq!(
        it.next(),
        Some(Err(ParseError::Truncated {
            site: RefusalSite::Walk,
            available: 1,
            required: 2,
        }))
    );
    assert!(it.next().is_none(), "a refusal terminates the walk");
}

/// `config.c:713-718`: zero/one bLength cannot stall a loop, and an overrun cannot be sliced.
#[test]
fn iterator_refuses_each_malformed_length() {
    assert_eq!(
        DescriptorIter::new(&[0, 0x24]).next(),
        Some(Err(ParseError::LengthBelowMinimum {
            site: RefusalSite::Walk,
            length: 0,
            minimum: 2,
        }))
    );
    assert_eq!(
        DescriptorIter::new(&[1, 0x24]).next(),
        Some(Err(ParseError::LengthBelowMinimum {
            site: RefusalSite::Walk,
            length: 1,
            minimum: 2,
        }))
    );
    assert_eq!(
        DescriptorIter::new(&[4, 0x24, 0xaa]).next(),
        Some(Err(ParseError::LengthExceedsBuffer {
            site: RefusalSite::Walk,
            length: 4,
            available: 3,
        }))
    );
}

/// Linux's `find_next_descriptor`, `config.c:22-44`, skips class/vendor descriptors by bLength.
#[test]
fn find_next_reports_literal_bytes_and_descriptor_count() {
    let bytes = [3, 0x24, 0xaa, 4, 0xff, 1, 2, 9, 4, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(find_next_descriptor(&bytes, 5, 4), Ok((7, 2)));
    assert_eq!(find_next_descriptor(&bytes[7..], 5, 4), Ok((0, 0)));
    assert_eq!(find_next_descriptor(&bytes[..7], 5, 4), Ok((7, 2)));
}

/// The search cannot inherit Linux's unchecked `h->bLength` advance (`config.c:30-36`).
#[test]
fn find_next_refuses_every_truncated_or_stalling_descriptor() {
    assert_eq!(
        find_next_descriptor(&[3, 0x24, 0xaa, 7], 5, 4),
        Err(ParseError::Truncated {
            site: RefusalSite::Walk,
            available: 1,
            required: 2,
        })
    );
    assert_eq!(
        find_next_descriptor(&[3, 0x24, 0xaa, 0, 0xff], 5, 4),
        Err(ParseError::LengthBelowMinimum {
            site: RefusalSite::Walk,
            length: 0,
            minimum: 2,
        })
    );
    assert_eq!(
        find_next_descriptor(&[3, 0x24, 0xaa, 4, 0xff, 0xbb], 5, 4),
        Err(ParseError::LengthExceedsBuffer {
            site: RefusalSite::Walk,
            length: 4,
            available: 3,
        })
    );
}
