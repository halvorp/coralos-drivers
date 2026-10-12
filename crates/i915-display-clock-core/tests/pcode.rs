// SPDX-License-Identifier: GPL-2.0-only
//! The PCU mailbox: status classes, acknowledgement, the READY wait, and the bounded resend loop (attempt bound AND time bound).
mod common;
use common::*;
use i915_display_clock_core::error::{Bound, Handshake, PcodeErrno};
use i915_display_clock_core::pcode::*;
use i915_display_clock_core::plan::DEFAULT_MAX_POLLS;
use i915_display_clock_core::regs::*;
use i915_display_clock_core::{pcode_status, ClockError};

#[test]
fn register_and_command_literals() {
    // i915_reg.h:1079-1109, :1179, :1182
    assert_eq!(GEN6_PCODE_MAILBOX, 0x138124);
    assert_eq!(GEN6_PCODE_DATA, 0x138128);
    assert_eq!(GEN6_PCODE_DATA1, 0x13812c);
    assert_eq!(GEN6_PCODE_READY, 0x8000_0000);
    assert_eq!(GEN6_PCODE_ERROR_MASK, 0xff);
    assert_eq!(SKL_PCODE_CDCLK_CONTROL, 0x7);
    assert_eq!(SKL_CDCLK_PREPARE_FOR_CHANGE, 0x3);
    assert_eq!(SKL_CDCLK_READY_FOR_CHANGE, 0x1);
    let r = PcodeRequest::cdclk_prepare();
    assert_eq!((r.mbox, r.request, r.reply_mask, r.reply, r.timeout_base_ms), (7, 3, 1, 1, 3)); // intel_cdclk.c:1178-1181
    assert_eq!(r.deadline_us(), 53_000); // 3 ms + the 50 ms atomic retry (intel_pcode.c:178-200)
    assert_eq!((PCODE_REQUEST_FAST_US, PCODE_WRITE_FAST_US, PCODE_RETRY_EXTRA_MS), (500, 250, 50));
}

#[test]
fn an_attempt_writes_data_data1_then_the_mailbox_with_ready() {
    // __snb_pcode_rw (intel_pcode.c:74-77)
    let r = PcodeRequest::cdclk_prepare();
    let w = r.attempt_writes();
    assert_eq!((w[0].0.offset(), w[0].1), (GEN6_PCODE_DATA, 3));
    assert_eq!((w[1].0.offset(), w[1].1), (GEN6_PCODE_DATA1, 0));
    assert_eq!((w[2].0.offset(), w[2].1), (GEN6_PCODE_MAILBOX, 0x8000_0007));
    let w = PcodeWrite::new(7, 2).attempt_writes();
    assert_eq!((w[0].1, w[2].1), (2, 0x8000_0007));
    let p = r.ready_poll();
    assert_eq!((p.reg.offset(), p.mask, p.want, p.timeout_us, p.what), (GEN6_PCODE_MAILBOX, GEN6_PCODE_READY, 0, 500, Handshake::PcodeMailboxReady));
    assert_eq!(PcodeWrite::new(7, 2).ready_poll().timeout_us, 250 + 1000); // 250 us fast + 1 ms slow
}

#[test]
fn status_classes_every_member() {
    // gen7_check_mailbox_status (intel_pcode.c:33-54)
    let rows = [
        (0x00u32, None),
        (0x01, Some(PcodeErrno::Enxio)),      // ILLEGAL_CMD
        (0x04, Some(PcodeErrno::Enxio)),      // GEN11_PCODE_ILLEGAL_SUBCOMMAND
        (0x02, Some(PcodeErrno::Etimedout)),  // GEN7_PCODE_TIMEOUT
        (0x03, Some(PcodeErrno::Einval)),     // GEN7_PCODE_ILLEGAL_DATA
        (0x06, Some(PcodeErrno::Ebusy)),      // GEN11_PCODE_LOCKED
        (0x11, Some(PcodeErrno::Eacces)),     // GEN11_PCODE_REJECTED
        (0x10, Some(PcodeErrno::Eoverflow)),  // GEN7_PCODE_MIN_FREQ_TABLE_GT_RATIO_OUT_OF_RANGE
        (0xff, Some(PcodeErrno::Unknown)),    // GEN6_PCODE_UNIMPLEMENTED_CMD: MISSING_CASE on gen7+
        (0x05, Some(PcodeErrno::Unknown)),
    ];
    for (code, want) in rows {
        assert_eq!(pcode_status(code), want, "status {code:#x}");
        // only the low byte is the status; READY and the command bits are not
        assert_eq!(pcode_status(0x8000_0700 | code), want, "status {code:#x} with other bits");
    }
}

