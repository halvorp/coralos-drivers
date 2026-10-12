// SPDX-License-Identifier: GPL-2.0-only
//! The forcewake protocol against a simulated register file. Expected words are Linux's: `_MASKED_BIT_ENABLE(FORCEWAKE_KERNEL)` =
//! 0x00010001, `_MASKED_BIT_DISABLE(FORCEWAKE_KERNEL)` = 0x00010000, FALLBACK enable/disable = 0x80008000 / 0x80000000.
mod common;
use common::*;
use i915_common_core::{I915Error, Reg, WriteGuard};
use i915_devinfo_core::lookup;
use i915_uncore_core::*;

const GET: u32 = 0x0001_0001;
const PUT: u32 = 0x0001_0000;
const FB_ON: u32 = 0x8000_8000;
const FB_OFF: u32 = 0x8000_0000;

fn guard(sim: Sim) -> WriteGuard<Sim> {
    WriteGuard::new(sim, &WRITE_ALLOWLIST)
}

#[test]
fn domain_triples_are_the_gen9_registers() {
    // (name, id, mask, set, ack) written out from intel_uncore.c:2212-2219 / intel_gt_regs.h
    let want = [("RENDER", 0usize, 1u32, 0xa278u32, 0xd84u32), ("GT", 1, 2, 0xa188, 0x130044), ("MEDIA", 2, 4, 0xa270, 0xd88)];
    assert_eq!(GEN9_DOMAINS.len(), 3);
    for (d, (name, id, mask, set, ack)) in GEN9_DOMAINS.iter().zip(want) {
        assert_eq!((d.name, d.id as usize, d.mask, d.set.offset(), d.ack.offset()), (name, id, mask, set, ack));
    }
    assert_eq!((FORCEWAKE_RENDER, FORCEWAKE_GT, FORCEWAKE_MEDIA), (1, 2, 4));
    assert_eq!(FORCEWAKE_ALL, FORCEWAKE_RENDER | FORCEWAKE_GT | FORCEWAKE_MEDIA);
    assert_eq!(FORCEWAKE_ALL, 7);
    assert_eq!(FORCEWAKE_ACK_TIMEOUT_MS, 50);
}

#[test]
fn vlv_domains_are_the_cherryview_registers() {
    let want = [(1u32, 0x1300b0u32, 0x1300b4u32), (4, 0x1300b8, 0x1300bc)];
    assert_eq!(VLV_DOMAINS.len(), 2);
    for (d, (mask, set, ack)) in VLV_DOMAINS.iter().zip(want) {
        assert_eq!((d.mask, d.set.offset(), d.ack.offset()), (mask, set, ack));
    }
}

#[test]
fn the_write_allow_list_is_exactly_the_three_set_registers() {
    let mut got: Vec<u32> = WRITE_ALLOWLIST.iter().map(|r| r.offset()).collect();
    got.sort();
    assert_eq!(got, vec![0xa188, 0xa270, 0xa278]);
    for d in GEN9_DOMAINS.iter() {
        assert!(WRITE_ALLOWLIST.contains(&d.set));
        assert!(!WRITE_ALLOWLIST.contains(&d.ack), "an ack register is read-only");
    }
}

#[test]
fn get_all_domains_writes_each_set_register_once_in_domain_order() {
    let mut u = Uncore::gen9();
    let mut g = guard(Sim::gen9(Beh::Immediate));
    u.forcewake_get(&mut g, FORCEWAKE_ALL).unwrap();
    assert_eq!(g.refusals(), 0);
    assert_eq!(g.into_inner().writes, vec![(0xa278, GET), (0xa188, GET), (0xa270, GET)]);
    assert_eq!(u.active(), 7);
    for id in [DomainId::Render, DomainId::Gt, DomainId::Media] {
        assert_eq!(u.wake_count(id), 1);
    }
}

