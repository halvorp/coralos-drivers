// SPDX-License-Identifier: GPL-2.0-only
//! Parser vectors from Linux `drivers/hid/hid-core.c`, `include/linux/hid.h`,
//! and `include/uapi/linux/hid.h`; original holders are named by the crate.

use hid_report_core::item::{fetch_item, FetchError};
use hid_report_core::parser::{
    Event, ParseError, Parser, ReportType, COLLECTION_TYPES, GLOBAL_STACK_SIZE, GLOBAL_TAGS,
    LOCAL_TAGS, MAIN_TAGS, MAX_REPORT_IDS, MAX_USAGES, REPORT_TYPE_NAMES,
};

fn item(bytes: &[u8]) -> hid_report_core::item::Item<'_> {
    fetch_item(bytes).unwrap().0
}

#[test]
fn linux_semantic_name_lists_and_counts_are_frozen() {
    // include/linux/hid.h:114-125. Literal expected list, never production-derived.
    assert_eq!(GLOBAL_TAGS.len(), 12);
    assert_eq!(
        GLOBAL_TAGS,
        [
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
        ]
    );
    // include/linux/hid.h:131-140.
    assert_eq!(LOCAL_TAGS.len(), 10);
    assert_eq!(
        LOCAL_TAGS,
        [
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
        ]
    );
    // include/linux/hid.h:79-83.
    assert_eq!(MAIN_TAGS.len(), 5);
    assert_eq!(
        MAIN_TAGS,
        [
            ("INPUT", 8),
            ("OUTPUT", 9),
            ("FEATURE", 11),
            ("BEGIN_COLLECTION", 10),
            ("END_COLLECTION", 12),
        ]
    );
    // include/uapi/linux/hid.h:49-54.
    assert_eq!(REPORT_TYPE_NAMES.len(), 3);
    assert_eq!(REPORT_TYPE_NAMES, ["INPUT", "OUTPUT", "FEATURE"]);
    // include/linux/hid.h:105-108.
    assert_eq!(COLLECTION_TYPES.len(), 4);
    assert_eq!(
        COLLECTION_TYPES,
        [
            ("PHYSICAL", 0),
            ("APPLICATION", 1),
            ("LOGICAL", 2),
            ("NAMED_ARRAY", 4),
        ]
    );
    assert_eq!(GLOBAL_STACK_SIZE, 4); // include/linux/hid.h:747
    assert_eq!(MAX_USAGES, 12_288); // include/linux/hid.h:480
    assert_eq!(MAX_REPORT_IDS, 256); // include/linux/hid.h:581
}

#[test]
fn globals_use_linux_signedness_nibble_quirk_and_push_pop() {
    let mut p = Parser::<8, 4>::new();
    p.apply_item(&item(&[0x05, 0x01])).unwrap(); // Usage Page 1
    p.apply_item(&item(&[0x15, 0xff])).unwrap(); // Logical Minimum -1
    p.apply_item(&item(&[0x25, 0x7f])).unwrap(); // signed max because min < 0
    p.apply_item(&item(&[0x35, 0x00])).unwrap();
    p.apply_item(&item(&[0x46, 0xff, 0xff])).unwrap(); // unsigned max because min >= 0
    p.apply_item(&item(&[0x55, 0x0f])).unwrap(); // Linux nibble compatibility => -1
    p.apply_item(&item(&[0x66, 0x11, 0xe1])).unwrap();
    p.apply_item(&item(&[0xa4])).unwrap(); // PUSH
    p.apply_item(&item(&[0x75, 8])).unwrap();
    p.apply_item(&item(&[0xb4])).unwrap(); // POP restores report size 0
    let g = p.global();
    assert_eq!(g.usage_page, 1);
    assert_eq!((g.logical_minimum, g.logical_maximum), (-1, 127)); // hid-core.c:431-440
    assert_eq!((g.physical_minimum, g.physical_maximum), (0, 65_535)); // :442-451
    assert_eq!(g.unit_exponent, -1); // :453-463
    assert_eq!(g.unit, 0xe111);
    assert_eq!(g.report_size, 0);
}

#[test]
fn all_global_bounds_name_the_refused_value() {
    let mut p = Parser::<1, 1>::new();
    assert_eq!(
        p.apply_item(&item(&[0x76, 0x01, 0x01])),
        Err(ParseError::InvalidReportSize {
            value: 257,
            maximum: 256
        })
    );
    assert_eq!(
        p.apply_item(&item(&[0x97, 0x01, 0x30, 0, 0])),
        Err(ParseError::InvalidReportCount {
            value: 12_289,
            maximum: 12_288
        })
    );
    assert_eq!(
        p.apply_item(&item(&[0x85, 0])),
        Err(ParseError::InvalidReportId {
            value: 0,
            minimum: 1,
            maximum_exclusive: 256
        })
    );
    assert_eq!(
        p.apply_item(&item(&[0x86, 0, 1])),
        Err(ParseError::InvalidReportId {
            value: 256,
            minimum: 1,
            maximum_exclusive: 256
        })
    );
    assert_eq!(
        p.apply_item(&item(&[0xc5, 0])),
        Err(ParseError::UnknownGlobalTag { tag: 12 })
    );

    let mut stack = Parser::<1, 1>::new();
    for _ in 0..4 {
        stack.apply_item(&item(&[0xa4])).unwrap();
    }
    assert_eq!(
        stack.apply_item(&item(&[0xa4])),
        Err(ParseError::GlobalStackOverflow {
            depth: 4,
            maximum: 4
        })
    );
    let mut empty = Parser::<1, 1>::new();
    assert_eq!(
        empty.apply_item(&item(&[0xb4])),
        Err(ParseError::GlobalStackUnderflow)
    );
}