#[test]
fn acknowledged_needs_status_zero_and_the_masked_reply() {
    let r = PcodeRequest::cdclk_prepare();
    assert!(r.acknowledged(0, 1));
    assert!(r.acknowledged(0, 0xffff_ffff)); // mask 1: only bit 0 matters
    assert!(!r.acknowledged(0, 0));
    assert!(!r.acknowledged(0, 2));
    assert!(!r.acknowledged(0x11, 1)); // a rejected status never acknowledges (intel_pcode.c:139)
    assert!(!r.acknowledged(0x03, 1));
}

#[test]
fn request_is_acknowledged_after_n_resends() {
    for n in [0u32, 1, 3, 50] {
        let mut s = Sim::new();
        s.pcu_ack_after = n;
        let attempts = run_pcode(&mut s, PcodeOp::Request(PcodeRequest::cdclk_prepare())).unwrap();
        assert_eq!(attempts, n + 1, "PCU acks the {}th request", n + 1);
        assert_eq!(s.pcu_requests, n + 1);
    }
}

#[test]
fn the_attempt_bound_is_enforced_when_the_clock_does_not_advance() {
    // the PCU never acknowledges and time never advances: only max_attempts can stop it
    let mut s = Sim::new();
    s.pcu_ack_after = u32::MAX;
    s.us_per_sample = 0;
    s.us_per_attempt = 0;
    let r = PcodeRequest::cdclk_prepare().with_max_attempts(7);
    let e = run_pcode(&mut s, PcodeOp::Request(r)).unwrap_err();
    assert_eq!(e, ClockError::PcodeNoAck { attempts: 7, elapsed_us: 0 });
    assert_eq!(e.name(), "PCODE-NO-ACK");
    assert_eq!(s.pcu_requests, 7);
    // the default bound is the constant
    let mut s = Sim::new();
    s.pcu_ack_after = u32::MAX;
    s.us_per_sample = 0;
    s.us_per_attempt = 0;
    let e = run_pcode(&mut s, PcodeOp::Request(PcodeRequest::cdclk_prepare())).unwrap_err();
    assert_eq!(e, ClockError::PcodeNoAck { attempts: PCODE_MAX_ATTEMPTS, elapsed_us: 0 });
}

#[test]
fn the_time_bound_is_enforced_at_3_plus_50_ms() {
    let mut s = Sim::new();
    s.pcu_ack_after = u32::MAX;
    s.us_per_sample = 0;
    s.us_per_attempt = 1000; // each resend costs 1 ms
    let e = run_pcode(&mut s, PcodeOp::Request(PcodeRequest::cdclk_prepare())).unwrap_err();
    match e {
        ClockError::PcodeNoAck { attempts, elapsed_us } => {
            assert_eq!(elapsed_us, 53_000);
            assert_eq!(attempts, 53); // the 53rd attempt is the first to reach 53 ms
        }
        e => panic!("{}", e.name()),
    }
}

#[test]
fn a_rejecting_pcu_reports_its_status_class_once_the_bound_is_hit() {
    // status != 0 wins over the generic timeout (`status ? status : ret`, intel_pcode.c:205)
    for (status, class) in [(0x11u32, PcodeErrno::Eacces), (0x02, PcodeErrno::Etimedout), (0x06, PcodeErrno::Ebusy), (0x77, PcodeErrno::Unknown)] {
        let mut s = Sim::new();
        s.pcu_status = status;
        s.us_per_sample = 0;
        s.us_per_attempt = 1000;
        let e = run_pcode(&mut s, PcodeOp::Request(PcodeRequest::cdclk_prepare())).unwrap_err();
        assert_eq!(e, ClockError::PcodeRejected { status, errno_class: class });
        assert_eq!(e.name(), "PCODE-REJECTED");
    }
}

