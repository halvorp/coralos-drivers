// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for default lookup and precedence. Expected values carry Linux FILE and LINE.
//!
//! Copyright 2011, 2012 Wolfson Microelectronics plc.
//! Original authors: Dimitris Papastamos and Mark Brown.

use regmap_regcache_core::defaults::{
    lookup_default, lookup_reg_default, sort_defaults, DefaultError, DefaultSource, DefaultValue,
    RegDefault, DEFAULT_SOURCE_NAMES,
};

/// regcache.c:25-43,724-747. Sort and binary search use ascending register addresses.
#[test]
fn defaults_sort_and_lookup_by_register() {
    let mut defaults = [
        RegDefault {
            reg: 0x20,
            def: 0x22,
        },
        RegDefault {
            reg: 0x00,
            def: 0x00,
        },
        RegDefault {
            reg: 0x10,
            def: 0x11,
        },
    ];
    sort_defaults(&mut defaults);
    assert_eq!(
        defaults,
        [
            RegDefault {
                reg: 0x00,
                def: 0x00
            },
            RegDefault {
                reg: 0x10,
                def: 0x11
            },
            RegDefault {
                reg: 0x20,
                def: 0x22
            },
        ]
    );
    assert_eq!(lookup_reg_default(&defaults, 0x00), Some(0));
    assert_eq!(lookup_reg_default(&defaults, 0x10), Some(1));
    assert_eq!(lookup_reg_default(&defaults, 0x20), Some(2));
    assert_eq!(lookup_reg_default(&defaults, 0x18), None);
}

/// regcache.c:95-123,188-207 and regcache-flat.c:69-99. THE SILENT BUG: table first, then raw,
/// then fallback. Register 0x08 is intentionally present in ALL THREE sources; 0x22 must win.
#[test]
fn default_lookup_order_is_table_then_raw_then_fallback() {
    let table = [RegDefault {
        reg: 0x08,
        def: 0x22,
    }];
    let raw = [0x10, 0x11, 0x33, 0x13];
    assert_eq!(
        lookup_default(0x08, 4, &table, Some(&raw), Some(0x44)),
        Ok(Some(DefaultValue {
            value: 0x22,
            source: DefaultSource::RegDefaults
        }))
    );

    // No table member: defaults_raw index 2 wins over the plausible callback fallback.
    assert_eq!(
        lookup_default(0x08, 4, &[], Some(&raw), Some(0x44)),
        Ok(Some(DefaultValue {
            value: 0x33,
            source: DefaultSource::DefaultsRaw
        }))
    );

    // Neither table nor raw: only now may default_reg be consulted.
    assert_eq!(
        lookup_default(0x08, 4, &[], None, Some(0x44)),
        Ok(Some(DefaultValue {
            value: 0x44,
            source: DefaultSource::DefaultRegFallback
        }))
    );
    assert_eq!(lookup_default(0x08, 4, &[], None, None), Ok(None));
}

/// The three source names and count are literal and independent of production lookup results.
#[test]
fn all_three_default_sources_are_pinned_by_count_and_name() {
    let expected = ["reg_defaults", "defaults_raw", "default_reg_fallback"];
    assert_eq!(DEFAULT_SOURCE_NAMES.len(), 3);
    assert_eq!(DEFAULT_SOURCE_NAMES, expected);
}

/// Safe-port bounds are named rather than indexing outside caller data.
#[test]
fn raw_default_refusals_name_register_and_bound() {
    assert_eq!(
        lookup_default(3, 4, &[], Some(&[1, 2]), None),
        Err(DefaultError::RegisterNotStrideAligned { reg: 3, stride: 4 })
    );
    assert_eq!(
        lookup_default(8, 4, &[], Some(&[1, 2]), None),
        Err(DefaultError::RawIndexOutOfRange { index: 2, count: 2 })
    );
    assert_eq!(
        lookup_default(0, 0, &[], Some(&[1]), None),
        Err(DefaultError::ZeroStride)
    );
}