#[test]
fn put_clears_each_set_register_and_the_ack_follows() {
    let mut u = Uncore::gen9();
    let mut g = guard(Sim::gen9(Beh::Immediate));
    u.forcewake_get(&mut g, FORCEWAKE_ALL).unwrap();
    u.forcewake_put(&mut g, FORCEWAKE_ALL).unwrap();
    u.wait_released(&mut g, FORCEWAKE_ALL).unwrap();
    assert_eq!(g.refusals(), 0);
    let s = g.into_inner();
    assert_eq!(&s.writes[3..], &[(0xa278, PUT), (0xa188, PUT), (0xa270, PUT)]);
    assert_eq!(u.active(), 0);
    assert!(s.doms.iter().all(|d| d.ack_val & KERNEL == 0));
}

#[test]
fn references_are_counted_and_only_the_first_get_and_last_put_touch_hardware() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Immediate);
    u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap();
    u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap();
    assert_eq!(s.writes.len(), 1, "second get is a refcount bump");
    u.forcewake_put(&mut s, FORCEWAKE_GT).unwrap();
    assert_eq!(s.writes.len(), 1, "first put keeps the domain awake");
    assert_eq!(u.active(), FORCEWAKE_GT);
    u.forcewake_put(&mut s, FORCEWAKE_GT).unwrap();
    assert_eq!(s.writes, vec![(0xa188, GET), (0xa188, PUT)]);
    assert_eq!(u.active(), 0);
}

#[test]
fn an_overlapping_get_asks_the_hardware_only_for_the_domains_that_had_no_reference() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Immediate);
    u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap();
    u.forcewake_get(&mut s, FORCEWAKE_GT | FORCEWAKE_MEDIA).unwrap();
    assert_eq!(s.writes, vec![(0xa188, GET), (0xa270, GET)]);
    assert_eq!(u.wake_count(DomainId::Gt), 2);
    assert_eq!(u.wake_count(DomainId::Media), 1);
    assert_eq!(u.wake_count(DomainId::Render), 0);
}

#[test]
fn an_unbalanced_put_is_named_and_writes_nothing() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Immediate);
    let e = u.forcewake_put(&mut s, FORCEWAKE_RENDER).unwrap_err();
    assert_eq!(e, I915Error::Unbalanced { domain_bit: 1 });
    assert_eq!(e.name(), "UNBALANCED-PUT");
    assert!(s.writes.is_empty());
    // a put that is balanced for GT but not MEDIA changes NOTHING (no half-applied put)
    u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap();
    let n = s.writes.len();
    assert_eq!(u.forcewake_put(&mut s, FORCEWAKE_GT | FORCEWAKE_MEDIA), Err(I915Error::Unbalanced { domain_bit: 4 }));
    assert_eq!(u.wake_count(DomainId::Gt), 1);
    assert_eq!(s.writes.len(), n);
}

#[test]
fn a_domain_the_platform_lacks_is_refused_by_name() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Immediate);
    assert_eq!(u.forcewake_get(&mut s, 1 << 3), Err(I915Error::NoSuchDomain { domain_bit: 8 }));
    assert_eq!(u.forcewake_get(&mut s, FORCEWAKE_GT | (1 << 4)), Err(I915Error::NoSuchDomain { domain_bit: 16 }));
    assert_eq!(u.forcewake_put(&mut s, 1 << 5), Err(I915Error::NoSuchDomain { domain_bit: 32 }));
    assert!(u.forcewake_get(&mut s, 0).is_ok());
    assert!(s.writes.is_empty());
    assert_eq!(u.wake_count(DomainId::Gt), 0);
}

#[test]
fn a_delayed_ack_within_the_bound_succeeds() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Delayed(5));
    u.set_poll_limit(100);
    u.forcewake_get(&mut s, FORCEWAKE_ALL).unwrap();
    assert_eq!(u.active(), 7);
    let reads_for_acks = s.reads.len();
    assert!(reads_for_acks > 3 + 3 * 5 - 3, "the waits really polled: {reads_for_acks}");
}

