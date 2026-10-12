// SPDX-License-Identifier: GPL-2.0-only
//! Graphics steppings: `enum intel_step` (intel_step.h:24-75), the Kaby Lake revid table and the `IS_GRAPHICS_STEP` range test.
//! Ported from drivers/gpu/drm/i915/intel_step.h, intel_step.c and i915_drv.h.

/// `enum intel_step` as an ordered value: `STEP_NONE = 0`, `STEP_A0 = 1` ... `STEP_J3 = 40` (letters A..J, four digits each,
/// intel_step.h:24-64), then `STEP_FUTURE`, `STEP_FOREVER` (:70-75). Ordering is the whole point: `IS_GRAPHICS_STEP` compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Step(u8);

impl Step {
    pub const NONE: Step = Step(0); // intel_step.h:71
    pub const A0: Step = Step::of(b'A', 0);
    pub const B0: Step = Step::of(b'B', 0);
    pub const C0: Step = Step::of(b'C', 0);
    pub const D0: Step = Step::of(b'D', 0);
    pub const D1: Step = Step::of(b'D', 1);
    pub const E0: Step = Step::of(b'E', 0);
    pub const F0: Step = Step::of(b'F', 0);
    pub const G0: Step = Step::of(b'G', 0);
    pub const J3: Step = Step::of(b'J', 3);
    pub const FUTURE: Step = Step(41); // intel_step.h:73
    pub const FOREVER: Step = Step(42); // intel_step.h:74

    /// `STEP_<letter><digit>`: the position in `STEP_NAME_LIST` (intel_step.h:24-64) counting from 1.
    pub const fn of(letter: u8, digit: u8) -> Step {
        assert!(letter >= b'A' && letter <= b'J' && digit <= 3, "STEP_NAME_LIST has letters A..J and digits 0..3");
        Step(1 + (letter - b'A') * 4 + digit)
    }
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// `IS_GRAPHICS_STEP(i915, since, until)` (i915_drv.h:372-374): `step >= since && step < until`. Linux also WARNs when the
/// stepping is `STEP_NONE`; here `STEP_NONE` is simply outside every range that starts above it, and a range that starts at
/// `Step::NONE` (Linux passes a literal `0`) includes it - callers get a real stepping from [`kbl_graphics_step`], never NONE.
pub const fn is_graphics_step(step: Step, since: Step, until: Step) -> bool {
    step.0 >= since.0 && step.0 < until.0
}

/// `kbl_revids[]` (intel_step.c:37-45): PCI revision id -> stepping. Index 0 is a hole.
const KBL_REVIDS: [Step; 8] = [
    Step::NONE,         // [0] absent
    Step::B0,           // intel_step.c:38
    Step::C0,           // intel_step.c:39
    Step::D0,           // intel_step.c:40
    Step::F0,           // intel_step.c:41
    Step::C0,           // intel_step.c:42
    Step::D1,           // intel_step.c:43
    Step::G0,           // intel_step.c:44
];

/// `intel_step_init` for a Kaby Lake part (intel_step.c:228-251): a revid inside the table with a real stepping maps to it; a
/// hole takes the NEXT populated revid's stepping (revid 0 therefore reads as `B0`); a revid past the end is `STEP_FUTURE`.
pub const fn kbl_graphics_step(revid: u8) -> Step {
    let size = KBL_REVIDS.len();
    let mut r = revid as usize;
    while r < size && KBL_REVIDS[r].0 == Step::NONE.0 {
        r += 1;
    }
    if r < size {
        KBL_REVIDS[r]
    } else {
        Step::FUTURE
    }
}
