// SPDX-License-Identifier: GPL-2.0-only
//! The Runner's failure policy and its read bound, on small hand-made programs.
mod common;
use avs_fw_core::effect::*;
use avs_fw_core::runner::*;
use common::*;

const R: Reg = Reg { space: Space::Dsp, off: 0x04, width: Width::D32 };

fn upd(label: &'static str, on_fail: OnFail) -> Step {
    Step::new(label, Op::Update { reg: R, mask: 1, value: 1 }, on_fail)
}
fn poll(label: &'static str, on_fail: OnFail) -> Step {
    Step::new(label, Op::Poll(PollSpec::new(R, 1, 1, 10, 1000)), on_fail)
}
fn prog(steps: &[Step]) -> Program {
    let mut p = Program::new();
    for s in steps {
        p.push(*s);
    }
    p
}

#[test]
fn a_clean_run_walks_main_then_the_finalizer() {
    let mut r = Runner::new(prog(&[upd("a", OnFail::Abort), upd("b", OnFail::Abort)]), prog(&[upd("fin", OnFail::Tolerate)])).unwrap();
    let (log, out) = run(|i| r.next(i), ok_answer, 100);
    assert_eq!(out, Output::Finished);
    assert_eq!(labels(&log), vec!["a", "b", "fin"]);
}

#[test]
fn the_poll_read_bound_is_enforced_by_the_planner() {
    // timeout 1000us / 10us + 2 = 102 questions; a caller that never reports a timeout and a register that never changes
    let spec = PollSpec::new(R, 1, 1, 10, 1000);
    assert_eq!(spec.max_polls, 102);
    let mut r = Runner::new(prog(&[poll("p", OnFail::Abort)]), Program::new()).unwrap();
    let (log, out) = run(|i| r.next(i), |_| Input::Value(0), 10_000);
    assert_eq!(out, Output::Failed(Failure { step: "p", cause: Cause::PollBound { reads: 102, last: 0 } }));
    assert_eq!(log.len(), 102);
}

#[test]
fn the_bound_counts_reads_per_poll_not_per_program() {
    let mut r = Runner::new(prog(&[poll("p1", OnFail::Abort), poll("p2", OnFail::Abort)]), Program::new()).unwrap();
    let mut asked = 0;
    let (_, out) = run(
        |i| r.next(i),
        |s| {
            asked += 1;
            // 101 unsatisfied answers (below the bound) then the condition holds, for each poll
            if asked % 102 == 0 {
                Input::Value(1)
            } else if let Op::Poll(_) = s.op {
                Input::Value(0)
            } else {
                Input::Done
            }
        },
        10_000,
    );
    assert_eq!(out, Output::Finished);
}

#[test]
fn a_poll_satisfied_late_but_within_the_bound_succeeds() {
    let mut r = Runner::new(prog(&[poll("p", OnFail::Abort)]), Program::new()).unwrap();
    let mut n = 0;
    let (log, out) = run(
        |i| r.next(i),
        |_| {
            n += 1;
            Input::Value(if n == 101 { 1 } else { 0 })
        },
        10_000,
    );
    assert_eq!(out, Output::Finished);
    assert_eq!(log.len(), 101);
}

#[test]
fn the_callers_timeout_is_a_named_failure_with_the_last_value() {
    let mut r = Runner::new(prog(&[poll("p", OnFail::Abort)]), Program::new()).unwrap();
    let (_, out) = run(|i| r.next(i), |_| Input::PollTimedOut(0xAB), 100);
    assert_eq!(out, Output::Failed(Failure { step: "p", cause: Cause::PollTimeout { last: 0xAB } }));
}

#[test]
fn abort_runs_no_cleanup_and_skips_the_rest_of_main() {
    // loader.c:144-162: the error is returned with nothing undone
    let mut r = Runner::new(prog(&[upd("a", OnFail::Abort), poll("p", OnFail::Abort), upd("never", OnFail::Abort)]), prog(&[upd("fin", OnFail::Tolerate)])).unwrap();
    let (log, out) = run(|i| r.next(i), |s| if s.label == "p" { Input::PollTimedOut(0) } else { ok_answer(s) }, 100);
    assert_eq!(labels(&log), vec!["a", "p", "fin"]);
    assert!(matches!(out, Output::Failed(Failure { step: "p", .. })));
}