#[test]
fn a_delayed_ack_past_the_bound_is_a_named_timeout_with_the_evidence() {
    let mut u = Uncore::gen9();
    let mut g = guard(Sim::gen9(Beh::Delayed(10_000)));
    u.set_poll_limit(40);
    let e = u.forcewake_get(&mut g, FORCEWAKE_RENDER).unwrap_err();
    assert_eq!(e, I915Error::AckTimeout { domain: 1, reg: Reg::new(0xd84), mask: 1, want: 1, got: 0, polls: 40 });
    assert_eq!(e.name(), "ACK-TIMEOUT");
    assert_eq!(g.refusals(), 0, "even the failure path wrote only allow-listed registers");
    assert_eq!(u.wake_count(DomainId::Render), 0, "the failed get's reference is dropped");
    assert_eq!(u.active(), 0);
}

#[test]
fn an_absent_ack_times_out_after_the_fallback_toggles_the_ten_passes_of_linux() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Never);
    u.set_poll_limit(20);
    let e = u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { domain: 2, mask: 1, want: 1, got: 0, .. }), "{e:?}");
    let w = s.writes_to(0xa188);
    // KERNEL request, then 10 passes of (FALLBACK on, FALLBACK off): intel_uncore.c:237-248
    assert_eq!(w[0], GET);
    assert_eq!(w.len(), 1 + 2 * 10);
    for p in 0..10 {
        assert_eq!((w[1 + 2 * p], w[2 + 2 * p]), (FB_ON, FB_OFF), "pass {}", p + 1);
    }
}

#[test]
fn a_stale_ack_that_never_clears_is_a_named_timeout_on_release_and_on_the_next_get() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::StuckSet);
    u.set_poll_limit(20);
    u.forcewake_get(&mut s, FORCEWAKE_MEDIA).unwrap();
    u.forcewake_put(&mut s, FORCEWAKE_MEDIA).unwrap();
    let e = u.wait_released(&mut s, FORCEWAKE_MEDIA).unwrap_err();
    assert_eq!(e, I915Error::AckTimeout { domain: 4, reg: Reg::new(0xd88), mask: 1, want: 0, got: 1, polls: 20 });
    // a second get first waits for the stale ack to clear (fw_domain_wait_ack_clear, intel_uncore.c:332) and says so
    let e2 = u.forcewake_get(&mut s, FORCEWAKE_MEDIA).unwrap_err();
    assert!(matches!(e2, I915Error::AckTimeout { domain: 4, want: 0, got: 1, .. }), "{e2:?}");
    assert_eq!(u.wake_count(DomainId::Media), 0);
}

#[test]
fn the_ack_workaround_is_used_when_only_the_fallback_toggle_wakes_the_gt() {
    let mut u = Uncore::gen9();
    let mut g = guard(Sim::gen9(Beh::NeedsFallback));
    u.set_poll_limit(20);
    u.forcewake_get(&mut g, FORCEWAKE_GT).unwrap();
    assert_eq!(g.refusals(), 0);
    let s = g.into_inner();
    let w = s.writes_to(0xa188);
    assert_eq!(w[0], GET);
    assert!(w.contains(&FB_ON) && w.contains(&FB_OFF), "the fallback bit was toggled: {w:x?}");
    assert_eq!(u.active(), FORCEWAKE_GT);
    assert!(s.doms[1].ack_val & KERNEL != 0);
}

#[test]
fn an_all_ones_ack_is_a_dead_bar_not_an_ack() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::AllOnes);
    let e = u.forcewake_get(&mut s, FORCEWAKE_RENDER).unwrap_err();
    assert_eq!(e, I915Error::MmioUnreliable { domain: 1, reg: Reg::new(0xd84) });
    assert_eq!(e.name(), "MMIO-UNRELIABLE");
    assert!(s.writes.is_empty(), "stopped at the first read, before any write");
    assert_eq!(u.wake_count(DomainId::Render), 0);
}

#[test]
fn the_wall_clock_deadline_bounds_a_wait_even_with_a_huge_poll_cap() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Never);
    s.deadline_after = Some(7);
    let e = u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { polls: 7, .. }), "{e:?}");
}