#[test]
fn usage_page_range_and_main_reset_match_linux() {
    let mut p = Parser::<4, 2>::new();
    p.apply_item(&item(&[0x05, 0x01])).unwrap(); // page 1
    p.apply_item(&item(&[0x09, 0x30])).unwrap(); // usage X => 0x00010030
    p.apply_item(&item(&[0x19, 0x31])).unwrap();
    p.apply_item(&item(&[0x29, 0x32])).unwrap();
    assert_eq!(
        p.usages().iter().map(|u| u.value).collect::<Vec<_>>(),
        [0x0001_0030, 0x0001_0031, 0x0001_0032]
    );
    p.apply_item(&item(&[0x05, 0x02])).unwrap(); // page changed before main
    p.apply_item(&item(&[0x81, 0x02])).unwrap();
    // hid-core.c:606-631 walks backwards and re-concatenates all short usages.
    assert!(
        p.usages().is_empty(),
        "hid-core.c:672 resets locals after every main item"
    );

    let mut extended = Parser::<2, 1>::new();
    extended
        .apply_item(&item(&[0x0b, 0x78, 0x56, 0x34, 0x12]))
        .unwrap();
    assert_eq!(
        extended.usages()[0].value,
        0x1234_5678,
        "four-byte usages already carry a page"
    );
}

#[test]
fn usage_and_delimiter_refusals_are_bounded_and_named() {
    let mut p = Parser::<2, 1>::new();
    p.apply_item(&item(&[0x19, 3])).unwrap();
    assert_eq!(
        p.apply_item(&item(&[0x29, 2])),
        Err(ParseError::UsageRangeDescending {
            minimum: 3,
            maximum: 2
        })
    );
    let mut p = Parser::<2, 1>::new();
    p.apply_item(&item(&[0x19, 1])).unwrap();
    assert_eq!(
        p.apply_item(&item(&[0x29, 3])),
        Err(ParseError::UsageCapacityExceeded {
            requested: 3,
            maximum: 2
        })
    );
    let mut p = Parser::<1, 1>::new();
    p.apply_item(&item(&[0x09, 1])).unwrap();
    assert_eq!(
        p.apply_item(&item(&[0x09, 2])),
        Err(ParseError::UsageCapacityExceeded {
            requested: 2,
            maximum: 1
        })
    );

    let mut d = Parser::<1, 1>::new();
    assert_eq!(
        d.apply_item(&item(&[0xa9, 0])),
        Err(ParseError::BogusCloseDelimiter { depth: 0 })
    );
    d.apply_item(&item(&[0xa9, 1])).unwrap();
    assert_eq!(
        d.apply_item(&item(&[0xa9, 1])),
        Err(ParseError::NestedDelimiter { depth: 1 })
    );
}

#[test]
fn collection_stack_records_usage_level_parent_and_lookup() {
    let mut p = Parser::<4, 4>::new();
    p.apply_item(&item(&[0x05, 1])).unwrap();
    p.apply_item(&item(&[0x09, 2])).unwrap();
    assert_eq!(
        p.apply_item(&item(&[0xa1, 1])),
        Ok(Event::CollectionOpened { index: 0 })
    );
    p.apply_item(&item(&[0x09, 0x30])).unwrap();
    assert_eq!(
        p.apply_item(&item(&[0xa1, 0])),
        Ok(Event::CollectionOpened { index: 1 })
    );
    assert_eq!(p.collection_depth(), 2);
    assert_eq!(p.lookup_collection(1), 0x0001_0002);
    assert_eq!(p.lookup_collection(0), 0x0001_0030);
    assert_eq!(p.collections()[0].level, 0);
    assert_eq!(p.collections()[0].parent, None);
    assert_eq!(p.collections()[1].level, 1);
    assert_eq!(p.collections()[1].parent, Some(0));
    assert_eq!(
        p.apply_item(&item(&[0xc0])),
        Ok(Event::CollectionClosed { index: 1 })
    );
    assert_eq!(
        p.apply_item(&item(&[0xc0])),
        Ok(Event::CollectionClosed { index: 0 })
    );
    assert_eq!(
        p.apply_item(&item(&[0xc0])),
        Err(ParseError::CollectionStackUnderflow)
    );
}

