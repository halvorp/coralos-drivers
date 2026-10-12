// SPDX-License-Identifier: GPL-2.0-only
//! The plan vocabulary's own bounds, and the pinned names of every handshake and refusal.
use i915_common_core::Reg;
use i915_display_clock_core::error::{Bound, Handshake, PcodeErrno};
use i915_display_clock_core::plan::*;
use i915_display_clock_core::ClockError;
use i915_kbl_display_core::power::Well;

fn poll(max: u32, timeout_ms: u32, policy: OnTimeout) -> PollPlan {
    PollPlan::wait_set(Handshake::Dpll0Lock, Reg::new(0x46010), 0x4000_0000, timeout_ms, policy).with_max_polls(max)
}

#[test]
fn a_plan_is_bounded_and_never_truncates() {
    let mut p = WritePlan::new();
    for i in 0..PLAN_CAP {
        p.write(i as u32, 0).unwrap();
    }
    assert_eq!(p.len(), PLAN_CAP);
    assert_eq!(p.write(99, 0), Err(ClockError::PlanFull { cap: PLAN_CAP }));
    assert_eq!(p.len(), PLAN_CAP);
    assert_eq!(ClockError::PlanFull { cap: 0 }.name(), "PLAN-FULL");
    // extend refuses too, and does so without a partial append being observable as success
    let mut q = WritePlan::new();
    q.write(1, 1).unwrap();
    assert!(p.extend(&q).is_err());
    assert_eq!(PLAN_CAP, 32);
}

#[test]
fn transaction_then_is_bounded() {
    let mut a = Transaction::new();
    for _ in 0..PLAN_CAP {
        a.rollback.write(1, 1).unwrap();
    }
    let mut b = Transaction::new();
    b.rollback.write(2, 2).unwrap();
    let before = a;
    assert!(a.then(&b).is_err());
    assert_eq!(a, before, "a refused append leaves the transaction as it was");
}

#[test]
fn poll_condition_is_checked_before_either_bound() {
    // an ack on the very last permitted sample, with the clock already past the deadline, is a success
    let mut b = PollBudget::new(poll(3, 1, OnTimeout::Fail));
    assert_eq!(b.observe(0, 0), PollVerdict::Pending);
    assert_eq!(b.observe(0, 999), PollVerdict::Pending);
    assert_eq!(b.observe(0x4000_0000, 5_000), PollVerdict::Met { polls: 3 });
}

#[test]
fn poll_sample_bound_stops_a_stopped_clock() {
    let mut b = PollBudget::new(poll(3, 1_000_000, OnTimeout::Fail));
    assert_eq!(b.observe(0, 0), PollVerdict::Pending);
    assert_eq!(b.observe(0, 0), PollVerdict::Pending);
    match b.observe(0, 0) {
        PollVerdict::Failed(ClockError::HandshakeTimeout { bound, polls, elapsed_us, .. }) => assert_eq!((bound, polls, elapsed_us), (Bound::Polls, 3, 0)),
        v => panic!("{v:?}"),
    }
}

#[test]
fn poll_time_bound_stops_a_slow_one() {
    let mut b = PollBudget::new(poll(u32::MAX, 5, OnTimeout::Fail));
    assert_eq!(b.observe(0, 4_999), PollVerdict::Pending);
    match b.observe(0, 5_000) {
        PollVerdict::Failed(ClockError::HandshakeTimeout { bound, polls, elapsed_us, got, mask, want, .. }) => {
            assert_eq!((bound, polls, elapsed_us), (Bound::Time, 2, 5_000));
            assert_eq!((mask, want, got), (0x4000_0000, 0x4000_0000, 0));
        }
        v => panic!("{v:?}"),
    }
}

#[test]
fn a_zero_timeout_is_one_sample() {
    // hsw_wait_for_power_well_disable with another requester: `reqs ? 0 : 1` ms - one look, no wait
    let mut b = PollBudget::new(poll(100, 0, OnTimeout::Tolerate));
    assert!(matches!(b.observe(0, 0), PollVerdict::Tolerated(ClockError::HandshakeTimeout { bound: Bound::Time, polls: 1, .. })));
    let mut b = PollBudget::new(poll(100, 0, OnTimeout::Tolerate));
    assert_eq!(b.observe(0x4000_0000, 0), PollVerdict::Met { polls: 1 });
}

#[test]
fn tolerate_returns_the_error_as_a_record_and_fail_returns_it_as_a_verdict() {
    let mut t = PollBudget::new(poll(1, 1, OnTimeout::Tolerate));
    assert!(matches!(t.observe(0, 0), PollVerdict::Tolerated(_)));
    let mut f = PollBudget::new(poll(1, 1, OnTimeout::Fail));
    assert!(matches!(f.observe(0, 0), PollVerdict::Failed(_)));
}

#[test]
fn wait_clear_matches_zero() {
    let p = PollPlan::wait_clear(Handshake::Dpll0Unlock, Reg::new(0x46010), 0x4000_0000, 1, OnTimeout::Fail);
    assert!(p.satisfied(0x8000_0000));
    assert!(!p.satisfied(0xc000_0000));
    assert_eq!((p.mask, p.want, p.timeout_us, p.max_polls), (0x4000_0000, 0, 1000, DEFAULT_MAX_POLLS));
    let s = PollPlan::wait_set(Handshake::Dpll0Lock, Reg::new(0x46010), 0x4000_0000, 5, OnTimeout::Fail);
    assert!(s.satisfied(0x4000_0001) && !s.satisfied(0x8000_0000));
    assert_eq!(s.timeout_us, 5000);
}

