// SPDX-License-Identifier: GPL-2.0-only
//! The reset state machine against intel_reset.c, driven by a tiny simulated register file. The protocol literals (RESET_CTL
//! masked writes, 700 / 2000 us budgets, GDRST double write, 50 us settle, 3 retries) are Linux's; every bound has a test that
//! FAILS if the check is removed (a hard iteration cap turns a missing bound into an assertion, not a hang).
use i915_common_core::Reg;
use i915_gt_core::engine::{engine_info, EngineInfo};
use i915_gt_core::error::{GtError, ResetStage};
use i915_gt_core::reset::*;
use i915_gt_core::{PollPlan, WritePlan};
use std::collections::HashMap;

const RCS0: u32 = 0;
const BCS0: u32 = 1;
const VCS0: u32 = 10;
const VECS0: u32 = 18;

fn gt2() -> Vec<EngineInfo> {
    [RCS0, BCS0, VCS0, VECS0].iter().map(|&i| engine_info(9, i).unwrap()).collect()
}

/// A register file that models just enough of the hardware.
struct Dev {
    regs: HashMap<u32, u32>,
    /// engines whose RESET_CTL never raises READY_TO_RESET after a REQUEST_RESET write
    stuck: Vec<u32>,
    /// GDRST self-clears (domain reset acked)
    gdrst_acks: bool,
    /// how many attempts' worth of GDRST failures before acking
    gdrst_dead_attempts: u32,
    gdrst_writes: u32,
}

impl Dev {
    fn new() -> Dev {
        Dev { regs: HashMap::new(), stuck: vec![], gdrst_acks: true, gdrst_dead_attempts: 0, gdrst_writes: 0 }
    }
    fn read(&self, r: Reg) -> u32 {
        *self.regs.get(&r.offset()).unwrap_or(&0)
    }
    fn write(&mut self, w: WritePlan) {
        let off = w.reg.offset();
        if off == 0x941c {
            self.gdrst_writes += 1;
            let acks = self.gdrst_acks && self.gdrst_writes > self.gdrst_dead_attempts;
            self.regs.insert(off, if acks { 0 } else { w.value });
            return;
        }
        // masked write: upper half selects the bits the lower half changes (_MASKED_FIELD)
        let (mask, data) = (w.value >> 16, w.value & 0xffff);
        let old = self.read(w.reg);
        let mut new = (old & !mask) | (data & mask);
        if off & 0xfff == 0x0d0 && new & 1 != 0 && !self.stuck.contains(&(off - 0xd0)) {
            new |= 2; // REQUEST_RESET -> READY_TO_RESET
        }
        if off & 0xfff == 0x0d0 && new & 1 == 0 {
            new &= !2; // request withdrawn
        }
        self.regs.insert(off, new);
    }
}

/// Run the planner to completion. `cap` turns a missing bound into a test failure instead of a hang.
fn run(p: &mut ResetPlanner, dev: &mut Dev, cap: usize) -> (Result<ResetDone, GtError>, Vec<Output>) {
    let mut trace = vec![];
    let mut input = Input::Begin;
    for _ in 0..cap {
        match p.step(input) {
            Err(e) => return (Err(e), trace),
            Ok(out) => {
                trace.push(out);
                input = match out {
                    Output::Read(r) => Input::Read(dev.read(r)),
                    Output::Write(w) => {
                        dev.write(w);
                        Input::Wrote
                    }
                    Output::Poll(pp) => Input::Polled(dev.read(pp.reg)),
                    Output::DelayUs(_) => Input::Delayed,
                    Output::Done(d) => return (Ok(d), trace),
                };
            }
        }
    }
    panic!("the reset planner did not terminate within {cap} steps: an unbounded wait");
}

fn w(reg: u32, value: u32) -> Output {
    Output::Write(WritePlan { reg: Reg::new(reg), value })
}
fn poll(reg: u32, mask: u32, want: u32, timeout_us: u32, max_polls: u32) -> Output {
    Output::Poll(PollPlan { reg: Reg::new(reg), mask, want, timeout_us, max_polls, what: "" })
}
/// Compare traces ignoring the `what` marker text of polls.
fn norm(t: &[Output]) -> Vec<Output> {
    t.iter()
        .map(|o| match o {
            Output::Poll(p) => Output::Poll(PollPlan { what: "", ..*p }),
            x => *x,
        })
        .collect()
}

