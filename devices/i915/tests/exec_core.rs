// SPDX-License-Identifier: GPL-2.0-only
//! The executor's core guarantees, on a simulated chip: refusal BY NAME and counted (not performed), bounded polls that yield, forcewake
//! held around GT access, the durable "begun" record, and every plan type running through the same guard.
//! These run in BOTH feature modes: the "shipped default" grant is built explicitly (`Executor::with_grant`) so the refusals that the
//! default build must make are proved inside a feature-ON test run too.
mod exec_common;
use exec_common::*;
use i915_common_core::{I915Error, Reg};
use i915_device::exec::allow::GgttWindow;
use i915_device::exec::journal::{check_previous_boot, parse_record, Phase, StepId, StepRecord};
use i915_device::exec::{ExecError, Executor};
use i915_devinfo_core::lookup;
use i915_gt_core::{Effect, PollPlan, RmwPlan, WritePlan};
use i915_uncore_core::WRITE_ALLOWLIST;

/// The shipped default: the three forcewake set registers.
static FW_ONLY: [Reg; 3] = WRITE_ALLOWLIST;
const ELSP: Reg = Reg::new(0x2230);
const RING_MODE: Reg = Reg::new(0x229c);

fn kbl() -> i915_devinfo_core::PlatformDescriptor {
    lookup(0x591c).unwrap()
}

fn shipped(chip: &Chip) -> Executor<Chip> {
    Executor::with_grant(chip.clone(), &kbl(), &FW_ONLY, None).unwrap()
}

fn awake_wr(ex: &mut Executor<Chip>, r: Reg, v: u32) -> Result<(), ExecError> {
    let d = ex.domains_for([r]);
    ex.awake(d, |ex| ex.wr(r, v))
}

// ---- refusal by name, counted, not performed ----------------------------------------------------------------------------------

#[test]
fn a_register_off_the_allow_list_is_refused_by_name_counted_and_never_performed() {
    let chip = Chip::new();
    let mut ex = shipped(&chip);
    let e = awake_wr(&mut ex, ELSP, 0xdead).unwrap_err();
    assert_eq!(e, ExecError::Hw(I915Error::WriteRefused { offset: 0x2230, width: 4 }));
    assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
    assert!(chip.all_writes().is_empty(), "the refused write never reached the chip");
    let s = ex.stats();
    assert_eq!(s.refused, 1);
    let first = s.first_refusal.expect("the first refusal is recorded");
    assert_eq!((first.offset, first.width, first.value), (0x2230, 4, 0xdead));
    // a second and third refusal count
    let _ = awake_wr(&mut ex, RING_MODE, 1);
    let _ = awake_wr(&mut ex, ELSP, 2);
    assert_eq!(ex.stats().refused, 3);
    assert_eq!(ex.stats().first_refusal.unwrap().offset, 0x2230, "the FIRST stays the first");
    assert!(chip.all_writes().is_empty());
}

#[test]
fn the_forcewake_registers_stay_writable_so_the_protocol_works() {
    let chip = Chip::new();
    let mut ex = shipped(&chip);
    let d = ex.domains_for([ELSP]);
    assert_ne!(d, 0, "the render engine registers are in a forcewake range");
    ex.awake(d, |_| Ok(())).unwrap();
    let fw = chip.0.borrow().fw_writes.clone();
    assert_eq!(fw.len(), 2, "one set, one clear");
    assert_eq!(ex.stats().refused, 0);
    assert!(ex.stats().forwarded >= 2);
}

#[test]
fn a_gt_register_access_without_its_forcewake_is_refused() {
    let chip = Chip::new();
    let mut ex = shipped(&chip);
    let e = ex.rd(ELSP).unwrap_err();
    assert_eq!(e.name(), "FORCEWAKE-NOT-HELD");
    let e = ex.wr(ELSP, 1).unwrap_err();
    assert_eq!(e.name(), "FORCEWAKE-NOT-HELD", "the forcewake check comes before the allow-list: a GT write needs the domain too");
    assert_eq!(ex.stats().refused, 0, "that is not an allow-list refusal");
    // after awake() the same read works and the domain is released and the release observed
    let d = ex.domains_for([ELSP]);
    assert_eq!(ex.awake(d, |ex| ex.rd(ELSP)).unwrap(), 0);
    assert_eq!(chip.0.borrow().doms[0].ack_val & 1, 0, "RENDER released");
}

