// SPDX-License-Identifier: GPL-2.0-only
//! HID global, local, and main item semantics, ported from Linux
//! `drivers/hid/hid-core.c` and `include/linux/hid.h`.
//!
//! Original Linux copyright holders: Andreas Gal; Vojtech Pavlik; Michael
//! Haboustak for Concept2, Inc.; Jiri Kosina; and the Linux HID authors.

use crate::item::{fetch_item, FetchError, Format, Item, ItemType};

pub const MAX_USAGES: usize = 12_288; // include/linux/hid.h:480
pub const MAX_REPORT_IDS: u32 = 256; // include/linux/hid.h:581
pub const GLOBAL_STACK_SIZE: usize = 4; // include/linux/hid.h:747

/// Global item tags (include/linux/hid.h:114-125), in Linux order.
pub const GLOBAL_TAGS: [(&str, u8); 12] = [
    ("USAGE_PAGE", 0),
    ("LOGICAL_MINIMUM", 1),
    ("LOGICAL_MAXIMUM", 2),
    ("PHYSICAL_MINIMUM", 3),
    ("PHYSICAL_MAXIMUM", 4),
    ("UNIT_EXPONENT", 5),
    ("UNIT", 6),
    ("REPORT_SIZE", 7),
    ("REPORT_ID", 8),
    ("REPORT_COUNT", 9),
    ("PUSH", 10),
    ("POP", 11),
];

/// Local item tags Linux names (include/linux/hid.h:131-140), in source order.
pub const LOCAL_TAGS: [(&str, u8); 10] = [
    ("USAGE", 0),
    ("USAGE_MINIMUM", 1),
    ("USAGE_MAXIMUM", 2),
    ("DESIGNATOR_INDEX", 3),
    ("DESIGNATOR_MINIMUM", 4),
    ("DESIGNATOR_MAXIMUM", 5),
    ("STRING_INDEX", 7),
    ("STRING_MINIMUM", 8),
    ("STRING_MAXIMUM", 9),
    ("DELIMITER", 10),
];

/// Main item tags Linux handles (include/linux/hid.h:79-83), in source order.
pub const MAIN_TAGS: [(&str, u8); 5] = [
    ("INPUT", 8),
    ("OUTPUT", 9),
    ("FEATURE", 11),
    ("BEGIN_COLLECTION", 10),
    ("END_COLLECTION", 12),
];

