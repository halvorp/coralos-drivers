// SPDX-License-Identifier: GPL-2.0-only
//! The ELSP / CSB tracker against the rules of `process_csb`: event decode, promote vs advance, the six-entry wrap, the
//! poisoned-entry MMIO fallback, the named refusals, the ELSP write order and the reset / resume plans. CSB event words are
//! composed from the GEN8_CTX_STATUS_* bits exactly as the DOC comment of intel_execlists_submission.c describes the flow
//! (idle -> active on the first submission, element switch on a context change, active -> idle when the list drains).
use i915_common_core::Reg;
use i915_gt_core::{Effect, WritePlan};
use i915_submit_core::execlists::*;
use i915_submit_core::SubmitError;

const A: Port = Port { ctx: 1, seqno: 2 };
const B: Port = Port { ctx: 2, seqno: 2 };
const C: Port = Port { ctx: 3, seqno: 4 };

// Linux's status bits written out as the numbers the hardware reports (intel_execlists_submission.c:150-155).
const IDLE_ACTIVE: u64 = 0x1;
const PREEMPTED: u64 = 0x2;
const ELEMENT_SWITCH: u64 = 0x4;
const ACTIVE_IDLE: u64 = 0x8;
const COMPLETE: u64 = 0x10;
const LITE_RESTORE: u64 = 0x8000;

/// A fake status page: entries 0..6, with the write pointer the hardware would have published.
struct Page {
    entries: [u64; 6],
}

impl Page {
    fn new() -> Page {
        Page { entries: [u64::MAX; 6] }
    }
    fn read(&self) -> impl FnMut(u32) -> u64 + '_ {
        move |i| self.entries[i as usize]
    }
}

fn no_mmio(_: u32) -> u64 {
    panic!("the MMIO mirror must be consulted only for a poisoned entry")
}

#[test]
fn csb_flags_decode_individually() {
    // every member of the family (trap 5): drive a real word through the decode for EACH bit
    let cases: [(u64, [bool; 6]); 6] = [
        (IDLE_ACTIVE, [true, false, false, false, false, false]),
        (PREEMPTED, [false, true, false, false, false, false]),
        (ELEMENT_SWITCH, [false, false, true, false, false, false]),
        (ACTIVE_IDLE, [false, false, false, true, false, false]),
        (COMPLETE, [false, false, false, false, true, false]),
        (LITE_RESTORE, [false, false, false, false, false, true]),
    ];
    for (word, want) in cases {
        let s = decode_csb(word | 0x77 << 32);
        assert_eq!([s.idle_to_active, s.preempted, s.element_switch, s.active_to_idle, s.complete, s.lite_restore], want, "{word:#x}");
        assert_eq!(s.ctx_id, 0x77, "the upper dword is the context id");
    }
    let all = decode_csb(IDLE_ACTIVE | PREEMPTED | ELEMENT_SWITCH | ACTIVE_IDLE | COMPLETE | LITE_RESTORE);
    assert!(all.idle_to_active && all.preempted && all.element_switch && all.active_to_idle && all.complete && all.lite_restore);
    let none = decode_csb(0);
    assert!(!none.idle_to_active && !none.preempted && !none.element_switch && !none.active_to_idle && !none.complete && !none.lite_restore);
}

#[test]
fn only_idle_to_active_and_preempted_promote_pending() {
    // gen8_csb_parse (intel_execlists_submission.c:1743-1746)
    for (word, promote) in [
        (IDLE_ACTIVE, true),
        (PREEMPTED, true),
        (ELEMENT_SWITCH, false),
        (ACTIVE_IDLE, false),
        (COMPLETE, false),
        (LITE_RESTORE, false),
        (ELEMENT_SWITCH | COMPLETE, false),
        (ACTIVE_IDLE | COMPLETE, false),
        (PREEMPTED | LITE_RESTORE, true),
        (IDLE_ACTIVE | LITE_RESTORE, true),
        (0, false),
    ] {
        assert_eq!(gen8_csb_parse(word), promote, "{word:#x}");
        assert_eq!(gen8_csb_parse(word | 0xdead_0000 << 32), promote, "the context id in the upper dword never promotes: {word:#x}");
    }
}