#[test]
fn constants_are_linux_literals() {
    assert_eq!(RESET_MAX_RETRIES, 3); // intel_reset.c:34
    assert_eq!(ENGINE_RESET_TIMEOUT_US, 700); // :591
    assert_eq!(GDRST_ACK_TIMEOUT_US, 2000); // :301
    assert_eq!(GDRST_SETTLE_US, 50); // :313
    assert_eq!(GDRST_LOOPS, 2); // :288
}

#[test]
fn full_reset_happy_path_trace_matches_the_linux_sequence() {
    let mut p = ResetPlanner::new(ResetScope::AllEngines, &gt2(), 1000).unwrap();
    assert_eq!(p.hw_mask(), 0x1, "ALL_ENGINES resets GEN6_GRDOM_FULL");
    assert_eq!(p.attempts_allowed(), 3);
    let mut dev = Dev::new();
    let (r, t) = run(&mut p, &mut dev, 1000);
    assert_eq!(r, Ok(ResetDone { attempts: 1, proceeded_past_not_ready: false }));
    // per engine: read RESET_CTL; write _MASKED_BIT_ENABLE(REQUEST_RESET) = 0x00010001; wait (ctl & READY_TO_RESET) == READY (700 us)
    let mut want = vec![];
    for base in [0x2000u32, 0x22000, 0x12000, 0x1a000] {
        let ctl = base + 0xd0;
        want.push(Output::Read(Reg::new(ctl)));
        want.push(w(ctl, 0x0001_0001));
        want.push(poll(ctl, 0x2, 0x2, 700, 1000));
    }
    // GDRST = FULL, ack wait (2000 us), issued twice (loops = 2), 50 us settle, then withdraw REQUEST_RESET: _MASKED_BIT_DISABLE
    for _ in 0..2 {
        want.push(w(0x941c, 0x1));
        want.push(poll(0x941c, 0x1, 0, 2000, 1000));
    }
    want.push(Output::DelayUs(50));
    for base in [0x2000u32, 0x22000, 0x12000, 0x1a000] {
        want.push(w(base + 0xd0, 0x0001_0000));
    }
    want.push(Output::Done(ResetDone { attempts: 1, proceeded_past_not_ready: false }));
    assert_eq!(norm(&t), want);
    assert_eq!(dev.gdrst_writes, 2);
}

#[test]
fn an_engine_that_is_already_ready_gets_no_request_write() {
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, RCS0).unwrap()], 100).unwrap();
    let mut dev = Dev::new();
    dev.regs.insert(0x20d0, 0x2); // READY_TO_RESET already set (intel_reset.c:585-586: `return 0`)
    let (r, t) = run(&mut p, &mut dev, 100);
    assert!(r.is_ok());
    let t = norm(&t);
    assert_eq!(t[0], Output::Read(Reg::new(0x20d0)));
    assert_eq!(t[1], w(0x941c, 0x2), "straight to GDRST with the RENDER domain");
}

#[test]
fn a_catastrophic_error_bypasses_ready_to_reset() {
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, RCS0).unwrap()], 100).unwrap();
    let mut dev = Dev::new();
    dev.regs.insert(0x20d0, 0x4); // RESET_CTL_CAT_ERROR
    // hardware clears CAT_ERROR once the masked write lands (modelled in the loop below)
    let mut input = Input::Begin;
    let mut seen = vec![];
    for _ in 0..40 {
        let out = p.step(input).unwrap();
        seen.push(out);
        input = match out {
            Output::Read(r) => Input::Read(dev.read(r)),
            Output::Write(wp) => {
                if wp.reg.offset() == 0x20d0 && wp.value == 0x0004_0004 {
                    dev.regs.insert(0x20d0, 0); // hardware clears it
                }
                Input::Wrote
            }
            Output::Poll(pp) => Input::Polled(dev.read(pp.reg)),
            Output::DelayUs(_) => Input::Delayed,
            Output::Done(_) => break,
        };
    }
    let seen = norm(&seen);
    // _MASKED_BIT_ENABLE(RESET_CTL_CAT_ERROR) = 0x00040004 and the wait is (ctl & CAT_ERROR) == 0 (intel_reset.c:571-575)
    assert_eq!(seen[1], w(0x20d0, 0x0004_0004));
    assert_eq!(seen[2], poll(0x20d0, 0x4, 0, 700, 100));
}