#[test]
fn a_grant_without_the_forcewake_registers_is_refused_at_construction() {
    static NO_FW: [Reg; 1] = [Reg::new(0x2230)];
    let e = Executor::with_grant(Chip::new(), &kbl(), &NO_FW, None).err().unwrap();
    assert_eq!(e.name(), "BAD-VALUE");
    static ONE_MISSING: [Reg; 2] = [WRITE_ALLOWLIST[0], WRITE_ALLOWLIST[1]];
    assert!(Executor::with_grant(Chip::new(), &kbl(), &ONE_MISSING, None).is_err());
}

#[test]
fn a_closed_ggtt_window_refuses_every_entry_store_by_name() {
    let chip = Chip::new();
    let mut ex = shipped(&chip);
    let e = ex.ggtt_store(0x80_0000, 0x1001).unwrap_err();
    assert_eq!(e, ExecError::GgttStoreRefused { offset: 0x80_0000 });
    assert_eq!(e.name(), "GGTT-STORE-NOT-ALLOWED");
    assert_eq!(chip.0.borrow().ggtt_writes, 0);
    assert_eq!(ex.stats().refused, 1);
    assert_eq!(ex.stats().ggtt_stores, 0);
}

#[test]
fn an_open_ggtt_window_stores_high_dword_first_and_only_inside_itself() {
    let chip = Chip::new();
    let mut ex = Executor::with_grant(chip.clone(), &kbl(), &FW_ONLY, Some(GgttWindow { base: 0x80_0000, bytes: 0x100 })).unwrap();
    ex.ggtt_store(0x80_0008, 0x0000_0007_1234_5003).unwrap();
    assert_eq!(chip.ggtt_entry(0x80_0008), 0x0000_0007_1234_5003);
    assert_eq!(chip.0.borrow().ggtt_write_offsets, [0x80_000c, 0x80_0008], "high dword first: the PRESENT bit (low dword) lands last");
    for off in [0x80_0100, 0x7f_fff8, 0x80_0004, 0x80_00fc] {
        assert_eq!(ex.ggtt_store(off, 1).unwrap_err().name(), "GGTT-STORE-NOT-ALLOWED", "{off:#x}");
    }
    assert_eq!(chip.0.borrow().ggtt_writes, 2, "nothing past the first entry reached the chip");
    // the window can be narrowed, never widened
    assert!(ex.set_ggtt_window(GgttWindow { base: 0x80_0000, bytes: 0x80 }).is_ok());
    assert!(ex.set_ggtt_window(GgttWindow { base: 0x80_0000, bytes: 0x200 }).is_err());
    assert!(ex.set_ggtt_window(GgttWindow { base: 0x70_0000, bytes: 0x10 }).is_err());
}

// ---- bounded polls ------------------------------------------------------------------------------------------------------------

#[test]
fn a_poll_stops_at_max_polls_and_yields_between_reads() {
    let chip = Chip::new();
    let mut ex = shipped(&chip);
    chip.set(0x101008, 0); // never becomes 1
    let e = ex.poll(Reg::new(0x101008), 1, 1, 1_000_000, 7, "TEST").unwrap_err();
    assert_eq!(e.name(), "POLL-TIMEOUT");
    assert!(matches!(e, ExecError::PollTimeout { polls: 7, mask: 1, want: 1, got: 0, what: "TEST", .. }), "{e:?}");
    assert_eq!(chip.0.borrow().relax_calls, 6, "seven reads, a yield between each pair - never a spin");
    assert_eq!(ex.stats().polls, 7);
}