#[test]
fn cherryview_domains_time_out_against_a_gen9_script_and_their_writes_are_refused() {
    // The negative control: drive the VLV/CHV forcewake registers (0x1300b0.. intel_gt_regs.h:1513-1516) at a Gen9 GPU.
    let mut u = Uncore::with_domains(&VLV_DOMAINS);
    let mut g = guard(Sim::gen9(Beh::Immediate));
    u.set_poll_limit(30);
    let e = u.forcewake_get(&mut g, FORCEWAKE_RENDER).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { domain: 1, want: 1, got: 0, .. }), "{e:?}");
    if let I915Error::AckTimeout { reg, .. } = e {
        assert_eq!(reg.offset(), 0x1300b4);
    }
    let r = g.first_refusal().expect("the CHV set register is not on the Gen9 allow-list");
    assert_eq!(r.offset, 0x1300b0);
    assert_eq!(g.writes(), 0, "nothing reached the Gen9 device");
}

#[test]
fn for_platform_builds_gen9_and_refuses_cherryview_by_generation() {
    assert!(Uncore::for_platform(&lookup(0x591c).unwrap()).is_ok());
    assert!(Uncore::for_platform(&lookup(0x1916).unwrap()).is_ok());
    assert_eq!(Uncore::for_platform(&lookup(0x22b0).unwrap()).unwrap_err(), I915Error::UnsupportedGeneration { devid: 0x22b0, gen: 8 });
}

#[test]
fn gt_registers_are_read_only_under_forcewake_and_refused_otherwise() {
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Immediate);
    s.plain.insert(0x9120, 0x0220_0000);
    // FUSE2 / EU_DISABLE / CTC_MODE live in the GT well; RPM_CONFIG0 and RP_STATE_CAP (0x145998: past the end of the Gen9 table,
    // intel_uncore.c:1306) need nothing.
    for off in [0x9120u32, 0x9134, 0x9138, 0x913c, 0xa26c] {
        let e = u.read32(&mut s, Reg::new(off)).unwrap_err();
        assert_eq!(e, I915Error::ForcewakeNotHeld { offset: off, domains: FORCEWAKE_GT }, "{off:#x}");
        assert_eq!(e.name(), "FORCEWAKE-NOT-HELD");
    }
    assert_eq!(u.read32(&mut s, Reg::new(0xd00)), Ok(0), "RPM_CONFIG0 is in the uncore range");
    assert_eq!(u.read32(&mut s, Reg::new(0x145998)), Ok(0), "GEN6_RP_STATE_CAP: no domain in the Gen9 table");
    assert_eq!(u.read32(&mut s, Reg::new(0x44074)), Ok(0), "GEN9_TIMESTAMP_OVERRIDE is below NEEDS_FORCE_WAKE's second range");
    u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap();
    assert_eq!(u.read32(&mut s, Reg::new(0x9120)), Ok(0x0220_0000));
    // holding RENDER does not satisfy a GT register
    let mut u2 = Uncore::gen9();
    u2.forcewake_get(&mut s, FORCEWAKE_RENDER).unwrap();
    assert!(u2.read32(&mut s, Reg::new(0x9120)).is_err());
}

#[test]
fn the_poll_cap_is_enforced_with_no_deadline_and_a_silent_ack() {
    // Regression for the 31 GB OOM: `poll` once ignored `max_polls` and spun forever on Beh::Never with no deadline.
    let mut u = Uncore::gen9();
    let mut s = Sim::gen9(Beh::Never);
    assert!(s.deadline_after.is_none());
    u.set_poll_limit(5);
    let e = u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { polls: 5, .. }), "{e:?}");
    // total work is finite: 5 polls per wait * (1 + 10 fallback passes * 2 + final) + the delay reads, far below any runaway
    assert!(s.reads.len() < 2_000, "reads bounded: {}", s.reads.len());
    // release path too
    let mut s = Sim::gen9(Beh::StuckSet);
    let mut u = Uncore::gen9();
    u.set_poll_limit(5);
    u.forcewake_get(&mut s, FORCEWAKE_GT).unwrap();
    u.forcewake_put(&mut s, FORCEWAKE_GT).unwrap();
    let e = u.wait_released(&mut s, FORCEWAKE_GT).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { polls: 5, .. }), "{e:?}");
}