#[test]
fn partial_reset_ors_the_engine_reset_domains_and_gets_one_attempt() {
    let engines = [engine_info(9, RCS0).unwrap(), engine_info(9, VCS0).unwrap()];
    let mut p = ResetPlanner::new(ResetScope::Engines, &engines, 100).unwrap();
    assert_eq!(p.hw_mask(), 0x2 | 0x4, "GEN6_GRDOM_RENDER | GEN6_GRDOM_MEDIA");
    assert_eq!(p.attempts_allowed(), 1);
    let mut dev = Dev::new();
    let (r, t) = run(&mut p, &mut dev, 100);
    assert_eq!(r, Ok(ResetDone { attempts: 1, proceeded_past_not_ready: false }));
    assert!(norm(&t).contains(&w(0x941c, 0x6)));
}

#[test]
fn ready_to_reset_that_never_asserts_times_out_by_name_after_exactly_max_polls() {
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, RCS0).unwrap()], 7).unwrap();
    let mut dev = Dev::new();
    dev.stuck.push(0x2000);
    let (r, t) = run(&mut p, &mut dev, 200);
    let e = r.unwrap_err();
    assert_eq!(e, GtError::ResetTimeout { stage: ResetStage::EngineReadyToReset, reg: Reg::new(0x20d0), mask: 0x2, want: 0x2, got: 0x1, polls: 7 });
    assert_eq!(e.name(), "RESET-READY-TIMEOUT");
    let polls = t.iter().filter(|o| matches!(o, Output::Poll(_))).count();
    assert_eq!(polls, 7, "the bound is enforced: exactly max_polls reads, then the named refusal");
    // the request is withdrawn before the error is returned (intel_reset.c:660-664: skip_reset: gen8_engine_reset_cancel)
    assert_eq!(*norm(&t).last().unwrap(), w(0x20d0, 0x0001_0000));
    // and the GDRST was NEVER written: a failed first-try prepare skips the reset entirely
    assert!(!t.iter().any(|o| matches!(o, Output::Write(wp) if wp.reg.offset() == 0x941c)));
}

#[test]
fn removing_the_poll_bound_would_be_caught_by_the_iteration_cap() {
    // a device that never answers, a huge max_polls and a small cap: the harness must PANIC (it cannot terminate in time),
    // proving the cap-based tests above are able to fail if the planner's own check were removed
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, RCS0).unwrap()], u32::MAX).unwrap();
    let mut dev = Dev::new();
    dev.stuck.push(0x2000);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut p, &mut dev, 500)));
    assert!(r.is_err(), "with an effectively unlimited bound the 500-step cap must trip");
}

#[test]
fn a_caller_clock_timeout_is_the_same_named_failure() {
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, RCS0).unwrap()], 1_000_000).unwrap();
    assert!(matches!(p.step(Input::Begin), Ok(Output::Read(_))));
    assert!(matches!(p.step(Input::Read(0)), Ok(Output::Write(_))));
    assert!(matches!(p.step(Input::Wrote), Ok(Output::Poll(_))));
    assert!(matches!(p.step(Input::Polled(0)), Ok(Output::Poll(_))));
    // 700 us elapsed on the caller's clock after 1 read
    assert!(matches!(p.step(Input::PollTimedOut(0)), Ok(Output::Write(_))), "cancel write first");
    let e = p.step(Input::Wrote).unwrap_err();
    assert_eq!(e, GtError::ResetTimeout { stage: ResetStage::EngineReadyToReset, reg: Reg::new(0x20d0), mask: 2, want: 2, got: 0, polls: 1 });
}

#[test]
fn a_full_reset_retries_and_proceeds_past_a_stuck_engine_on_the_second_attempt() {
    let mut p = ResetPlanner::new(ResetScope::AllEngines, &gt2(), 5).unwrap();
    let mut dev = Dev::new();
    dev.stuck.push(0x2000); // RCS0 never gets ready
    let (r, t) = run(&mut p, &mut dev, 500);
    // attempt 0: RCS0 times out -> skip_reset. attempt 1 (retry >= 1): reset_non_ready -> carry on to GDRST (intel_reset.c:613-636)
    assert_eq!(r, Ok(ResetDone { attempts: 2, proceeded_past_not_ready: true }));
    assert_eq!(dev.gdrst_writes, 2, "one GDRST pair, only in the second attempt");
    let t = norm(&t);
    let first_gdrst = t.iter().position(|o| *o == w(0x941c, 1)).unwrap();
    let first_cancel = t.iter().position(|o| *o == w(0x20d0, 0x0001_0000)).unwrap();
    assert!(first_cancel < first_gdrst, "attempt 0 cancels before attempt 1 resets");
}