#[test]
fn a_poll_stops_at_its_deadline_before_max_polls() {
    let chip = Chip::new();
    chip.0.borrow_mut().deadline_after_reads = 3;
    let mut ex = shipped(&chip);
    let e = ex.poll(Reg::new(0x101008), 1, 1, 700, 100_000, "TEST").unwrap_err();
    assert!(matches!(e, ExecError::PollTimeout { polls: 3, .. }), "{e:?}");
}

#[test]
fn the_condition_is_checked_before_either_bound() {
    let chip = Chip::new();
    chip.set(0x101008, 1);
    let mut ex = shipped(&chip);
    assert_eq!(ex.poll(Reg::new(0x101008), 1, 1, 1000, 1, "TEST").unwrap(), 1, "satisfied on the very read that exhausts max_polls");
    assert_eq!(chip.0.borrow().relax_calls, 0);
}

#[test]
fn a_zero_poll_bound_and_an_all_ones_read_are_refused() {
    let chip = Chip::new();
    let mut ex = shipped(&chip);
    assert_eq!(ex.poll(Reg::new(0x101008), 1, 1, 1000, 0, "TEST").unwrap_err().name(), "BAD-VALUE");
    chip.set(0x101008, u32::MAX);
    assert_eq!(ex.poll(Reg::new(0x101008), 1, 0, 1000, 5, "TEST").unwrap_err().name(), "MMIO-UNRELIABLE", "a dead BAR is not an answer");
}

#[test]
fn delays_sleep_in_whole_milliseconds_and_zero_is_free() {
    let chip = Chip::new();
    let mut ex = shipped(&chip);
    for us in [0, 1, 50, 1000, 1001, 2500] {
        ex.delay_us(us);
    }
    assert_eq!(chip.0.borrow().park_ms, [1, 1, 1, 2, 3], "a delay is a floor: 50 us parks 1 ms; 0 parks nothing");
}

// ---- gt-core effects ----------------------------------------------------------------------------------------------------------

static GRANT_TEST: [Reg; 6] = [WRITE_ALLOWLIST[0], WRITE_ALLOWLIST[1], WRITE_ALLOWLIST[2], Reg::new(0x2230), Reg::new(0x229c), Reg::new(0x20d0)];

#[test]
fn gt_effects_run_in_order_with_rmw_semantics_and_a_bounded_poll() {
    let chip = Chip::new();
    chip.set(0x229c, 0x00f0);
    let mut ex = Executor::with_grant(chip.clone(), &kbl(), &GRANT_TEST, None).unwrap();
    let plan = [
        Effect::Write(WritePlan { reg: ELSP, value: 7 }),
        Effect::Rmw(RmwPlan { reg: RING_MODE, clear: 0x0f0, set: 0x003, skip_if_unchanged: false }),
        Effect::Rmw(RmwPlan { reg: RING_MODE, clear: 0, set: 0x003, skip_if_unchanged: true }), // already 0x3: skipped
        Effect::PostingRead(RING_MODE),
        Effect::DelayUs(50),
        Effect::Poll(PollPlan { reg: RING_MODE, mask: 3, want: 3, timeout_us: 700, max_polls: 4, what: "mode" }),
    ];
    ex.run_gt_effects(&plan).unwrap();
    assert_eq!(chip.all_writes(), [(0x2230, 7), (0x229c, 0x003)]);
    assert_eq!(ex.stats().posting_reads, 1);
    assert_eq!(chip.0.borrow().park_ms, [1]);
    // a poll that never holds is a named timeout at its own max_polls
    let never = [Effect::Poll(PollPlan { reg: RING_MODE, mask: 0xc, want: 0xc, timeout_us: 700, max_polls: 3, what: "never" })];
    let e = ex.run_gt_effects(&never).unwrap_err();
    assert!(matches!(e, ExecError::PollTimeout { what: "never", polls: 3, .. }), "{e:?}");
}