#[test]
fn every_handshake_name_is_pinned() {
    let names = [
        (Handshake::PowerWellEnable(Well::Pw1), "PW1-ENABLE-ACK"),
        (Handshake::PowerWellEnable(Well::Pw2), "PW2-ENABLE-ACK"),
        (Handshake::PowerWellEnable(Well::MiscIo), "MISC-IO-ENABLE-ACK"),
        (Handshake::PowerWellEnable(Well::DdiAE), "DDI-A-E-ENABLE-ACK"),
        (Handshake::PowerWellEnable(Well::DdiB), "DDI-B-ENABLE-ACK"),
        (Handshake::PowerWellEnable(Well::DdiC), "DDI-C-ENABLE-ACK"),
        (Handshake::PowerWellEnable(Well::DdiD), "DDI-D-ENABLE-ACK"),
        (Handshake::PowerWellDisable(Well::Pw1), "PW1-DISABLE-ACK"),
        (Handshake::PowerWellDisable(Well::Pw2), "PW2-DISABLE-ACK"),
        (Handshake::PowerWellDisable(Well::MiscIo), "MISC-IO-DISABLE-ACK"),
        (Handshake::PowerWellDisable(Well::DdiAE), "DDI-A-E-DISABLE-ACK"),
        (Handshake::PowerWellDisable(Well::DdiB), "DDI-B-DISABLE-ACK"),
        (Handshake::PowerWellDisable(Well::DdiC), "DDI-C-DISABLE-ACK"),
        (Handshake::PowerWellDisable(Well::DdiD), "DDI-D-DISABLE-ACK"),
        (Handshake::FuseDistribution(0), "FUSE-PG0-DISTRIBUTION"),
        (Handshake::FuseDistribution(1), "FUSE-PG1-DISTRIBUTION"),
        (Handshake::FuseDistribution(2), "FUSE-PG2-DISTRIBUTION"),
        (Handshake::PcodeMailboxReady, "PCODE-MAILBOX-READY"),
        (Handshake::Dpll0Lock, "DPLL0-LOCK"),
        (Handshake::Dpll0Unlock, "DPLL0-UNLOCK"),
        (Handshake::DpllLock(1), "DPLL1-LOCK"),
        (Handshake::DpllLock(2), "DPLL2-LOCK"),
        (Handshake::DpllLock(3), "DPLL3-LOCK"),
    ];
    for (h, n) in names {
        assert_eq!(h.name(), n);
    }
}

#[test]
fn every_error_name_is_pinned() {
    let r = Reg::new(0);
    let z = |w| ClockError::WellOrder { well: w, needs: w };
    let all = [
        (ClockError::HandshakeTimeout { what: Handshake::Dpll0Lock, bound: Bound::Time, reg: r, mask: 0, want: 0, got: 0, polls: 0, elapsed_us: 0 }, "HANDSHAKE-TIMEOUT"),
        (ClockError::PcodeBusy { mailbox: 0 }, "PCODE-BUSY"),
        (ClockError::PcodeRejected { status: 0, errno_class: PcodeErrno::Enxio }, "PCODE-REJECTED"),
        (ClockError::PcodeNoAck { attempts: 0, elapsed_us: 0 }, "PCODE-NO-ACK"),
        (z(Well::Pw2), "POWER-WELL-ORDER"),
        (ClockError::WellStillNeeded { well: Well::Pw1, held_by: Well::Pw2 }, "POWER-WELL-STILL-NEEDED"),
        (ClockError::UnbalancedPut { what: "" }, "UNBALANCED-PUT"),
        (ClockError::PlanFull { cap: 0 }, "PLAN-FULL"),
        (ClockError::CdclkVcoInvalid { vco_khz: 0 }, "CDCLK-VCO-INVALID"),
        (ClockError::CdclkSkylakeVco8640 { vco_khz: 0 }, "CDCLK-SKL-VCO-8640-WA1183"),
        (ClockError::CdclkNotAStep { cdclk_khz: 0, vco_khz: 0 }, "CDCLK-NOT-A-STEP"),
        (ClockError::CdclkAboveMax { min_khz: 0, max_khz: 0 }, "CDCLK-ABOVE-MAX"),
        (ClockError::WrpllNoDivider { clock_khz: 0 }, "WRPLL-NO-DIVIDER"),
        (ClockError::WrpllDividerUnencodable { clock_khz: 0, divider: 0 }, "WRPLL-DIVIDER-UNENCODABLE"),
        (ClockError::DpRateUnsupported { port_clock_khz: 0 }, "DP-RATE-UNSUPPORTED"),
        (ClockError::Dpll0VcoMismatch { needs_khz: 0, running_khz: 0 }, "DPLL0-VCO-MISMATCH"),
        (ClockError::NoSuchDpll { id: 0 }, "NO-SUCH-DPLL"),
        (ClockError::NoFreeDpll { allowed_mask: 0 }, "NO-FREE-DPLL"),
    ];
    assert_eq!(all.len(), 18);
    for (e, n) in all {
        assert_eq!(e.name(), n);
    }
}