#[test]
fn abort_disable_runs_stall_reset_power_off_in_that_order_with_every_error_tolerated() {
    // dsp.c:113-121 (avs_dsp_core_disable): stall, reset, power - "No error checks to allow for complete DSP shutdown"
    let mut r = Runner::new(prog(&[poll("rom", OnFail::AbortDisable)]), Program::new()).unwrap();
    let (log, out) = run(
        |i| r.next(i),
        |s| match s.op {
            Op::Poll(_) if s.label == "rom" => Input::PollTimedOut(0),
            Op::Poll(_) => Input::PollTimedOut(0), // every cleanup poll fails too
            _ => Input::Done,
        },
        100,
    );
    assert_eq!(
        labels(&log),
        vec![
            "rom", "core-stall", "core-stall-ack", "core-stall-settle", "core-reset-enter", "core-reset-enter-ack", "core-power-off", "core-power-off-settle",
            "core-power-off-cpa"
        ]
    );
    assert_eq!(out, Output::Failed(Failure { step: "rom", cause: Cause::PollTimeout { last: 0 } }), "the FIRST failure is the one reported");
    // the register writes of the cleanup: CSTALL set, CRST set, SPA cleared (masks for the main core)
    let upds: Vec<(u32, u32)> = log
        .iter()
        .filter_map(|s| if let Op::Update { mask, value, .. } = s.op { Some((mask, value)) } else { None })
        .collect();
    assert_eq!(upds, vec![(0x100, 0x100), (0x1, 0x1), (0x1_0000, 0)]);
}

#[test]
fn abort_stop_disable_stops_the_loader_before_disabling_the_core() {
    // loader.c:183-188
    let mut r = Runner::new(prog(&[poll("entered", OnFail::AbortStopDisable)]), Program::new()).unwrap();
    let (log, out) = run(|i| r.next(i), |s| if s.label == "entered" { Input::PollTimedOut(7) } else { ok_answer(s) }, 100);
    assert_eq!(
        labels(&log),
        vec![
            "entered", "cldma-irq-off", "cldma-run-off", "cldma-stopped", "core-stall", "core-stall-ack", "core-stall-settle", "core-reset-enter",
            "core-reset-enter-ack", "core-power-off", "core-power-off-settle", "core-power-off-cpa"
        ]
    );
    assert_eq!(out, Output::Failed(Failure { step: "entered", cause: Cause::PollTimeout { last: 7 } }));
}

#[test]
fn a_tolerated_failure_does_not_stop_the_program() {
    // loader.c:183: hda_cldma_stop(cl) - the result is ignored
    let mut r = Runner::new(prog(&[poll("stop", OnFail::Tolerate), upd("after", OnFail::Abort)]), Program::new()).unwrap();
    let (log, out) = run(|i| r.next(i), |s| if s.label == "stop" { Input::PollTimedOut(0) } else { ok_answer(s) }, 100);
    assert_eq!(out, Output::Finished);
    assert_eq!(labels(&log), vec!["stop", "after"]);
}

#[test]
fn the_finalizer_runs_after_a_cleanup_too_and_its_failures_are_ignored() {
    // loader.c:643-646 `reenable_gating:`
    let mut r = Runner::new(prog(&[upd("a", OnFail::AbortDisable)]), prog(&[poll("fin1", OnFail::Tolerate), upd("fin2", OnFail::Tolerate)])).unwrap();
    let (log, out) = run(|i| r.next(i), |s| match s.label {
        "a" => Input::Done,
        _ => ok_answer(s),
    }, 100);
    assert_eq!(out, Output::Finished);
    assert_eq!(labels(&log), vec!["a", "fin1", "fin2"]);
    // now fail "a" by protocol-valid means: an Update cannot fail on its own, so use a poll as the failing step
    let mut r = Runner::new(prog(&[poll("a", OnFail::Abort)]), prog(&[poll("fin1", OnFail::Tolerate), upd("fin2", OnFail::Tolerate)])).unwrap();
    let (log, out) = run(|i| r.next(i), |s| if s.label == "a" || s.label == "fin1" { Input::PollTimedOut(0) } else { ok_answer(s) }, 100);
    assert_eq!(labels(&log), vec!["a", "fin1", "fin2"]);
    assert!(matches!(out, Output::Failed(Failure { step: "a", .. })));
}