#[test]
fn a_plan_stops_at_its_first_refused_write_and_does_nothing_after() {
    let chip = Chip::new();
    let mut ex = Executor::with_grant(chip.clone(), &kbl(), &GRANT_TEST, None).unwrap();
    let plan = [
        Effect::Write(WritePlan { reg: ELSP, value: 1 }),
        Effect::Write(WritePlan { reg: Reg::new(0x2098), value: 2 }), // RING_HWSTAM: not granted
        Effect::Write(WritePlan { reg: RING_MODE, value: 3 }),
    ];
    let e = ex.run_gt_effects(&plan).unwrap_err();
    assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
    assert_eq!(chip.all_writes(), [(0x2230, 1)], "the write after the refusal was not made");
    assert_eq!(ex.stats().refused, 1);
}

// ---- the durable "begun" record ------------------------------------------------------------------------------------------------

#[test]
fn the_begun_record_is_printed_before_the_first_write_of_the_step() {
    let chip = Chip::new();
    let mut ex = Executor::with_grant(chip.clone(), &kbl(), &GRANT_TEST, None).unwrap();
    let mut log = Lines::default();
    let seen_before_write = ex
        .step(&mut log, StepId::ELSP_SUBMIT, "ELSP-SUBMIT", |ex, log| {
            let last = log.0.last().cloned().unwrap_or_default();
            assert!(chip.all_writes().is_empty());
            ex.run_gt_effects(&[Effect::Write(WritePlan { reg: ELSP, value: 9 })])?;
            Ok(last)
        })
        .unwrap();
    assert_eq!(seen_before_write, "[I915-STEP begun id=0x3003 name=ELSP-SUBMIT seq=1]");
    assert_eq!(log.0.last().unwrap(), "[I915-STEP done id=0x3003 name=ELSP-SUBMIT seq=1]");
    assert_eq!(ex.open_step(), None);
}

#[test]
fn a_failed_step_says_so_by_name_and_steps_cannot_nest() {
    let chip = Chip::new();
    let mut ex = shipped(&chip);
    let mut log = Lines::default();
    let r: Result<(), _> = ex.step(&mut log, StepId::ENGINE_RESET, "ENGINE-RESET", |ex, log| {
        let nested: Result<(), _> = ex.step(log, StepId::ELSP_SUBMIT, "X", |_, _| Ok(()));
        assert_eq!(nested.unwrap_err(), ExecError::StepNested { open: 0x3004, new: 0x3003 });
        awake_wr(ex, ELSP, 1)
    });
    assert_eq!(r.unwrap_err().name(), "WRITE-NOT-ALLOWED");
    assert_eq!(log.0.last().unwrap(), "[I915-STEP failed id=0x3004 name=ENGINE-RESET seq=1 why=WRITE-NOT-ALLOWED]");
    assert_eq!(ex.open_step(), None, "a failed step is closed");
}

#[test]
fn records_parse_and_a_begun_without_an_end_refuses_the_next_boot() {
    assert_eq!(parse_record("[I915-STEP begun id=0x3003 name=ELSP-SUBMIT seq=7]"), Some(StepRecord { id: StepId(0x3003), phase: Phase::Begun }));
    assert_eq!(parse_record("[I915-STEP done id=0x2001 name=GGTT-BIND seq=2]"), Some(StepRecord { id: StepId(0x2001), phase: Phase::Done }));
    assert_eq!(parse_record("[I915-STEP failed id=0x3004 name=X seq=1 why=ENGINE-HUNG]"), Some(StepRecord { id: StepId(0x3004), phase: Phase::Failed }));
    for junk in ["", "[I915-STEP", "[I915-STEP begun id=3003]", "[I915-STEP maybe id=0x3003 ]", "[I915-EXEC seqno=1]", "[I915-STEP begun id=0xzz]"] {
        assert_eq!(parse_record(junk), None, "{junk:?}");
    }
    let ok = ["[I915-STEP begun id=0x3003 name=A seq=1]", "noise", "[I915-STEP done id=0x3003 name=A seq=1]"];
    assert!(check_previous_boot(ok.iter().copied()).is_ok());
    let failed = ["[I915-STEP begun id=0x3004 name=A seq=1]", "[I915-STEP failed id=0x3004 name=A seq=1 why=ENGINE-HUNG]"];
    assert!(check_previous_boot(failed.iter().copied()).is_ok(), "a step that failed BY NAME ended; the device said so");
    let died = ["[I915-STEP begun id=0x3003 name=A seq=1]", "[I915-STEP done id=0x3003 name=A seq=1]", "[I915-STEP begun id=0x3004 name=B seq=2]"];
    let e = check_previous_boot(died.iter().copied()).unwrap_err();
    assert_eq!(e, ExecError::PreviousStepNotDone { id: 0x3004 });
    assert_eq!(e.name(), "PREVIOUS-STEP-NOT-DONE");
    // a later boot that completed the same step clears it
    let recovered = ["[I915-STEP begun id=0x3004 name=B seq=2]", "[I915-STEP begun id=0x3004 name=B seq=1]", "[I915-STEP done id=0x3004 name=B seq=1]"];
    assert!(check_previous_boot(recovered.iter().copied()).is_ok());
}

