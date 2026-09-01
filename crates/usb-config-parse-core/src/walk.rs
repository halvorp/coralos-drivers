// SPDX-License-Identifier: GPL-2.0-only
//! Bounded configuration/interface/endpoint descriptor walking.
//!
//! Ported from Linux `drivers/usb/core/config.c:22-44`, `config.c:520-535`,
//! `config.c:600-610`, and `config.c:699-718`.
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! core and Chapter 9 header authors and contributors.

use crate::descriptor::{parse_header, DescriptorHeader, ParseError, RefusalSite};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor<'a> {
    pub offset: usize,
    pub header: DescriptorHeader,
    pub bytes: &'a [u8],
}

/// Allocation-free iterator over a descriptor stream. Every advance validates the two-byte header,
/// `bLength >= 2`, and `bLength <= remaining` before slicing (`config.c:699-718`).
#[derive(Debug, Clone)]
pub struct DescriptorIter<'a> {
    remaining: &'a [u8],
    offset: usize,
    stopped: bool,
}

impl<'a> DescriptorIter<'a> {
    /// Create a bounded walk over `bytes`.
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            remaining: bytes,
            offset: 0,
            stopped: false,
        }
    }
}

impl<'a> Iterator for DescriptorIter<'a> {
    type Item = Result<Descriptor<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.stopped || self.remaining.is_empty() {
            return None;
        }
        let header = match parse_header(self.remaining) {
            Ok(header) => header,
            Err(error) => {
                self.stopped = true;
                return Some(Err(walk_error(error)));
            }
        };
        let length = header.length as usize;
        let (descriptor, rest) = self.remaining.split_at(length);
        let offset = self.offset;
        self.offset += length;
        self.remaining = rest;
        Some(Ok(Descriptor {
            offset,
            header,
            bytes: descriptor,
        }))
    }
}

/// Return bytes and descriptors skipped before either requested descriptor type. This is Linux's
/// `find_next_descriptor` (`config.c:22-44`) with the missing hostile-input checks made explicit.
pub fn find_next_descriptor(
    bytes: &[u8],
    first_type: u8,
    second_type: u8,
) -> Result<(usize, usize), ParseError> {
    let mut skipped_descriptors = 0;
    for descriptor in DescriptorIter::new(bytes) {
        let descriptor = descriptor?;
        if descriptor.header.descriptor_type == first_type
            || descriptor.header.descriptor_type == second_type
        {
            return Ok((descriptor.offset, skipped_descriptors));
        }
        skipped_descriptors += 1;
    }
    Ok((bytes.len(), skipped_descriptors))
}

fn walk_error(error: ParseError) -> ParseError {
    match error {
        ParseError::Truncated {
            available,
            required,
            ..
        } => ParseError::Truncated {
            site: RefusalSite::Walk,
            available,
            required,
        },
        ParseError::LengthBelowMinimum {
            length, minimum, ..
        } => ParseError::LengthBelowMinimum {
            site: RefusalSite::Walk,
            length,
            minimum,
        },
        ParseError::LengthExceedsBuffer {
            length, available, ..
        } => ParseError::LengthExceedsBuffer {
            site: RefusalSite::Walk,
            length,
            available,
        },
        other => other,
    }
}
