// SPDX-License-Identifier: GPL-2.0-only
//! `drm_dp_dpcd_access()` retry behaviour (drm_dp_helper.c:590-662) and the read probe (:664-740).
#![allow(unused_imports)]
mod common;
use common::*;
use i915_display_link_core::aux::*;
use i915_display_link_core::dpcd::*;
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::regs::*;
use i915_vbt_core::child::AuxCh;

fn read_req(addr: u32, len: usize) -> DpcdXfer {
    DpcdXfer::new(AuxCh::A, DpcdRequest::read(addr, len))
}

#[test]
fn a_read_probes_dp_training_pattern_set_first() {
    let mut hw = AuxHw::new(AuxCh::A, |n, _| if n == 0 { ack_read(&[0x00]) } else { ack_read(&[0x14, 0x0a]) });
    let ok = run_dpcd(&mut hw, &mut read_req(0x000, 2)).unwrap();
    assert_eq!(ok.len, 2);
    assert_eq!(&ok.data[..2], &[0x14, 0x0a]);
    assert_eq!(hw.sends, 2);
    // probe: native read, address 0x102, one byte => header 90 01 02 00
    assert_eq!(hw.sent[0], vec![0x90, 0x01, 0x02, 0x00]);
    assert_eq!(hw.sent[1], vec![0x90, 0x00, 0x00, 0x01]);
}

#[test]
fn a_write_is_not_probed() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| ack_write());
    let mut x = DpcdXfer::new(AuxCh::A, DpcdRequest::write(DP_SET_POWER, &[DP_SET_POWER_D0]));
    let ok = run_dpcd(&mut hw, &mut x).unwrap();
    assert_eq!(ok.len, 1);
    assert_eq!(hw.sends, 1);
    assert_eq!(hw.sent[0], vec![0x80, 0x06, 0x00, 0x00, 0x01]);
}

#[test]
fn a_nacking_sink_is_tried_exactly_32_times_with_a_500us_sleep_between() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE, vec![0x10])); // NACK
    let mut x = DpcdXfer::new(AuxCh::A, DpcdRequest::write(DP_SET_POWER, &[1]));
    let e = run_dpcd(&mut hw, &mut x).unwrap_err();
    assert_eq!(hw.sends, 32);
    assert_eq!(DPCD_ACCESS_RETRIES, 32);
    assert_eq!(e.attempts, 32);
    assert_eq!(e.first, DpcdErr::Nack { reply: DP_AUX_NATIVE_REPLY_NACK });
    assert_eq!(e.name(), "DPCD-EIO");
    assert_eq!(hw.sleeps_us, vec![(500, 600); 31]); // AUX_RETRY_INTERVAL .. +100 before every retry
}

#[test]
fn a_deferring_sink_is_also_eio() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE, vec![0x20])); // DEFER
    let mut x = DpcdXfer::new(AuxCh::A, DpcdRequest::write(DP_SET_POWER, &[1]));
    let e = run_dpcd(&mut hw, &mut x).unwrap_err();
    assert_eq!(e.first, DpcdErr::Nack { reply: DP_AUX_NATIVE_REPLY_DEFER });
}

#[test]
fn timeouts_skip_the_retry_sleep() {
    // every AUX transfer ends -ETIMEDOUT (5 sends each): 32 attempts, no 500 us sleeps
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_TIME_OUT_ERROR, vec![]));
    let mut x = DpcdXfer::new(AuxCh::A, DpcdRequest::write(DP_SET_POWER, &[1]));
    let e = run_dpcd(&mut hw, &mut x).unwrap_err();
    assert_eq!(hw.sends, 32 * 5);
    assert!(hw.sleeps_us.is_empty());
    assert!(matches!(e.first, DpcdErr::Aux(f) if f.errno == AuxErrno::TimedOut));
    assert_eq!(e.name(), "DPCD-AUX-FAILED");
}

