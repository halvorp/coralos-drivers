// SPDX-License-Identifier: GPL-2.0-only
//! The GT frequency request (Atlas 2026-10-30, "choppy"): the service puts the PCU under the software request at RPn, requests RP0 on
//! the poll that sees submitted work in flight, and falls back to RPn `BOOST_IDLE_MS` after the last busy poll - through the write
//! allow-list, with the firmware's state logged BEFORE the first write and the actual frequency sampled while busy.
#![cfg(feature = "p3-exec")]
mod exec_common;
#[macro_use]
mod uapi_world;
use exec_common::kbl;
use i915_device::uapi::Hw;
use i915_gt_core::rps::BOOST_IDLE_MS;
use uapi_world::*;

/// The Pixelbook Go's `RP_STATE_CAP` (ring 2026-10-30): RP0 0x15 x 3 = 63 (1050 MHz), RP1 = RPn = 0x06 x 3 = 18 (300 MHz).
const ATLAS_CAP: u32 = 0x60615;
const RPNSWREQ: u32 = 0xa008;
const RP_CONTROL: u32 = 0xa024;
const RPSTAT1: u32 = 0xa01c;
const RP0: u32 = 63 << 23;
const RPN: u32 = 18 << 23;

fn lines(w: &W, pat: &str) -> Vec<String> {
    w.log.0.iter().filter(|l| l.contains(pat)).cloned().collect()
}

/// One batch submitted on a fresh session; returns its seqno.
fn submit_one(w: &mut W) -> u32 {
    let inc = open(w);
    let (batch, result) = (create(w, inc, 4096), create(w, inc, 4096));
    let bl = lease(w, inc, batch, 4096);
    write_batch(w, &bl);
    let (r, head) = submit(w, inc, &exec_payload(result, batch, &[], None, 0));
    head.unwrap_or_else(|| panic!("EXECBUFFER2 refused: errno {}", r.errno)).seqno
}

fn enabled() -> W {
    let mut w = world();
    w.chip.set(RP_CONTROL, 0);
    w.chip.set(RPNSWREQ, 0);
    w.chip.set(RPSTAT1, RPN);
    w.svc.enable_rps(&mut hw!(w), &kbl(), ATLAS_CAP);
    w
}

#[test]
fn enable_logs_the_firmware_state_then_takes_software_control_at_rpn() {
    let w = enabled();
    assert_eq!(w.chip.writes_to(RP_CONTROL), vec![0x400], "GEN9_RPSWCTL_ENABLE, nothing else (no RP_ENABLE: its interrupts are not serviced)");
    assert_eq!(w.chip.writes_to(RPNSWREQ), vec![RPN]);
    assert_eq!(w.svc.rps_requested(), Some(18));
    let e = lines(&w, "[I915-RPS ENABLE");
    assert_eq!(e.len(), 1, "{:?}", w.log.0);
    assert!(e[0].contains("platform=kbl-gt2 rp0_mhz=1050 rp1_mhz=300 rpn_mhz=300 was_rp_control=0x0 was_rpnswreq=0x0 was_cagf_mhz=300"), "{}", e[0]);
    assert!(e[0].contains("rp_control=0x400 rpnswreq=0x9000000 confirm=OK"), "the write is read back: {}", e[0]);
    assert_eq!(w.svc.next_deadline_ms(w.now, false), None, "at RPn and idle the device has no deadline");
}

#[test]
fn work_in_flight_boosts_to_rp0_once_and_idle_decays_to_rpn() {
    let mut w = enabled();
    let seqno = submit_one(&mut w);
    poll(&mut w);
    assert_eq!(w.chip.writes_to(RPNSWREQ), vec![RPN, RP0], "the poll that sees work in flight requests RP0");
    poll(&mut w);
    poll(&mut w);
    assert_eq!(w.chip.writes_to(RPNSWREQ).len(), 2, "no redundant request while it stays busy");

    let tl = w.svc.timeline_addrs()[0];
    gpu_complete(&mut w, tl, seqno);
    poll(&mut w);
    assert_eq!(w.svc.requests_in_flight(), 0);
    let last_busy = w.now - 1; // the poll before this one saw the request still in flight
    let d = w.svc.next_deadline_ms(w.now, false).expect("a standing boost asks for its decay deadline");
    assert!(d >= last_busy + BOOST_IDLE_MS && d <= w.now + BOOST_IDLE_MS, "deadline {} last busy {}", d, last_busy);
    poll(&mut w);
    assert_eq!(w.chip.writes_to(RPNSWREQ).len(), 2, "idle, but not for BOOST_IDLE_MS yet");

    w.now = d - 1; // poll() advances the clock by one: this poll runs AT the deadline
    poll(&mut w);
    assert_eq!(w.chip.writes_to(RPNSWREQ), vec![RPN, RP0, RPN], "decayed to RPn at the deadline");
    assert_eq!(w.svc.rps_requested(), Some(18));
    assert_eq!(w.svc.next_deadline_ms(w.now, false), None, "back at RPn: no deadline, no idle wake-ups");
}

#[test]
fn the_actual_frequency_is_sampled_while_busy_at_most_every_freq_log_ms() {
    let mut w = enabled();
    w.chip.set(RPSTAT1, RP0 | 0x1234); // the low bits are not CAGF
    let seqno = submit_one(&mut w);
    for _ in 0..50 {
        poll(&mut w);
    }
    let s = lines(&w, "[I915-GT-FREQ");
    assert_eq!(s.len(), 1, "one sample per FREQ_LOG_MS: {:?}", s);
    assert!(s[0].contains("cagf_mhz=1050 req_mhz=1050") && s[0].contains("busy=1 boosts=1 decays=0"), "{}", s[0]);
    // idle: no sample however long it lasts (the stall bound is 1 s, so the next busy period is a NEW submission)
    let tl = w.svc.timeline_addrs()[0];
    gpu_complete(&mut w, tl, seqno);
    poll(&mut w);
    w.now += i915_gt_core::rps::FREQ_LOG_MS;
    poll(&mut w);
    assert_eq!(lines(&w, "[I915-GT-FREQ").len(), 1, "never sampled at idle");
    submit_one(&mut w);
    poll(&mut w);
    assert_eq!(lines(&w, "[I915-GT-FREQ").len(), 2, "busy again, FREQ_LOG_MS after the last sample");
}

#[test]
fn unusable_caps_are_refused_by_name_and_nothing_is_ever_written() {
    // the NEGATIVE CONTROL: a refused RP_STATE_CAP read (all ones) or a zero cap must never turn into a frequency request
    for cap in [0, 0xffff_ffff] {
        let mut w = world();
        w.svc.enable_rps(&mut hw!(w), &kbl(), cap);
        assert_eq!(lines(&w, "[I915-RPS REFUSED why=CAPS-UNUSABLE").len(), 1, "{:?}", w.log.0);
        submit_one(&mut w);
        poll(&mut w);
        assert!(w.chip.writes_to(RPNSWREQ).is_empty() && w.chip.writes_to(RP_CONTROL).is_empty());
        assert_eq!(w.svc.rps_requested(), None);
    }
}

#[test]
fn a_device_never_enabled_requests_nothing() {
    let mut w = world();
    submit_one(&mut w);
    poll(&mut w);
    assert!(w.chip.writes_to(RPNSWREQ).is_empty());
    assert!(lines(&w, "[I915-GT-FREQ").is_empty());
}