// ---- display plans meet the same guard ----------------------------------------------------------------------------------------

mod display {
    use super::*;
    use i915_display_clock_core::pcode::{PcodeOp, PcodeRequest};
    use i915_display_clock_core::plan::{OnTimeout, PollPlan as ClockPoll, Transaction, WritePlan as ClockPlan};
    use i915_display_clock_core::Handshake;

    const CDCLK_CTL: u32 = 0x46000; // intel_cdclk_regs.h: CDCLK_CTL

    #[test]
    fn a_display_clock_plan_is_refused_at_its_first_write_by_name() {
        let chip = Chip::new();
        let mut ex = shipped(&chip);
        let mut plan = ClockPlan::new();
        plan.write(CDCLK_CTL, 0x1234).unwrap();
        plan.write(CDCLK_CTL + 4, 1).unwrap();
        let e = ex.run_clock_plan(&plan).unwrap_err();
        assert_eq!(e, ExecError::Hw(I915Error::WriteRefused { offset: CDCLK_CTL, width: 4 }));
        assert!(chip.all_writes().is_empty());
        assert_eq!(ex.stats().refused, 1);
    }

    #[test]
    fn a_display_clock_poll_is_bounded_by_its_budget_and_tolerate_continues() {
        let chip = Chip::new();
        let mut ex = shipped(&chip);
        let p = ClockPoll::wait_set(Handshake::PcodeMailboxReady, Reg::new(0x46010), 1, 3, OnTimeout::Fail).with_max_polls(5);
        let mut plan = ClockPlan::new();
        plan.poll(p).unwrap();
        let e = ex.run_clock_plan(&plan).unwrap_err();
        assert_eq!(e.name(), "HANDSHAKE-TIMEOUT");
        assert_eq!(chip.0.borrow().reads.iter().filter(|r| **r == 0x46010).count(), 5, "exactly max_polls samples");
        assert_eq!(chip.0.borrow().relax_calls, 4);
        let mut plan = ClockPlan::new();
        plan.poll(p.with_max_polls(2)).unwrap();
        let tolerant = ClockPoll { on_timeout: OnTimeout::Tolerate, ..p.with_max_polls(2) };
        let mut t = ClockPlan::new();
        t.poll(tolerant).unwrap();
        t.posting_read(0x46010).unwrap();
        ex.run_clock_plan(&t).unwrap();
    }

    #[test]
    fn a_failed_transaction_runs_its_rollback_and_returns_the_forward_error() {
        let chip = Chip::new();
        let mut ex = shipped(&chip);
        let mut tx = Transaction::new();
        tx.forward.write(CDCLK_CTL, 1).unwrap();
        tx.rollback.write(CDCLK_CTL, 2).unwrap();
        let e = ex.run_clock_transaction(&tx).unwrap_err();
        assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
        assert_eq!(ex.stats().refused, 2, "the forward write AND the rollback write were both attempted and both refused by name");
    }

    static PCODE_GRANT: [Reg; 6] = [WRITE_ALLOWLIST[0], WRITE_ALLOWLIST[1], WRITE_ALLOWLIST[2], Reg::new(0x138128), Reg::new(0x13812c), Reg::new(0x138124)];