#[test]
fn csb_entry_addresses() {
    assert_eq!([0, 1, 2, 3, 4, 5].map(csb_entry_dword), [0x10, 0x12, 0x14, 0x16, 0x18, 0x1a]);
    assert_eq!([0, 1, 5].map(|i| csb_entry_mmio(0x2000, i).offset()), [0x2370, 0x2378, 0x2398]);
    assert_eq!(csb_entry_mmio(0x22000, 2).offset(), 0x22380);
}

#[test]
fn a_single_context_goes_idle_to_active_to_idle() {
    let mut x = Execlists::new();
    assert_eq!(x.csb_head(), 5, "after reset_csb_pointers the head is one behind entry 0");
    assert!(x.is_idle());
    x.set_pending([Some(A), None]).unwrap();
    assert!(x.has_pending() && !x.is_idle());
    let mut page = Page::new();
    page.entries[0] = IDLE_ACTIVE | 0x20 << 32;
    let out = x.process_csb(0, page.read(), no_mmio).unwrap();
    assert_eq!(out.events.as_slice(), &[CsbEvent { index: 0, raw: IDLE_ACTIVE | 0x20 << 32, promote: true }]);
    assert!(out.inactive.as_slice().is_empty());
    assert!(out.paused_cleared, "a promote runs ring_set_paused(engine, 0)");
    assert!(!x.has_pending());
    assert_eq!(x.inflight(), &[Some(A), None]);
    assert_eq!(x.active_ports().collect::<Vec<_>>(), [A]);
    assert_eq!(x.csb_head(), 0);

    page.entries[1] = ACTIVE_IDLE | COMPLETE;
    let out = x.process_csb(1, page.read(), no_mmio).unwrap();
    assert_eq!(out.inactive.as_slice(), &[A]);
    assert!(!out.paused_cleared);
    assert!(x.is_idle());
    assert_eq!(x.csb_head(), 1);
}