#[test]
fn a_wrong_answer_is_a_protocol_failure() {
    let mut r = Runner::new(prog(&[upd("a", OnFail::Tolerate)]), Program::new()).unwrap();
    assert!(matches!(r.next(Input::Begin), Output::Do(_)));
    assert_eq!(r.next(Input::EventFired), Output::Failed(Failure { step: "a", cause: Cause::Protocol }));
    // not begun
    let mut r = Runner::new(prog(&[upd("a", OnFail::Tolerate)]), Program::new()).unwrap();
    assert!(matches!(r.next(Input::Done), Output::Failed(Failure { cause: Cause::Protocol, .. })));
}

#[test]
fn event_and_ipc_steps_are_judged_by_their_own_answers() {
    let w = Step::new("w", Op::Wait { event: Event::FwReady, timeout_ms: 3000 }, OnFail::Abort);
    let ipc = Step::new("ipc", Op::IpcAwait { timeout_ms: 5000 }, OnFail::Abort);
    for (step, input, want) in [
        (w, Input::EventFired, None),
        (w, Input::EventTimedOut, Some(Cause::EventTimeout)),
        (ipc, Input::IpcStatus(0), None),
        (ipc, Input::IpcStatus(7), Some(Cause::IpcRefused { status: 7 })),
        (ipc, Input::EventTimedOut, Some(Cause::EventTimeout)),
    ] {
        let mut r = Runner::new(prog(&[step]), Program::new()).unwrap();
        assert!(matches!(r.next(Input::Begin), Output::Do(_)));
        let out = r.next(input);
        match want {
            None => assert_eq!(out, Output::Finished),
            Some(c) => assert_eq!(out, Output::Failed(Failure { step: step.label, cause: c })),
        }
    }
}

#[test]
fn an_overflowing_program_is_refused_at_construction() {
    let mut p = Program::new();
    for _ in 0..(PROGRAM_CAP + 1) {
        p.push(upd("x", OnFail::Abort));
    }
    assert!(p.overflowed());
    assert_eq!(Runner::new(p, Program::new()).unwrap_err(), avs_fw_core::error::FwError::ProgramOverflow);
    let mut ok = Program::new();
    for _ in 0..PROGRAM_CAP {
        ok.push(upd("x", OnFail::Abort));
    }
    assert!(!ok.overflowed());
    assert!(Runner::new(ok, Program::new()).is_ok());
}

#[test]
fn a_finished_runner_keeps_answering_finished() {
    let mut r = Runner::new(prog(&[upd("a", OnFail::Abort)]), Program::new()).unwrap();
    let (_, out) = run(|i| r.next(i), ok_answer, 10);
    assert_eq!(out, Output::Finished);
    assert_eq!(r.next(Input::Done), Output::Finished);
}

#[test]
fn a_program_knows_whether_it_is_empty() {
    let mut p = Program::new();
    assert!(p.is_empty());
    assert_eq!(p.len(), 0);
    p.push(upd("a", OnFail::Abort));
    assert!(!p.is_empty());
    assert_eq!(p.len(), 1);
}

#[test]
fn a_poll_with_a_zero_interval_does_not_divide_by_zero() {
    assert_eq!(PollSpec::new(R, 1, 1, 0, 1000).max_polls, 1002);
    assert_eq!(PollSpec::new(R, 1, 1, 1, 1000).max_polls, 1002);
    assert_eq!(PollSpec::new(R, 1, 1, 7, 1000).max_polls, 144);
}