#[test]
fn a_mailbox_that_never_reports_ready_times_out_by_name() {
    let mut s = Sim::new();
    s.pcu_ready_stuck = true;
    // READY stays set after the first attempt: attempt 2 sees it BUSY on entry. Use a fresh mailbox each: first attempt times out.
    s.us_per_attempt = 0;
    s.us_per_sample = 1;
    let r = PcodeRequest::cdclk_prepare().with_max_attempts(1);
    let e = run_pcode(&mut s, PcodeOp::Request(r)).unwrap_err();
    match e {
        ClockError::HandshakeTimeout { what, bound, reg, mask, want, polls, .. } => {
            assert_eq!(what, Handshake::PcodeMailboxReady);
            assert_eq!(what.name(), "PCODE-MAILBOX-READY");
            assert_eq!(bound, Bound::Polls); // attempts exhausted
            assert_eq!((reg.offset(), mask, want, polls), (GEN6_PCODE_MAILBOX, GEN6_PCODE_READY, 0, 1));
        }
        e => panic!("{}", e.name()),
    }
    // unbounded by attempts, the time bound stops it
    let mut s = Sim::new();
    s.pcu_ready_stuck = true;
    s.us_per_sample = 100; // 500 us READY poll = 5 samples per attempt
    s.us_per_attempt = 0;
    let e = run_pcode(&mut s, PcodeOp::Request(PcodeRequest::cdclk_prepare())).unwrap_err();
    // after the first attempt the mailbox is left with READY set, so every later attempt is BUSY
    assert!(matches!(e, ClockError::PcodeBusy { .. } | ClockError::HandshakeTimeout { .. }), "{}", e.name());
}

#[test]
fn a_busy_mailbox_is_refused_by_name() {
    // READY already set on entry: -EAGAIN (intel_pcode.c:71-72)
    let mut s = Sim::new();
    s.set(GEN6_PCODE_MAILBOX, GEN6_PCODE_READY);
    s.us_per_attempt = 1000;
    let e = run_pcode(&mut s, PcodeOp::Request(PcodeRequest::cdclk_prepare())).unwrap_err();
    assert_eq!(e, ClockError::PcodeBusy { mailbox: GEN6_PCODE_READY });
    assert_eq!(e.name(), "PCODE-BUSY");
    assert!(s.writes.is_empty(), "a busy mailbox must not be written");
    let e = run_pcode(&mut s, PcodeOp::Write(PcodeWrite::new(7, 1))).unwrap_err();
    assert_eq!(e.name(), "PCODE-BUSY");
    assert!(s.writes.is_empty());
}

#[test]
fn write_succeeds_on_status_zero_and_reports_the_class_otherwise() {
    let mut s = Sim::new();
    assert_eq!(run_pcode(&mut s, PcodeOp::Write(PcodeWrite::new(7, 3))), Ok(1));
    assert_eq!(s.writes_to(GEN6_PCODE_DATA), [3]);
    for (status, class) in [(0x01u32, PcodeErrno::Enxio), (0x03, PcodeErrno::Einval), (0x11, PcodeErrno::Eacces)] {
        let mut s = Sim::new();
        s.pcu_status = status;
        assert_eq!(
            run_pcode(&mut s, PcodeOp::Write(PcodeWrite::new(7, 3))),
            Err(ClockError::PcodeRejected { status, errno_class: class })
        );
    }
}

#[test]
fn the_poll_sample_cap_is_a_real_constant() {
    assert_eq!(DEFAULT_MAX_POLLS, 100_000);
    assert_eq!(PCODE_MAX_ATTEMPTS, 100_000);
}

#[test]
fn success_is_checked_before_the_bounds() {
    // __wait_for: "Guarantee COND check prior to timeout" - an ack arriving on the very last permitted attempt succeeds
    let r = PcodeRequest::cdclk_prepare().with_max_attempts(3);
    let mut s = PcodeSession::new(r);
    let miss = AttemptOutcome::Replied { mailbox: 0, data: 0 };
    let hit = AttemptOutcome::Replied { mailbox: 0, data: 1 };
    assert_eq!(s.after_attempt(miss, 0), SessionStep::Resend);
    assert_eq!(s.after_attempt(miss, 0), SessionStep::Resend);
    assert_eq!(s.after_attempt(hit, 999_999_999), SessionStep::Acknowledged { attempts: 3 });
    let mut s = PcodeSession::new(r);
    s.after_attempt(miss, 0);
    s.after_attempt(miss, 0);
    assert!(matches!(s.after_attempt(miss, 0), SessionStep::Fail(ClockError::PcodeNoAck { attempts: 3, .. })));
}
