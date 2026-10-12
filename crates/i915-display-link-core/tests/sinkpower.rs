// SPDX-License-Identifier: GPL-2.0-only
//! intel_dp_set_power(), intel_dp.c:3778-3821.
use i915_display_link_core::dpcd::*;
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::sinkpower::*;

fn ok1() -> Result<DpcdOk, DpcdFail> {
    Ok(DpcdOk { len: 1, data: [1; 20] })
}
fn bad() -> Result<DpcdOk, DpcdFail> {
    Err(DpcdFail { first: DpcdErr::Nack { reply: 1 }, attempts: 32 })
}

fn drive(sp: &mut SetPower, results: &mut Vec<Result<DpcdOk, DpcdFail>>) -> (Vec<PowerStep>, Result<(), SetPowerError>) {
    let mut log = vec![];
    let mut reply = None;
    for _ in 0..50 {
        let s = sp.step(reply.take());
        log.push(s);
        match s {
            PowerStep::Write(_) => reply = Some(results.remove(0)),
            PowerStep::SleepMs(_) => reply = None,
            PowerStep::Done(r) => return (log, r),
        }
    }
    panic!("set_power did not finish");
}

#[test]
fn d0_succeeds_first_try() {
    let mut sp = SetPower::new(DP_SET_POWER_D0, 0x14).unwrap();
    let (log, r) = drive(&mut sp, &mut vec![ok1()]);
    assert_eq!(r, Ok(()));
    assert_eq!(log[0], PowerStep::Write(DpcdRequest::write(0x600, &[1])));
}

#[test]
fn d0_retries_with_a_1ms_sleep_up_to_three_times() {
    let mut sp = SetPower::new(DP_SET_POWER_D0, 0x14).unwrap();
    let (log, r) = drive(&mut sp, &mut vec![bad(), bad(), ok1()]);
    assert_eq!(r, Ok(()));
    assert_eq!(log.iter().filter(|s| matches!(s, PowerStep::SleepMs(1))).count(), 2);
    assert_eq!(log.iter().filter(|s| matches!(s, PowerStep::Write(_))).count(), 3);
    // three failures: given up, named, after exactly three writes
    let mut sp = SetPower::new(DP_SET_POWER_D0, 0x14).unwrap();
    let (log, r) = drive(&mut sp, &mut vec![bad(), bad(), bad()]);
    assert_eq!(log.iter().filter(|s| matches!(s, PowerStep::Write(_))).count(), SET_POWER_D0_TRIES as usize);
    let e = r.unwrap_err();
    assert!(matches!(e, SetPowerError::Failed { mode: 1, .. }));
    assert_eq!(e.name(), "SET-POWER-FAILED");
}

#[test]
fn a_short_ack_counts_as_a_failed_try() {
    let mut sp = SetPower::new(DP_SET_POWER_D0, 0x14).unwrap();
    let zero = Ok(DpcdOk { len: 0, data: [0; 20] });
    let (_, r) = drive(&mut sp, &mut vec![zero.clone(), zero.clone(), zero]);
    assert!(r.is_err());
}

#[test]
fn d3_is_a_single_write() {
    let mut sp = SetPower::new(DP_SET_POWER_D3, 0x14).unwrap();
    let (log, r) = drive(&mut sp, &mut vec![bad()]);
    assert!(matches!(r, Err(SetPowerError::Failed { mode: 2, .. })));
    assert_eq!(log.len(), 2); // one write, then done
    assert_eq!(log[0], PowerStep::Write(DpcdRequest::write(0x600, &[2])));
}

#[test]
fn a_pre_dpcd_11_sink_is_left_alone() {
    let e = SetPower::new(DP_SET_POWER_D0, 0x10).err().unwrap();
    assert_eq!(e, SetPowerError::NotApplicable);
    assert_eq!(e.name(), "SET-POWER-NOT-APPLICABLE");
    assert!(SetPower::new(DP_SET_POWER_D0, DP_DPCD_REV_11).is_ok());
}

#[test]
fn a_short_ack_is_reported_with_its_length_and_a_missing_reply_is_out_of_sequence() {
    let mut sp = SetPower::new(DP_SET_POWER_D3, 0x14).unwrap();
    let _ = sp.step(None);
    match sp.step(Some(Ok(DpcdOk { len: 0, data: [0; 20] }))) {
        PowerStep::Done(Err(SetPowerError::Failed { mode: 2, last })) => {
            assert_eq!(last.first, DpcdErr::Short { got: 0, want: 1 });
            assert_eq!(last.attempts, 1);
        }
        other => panic!("{other:?}"),
    }
    let mut sp = SetPower::new(DP_SET_POWER_D3, 0x14).unwrap();
    let _ = sp.step(None);
    let r = sp.step(None);
    assert_eq!(r, PowerStep::Done(Err(SetPowerError::OutOfSequence)));
    assert_eq!(SetPowerError::OutOfSequence.name(), "SET-POWER-OUT-OF-SEQUENCE");
}