#[test]
fn the_first_error_is_the_one_reported() {
    // attempt 0: NACK; every later attempt: short ACK (EPROTO). The report keeps the first.
    let mut hw = AuxHw::new(AuxCh::A, |n, _| if n == 0 { (DP_AUX_CH_CTL_DONE, vec![0x10]) } else { (DP_AUX_CH_CTL_DONE, vec![0x00, 0x00]) });
    let m = DpcdRequest::write(0x100, &[1, 2, 3]);
    let e = run_dpcd(&mut hw, &mut DpcdXfer::new(AuxCh::A, m)).unwrap_err();
    assert_eq!(e.first, DpcdErr::Nack { reply: DP_AUX_NATIVE_REPLY_NACK });
}

#[test]
fn a_short_ack_is_eproto_with_the_counts() {
    // read of 4 bytes, the sink ACKs but returns 2
    let mut hw = AuxHw::new(AuxCh::A, |n, _| if n == 0 { ack_read(&[0]) } else { ack_read(&[1, 2]) });
    let e = run_dpcd(&mut hw, &mut read_req(0x10, 4)).unwrap_err();
    assert_eq!(e.first, DpcdErr::Short { got: 2, want: 4 });
    assert_eq!(e.name(), "DPCD-EPROTO");
    assert_eq!(e.attempts, 32);
}

#[test]
fn success_on_the_third_attempt_stops_retrying() {
    let mut hw = AuxHw::new(AuxCh::A, |n, _| if n < 2 { (DP_AUX_CH_CTL_DONE, vec![0x10]) } else { ack_write() });
    let mut x = DpcdXfer::new(AuxCh::A, DpcdRequest::write(DP_SET_POWER, &[1]));
    assert_eq!(run_dpcd(&mut hw, &mut x).unwrap().len, 1);
    assert_eq!(hw.sends, 3);
    assert_eq!(hw.sleeps_us.len(), 2);
}

#[test]
fn a_failed_probe_fails_the_read_without_the_real_access() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE, vec![0x10]));
    let e = run_dpcd(&mut hw, &mut read_req(0x000, 15)).unwrap_err();
    assert_eq!(e.attempts, 32);
    assert_eq!(hw.sends, 32); // only the probe's 32 attempts
    // every send was the 1-byte probe
    assert!(hw.sent.iter().all(|s| s == &vec![0x90, 0x01, 0x02, 0x00]));
}

#[test]
fn oversized_dpcd_requests_are_refused_by_name_every_attempt() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| panic!("nothing is sent"));
    let mut x = DpcdXfer::new(AuxCh::A, DpcdRequest::write(0x100, &[0u8; 17]));
    let e = run_dpcd(&mut hw, &mut x).unwrap_err();
    assert!(matches!(e.first, DpcdErr::Aux(f) if f.errno == AuxErrno::TooBig));
    assert_eq!(hw.sends, 0);
}

#[test]
fn the_message_size_limits_follow_the_five_data_registers() {
    assert_eq!(DPCD_MAX_WRITE, 16); // 20 bytes on the wire minus the 4-byte header
    assert_eq!(DPCD_MAX_READ, 19); // 20 bytes on the wire minus the reply byte
    assert_eq!(AUX_RETRY_INTERVAL_US, 500);
    // the largest read and write both fit
    let mut hw = AuxHw::new(AuxCh::A, |n, _| if n == 0 { ack_read(&[0]) } else { ack_read(&[5; 19]) });
    let ok = run_dpcd(&mut hw, &mut read_req(0x100, DPCD_MAX_READ)).unwrap();
    assert_eq!(ok.len, 19);
    let mut hw = AuxHw::new(AuxCh::A, |_, _| ack_write());
    let ok = run_dpcd(&mut hw, &mut DpcdXfer::new(AuxCh::A, DpcdRequest::write(0x100, &[1; DPCD_MAX_WRITE]))).unwrap();
    assert_eq!(ok.len, 16);
    // one more is refused, a write of 17 or a read of 20
    let mut hw = AuxHw::new(AuxCh::A, |n, _| if n == 0 { ack_read(&[0]) } else { ack_read(&[5; 19]) });
    assert!(run_dpcd(&mut hw, &mut read_req(0x100, 20)).is_err());
}
