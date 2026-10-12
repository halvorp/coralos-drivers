// SPDX-License-Identifier: GPL-2.0-only
//! The bounded-wait vocabulary: a poll ends at its read cap whatever the register does.
mod common;
use i915_common_core::Reg;
use i915_display_link_core::plan::*;

fn plan(interval: u32, timeout: u32) -> PollPlan {
    PollPlan::with_timeout(Reg::new(0x1000), 0x80, 0x80, interval, timeout, "test wait")
}

#[test]
fn max_polls_is_timeout_over_interval_plus_the_first_read() {
    // poll_timeout_us(): one read, then one per sleep until the timeout passes
    assert_eq!(plan(10_000, 5_000_000).max_polls, 501);
    assert_eq!(plan(10, 2_000).max_polls, 201);
    assert_eq!(plan(50, 10_000).max_polls, 201);
    assert_eq!(plan(100, 99).max_polls, 1);
    assert_eq!(plan(0, 5).max_polls, 6); // an interval of 0 would make the cap infinite: treated as 1
}

#[test]
fn judge_is_satisfied_again_or_timed_out() {
    let p = plan(10, 30); // max_polls 4
    assert_eq!(p.judge(1, 0x80), PollStep::Satisfied);
    assert_eq!(p.judge(1, 0xFF), PollStep::Satisfied); // only the mask matters
    assert_eq!(p.judge(1, 0), PollStep::Again);
    assert_eq!(p.judge(3, 0), PollStep::Again);
    match p.judge(4, 0x7f) {
        PollStep::TimedOut(t) => assert_eq!((t.what, t.reg, t.mask, t.want, t.got, t.polls), ("test wait", Reg::new(0x1000), 0x80, 0x80, 0x7f, 4)),
        other => panic!("{other:?}"),
    }
    // a satisfying read on the very last allowed poll still succeeds
    assert_eq!(p.judge(4, 0x80), PollStep::Satisfied);
}

#[test]
fn run_stops_at_the_cap_even_if_the_register_never_changes() {
    let p = plan(10, 1_000); // 101 reads
    let mut reads = 0u32;
    let mut slept = 0u32;
    let r = p.run(|| { reads += 1; 0 }, |us| slept += us);
    assert_eq!(reads, 101);
    assert_eq!(slept, 100 * 10); // sleeps BETWEEN reads, none after the last
    let t = r.unwrap_err();
    assert_eq!(t.polls, 101);
    assert_eq!(common::judge_all_unsatisfied(&p), 101);
}

#[test]
fn run_returns_the_satisfying_value_and_stops_reading() {
    let p = plan(10, 1_000);
    let mut reads = 0;
    let r = p.run(|| { reads += 1; if reads == 7 { 0x8F } else { 0 } }, |_| {});
    assert_eq!(r, Ok(0x8F));
    assert_eq!(reads, 7);
}

#[test]
fn written_regs_lists_every_writer_and_no_reader() {
    let p = vec![
        Effect::Write { reg: Reg::new(1), value: 0 },
        Effect::PostingRead { reg: Reg::new(2) },
        Effect::Rmw { reg: Reg::new(3), clear: 0, set: 0 },
        Effect::ClearIfSet { reg: Reg::new(4), bits: 1 },
        Effect::Poll(plan(1, 1)),
        Effect::DelayUs(5),
    ];
    assert_eq!(written_regs(&p), vec![Reg::new(1), Reg::new(3), Reg::new(4)]);
}
