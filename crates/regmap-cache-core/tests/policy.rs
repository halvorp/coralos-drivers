// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux `drivers/base/regmap/regmap.c` cache predicates.
//!
//! Copyright 2011 Wolfson Microelectronics plc. Original author: Mark Brown.

use regmap_cache_core::policy::{
    cacheable, check_range_table, noinc_allowed, precious, readable, reg_in_ranges, volatile,
    volatile_range, writeable, AccessInput, Override, Range, ReadableInput, SpecialInput,
};

fn access(callback: Override, table: Override) -> AccessInput {
    AccessInput { reg: 4, max_register: Some(8), callback, table }
}

fn special(readable: bool, callback: Override, table: Override, has_cache_ops: bool) -> SpecialInput {
    SpecialInput { readable, has_combined_format_write: false, callback, table, has_cache_ops }
}

/// regmap.c:56-87: ranges are inclusive, deny wins, and an empty allow-list allows every register.
#[test]
fn range_and_table_rules_match_linux_in_both_directions() {
    let ranges = [Range { min: 2, max: 4 }, Range { min: 8, max: 9 }];
    assert!(reg_in_ranges(2, &ranges));
    assert!(reg_in_ranges(4, &ranges));
    assert!(!reg_in_ranges(5, &ranges));
    assert!(check_range_table(99, &[], &[]), "zero yes-ranges means any register is OK");
    assert!(check_range_table(3, &[Range { min: 2, max: 4 }], &[]));
    assert!(!check_range_table(5, &[Range { min: 2, max: 4 }], &[]));
    assert!(!check_range_table(3, &[Range { min: 2, max: 4 }], &[Range { min: 3, max: 3 }]), "no-range overrides yes-range");
}

/// regmap.c:90-103: max first, callback before table, otherwise writeable.
#[test]
fn writeable_asserts_allow_and_deny_directions() {
    assert!(writeable(access(Override::Absent, Override::Absent)));
    assert!(!writeable(access(Override::Answer(false), Override::Answer(true))));
    assert!(writeable(access(Override::Answer(true), Override::Answer(false))));
    assert!(!writeable(AccessInput { reg: 9, ..access(Override::Answer(true), Override::Absent) }), "maximum wins even over callback");
}

/// regmap.c:127-144: each prerequisite can deny; callback/table/default can each allow.
#[test]
fn readable_asserts_allow_and_deny_directions() {
    let base = ReadableInput { access: access(Override::Absent, Override::Absent), has_reg_read: true, has_combined_format_write: false };
    assert!(readable(base));
    assert!(!readable(ReadableInput { has_reg_read: false, ..base }));
    assert!(!readable(ReadableInput { has_combined_format_write: true, ..base }));
    assert!(!readable(ReadableInput { access: access(Override::Answer(false), Override::Answer(true)), ..base }));
    assert!(readable(ReadableInput { access: access(Override::Answer(true), Override::Answer(false)), ..base }));
    assert!(!readable(ReadableInput { access: AccessInput { reg: 9, ..access(Override::Absent, Override::Absent) }, ..base }));
}

/// THE SILENT BUG, regmap.c:147-161 and regcache.c:280-321. Pin BOTH directions: a volatile
/// register is not cacheable, while a normal register is cacheable. The failures otherwise look alike.
#[test]
fn volatile_and_cacheable_are_asserted_in_both_directions() {
    assert!(volatile(special(true, Override::Answer(true), Override::Absent, true)));
    assert!(!cacheable(true, true), "volatile must never enter cache");
    assert!(!volatile(special(true, Override::Answer(false), Override::Absent, true)));
    assert!(cacheable(true, false), "normal register must remain cacheable");
    assert!(!cacheable(false, false), "no cache means no cached register");

    assert!(!volatile(special(true, Override::Absent, Override::Absent, true)), "cache ops make unspecified registers normal");
    assert!(volatile(special(true, Override::Absent, Override::Absent, false)), "without cache ops unspecified registers are volatile");
    assert!(!volatile(special(false, Override::Answer(true), Override::Absent, true)), "unreadable wins over callback");
    let combined = SpecialInput { readable: false, has_combined_format_write: true, callback: Override::Answer(true), table: Override::Absent, has_cache_ops: true };
    assert!(volatile(combined), "combined write format skips the readability guard");
    assert!(volatile(special(true, Override::Answer(true), Override::Answer(false), true)), "callback wins over table");
    assert!(!volatile(special(true, Override::Answer(false), Override::Answer(true), false)), "callback false wins too");
}

/// regmap.c:164-176: unreadable is never precious; callback/table can answer either way; default false.
#[test]
fn precious_asserts_true_and_false_directions() {
    assert!(!precious(special(false, Override::Answer(true), Override::Absent, true)));
    assert!(precious(special(true, Override::Answer(true), Override::Answer(false), true)));
    assert!(!precious(special(true, Override::Answer(false), Override::Answer(true), true)));
    assert!(precious(special(true, Override::Absent, Override::Answer(true), true)));
    assert!(!precious(special(true, Override::Absent, Override::Absent, true)));
}

/// regmap.c:178-198: callback, then table, then true. Pin every allow/deny direction.
#[test]
fn no_increment_predicate_asserts_both_directions() {
    assert!(noinc_allowed(Override::Absent, Override::Absent));
    assert!(noinc_allowed(Override::Answer(true), Override::Answer(false)));
    assert!(!noinc_allowed(Override::Answer(false), Override::Answer(true)));
    assert!(noinc_allowed(Override::Absent, Override::Answer(true)));
    assert!(!noinc_allowed(Override::Absent, Override::Answer(false)));
}

/// regmap.c:200-209: range is volatile only if EVERY register is volatile.
#[test]
fn volatile_range_asserts_both_directions() {
    assert!(volatile_range(&[true, true]));
    assert!(!volatile_range(&[true, false]));
    assert!(volatile_range(&[]), "Linux loop has no counterexample for an empty range");
}
