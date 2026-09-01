// SPDX-License-Identifier: GPL-2.0-only
//! HID item-prefix decoding, ported from Linux `drivers/hid/hid-core.c` and
//! `include/linux/hid.h`.
//!
//! Original Linux copyright holders: Andreas Gal; Vojtech Pavlik; Michael
//! Haboustak for Concept2, Inc.; Jiri Kosina; and the Linux HID authors.

/// Item format names from `HID_ITEM_FORMAT_*` (include/linux/hid.h:57-58).
pub const FORMAT_NAMES: [&str; 2] = ["SHORT", "LONG"];
/// Item type names in dispatch order (include/linux/hid.h:70-73).
pub const TYPE_NAMES: [&str; 4] = ["MAIN", "GLOBAL", "LOCAL", "RESERVED"];
/// The short-prefix tag which introduces a long item (include/linux/hid.h:64).
pub const ITEM_TAG_LONG: u8 = 15;

/// Item wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Short,
    Long,
}

/// Item type encoded in prefix bits 3:2 (hid-core.c:787).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Main,
    Global,
    Local,
    Reserved,
}

/// A decoded item. `data` contains zero, one, two, or four little-endian short
/// item bytes; long-item payload remains borrowed and allocation-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item<'a> {
    pub format: Format,
    pub item_type: ItemType,
    pub tag: u8,
    pub size: u8,
    pub data: u32,
    pub long_data: &'a [u8],
}

impl Item<'_> {
    /// Unsigned short-item data (hid-core.c:377-385).
    pub const fn unsigned_data(&self) -> u32 {
        self.data
    }

    /// Signed short-item data, sign-extended according to item size
    /// (hid-core.c:387-395).
    pub const fn signed_data(&self) -> i32 {
        match self.size {
            1 => self.data as u8 as i8 as i32,
            2 => self.data as u16 as i16 as i32,
            4 => self.data as i32,
            _ => 0,
        }
    }
}

/// A bounded, named item-decoding refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchError {
    EmptyInput,
    LongHeaderTruncated { available: usize, required: usize },
    LongPayloadTruncated { declared: u8, available: usize },
    ShortPayloadTruncated { declared: u8, available: usize },
}

const fn item_type(bits: u8) -> ItemType {
    match bits {
        0 => ItemType::Main,
        1 => ItemType::Global,
        2 => ItemType::Local,
        _ => ItemType::Reserved,
    }
}

/// Decode one descriptor item and return it with the number of consumed bytes.
///
/// This is Linux `fetch_item` (drivers/hid/hid-core.c:778-831), but every NULL
/// return is split into a named refusal carrying the declared and available
/// bounds.
pub fn fetch_item(bytes: &[u8]) -> Result<(Item<'_>, usize), FetchError> {
    let prefix = *bytes.first().ok_or(FetchError::EmptyInput)?;
    let ty = item_type((prefix >> 2) & 3); // hid-core.c:787
    let short_tag = (prefix >> 4) & 15; // hid-core.c:788

    if short_tag == ITEM_TAG_LONG {
        // hid-core.c:790
        if bytes.len() < 3 {
            return Err(FetchError::LongHeaderTruncated {
                available: bytes.len().saturating_sub(1),
                required: 2,
            });
        }
        let size = bytes[1]; // hid-core.c:797
        let tag = bytes[2]; // hid-core.c:798
        let available = bytes.len() - 3;
        if available < size as usize {
            return Err(FetchError::LongPayloadTruncated {
                declared: size,
                available,
            });
        }
        let end = 3 + size as usize;
        return Ok((
            Item {
                format: Format::Long,
                item_type: ty,
                tag,
                size,
                data: 0,
                long_data: &bytes[3..end],
            },
            end,
        ));
    }

    // `BIT(b & 3) >> 1`: size codes 0,1,2,3 become 0,1,2,4 (hid-core.c:809).
    let size = 1u8 << (prefix & 3) >> 1;
    let available = bytes.len() - 1;
    if available < size as usize {
        return Err(FetchError::ShortPayloadTruncated {
            declared: size,
            available,
        });
    }
    let data = match size {
        0 => 0,
        1 => bytes[1] as u32,
        2 => u16::from_le_bytes([bytes[1], bytes[2]]) as u32, // hid-core.c:822-824
        4 => u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]), // hid-core.c:826-827
        _ => 0,
    };
    let consumed = 1 + size as usize;
    Ok((
        Item {
            format: Format::Short,
            item_type: ty,
            tag: short_tag,
            size,
            data,
            long_data: &[],
        },
        consumed,
    ))
}
