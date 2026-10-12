// SPDX-License-Identifier: GPL-2.0-only
//! The bus gate, the wire encoding and the runner's own bounds.
mod common;
use da7219_core::bus::*;
use da7219_core::runner::*;
use i2c_core_msg_core::flags::I2C_M_RD;
use i2c_designware_master_core::setup::validate_message;

#[test]
fn the_codec_address_is_the_acpi_one_and_the_amplifiers_are_the_recovery_ones() {
    // ATLAS-PERIPHERALS-FROM-RECOVERY.md section 3.2: DA7219 0x1A, MAX98373 right 0x31, left 0x32
    assert_eq!(DA7219_I2C_ADDR, 0x1A);
    assert_eq!(MAX98373_RIGHT_ADDR, 0x31);
    assert_eq!(MAX98373_LEFT_ADDR, 0x32);
}

#[test]
fn the_gate_refuses_both_speaker_amplifiers_by_name_and_everything_but_the_codec() {
    assert_eq!(check_target(0x31), Err(BusError::SpeakerAmplifier { addr: 0x31 }));
    assert_eq!(check_target(0x32), Err(BusError::SpeakerAmplifier { addr: 0x32 }));
    assert_eq!(check_target(0x1A), Ok(()));
    // a valid address that is not the codec
    assert_eq!(check_target(0x2A), Err(BusError::NotTheCodec { addr: 0x2A }));
    // a reserved 7-bit address (general call) is not a valid target at all
    assert_eq!(check_target(0x00), Err(BusError::BadAddress { addr: 0x00 }));
    assert_eq!(check_target(0x7C), Err(BusError::BadAddress { addr: 0x7C }));
}

#[test]
fn no_wire_builder_will_put_a_message_on_an_amplifier() {
    for addr in [0x31u16, 0x32] {
        assert_eq!(wire::write_to(addr, 0x47, 0x80), Err(BusError::SpeakerAmplifier { addr }));
        assert_eq!(wire::write_pair_to(addr, 0xC4, 0, 0), Err(BusError::SpeakerAmplifier { addr }));
        assert_eq!(wire::read_to(addr, 0x6B, 1), Err(BusError::SpeakerAmplifier { addr }));
    }
}

#[test]
fn a_register_write_is_one_two_byte_message_and_a_read_is_a_write_then_a_read() {
    let w = wire::write(0x47, 0x80).unwrap();
    assert_eq!(w.messages().len(), 1);
    assert_eq!((w.msgs[0].addr, w.msgs[0].flags, w.msgs[0].len, w.out[0], w.out[1]), (0x1A, 0, 2, 0x47, 0x80));
    let r = wire::read(0xC2, 2).unwrap();
    assert_eq!(r.messages().len(), 2);
    assert_eq!((r.msgs[0].flags, r.msgs[0].len, r.out[0]), (0, 1, 0xC2));
    assert_eq!((r.msgs[1].flags, r.msgs[1].len), (I2C_M_RD, 2));
    let p = wire::write_pair(0xC4, 0xFF, 0xFF).unwrap();
    assert_eq!((p.msgs[0].len, p.out), (3, [0xC4, 0xFF, 0xFF]));
}

#[test]
fn the_read_pair_is_a_legal_designware_transaction() {
    let r = wire::read(0x6B, 1).unwrap();
    // the controller's own rule: same target, a restart between the write and the read (i2c-designware-master.c:826-855)
    assert_eq!(validate_message(r.messages(), 1, false), Ok(()));
}

fn plan_of(n: usize) -> Plan {
    let mut p = Plan::new();
    for _ in 0..n {
        p.push(Step::abort("s", Op::DelayMs(1)));
    }
    p
}

#[test]
fn a_plan_holds_exactly_its_cap_and_a_longer_one_is_refused_by_name() {
    assert_eq!(PLAN_CAP, 64);
    assert!(plan_of(PLAN_CAP).checked().is_ok());
    assert!(plan_of(PLAN_CAP + 1).overflowed());
    assert_eq!(plan_of(PLAN_CAP + 1).checked(), Err(da7219_core::PlanError::Overflow { cap: 64 }));
    assert!(Runner::new(plan_of(PLAN_CAP + 1)).is_err());
    let mut a = plan_of(40);
    a.extend(&plan_of(40));
    assert!(a.overflowed(), "extend carries the overflow");
}