#[test]
fn a_full_reset_gives_up_after_exactly_three_attempts_when_gdrst_never_acks() {
    let mut p = ResetPlanner::new(ResetScope::AllEngines, &gt2(), 4).unwrap();
    let mut dev = Dev::new();
    dev.gdrst_acks = false;
    let (r, t) = run(&mut p, &mut dev, 1000);
    let e = r.unwrap_err();
    assert_eq!(e.name(), "GDRST-ACK-TIMEOUT");
    assert_eq!(e, GtError::ResetTimeout { stage: ResetStage::GdrstAck, reg: Reg::new(0x941c), mask: 0x1, want: 0, got: 1, polls: 4 });
    assert_eq!(dev.gdrst_writes, 3, "RESET_MAX_RETRIES attempts, one GDRST write each (the failed wait stops the double write)");
    let delays = t.iter().filter(|o| matches!(o, Output::DelayUs(50))).count();
    assert_eq!(delays, 3, "the 50 us settle still happens after a failed ack (intel_reset.c:313)");
}

#[test]
fn a_partial_reset_is_not_retried() {
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, BCS0).unwrap()], 4).unwrap();
    let mut dev = Dev::new();
    dev.gdrst_acks = false;
    let (r, _) = run(&mut p, &mut dev, 200);
    assert_eq!(r.unwrap_err().name(), "GDRST-ACK-TIMEOUT");
    assert_eq!(dev.gdrst_writes, 1, "retries = 1 for a single-engine reset (intel_reset.c:766)");
}

#[test]
fn gdrst_whose_first_write_never_acks_is_a_timeout_not_a_hang() {
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, VECS0).unwrap()], 50).unwrap();
    let mut dev = Dev::new();
    dev.gdrst_dead_attempts = 1; // first GDRST write does not ack... then it never would; poll bound applies
    let (r, _) = run(&mut p, &mut dev, 400);
    assert!(r.is_err(), "the first write never acks inside the poll bound: that is a timeout, not a hang");
}

#[test]
fn inputs_that_do_not_answer_the_pending_output_are_refused() {
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, RCS0).unwrap()], 10).unwrap();
    assert_eq!(p.step(Input::Wrote).unwrap_err().name(), "RESET-OUT-OF-ORDER");
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, RCS0).unwrap()], 10).unwrap();
    p.step(Input::Begin).unwrap();
    assert!(matches!(p.step(Input::Polled(0)), Err(GtError::ResetOutOfOrder { expected: "Read" })));
    // once finished, nothing more is accepted
    let mut p = ResetPlanner::new(ResetScope::Engines, &[engine_info(9, RCS0).unwrap()], 10).unwrap();
    let mut dev = Dev::new();
    let (r, _) = run(&mut p, &mut dev, 100);
    assert!(r.is_ok());
    assert!(matches!(p.step(Input::Begin), Err(GtError::ResetOutOfOrder { .. })));
}

#[test]
fn a_zero_bound_or_no_engines_is_refused_at_construction() {
    let e = engine_info(9, RCS0).unwrap();
    assert_eq!(ResetPlanner::new(ResetScope::Engines, &[e], 0).unwrap_err(), GtError::ZeroBound { what: "reset max_polls" });
    assert_eq!(ResetPlanner::new(ResetScope::AllEngines, &[], 5).unwrap_err().name(), "NO-SUCH-ENGINE");
    assert_eq!(GtError::ZeroBound { what: "x" }.name(), "ZERO-BOUND");
}

#[test]
fn the_sequence_holds_all_forcewake_domains() {
    let p = ResetPlanner::new(ResetScope::AllEngines, &gt2(), 5).unwrap();
    assert!(p.forcewake_all(), "__intel_gt_reset takes FORCEWAKE_ALL around the retries (intel_reset.c:779)");
    assert_eq!(p.scope(), ResetScope::AllEngines);
}
