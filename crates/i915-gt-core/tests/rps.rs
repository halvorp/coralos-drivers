// SPDX-License-Identifier: GPL-2.0-only
//! Integration tests for the Gen9 RPS waitboost planner.

use i915_common_core::Reg;
use i915_gt_core::effect::{Effect, WritePlan};
use i915_gt_core::rps::{Waitboost, Why, BOOST_IDLE_MS, FREQ_LOG_MS, RPS_WRITE_SET};
use i915_devinfo_core::rps::RpsLayout;

fn kbl_layout() -> RpsLayout {
    let desc = i915_devinfo_core::lookup(0x591c).unwrap();
    RpsLayout::for_platform(&desc).unwrap()
}

fn assert_in_write_set(effect: Effect) -> WritePlan {
    match effect {
        Effect::Write(wp) => {
            assert!(
                RPS_WRITE_SET.contains(&wp.reg),
                "register {:?} not in RPS_WRITE_SET",
                wp.reg
            );
            wp
        }
        other => panic!("expected Effect::Write, got {:?}", other),
    }
}

#[test]
fn cap_validation_and_decode() {
    let layout = kbl_layout();

    assert!(Waitboost::new(layout, 0).is_none());
    assert!(Waitboost::new(layout, 0xffff_ffff).is_none());
    assert!(Waitboost::new(layout, 0x0015_0006).is_none());

    let wb = Waitboost::new(layout, 0x0006_0615).unwrap();
    assert_eq!(wb.layout(), layout);
    let caps = wb.caps();
    assert_eq!(caps.rp0, 63);
    assert_eq!(caps.rp1, 18);
    assert_eq!(caps.rpn, 18);
    assert_eq!(wb.requested(), None);
    assert_eq!(wb.boosts, 0);
    assert_eq!(wb.decays, 0);
}

#[test]
fn pre_enable_inactivity_and_lifecycle() {
    let layout = kbl_layout();
    let mut wb = Waitboost::new(layout, 0x0006_0615).unwrap();

    // Pre-enable inactivity: no transitions, deadlines, or samples.
    assert_eq!(wb.requested(), None);
    assert_eq!(wb.next_deadline(), None);
    assert_eq!(wb.tick(50, true), None);
    assert_eq!(wb.tick(200, false), None);
    assert!(!wb.sample_due(50, true));
    assert!(!wb.sample_due(50, false));
    assert_eq!(wb.requested(), None);
    assert_eq!(wb.boosts, 0);
    assert_eq!(wb.decays, 0);

    // Enable plan writes GEN6_RP_CONTROL (0xa024 = 0x400) then GEN6_RPNSWREQ (0xa008 = 18 << 23).
    let plan = wb.enable_plan(100);
    let w0 = assert_in_write_set(plan[0]);
    let w1 = assert_in_write_set(plan[1]);
    assert_eq!(
        w0,
        WritePlan {
            reg: Reg::new(0xa024),
            value: 0x400,
        }
    );
    assert_eq!(
        w1,
        WritePlan {
            reg: Reg::new(0xa008),
            value: 18 << 23,
        }
    );
    assert_eq!(wb.requested(), Some(18));
    assert_eq!(wb.next_deadline(), None);

    // Idle ticks while already at RPn do nothing.
    assert_eq!(wb.tick(250, false), None);

    // First busy tick boosts to RP0 (21 * 3 = 63 in 16.67 MHz units -> 63 << 23).
    let (eff, why) = wb.tick(300, true).unwrap();
    let boost_write = assert_in_write_set(eff);
    assert_eq!(why, Why::Boost);
    assert_eq!(
        boost_write,
        WritePlan {
            reg: Reg::new(0xa008),
            value: 63 << 23,
        }
    );
    assert_eq!(wb.requested(), Some(63));
    assert_eq!(wb.boosts, 1);
    assert_eq!(wb.decays, 0);
    assert_eq!(wb.next_deadline(), Some(300 + BOOST_IDLE_MS));

    // Subsequent busy tick while already at RP0 emits no write, but advances the deadline.
    assert_eq!(wb.tick(350, true), None);
    assert_eq!(wb.boosts, 1);
    assert_eq!(wb.next_deadline(), Some(350 + BOOST_IDLE_MS));

    // Idle tick just before the 100 ms boundary does not decay.
    assert_eq!(wb.tick(350 + BOOST_IDLE_MS - 1, false), None);
    assert_eq!(wb.requested(), Some(63));
    assert_eq!(wb.decays, 0);
    assert_eq!(wb.next_deadline(), Some(350 + BOOST_IDLE_MS));

    // Idle tick at the exact 100 ms boundary decays to RPn (18 << 23) and clears deadline.
    let (eff, why) = wb.tick(350 + BOOST_IDLE_MS, false).unwrap();
    let decay_write = assert_in_write_set(eff);
    assert_eq!(why, Why::Idle);
    assert_eq!(
        decay_write,
        WritePlan {
            reg: Reg::new(0xa008),
            value: 18 << 23,
        }
    );
    assert_eq!(wb.requested(), Some(18));
    assert_eq!(wb.boosts, 1);
    assert_eq!(wb.decays, 1);
    assert_eq!(wb.next_deadline(), None);
}

#[test]
fn sample_timing_at_2000ms() {
    let layout = kbl_layout();
    let mut wb = Waitboost::new(layout, 0x0006_0615).unwrap();

    assert!(!wb.sample_due(0, true));
    let _ = wb.enable_plan(10);

    // Idle never triggers a sample.
    assert!(!wb.sample_due(10, false));

    // First busy sample triggers immediately and schedules next at +2000 ms.
    assert!(wb.sample_due(100, true));
    assert!(!wb.sample_due(100, true));
    assert!(!wb.sample_due(100 + FREQ_LOG_MS - 1, true));
    assert!(!wb.sample_due(100 + FREQ_LOG_MS, false));
    assert!(wb.sample_due(100 + FREQ_LOG_MS, true));
    assert!(!wb.sample_due(100 + FREQ_LOG_MS + 1999, true));
    assert!(wb.sample_due(100 + 2 * FREQ_LOG_MS, true));
}

#[test]
fn equal_caps_never_boost_or_decay() {
    let layout = kbl_layout();
    let mut wb = Waitboost::new(layout, 0x0006_0606).unwrap();
    let caps = wb.caps();
    assert_eq!(caps.rp0, 18);
    assert_eq!(caps.rp1, 18);
    assert_eq!(caps.rpn, 18);

    let plan = wb.enable_plan(0);
    for eff in plan {
        let _ = assert_in_write_set(eff);
    }
    assert_eq!(wb.requested(), Some(18));
    assert_eq!(wb.next_deadline(), None);

    assert_eq!(wb.tick(10, true), None);
    assert_eq!(wb.next_deadline(), None);
    assert_eq!(wb.tick(500, false), None);
    assert_eq!(wb.boosts, 0);
    assert_eq!(wb.decays, 0);
}