    fn pcode_chip(reply: u32) -> Chip {
        let chip = Chip::new();
        chip.0.borrow_mut().on_write = Some(Box::new(move |regs, reg, _v| {
            if reg == 0x138124 {
                regs.insert(0x138124, 0); // READY clears and the status byte is 0 (success)
                regs.insert(0x138128, reply);
            }
        }));
        chip
    }

    #[test]
    fn a_pcode_request_is_acknowledged_when_the_reply_matches() {
        let req = PcodeRequest::cdclk_prepare();
        let chip = pcode_chip(req.reply);
        let mut ex = Executor::with_grant(chip.clone(), &kbl(), &PCODE_GRANT, None).unwrap();
        let mut plan = ClockPlan::new();
        plan.pcode(PcodeOp::Request(req)).unwrap();
        ex.run_clock_plan(&plan).unwrap();
        assert_eq!(chip.all_writes().len(), 3, "one attempt: data, data1, mailbox");
    }

    #[test]
    fn a_pcode_request_that_is_never_acknowledged_resends_only_max_attempts_times() {
        let req = PcodeRequest::cdclk_prepare().with_max_attempts(4);
        let chip = pcode_chip(0);
        let mut ex = Executor::with_grant(chip.clone(), &kbl(), &PCODE_GRANT, None).unwrap();
        let mut plan = ClockPlan::new();
        plan.pcode(PcodeOp::Request(req)).unwrap();
        let e = ex.run_clock_plan(&plan).unwrap_err();
        assert_eq!(e.name(), "PCODE-NO-ACK");
        assert_eq!(chip.all_writes().len(), 12, "4 attempts x 3 writes, then it stopped");
    }

    #[test]
    fn a_pcode_request_is_refused_by_name_when_the_mailbox_is_not_granted() {
        let chip = pcode_chip(1);
        let mut ex = shipped(&chip);
        let mut plan = ClockPlan::new();
        plan.pcode(PcodeOp::Request(PcodeRequest::cdclk_prepare())).unwrap();
        assert_eq!(ex.run_clock_plan(&plan).unwrap_err().name(), "WRITE-NOT-ALLOWED");
        assert!(chip.all_writes().is_empty());
    }
}

mod link {
    use super::*;
    use i915_display_link_core::plan::{Effect as LinkEffect, PollPlan as LinkPoll};

    static LINK_GRANT: [Reg; 4] = [WRITE_ALLOWLIST[0], WRITE_ALLOWLIST[1], WRITE_ALLOWLIST[2], Reg::new(0x64000)];

    #[test]
    fn link_effects_are_refused_by_name_without_a_grant_and_run_with_one() {
        let chip = Chip::new();
        let mut ex = shipped(&chip);
        let e = ex.run_link_effects(&[LinkEffect::Write { reg: Reg::new(0x64000), value: 1 }]).unwrap_err();
        assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
        let mut ex = Executor::with_grant(chip.clone(), &kbl(), &LINK_GRANT, None).unwrap();
        chip.set(0x64000, 0xf0);
        ex.run_link_effects(&[LinkEffect::ClearIfSet { reg: Reg::new(0x64000), bits: 0x30 }, LinkEffect::ClearIfSet { reg: Reg::new(0x64000), bits: 0x01 }]).unwrap();
        assert_eq!(chip.all_writes(), [(0x64000, 0xc0)], "the second ClearIfSet found its bit clear and wrote nothing");
    }

    #[test]
    fn a_link_poll_reads_at_most_max_polls_times_sleeping_between() {
        let chip = Chip::new();
        let mut ex = shipped(&chip);
        let p = LinkPoll::with_timeout(Reg::new(0x64010), 1, 1, 500, 2000, "AUX busy");
        assert_eq!(p.max_polls, 5);
        let e = ex.run_link_effects(&[LinkEffect::Poll(p)]).unwrap_err();
        assert!(matches!(e, ExecError::PollTimeout { what: "AUX busy", polls: 5, .. }), "{e:?}");
        assert_eq!(chip.0.borrow().park_ms.len(), 4, "a sleep between each pair of reads");
    }
}