fn poll_plan(max: u8, on_fail: OnFail) -> Plan {
    let mut p = Plan::new();
    p.push(Step::new("wait-lock", Op::Poll { reg: 0x25, mask: 0x80, want: 0x80, interval_ms: 50, max_polls: max }, on_fail));
    p.push(Step::abort("after", Op::Write { reg: 0x10, value: 1 }));
    p
}

#[test]
fn a_poll_stops_at_exactly_max_polls_with_a_named_failure() {
    let mut f = common::Fake::reset(); // PLL_SRM_STS reads 0: never locks
    let e = f.run(poll_plan(8, OnFail::Abort)).unwrap_err();
    assert_eq!(e, Failure { step: "wait-lock", cause: Cause::PollBound { reads: 8, last: 0 } });
    assert_eq!(f.poll_reads, 8, "exactly max_polls reads, no more");
    assert!(!f.steps.contains(&"after"), "an aborted plan issues nothing after the failure");
    // one read fewer is not enough to fail early
    let mut g = common::Fake::reset();
    g.regs[0x25] = 0x80;
    assert!(g.run(poll_plan(8, OnFail::Abort)).unwrap().is_none());
    assert_eq!(g.poll_reads, 1);
}

#[test]
fn a_tolerated_failure_is_remembered_and_the_plan_goes_on() {
    let mut f = common::Fake::reset();
    let t = f.run(poll_plan(3, OnFail::Tolerate)).unwrap();
    assert_eq!(t, Some(Failure { step: "wait-lock", cause: Cause::PollBound { reads: 3, last: 0 } }));
    assert!(f.steps.contains(&"after"));
}

#[test]
fn verify_names_the_register_the_mask_and_what_it_read() {
    let mut p = Plan::new();
    p.push(Step::abort("hp-l-muted", Op::Verify { reg: 0x6B, mask: 0x40, want: 0x40 }));
    let mut f = common::Fake::reset();
    f.regs[0x6B] = 0x00;
    assert_eq!(f.run(p).unwrap_err(), Failure { step: "hp-l-muted", cause: Cause::VerifyMismatch { reg: 0x6B, mask: 0x40, want: 0x40, got: 0x00 } });
}

#[test]
fn a_nak_and_a_protocol_error_are_named() {
    let mut f = common::Fake::reset();
    f.nak_at = Some("after");
    assert_eq!(f.run(poll_plan(3, OnFail::Tolerate)).unwrap_err(), Failure { step: "after", cause: Cause::BusNak });
    // a Value answered to a Write is a protocol error, and the runner stays failed
    let mut p = Plan::new();
    p.push(Step::abort("w", Op::Write { reg: 1, value: 1 }));
    let mut r = Runner::new(p).unwrap();
    assert!(matches!(r.next(Input::Begin), Output::Do(_)));
    let bad = r.next(Input::Value(0));
    assert_eq!(bad, Output::Failed(Failure { step: "w", cause: Cause::Protocol }));
    assert_eq!(r.next(Input::Done), bad, "a failed runner stays failed");
    // not begun
    let mut r2 = Runner::new(Plan::new()).unwrap();
    assert!(matches!(r2.next(Input::Done), Output::Failed(Failure { step: "begin", cause: Cause::Protocol })));
}

#[test]
fn the_wait_bound_of_a_step_is_its_delay_or_its_poll_budget() {
    assert_eq!(Step::abort("d", Op::DelayMs(40)).max_wait_ms(), 40);
    assert_eq!(Step::abort("p", Op::Poll { reg: 0, mask: 0, want: 0, interval_ms: 50, max_polls: 8 }).max_wait_ms(), 400);
    assert_eq!(Step::abort("w", Op::Write { reg: 0, value: 0 }).max_wait_ms(), 0);
    let mut p = Plan::new();
    p.push(Step::abort("d", Op::DelayMs(40)));
    p.push(Step::abort("p", Op::Poll { reg: 0, mask: 0, want: 0, interval_ms: 10, max_polls: 5 }));
    assert_eq!(p.max_wait_ms(), 90);
}