#[test]
fn two_contexts_complete_in_order_through_an_element_switch() {
    let mut x = Execlists::new();
    x.set_pending([Some(A), Some(B)]).unwrap();
    let mut page = Page::new();
    page.entries[0] = IDLE_ACTIVE;
    page.entries[1] = ELEMENT_SWITCH | COMPLETE;
    page.entries[2] = ACTIVE_IDLE | COMPLETE;
    // all three events arrive in one interrupt
    let out = x.process_csb(2, page.read(), no_mmio).unwrap();
    assert_eq!(out.events.len(), 3);
    assert_eq!(out.events.as_slice().iter().map(|e| e.index).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(out.events.as_slice().iter().map(|e| e.promote).collect::<Vec<_>>(), [true, false, false]);
    assert_eq!(out.inactive.as_slice(), &[A, B]);
    assert!(x.is_idle());
}

#[test]
fn one_event_at_a_time_tracks_the_active_port() {
    let mut x = Execlists::new();
    x.set_pending([Some(A), Some(B)]).unwrap();
    let mut page = Page::new();
    page.entries[0] = IDLE_ACTIVE;
    x.process_csb(0, page.read(), no_mmio).unwrap();
    assert_eq!(x.active_index(), 0);
    page.entries[1] = ELEMENT_SWITCH | COMPLETE;
    let out = x.process_csb(1, page.read(), no_mmio).unwrap();
    assert_eq!(out.inactive.as_slice(), &[A]);
    assert_eq!(x.active_index(), 1);
    assert_eq!(x.active_ports().collect::<Vec<_>>(), [B], "B is now executing");
    assert!(!x.is_idle());
}

#[test]
fn a_preemption_promote_retires_whatever_was_in_flight() {
    let mut x = Execlists::new();
    x.set_pending([Some(A), Some(B)]).unwrap();
    let mut page = Page::new();
    page.entries[0] = IDLE_ACTIVE;
    x.process_csb(0, page.read(), no_mmio).unwrap();
    // resubmit [B, C] while A and B are in flight (lite restore of B plus a new context): the hardware reports PREEMPTED
    x.set_pending([Some(B), Some(C)]).unwrap();
    page.entries[1] = PREEMPTED | LITE_RESTORE;
    let out = x.process_csb(1, page.read(), no_mmio).unwrap();
    assert_eq!(out.inactive.as_slice(), &[A, B], "both old in-flight ports leave flight");
    assert_eq!(x.inflight(), &[Some(B), Some(C)]);
    assert_eq!(x.active_index(), 0);
}

#[test]
fn a_promote_after_the_first_port_completed_retires_only_the_remaining_one() {
    let mut x = Execlists::new();
    x.set_pending([Some(A), Some(B)]).unwrap();
    let mut page = Page::new();
    page.entries[0] = IDLE_ACTIVE;
    page.entries[1] = ELEMENT_SWITCH | COMPLETE; // A done, B running
    x.process_csb(1, page.read(), no_mmio).unwrap();
    x.set_pending([Some(B), Some(C)]).unwrap();
    page.entries[2] = PREEMPTED | LITE_RESTORE;
    let out = x.process_csb(2, page.read(), no_mmio).unwrap();
    assert_eq!(out.inactive.as_slice(), &[B], "only the active port (B) is replaced; A already left");
}

#[test]
fn entries_consumed_across_the_wrap_come_back_in_order() {
    let mut x = Execlists::new();
    let mut page = Page::new();
    let mut head = 5u32;
    for _ in 0..2 {
        x.set_pending([Some(A), None]).unwrap();
        let (e0, e1) = ((head + 1) % 6, (head + 2) % 6);
        page.entries[e0 as usize] = IDLE_ACTIVE;
        page.entries[e1 as usize] = ACTIVE_IDLE | COMPLETE;
        x.process_csb(e1, page.read(), no_mmio).unwrap();
        head = e1;
    }
    x.set_pending([Some(A), Some(B)]).unwrap();
    page.entries[4] = IDLE_ACTIVE;
    x.process_csb(4, page.read(), no_mmio).unwrap();
    page.entries[5] = ELEMENT_SWITCH | COMPLETE;
    page.entries[0] = ACTIVE_IDLE | COMPLETE;
    let out = x.process_csb(0, page.read(), no_mmio).unwrap();
    assert_eq!(out.events.as_slice().iter().map(|e| e.index).collect::<Vec<_>>(), [5, 0], "4 -> 5 -> 0 wraps at the sixth entry");
    assert_eq!(out.inactive.as_slice(), &[A, B]);
    assert_eq!(x.csb_head(), 0);
}

#[test]
fn the_longest_possible_walk_of_five_entries_terminates_with_a_named_refusal() {
    // head 0 and write pointer 5 is the longest legal pass: entries 1..=5
    let mut x = Execlists::new();
    let mut page = Page::new();
    x.set_pending([Some(A), Some(B)]).unwrap();
    page.entries[0] = IDLE_ACTIVE;
    x.process_csb(0, page.read(), no_mmio).unwrap();
    for e in 1..=5 {
        page.entries[e] = 0x0; // five events with no promote bit, but only two ports exist to complete
    }
    let e = x.process_csb(5, page.read(), no_mmio).unwrap_err();
    // after A and B are advanced past, the third non-promote finds nothing in flight
    assert_eq!(e, SubmitError::CsbCompleteWithoutActive);
}

#[test]
fn no_new_entries_is_no_work() {
    let mut x = Execlists::new();
    let page = Page::new();
    let out = x.process_csb(5, page.read(), no_mmio).unwrap();
    assert!(out.events.as_slice().is_empty() && out.inactive.as_slice().is_empty() && !out.paused_cleared);
    assert_eq!(x.csb_head(), 5);
}

#[test]
fn the_write_pointer_must_name_a_real_entry() {
    // Linux would loop forever on a tail it can never reach (head wraps at 6): refuse by name instead
    let mut x = Execlists::new();
    let page = Page::new();
    for bad in [6u32, 7, 0x1f, 0xff] {
        let e = x.process_csb(bad, page.read(), no_mmio).unwrap_err();
        assert_eq!(e, SubmitError::CsbWritePtrOutOfRange { ptr: bad, entries: 6 });
        assert_eq!(e.name(), "CSB-WRITE-PTR-OUT-OF-RANGE");
        assert_eq!(x.csb_head(), 5, "a refused pass consumes nothing");
    }
    // only the low byte counts (`u8 tail = READ_ONCE(*csb_write)`): 0x100 is pointer 0, and the upper bits are ignored
    let mut page = Page::new();
    page.entries[0] = IDLE_ACTIVE;
    x.set_pending([Some(A), None]).unwrap();
    let out = x.process_csb(0x100, page.read(), no_mmio).unwrap();
    assert_eq!(out.events.len(), 1);
    // and a high byte that makes the low byte invalid is still invalid
    assert_eq!(x.process_csb(0xabc6, Page::new().read(), no_mmio).unwrap_err().name(), "CSB-WRITE-PTR-OUT-OF-RANGE");
}

#[test]
fn a_poisoned_entry_falls_back_to_the_mmio_mirror() {
    let mut x = Execlists::new();
    x.set_pending([Some(A), None]).unwrap();
    let page = Page::new(); // every entry still all-ones
    let mut asked = vec![];
    let out = x
        .process_csb(0, page.read(), |i| {
            asked.push(i);
            IDLE_ACTIVE | 0x20 << 32
        })
        .unwrap();
    assert_eq!(asked, [0], "exactly the stale entry is read from the RING_EXECLIST_STATUS_BUF mirror");
    assert_eq!(out.events.as_slice()[0].raw, IDLE_ACTIVE | 0x20 << 32);
    assert!(out.events.as_slice()[0].promote);
}

#[test]
fn an_entry_stale_in_both_places_is_refused_by_name() {
    let mut x = Execlists::new();
    x.set_pending([Some(A), None]).unwrap();
    let page = Page::new();
    let e = x.process_csb(0, page.read(), |_| u64::MAX).unwrap_err();
    assert_eq!(e, SubmitError::CsbEntryStale { index: 0 });
    assert_eq!(e.name(), "CSB-ENTRY-STALE");
}

#[test]
fn an_impossible_event_is_refused_not_guessed_at() {
    // promote with nothing pending (ERROR_CSB)
    let mut x = Execlists::new();
    let mut page = Page::new();
    page.entries[0] = IDLE_ACTIVE;
    let e = x.process_csb(0, page.read(), no_mmio).unwrap_err();
    assert_eq!(e, SubmitError::CsbPromoteWithoutPending);
    assert_eq!(e.name(), "CSB-PROMOTE-WITHOUT-PENDING");
    // complete with nothing in flight
    let mut x = Execlists::new();
    let mut page = Page::new();
    page.entries[0] = ACTIVE_IDLE | COMPLETE;
    let e = x.process_csb(0, page.read(), no_mmio).unwrap_err();
    assert_eq!(e, SubmitError::CsbCompleteWithoutActive);
    assert_eq!(e.name(), "CSB-COMPLETE-WITHOUT-ACTIVE");
}

#[test]
fn a_second_submission_before_the_ack_is_refused() {
    let mut x = Execlists::new();
    x.set_pending([Some(A), None]).unwrap();
    let e = x.set_pending([Some(B), None]).unwrap_err();
    assert_eq!(e, SubmitError::ElspPendingUnacked);
    assert_eq!(e.name(), "ELSP-PENDING-UNACKED");
    assert_eq!(x.pending(), &[Some(A), None], "the refused submission changed nothing");
}

#[test]
fn reset_returns_every_outstanding_port_and_rewinds_the_csb() {
    let mut x = Execlists::new();
    x.set_pending([Some(A), Some(B)]).unwrap();
    let mut page = Page::new();
    page.entries[0] = IDLE_ACTIVE;
    x.process_csb(0, page.read(), no_mmio).unwrap();
    x.set_pending([Some(B), Some(C)]).unwrap();
    let lost = x.reset();
    assert_eq!(lost.as_slice(), &[B, C, A, B], "pending first, then what was in flight");
    assert!(x.is_idle());
    assert_eq!(x.csb_head(), 5);
    assert_eq!(x.inflight(), &[None, None]);
}

#[test]
fn elsp_writes_port_one_first_upper_then_lower() {
    // execlists_submit_ports drives write_desc for n = 1, then 0; write_desc writes upper_32_bits then lower_32_bits to the
    // single RING_ELSP register (intel_execlists_submission.c:723-733, :905-940)
    let d0 = 0x0000_0040_1234_511d_u64;
    let d1 = 0x0000_0020_5678_9119_u64;
    let w = elsp_writes(0x2000, [d0, d1]);
    let elsp = Reg::new(0x2230);
    assert_eq!(
        w,
        [
            WritePlan { reg: elsp, value: 0x20 },
            WritePlan { reg: elsp, value: 0x5678_9119 },
            WritePlan { reg: elsp, value: 0x40 },
            WritePlan { reg: elsp, value: 0x1234_511d },
        ]
    );
    // an empty port writes a zero descriptor
    let w = elsp_writes(0x22000, [d0, 0]);
    assert_eq!([w[0].value, w[1].value, w[2].value, w[3].value], [0, 0, 0x40, 0x1234_511d]);
    assert!(w.iter().all(|p| p.reg.offset() == 0x22230));
}

#[test]
fn the_csb_reset_plan_follows_reset_csb_pointers() {
    // intel_execlists_submission.c:2804-2844 for csb_size 6: reset_value 5; ptr = 0xffff << 16 | 5 << 8 | 5, written twice
    let plan = plan_reset_csb_pointers(0x2000);
    let ptr = Reg::new(0x23a0);
    let want = [
        Step::Hwsp(HwspStore::Dword { dword: 0x32, value: 0 }),
        Step::Mmio(Effect::Write(WritePlan { reg: ptr, value: 0xffff_0505 })),
        Step::Mmio(Effect::PostingRead(ptr)),
        Step::Hwsp(HwspStore::Dword { dword: 0x1f, value: 5 }),
        Step::Hwsp(HwspStore::PoisonCsb { entries: 6 }),
        Step::Mmio(Effect::Write(WritePlan { reg: ptr, value: 0xffff_0505 })),
        Step::Mmio(Effect::PostingRead(ptr)),
    ];
    assert_eq!(plan.steps.as_slice(), &want);
}

#[test]
fn the_enable_plan_follows_enable_execlists() {
    // intel_execlists_submission.c:2928-2950 (GRAPHICS_VER < 11): HWSTAM, RING_MODE_GEN7 |= GFX_RUN_LIST_ENABLE (masked),
    // RING_MI_MODE &= ~STOP_RING (masked), RING_HWS_PGA = status page, posting read
    let plan = plan_enable_execlists(0x2000, 0x0001_0000).unwrap();
    assert_eq!(
        plan.as_slice(),
        &[
            Effect::Write(WritePlan { reg: Reg::new(0x2098), value: 0xffff_ffff }),
            Effect::Write(WritePlan { reg: Reg::new(0x229c), value: 0x8000_8000 }),
            Effect::Write(WritePlan { reg: Reg::new(0x209c), value: 0x0100_0000 }),
            Effect::Write(WritePlan { reg: Reg::new(0x2080), value: 0x0001_0000 }),
            Effect::PostingRead(Reg::new(0x2080)),
        ]
    );
    let e = plan_enable_execlists(0x2000, 0x0001_0800).unwrap_err();
    assert_eq!(e, SubmitError::GgttUnaligned { what: "status page", addr: 0x0001_0800 });
}
