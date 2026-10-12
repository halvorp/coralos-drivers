// SPDX-License-Identifier: GPL-2.0-only
//! Stepping order and the Kaby Lake revid table (intel_step.h, intel_step.c).
use i915_gt_core::step::*;

#[test]
fn step_order_is_the_enum_order() {
    // STEP_NONE = 0, A0 = 1 ... J3 = 40, FUTURE, FOREVER (intel_step.h:24-75)
    assert_eq!(Step::NONE.index(), 0);
    assert_eq!(Step::A0.index(), 1);
    assert_eq!(Step::B0.index(), 5);
    assert_eq!(Step::C0.index(), 9);
    assert_eq!(Step::D0.index(), 13);
    assert_eq!(Step::D1.index(), 14);
    assert_eq!(Step::E0.index(), 17);
    assert_eq!(Step::F0.index(), 21);
    assert_eq!(Step::G0.index(), 25);
    assert_eq!(Step::J3.index(), 40);
    assert_eq!(Step::FUTURE.index(), 41);
    assert_eq!(Step::FOREVER.index(), 42);
    assert!(Step::A0 < Step::B0 && Step::B0 < Step::C0 && Step::J3 < Step::FUTURE && Step::FUTURE < Step::FOREVER);
}

#[test]
fn kbl_revid_table() {
    // kbl_revids[] (intel_step.c:37-45)
    let want = [(1u8, Step::B0), (2, Step::C0), (3, Step::D0), (4, Step::F0), (5, Step::C0), (6, Step::D1), (7, Step::G0)];
    for (revid, step) in want {
        assert_eq!(kbl_graphics_step(revid), step, "revid {revid}");
    }
}

#[test]
fn a_hole_takes_the_next_revid_and_past_the_end_is_future() {
    assert_eq!(kbl_graphics_step(0), Step::B0, "revid 0 is a hole: the next populated revid (1) is used");
    assert_eq!(kbl_graphics_step(8), Step::FUTURE);
    assert_eq!(kbl_graphics_step(0x20), Step::FUTURE);
    assert_eq!(kbl_graphics_step(255), Step::FUTURE);
}

#[test]
fn is_graphics_step_is_half_open() {
    assert!(is_graphics_step(Step::B0, Step::NONE, Step::C0));
    assert!(!is_graphics_step(Step::C0, Step::NONE, Step::C0), "until is exclusive");
    assert!(is_graphics_step(Step::C0, Step::C0, Step::FOREVER), "since is inclusive");
    assert!(is_graphics_step(Step::FUTURE, Step::C0, Step::FOREVER));
    assert!(!is_graphics_step(Step::FOREVER, Step::C0, Step::FOREVER));
    assert!(is_graphics_step(Step::E0, Step::A0, Step::F0) && !is_graphics_step(Step::F0, Step::A0, Step::F0));
}

#[test]
fn step_of_rejects_letters_outside_a_to_j() {
    assert_eq!(Step::of(b'A', 0), Step::A0);
    assert_eq!(Step::of(b'J', 3), Step::J3);
}

#[test]
#[should_panic(expected = "STEP_NAME_LIST")]
fn step_of_k_is_refused() {
    let _ = Step::of(b'K', 0);
}
