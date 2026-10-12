// SPDX-License-Identifier: GPL-2.0-only
//! The fixed-capacity vector every plan and list is built on: a push past the bound is a NAMED refusal that leaves the contents
//! unchanged.
use i915_gt_core::{FixedVec, GtError};

#[test]
fn push_past_capacity_is_refused_by_name_and_changes_nothing() {
    let mut v = FixedVec::<u32, 3>::new("three", 0);
    for i in 1..=3 {
        v.push(i).unwrap();
    }
    assert_eq!(v.push(4), Err(GtError::ListFull { list: "three", capacity: 3 }));
    assert_eq!(v.as_slice(), [1, 2, 3]);
    assert_eq!(v.len(), 3);
    assert_eq!(v.capacity(), 3);
    assert!(!v.is_empty());
    assert_eq!(GtError::ListFull { list: "x", capacity: 1 }.name(), "LIST-FULL");
}

#[test]
fn an_empty_vector_is_empty() {
    let v = FixedVec::<u8, 2>::new("e", 0);
    assert!(v.is_empty());
    assert_eq!(v.as_slice(), []);
}

#[test]
fn plans_that_outgrow_their_buffer_refuse() {
    // a wa apply plan has exactly one effect per entry: a list at capacity N always fits a plan of N
    use i915_common_core::Reg;
    use i915_gt_core::wa::{plan_apply, WaList};
    let mut w = WaList::<2>::new("t", "e");
    w.write_or(Reg::new(0x100), false, 1).unwrap();
    w.write_or(Reg::new(0x104), false, 1).unwrap();
    assert_eq!(plan_apply(&w).unwrap().effects.len(), 2);
}