#[test]
fn collection_capacity_refusal_names_count_and_maximum() {
    let mut p = Parser::<1, 1>::new();
    p.apply_item(&item(&[0xa1, 1])).unwrap();
    // A second nested collection first exhausts the fixed stack, explicitly.
    assert_eq!(
        p.apply_item(&item(&[0xa1, 1])),
        Err(ParseError::CollectionStackOverflow {
            depth: 1,
            maximum: 1
        })
    );
    p.apply_item(&item(&[0xc0])).unwrap();
    // With stack space restored, the collection record capacity is the refusal.
    assert_eq!(
        p.apply_item(&item(&[0xa1, 1])),
        Err(ParseError::CollectionCapacityExceeded {
            count: 1,
            maximum: 1
        })
    );
}

#[test]
fn report_size_times_count_accumulates_per_id_and_type() {
    let descriptor = [
        0x05, 0x01, // Usage Page (Generic Desktop)
        0x09, 0x02, // Usage (Mouse)
        0xa1, 0x01, // Collection (Application)
        0x85, 0x01, // Report ID 1
        0x75, 0x08, // Report Size 8
        0x95, 0x03, // Report Count 3
        0x09, 0x30, // Usage X
        0x81, 0x02, // Input
        0x09, 0x31, // Usage Y
        0x81, 0x06, // Input
        0x91, 0x02, // Output (same size/count, separate report type)
        0xc0, // End Collection
    ];
    let mut p = Parser::<8, 4>::new();
    let mut fields = Vec::new();
    p.parse_descriptor(&descriptor, |event, usages| {
        if let Event::Field(field) = event {
            fields.push((field, usages.iter().map(|u| u.value).collect::<Vec<_>>()));
        }
    })
    .unwrap();
    assert_eq!(fields.len(), 3);
    assert_eq!(
        (
            fields[0].0.report_offset,
            fields[0].0.report_size,
            fields[0].0.report_count
        ),
        (0, 8, 3)
    );
    assert_eq!(fields[0].0.application, 0x0001_0002);
    assert_eq!(fields[0].1, [0x0001_0030]);
    assert_eq!(fields[1].0.report_offset, 24);
    assert_eq!(fields[1].1, [0x0001_0031]);
    assert_eq!(fields[2].0.report_type, ReportType::Output);
    assert_eq!(fields[2].0.report_offset, 0);
    assert_eq!(
        fields[2].0.usage_count, 0,
        "locals reset after the preceding Input"
    );
    assert_eq!(p.report_bits(1, ReportType::Input), 48); // hid-core.c:319-320
    assert_eq!(p.report_bits(1, ReportType::Output), 24);
    assert_eq!(p.report_bits(1, ReportType::Feature), 0);
}

#[test]
fn a_field_refuses_linux_invalid_logical_ranges_by_name() {
    let mut signed = Parser::<1, 1>::new();
    signed.apply_item(&item(&[0x15, 0xff])).unwrap(); // Logical Minimum -1
    signed.apply_item(&item(&[0x25, 0xfe])).unwrap(); // Logical Maximum -2
    assert_eq!(
        signed.apply_item(&item(&[0x81, 0])),
        Err(ParseError::InvalidLogicalRange {
            minimum: -1,
            maximum: -2
        })
    ); // drivers/hid/hid-core.c:306-317

    let mut unsigned = Parser::<1, 1>::new();
    unsigned.apply_item(&item(&[0x15, 2])).unwrap();
    unsigned.apply_item(&item(&[0x25, 1])).unwrap();
    assert_eq!(
        unsigned.apply_item(&item(&[0x81, 0])),
        Err(ParseError::InvalidLogicalRange {
            minimum: 2,
            maximum: 1
        })
    );
}

#[test]
fn complete_descriptor_reports_fetch_long_and_balance_failures() {
    let mut p = Parser::<1, 1>::new();
    assert_eq!(
        p.parse_descriptor(&[], |_, _| {}),
        Err(ParseError::Fetch {
            offset: 0,
            source: FetchError::EmptyInput
        })
    );
    let mut p = Parser::<1, 1>::new();
    assert_eq!(
        p.parse_descriptor(&[0x05], |_, _| {}),
        Err(ParseError::Fetch {
            offset: 0,
            source: FetchError::ShortPayloadTruncated {
                declared: 1,
                available: 0
            }
        })
    );
    let mut p = Parser::<1, 1>::new();
    assert_eq!(
        p.parse_descriptor(&[0xfe, 1, 0x42, 0xaa], |_, _| {}),
        Err(ParseError::UnexpectedLongItem {
            offset: 0,
            tag: 0x42,
            size: 1
        })
    );
    let mut p = Parser::<1, 1>::new();
    assert_eq!(
        p.parse_descriptor(&[0xa1, 1], |_, _| {}),
        Err(ParseError::UnbalancedCollections { depth: 1 })
    );
    let mut p = Parser::<1, 1>::new();
    assert_eq!(
        p.parse_descriptor(&[0xa9, 1], |_, _| {}),
        Err(ParseError::UnbalancedDelimiter { depth: 1 })
    );
}