/// Report type names from Linux's three-value enum (include/uapi/linux/hid.h:49-54).
pub const REPORT_TYPE_NAMES: [&str; 3] = ["INPUT", "OUTPUT", "FEATURE"];
/// Collection type names Linux defines (include/linux/hid.h:105-108).
pub const COLLECTION_TYPES: [(&str, u8); 4] = [
    ("PHYSICAL", 0),
    ("APPLICATION", 1),
    ("LOGICAL", 2),
    ("NAMED_ARRAY", 4),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportType {
    Input,
    Output,
    Feature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalState {
    pub usage_page: u32,
    pub logical_minimum: i32,
    pub logical_maximum: i32,
    pub physical_minimum: i32,
    pub physical_maximum: i32,
    pub unit_exponent: i32,
    pub unit: u32,
    pub report_id: u32,
    pub report_size: u32,
    pub report_count: u32,
}

impl GlobalState {
    pub const fn new() -> Self {
        Self {
            usage_page: 0,
            logical_minimum: 0,
            logical_maximum: 0,
            physical_minimum: 0,
            physical_maximum: 0,
            unit_exponent: 0,
            unit: 0,
            report_id: 0,
            report_size: 0,
            report_count: 0,
        }
    }
}

impl Default for GlobalState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub value: u32,
    pub size: u8,
    pub collection_index: usize,
}

const EMPTY_USAGE: Usage = Usage {
    value: 0,
    size: 0,
    collection_index: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collection {
    pub parent: Option<usize>,
    pub collection_type: u8,
    pub usage: u32,
    pub level: usize,
}

const EMPTY_COLLECTION: Collection = Collection {
    parent: None,
    collection_type: 0,
    usage: 0,
    level: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MainItem {
    pub report_type: ReportType,
    pub flags: u32,
    pub report_id: u32,
    pub report_offset: u32,
    pub report_size: u32,
    pub report_count: u32,
    pub application: u32,
    pub physical: u32,
    pub logical: u32,
    pub usage_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    None,
    CollectionOpened { index: usize },
    CollectionClosed { index: usize },
    Field(MainItem),
}

/// Every parser refusal names the rejected value and bound where one exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Fetch {
        offset: usize,
        source: FetchError,
    },
    UnexpectedLongItem {
        offset: usize,
        tag: u8,
        size: u8,
    },
    UnknownGlobalTag {
        tag: u8,
    },
    GlobalStackOverflow {
        depth: usize,
        maximum: usize,
    },
    GlobalStackUnderflow,
    InvalidReportSize {
        value: u32,
        maximum: u32,
    },
    InvalidReportCount {
        value: u32,
        maximum: u32,
    },
    InvalidReportId {
        value: u32,
        minimum: u32,
        maximum_exclusive: u32,
    },
    InvalidLogicalRange {
        minimum: i32,
        maximum: i32,
    },
    UsageCapacityExceeded {
        requested: usize,
        maximum: usize,
    },
    UsageRangeDescending {
        minimum: u32,
        maximum: u32,
    },
    NestedDelimiter {
        depth: u32,
    },
    BogusCloseDelimiter {
        depth: u32,
    },
    CollectionStackOverflow {
        depth: usize,
        maximum: usize,
    },
    CollectionStackUnderflow,
    CollectionCapacityExceeded {
        count: usize,
        maximum: usize,
    },
    ReportBitsOverflow {
        current: u32,
        addend: u32,
    },
    UnbalancedCollections {
        depth: usize,
    },
    UnbalancedDelimiter {
        depth: u32,
    },
}

/// Fixed-capacity, allocation-free parser. Capacities are caller-selected and
/// every excess is refused by name rather than silently clamped.
pub struct Parser<const U: usize, const C: usize> {
    global: GlobalState,
    globals: [GlobalState; GLOBAL_STACK_SIZE],
    global_depth: usize,
    usages: [Usage; U],
    usage_count: usize,
    usage_minimum: u32,
    delimiter_depth: u32,
    delimiter_branch: u32,
    collections: [Collection; C],
    collection_count: usize,
    collection_stack: [usize; C],
    collection_depth: usize,
    report_bits: [[u32; 3]; 256],
}

impl<const U: usize, const C: usize> Parser<U, C> {
    /// Construct an empty parser (hid-core.c:1312-1330 zero-initialisation).
    pub const fn new() -> Self {
        Self {
            global: GlobalState::new(),
            globals: [GlobalState::new(); GLOBAL_STACK_SIZE],
            global_depth: 0,
            usages: [EMPTY_USAGE; U],
            usage_count: 0,
            usage_minimum: 0,
            delimiter_depth: 0,
            delimiter_branch: 0,
            collections: [EMPTY_COLLECTION; C],
            collection_count: 0,
            collection_stack: [0; C],
            collection_depth: 0,
            report_bits: [[0; 3]; 256],
        }
    }

    /// Current global environment (hid-core.c:401-500).
    pub const fn global(&self) -> GlobalState {
        self.global
    }
    /// Current local usages, valid until the next main item (hid-core.c:672).
    pub fn usages(&self) -> &[Usage] {
        &self.usages[..self.usage_count]
    }
    /// Collections opened so far (hid-core.c:198-204).
    pub fn collections(&self) -> &[Collection] {
        &self.collections[..self.collection_count]
    }
    /// Current collection depth.
    pub const fn collection_depth(&self) -> usize {
        self.collection_depth
    }
    /// Accumulated report bits for one report ID and type (hid-core.c:319-320).
    pub const fn report_bits(&self, id: u8, ty: ReportType) -> u32 {
        self.report_bits[id as usize][report_index(ty)]
    }

    /// Find the innermost open collection of a type (hid-core.c:231-241).
    pub fn lookup_collection(&self, collection_type: u8) -> u32 {
        let mut depth = self.collection_depth;
        while depth != 0 {
            depth -= 1;
            let index = self.collection_stack[depth];
            if self.collections[index].collection_type == collection_type {
                return self.collections[index].usage;
            }
        }
        0
    }

    /// Apply one already-decoded short item.
    pub fn apply_item(&mut self, item: &Item<'_>) -> Result<Event, ParseError> {
        if item.format != Format::Short {
            return Err(ParseError::UnexpectedLongItem {
                offset: 0,
                tag: item.tag,
                size: item.size,
            });
        }
        match item.item_type {
            ItemType::Global => {
                self.apply_global(item)?;
                Ok(Event::None)
            }
            ItemType::Local => {
                self.apply_local(item)?;
                Ok(Event::None)
            }
            ItemType::Main => self.apply_main(item),
            ItemType::Reserved => Ok(Event::None), // hid-core.c:681-684
        }
    }

    /// Parse a complete descriptor and emit each main-item event to a callback.
    /// Long items are decoded by `fetch_item` but refused as Linux does in
    /// `hid_open_report` (hid-core.c:1336-1338).
    pub fn parse_descriptor<F>(&mut self, bytes: &[u8], mut emit: F) -> Result<(), ParseError>
    where
        F: FnMut(Event, &[Usage]),
    {
        if bytes.is_empty() {
            return Err(ParseError::Fetch {
                offset: 0,
                source: FetchError::EmptyInput,
            });
        }
        let mut offset = 0;
        while offset < bytes.len() {
            let (item, consumed) = fetch_item(&bytes[offset..])
                .map_err(|source| ParseError::Fetch { offset, source })?;
            if item.format == Format::Long {
                return Err(ParseError::UnexpectedLongItem {
                    offset,
                    tag: item.tag,
                    size: item.size,
                });
            }
            let event = self.apply_item(&item)?;
            if event != Event::None {
                let usage_count = match event {
                    Event::Field(field) => field.usage_count,
                    _ => 0,
                };
                emit(event, &self.usages[..usage_count]);
            }
            offset += consumed;
        }
        if self.collection_depth != 0 {
            return Err(ParseError::UnbalancedCollections {
                depth: self.collection_depth,
            });
        }
        if self.delimiter_depth != 0 {
            return Err(ParseError::UnbalancedDelimiter {
                depth: self.delimiter_depth,
            });
        }
        Ok(())
    }

    fn apply_global(&mut self, item: &Item<'_>) -> Result<(), ParseError> {
        let u = item.unsigned_data();
        let s = item.signed_data();
        match item.tag {
            10 => {
                if self.global_depth == GLOBAL_STACK_SIZE {
                    return Err(ParseError::GlobalStackOverflow {
                        depth: self.global_depth,
                        maximum: GLOBAL_STACK_SIZE,
                    });
                }
                self.globals[self.global_depth] = self.global;
                self.global_depth += 1; // hid-core.c:405-414
            }
            11 => {
                if self.global_depth == 0 {
                    return Err(ParseError::GlobalStackUnderflow);
                }
                self.global_depth -= 1;
                self.global = self.globals[self.global_depth]; // hid-core.c:416-425
            }
            0 => self.global.usage_page = u, // hid-core.c:427-429
            1 => self.global.logical_minimum = s,
            2 => {
                self.global.logical_maximum = if self.global.logical_minimum < 0 {
                    s
                } else {
                    u as i32
                }
            }
            3 => self.global.physical_minimum = s,
            4 => {
                self.global.physical_maximum = if self.global.physical_minimum < 0 {
                    s
                } else {
                    u as i32
                }
            }
            5 => {
                self.global.unit_exponent = if s & !0xf == 0 {
                    sign_extend_nibble(s)
                } else {
                    s
                }
            } // hid-core.c:453-463
            6 => self.global.unit = u,
            7 => {
                if u > 256 {
                    return Err(ParseError::InvalidReportSize {
                        value: u,
                        maximum: 256,
                    });
                }
                self.global.report_size = u; // hid-core.c:469-476
            }
            8 => {
                if u == 0 || u >= MAX_REPORT_IDS {
                    return Err(ParseError::InvalidReportId {
                        value: u,
                        minimum: 1,
                        maximum_exclusive: MAX_REPORT_IDS,
                    });
                }
                self.global.report_id = u; // hid-core.c:487-495
            }
            9 => {
                if u > MAX_USAGES as u32 {
                    return Err(ParseError::InvalidReportCount {
                        value: u,
                        maximum: MAX_USAGES as u32,
                    });
                }
                self.global.report_count = u; // hid-core.c:478-485
            }
            tag => return Err(ParseError::UnknownGlobalTag { tag }), // hid-core.c:497-499
        }
        Ok(())
    }

    fn apply_local(&mut self, item: &Item<'_>) -> Result<(), ParseError> {
        let data = item.unsigned_data();
        match item.tag {
            10 => {
                if data != 0 {
                    if self.delimiter_depth != 0 {
                        return Err(ParseError::NestedDelimiter {
                            depth: self.delimiter_depth,
                        });
                    }
                    self.delimiter_depth += 1;
                    self.delimiter_branch += 1; // hid-core.c:518-530
                } else {
                    if self.delimiter_depth < 1 {
                        return Err(ParseError::BogusCloseDelimiter {
                            depth: self.delimiter_depth,
                        });
                    }
                    self.delimiter_depth -= 1; // hid-core.c:531-536
                }
            }
            0 if self.delimiter_branch <= 1 => self.add_usage(data, item.size)?,
            1 if self.delimiter_branch <= 1 => self.usage_minimum = data,
            2 if self.delimiter_branch <= 1 => {
                if data < self.usage_minimum {
                    return Err(ParseError::UsageRangeDescending {
                        minimum: self.usage_minimum,
                        maximum: data,
                    });
                }
                let requested = data as u64 - self.usage_minimum as u64 + 1;
                let capacity = core::cmp::min(U, MAX_USAGES);
                if requested > usize::MAX as u64
                    || requested as usize > capacity.saturating_sub(self.usage_count)
                {
                    return Err(ParseError::UsageCapacityExceeded {
                        requested: self.usage_count.saturating_add(requested as usize),
                        maximum: capacity,
                    });
                }
                let mut usage = self.usage_minimum;
                loop {
                    self.add_usage(usage, item.size)?;
                    if usage == data {
                        break;
                    }
                    usage += 1;
                }
            }
            _ => {} // alternatives and unknown locals are ignored (hid-core.c:542-594)
        }
        Ok(())
    }

    fn add_usage(&mut self, usage: u32, size: u8) -> Result<(), ParseError> {
        if self.usage_count >= U || self.usage_count >= MAX_USAGES {
            return Err(ParseError::UsageCapacityExceeded {
                requested: self.usage_count + 1,
                maximum: core::cmp::min(U, MAX_USAGES),
            });
        }
        let value = if size <= 2 {
            complete_usage(self.global.usage_page, usage)
        } else {
            usage
        };
        let collection_index = if self.collection_depth == 0 {
            0
        } else {
            self.collection_stack[self.collection_depth - 1]
        };
        self.usages[self.usage_count] = Usage {
            value,
            size,
            collection_index,
        };
        self.usage_count += 1; // hid-core.c:260-280
        Ok(())
    }

    fn apply_main(&mut self, item: &Item<'_>) -> Result<Event, ParseError> {
        self.concatenate_last_usage_page(); // hid-core.c:643
        let data = item.unsigned_data();
        let result = match item.tag {
            10 => self.open_collection((data & 0xff) as u8),
            12 => self.close_collection(),
            8 => self.add_field(ReportType::Input, data),
            9 => self.add_field(ReportType::Output, data),
            11 => self.add_field(ReportType::Feature, data),
            _ => Ok(Event::None), // reserved/unknown main tags warn and continue (hid-core.c:663-669)
        };
        self.reset_local(); // hid-core.c:672, including failures
        result
    }

    fn open_collection(&mut self, collection_type: u8) -> Result<Event, ParseError> {
        if self.collection_depth >= C {
            return Err(ParseError::CollectionStackOverflow {
                depth: self.collection_depth,
                maximum: C,
            });
        }
        if self.collection_count >= C {
            return Err(ParseError::CollectionCapacityExceeded {
                count: self.collection_count,
                maximum: C,
            });
        }
        let index = self.collection_count;
        let usage = if self.usage_count == 0 {
            0
        } else {
            self.usages[0].value
        }; // hid-core.c:157
        let parent = if self.collection_depth == 0 {
            None
        } else {
            Some(self.collection_stack[self.collection_depth - 1])
        };
        self.collection_stack[self.collection_depth] = index;
        self.collection_depth += 1;
        self.collection_count += 1;
        self.collections[index] = Collection {
            parent,
            collection_type,
            usage,
            level: self.collection_depth - 1,
        }; // hid-core.c:195-204
        Ok(Event::CollectionOpened { index })
    }

    fn close_collection(&mut self) -> Result<Event, ParseError> {
        if self.collection_depth == 0 {
            return Err(ParseError::CollectionStackUnderflow);
        }
        self.collection_depth -= 1; // hid-core.c:216-223
        Ok(Event::CollectionClosed {
            index: self.collection_stack[self.collection_depth],
        })
    }

    fn add_field(&mut self, report_type: ReportType, flags: u32) -> Result<Event, ParseError> {
        let invalid_range = if self.global.logical_minimum < 0 {
            self.global.logical_maximum < self.global.logical_minimum
        } else {
            (self.global.logical_maximum as u32) < self.global.logical_minimum as u32
        };
        if invalid_range {
            return Err(ParseError::InvalidLogicalRange {
                minimum: self.global.logical_minimum,
                maximum: self.global.logical_maximum,
            }); // drivers/hid/hid-core.c:306-317
        }
        let id = self.global.report_id as usize;
        let ty = report_index(report_type);
        let offset = self.report_bits[id][ty];
        let addend = self
            .global
            .report_size
            .checked_mul(self.global.report_count)
            .ok_or(ParseError::ReportBitsOverflow {
                current: offset,
                addend: u32::MAX,
            })?;
        self.report_bits[id][ty] =
            offset
                .checked_add(addend)
                .ok_or(ParseError::ReportBitsOverflow {
                    current: offset,
                    addend,
                })?; // hid-core.c:319-320
        Ok(Event::Field(MainItem {
            report_type,
            flags,
            report_id: self.global.report_id,
            report_offset: offset,
            report_size: self.global.report_size,
            report_count: self.global.report_count,
            application: self.lookup_collection(1), // include/linux/hid.h:106
            physical: self.lookup_collection(0),    // include/linux/hid.h:105
            logical: self.lookup_collection(2),     // include/linux/hid.h:107
            usage_count: self.usage_count,
        }))
    }

    fn concatenate_last_usage_page(&mut self) {
        let mut i = self.usage_count;
        while i != 0 {
            i -= 1;
            if self.usages[i].size > 2 {
                continue;
            }
            if self.usages[i].value >> 16 == self.global.usage_page {
                break;
            }
            self.usages[i].value = complete_usage(self.global.usage_page, self.usages[i].value);
        } // hid-core.c:606-631
    }

    fn reset_local(&mut self) {
        self.usage_count = 0;
        self.usage_minimum = 0;
        self.delimiter_depth = 0;
        self.delimiter_branch = 0;
    }
}

impl<const U: usize, const C: usize> Default for Parser<U, C> {
    fn default() -> Self {
        Self::new()
    }
}

const fn report_index(ty: ReportType) -> usize {
    match ty {
        ReportType::Input => 0,
        ReportType::Output => 1,
        ReportType::Feature => 2,
    }
}

const fn complete_usage(page: u32, usage: u32) -> u32 {
    (usage & 0xffff) | ((page & 0xffff) << 16) // hid-core.c:249-254
}

const fn sign_extend_nibble(value: i32) -> i32 {
    if value & 8 != 0 {
        value | !0xf
    } else {
        value & 0xf
    }
}
