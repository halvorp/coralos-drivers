// SPDX-License-Identifier: GPL-2.0-only
//! The execlist port tracker and the context status buffer (CSB) consumer: `write_desc`, `execlists_submit_ports`,
//! `gen8_csb_parse`, `wa_csb_read`, `csb_read`, `process_csb`, `reset_csb_pointers` and `enable_execlists`
//! (gt/intel_execlists_submission.c:723-732, :905-939, :1743-1746, :1748-1780, :1782-1805, :1814-2044, :2804-2845, :2928-2950).
//! Gen8/Gen9 only: a legacy ELSP (write the upper then the lower descriptor dword to ONE register, port 1 first), a
//! six-entry CSB in the engine status page, and no ELSQ.
//!
//! The tracker mirrors `execlists->pending[]`, `->inflight[]` and `->active`: `pending` is what the driver wrote to the
//! ELSP and the hardware has not yet acknowledged; the first "promote" event ([`GEN8_CTX_STATUS_IDLE_ACTIVE`] or
//! [`GEN8_CTX_STATUS_PREEMPTED`]) moves it to `inflight`; every other event retires one in-flight port from the front. A
//! port that leaves flight is returned to the caller (Linux's `*inactive++`), which "schedules it out".
use crate::error::SubmitError;
use crate::lrc_reg::*;
use i915_common_core::reg::{masked_bit_disable, masked_bit_enable};
use i915_common_core::Reg;
use i915_gt_core::{Effect, FixedVec, WritePlan};
use i915_gt_regs_core as r;

/// KEPT FIXED (D10): the ELSP has exactly two submit ports - hardware.
/// `execlists_num_ports` on a legacy-ELSP part: two submit ports.
pub const NUM_PORTS: usize = 2;

/// An ELSP port: the context and the seqno of the LAST request of it that this submission covers (Linux keeps the request
/// pointer; the completion test `completed(rq)` is a seqno comparison, so the pair carries the same information).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Port {
    pub ctx: u32,
    pub seqno: u32,
}

/// The decoded flag bits of a CSB entry's context-status dword (the low dword of the qword), and its context id (high dword).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsbStatus {
    pub idle_to_active: bool,
    pub preempted: bool,
    pub element_switch: bool,
    pub active_to_idle: bool,
    pub complete: bool,
    pub lite_restore: bool,
    /// The upper dword: the hardware context id of the context the event concerns.
    pub ctx_id: u32,
}

/// Decode a Gen8..Gen10 CSB qword into its named flags.
pub fn decode_csb(csb: u64) -> CsbStatus {
    let lo = csb as u32;
    CsbStatus {
        idle_to_active: lo & GEN8_CTX_STATUS_IDLE_ACTIVE != 0,
        preempted: lo & GEN8_CTX_STATUS_PREEMPTED != 0,
        element_switch: lo & GEN8_CTX_STATUS_ELEMENT_SWITCH != 0,
        active_to_idle: lo & GEN8_CTX_STATUS_ACTIVE_IDLE != 0,
        complete: lo & GEN8_CTX_STATUS_COMPLETE != 0,
        lite_restore: lo & GEN8_CTX_STATUS_LITE_RESTORE != 0,
        ctx_id: (csb >> 32) as u32,
    }
}

/// `gen8_csb_parse` (intel_execlists_submission.c:1743-1746): does this event promote `pending` to `inflight`?
/// `csb & (GEN8_CTX_STATUS_IDLE_ACTIVE | GEN8_CTX_STATUS_PREEMPTED)` over the whole qword.
pub const fn gen8_csb_parse(csb: u64) -> bool {
    csb & (GEN8_CTX_STATUS_IDLE_ACTIVE | GEN8_CTX_STATUS_PREEMPTED) as u64 != 0
}

/// The CSB entry's dword index in the status page (`I915_HWS_CSB_BUF0_INDEX + 2 * idx`).
pub const fn csb_entry_dword(idx: u32) -> u32 {
    I915_HWS_CSB_BUF0_INDEX + 2 * idx
}

/// The MMIO mirror of CSB entry `idx` (`wa_csb_read`, intel_execlists_submission.c:1749-1780): `base + 0x370 + 8 * idx`.
pub const fn csb_entry_mmio(base: u32, idx: u32) -> Reg {
    Reg::new(base + GEN8_EXECLISTS_STATUS_BUF + 8 * idx)
}

/// One consumed CSB entry, for tracing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsbEvent {
    pub index: u32,
    pub raw: u64,
    pub promote: bool,
}

/// What one [`Execlists::process_csb`] pass did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsbOutcome {
    /// Entries consumed, in order. The executor poisons each consumed entry with all-ones (`csb_read`'s
    /// `WRITE_ONCE(*csb, -1)`, :1801) so a later reuse is recognisable.
    pub events: FixedVec<CsbEvent, 6>,
    /// Ports that left flight, in order: the caller schedules each out. At most four can: two replaced by a promote and the two
    /// it brought in; a fifth completion finds nothing in flight and is [`SubmitError::CsbCompleteWithoutActive`] first.
    pub inactive: FixedVec<Port, 4>,
    /// A promote happened, so `ring_set_paused(engine, 0)` ran: the status-page dword `I915_GEM_HWS_PREEMPT` must be 0.
    pub paused_cleared: bool,
}

/// A store into the engine status page the executor must perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwspStore {
    /// `status_page.addr[dword] = value`.
    Dword { dword: u32, value: u32 },
    /// `memset(csb_status, -1, entries * sizeof(u64))`.
    PoisonCsb { entries: u32 },
}

/// One ordered step of a plan that interleaves register effects with status-page stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Mmio(Effect),
    Hwsp(HwspStore),
}

/// `reset_csb_pointers` (intel_execlists_submission.c:2804-2844) as an ordered plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsbResetPlan {
    pub steps: FixedVec<Step, 8>,
}

/// `execlists->{pending,inflight,active,csb_head}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Execlists {
    pending: [Option<Port>; NUM_PORTS],
    inflight: [Option<Port>; NUM_PORTS],
    active: usize,
    csb_head: u32,
}

impl Default for Execlists {
    fn default() -> Self {
        Execlists::new()
    }
}

impl Execlists {
    /// A tracker after `reset_csb_pointers`: `csb_head` is one behind entry 0, i.e. `csb_size - 1`.
    pub const fn new() -> Execlists {
        Execlists { pending: [None; NUM_PORTS], inflight: [None; NUM_PORTS], active: 0, csb_head: GEN8_CSB_ENTRIES - 1 }
    }
    pub const fn csb_head(&self) -> u32 {
        self.csb_head
    }
    pub const fn pending(&self) -> &[Option<Port>; NUM_PORTS] {
        &self.pending
    }
    pub const fn inflight(&self) -> &[Option<Port>; NUM_PORTS] {
        &self.inflight
    }
    /// `execlists->active`: the index into `inflight` of the port the hardware is executing.
    pub const fn active_index(&self) -> usize {
        self.active
    }
    pub fn has_pending(&self) -> bool {
        self.pending[0].is_some()
    }
    /// The in-flight ports from `active` on (what `execlists_active()` walks).
    pub fn active_ports(&self) -> impl Iterator<Item = Port> + '_ {
        self.inflight.iter().skip(self.active).map_while(|p| *p)
    }
    pub fn is_idle(&self) -> bool {
        self.pending[0].is_none() && self.active_ports().next().is_none()
    }

    /// Record a submission (`pending[]` as `execlists_dequeue` fills it). Refuses while a previous one is unacknowledged:
    /// `__execlists_submission_tasklet` only dequeues with `!pending[0]` (:2477-2478).
    pub fn set_pending(&mut self, ports: [Option<Port>; NUM_PORTS]) -> Result<(), SubmitError> {
        if self.has_pending() {
            return Err(SubmitError::ElspPendingUnacked);
        }
        self.pending = ports;
        Ok(())
    }

    /// `process_csb` (intel_execlists_submission.c:1814-2044), Gen8/Gen9. `csb_write` is the status-page dword
    /// [`I915_HWS_CSB_WRITE_INDEX`]; `hwsp(idx)` reads CSB entry `idx` from the status page and `mmio(idx)` reads the same entry
    /// from `RING_EXECLIST_STATUS_BUF` ([`csb_entry_mmio`]); the latter is consulted only for a poisoned (all-ones) entry
    /// (`csb_read` -> `wa_csb_read`, :1782-1805).
    pub fn process_csb(&mut self, csb_write: u32, mut hwsp: impl FnMut(u32) -> u64, mut mmio: impl FnMut(u32) -> u64) -> Result<CsbOutcome, SubmitError> {
        let mut out = CsbOutcome { events: FixedVec::new("csb events", CsbEvent { index: 0, raw: 0, promote: false }), inactive: FixedVec::new("inactive ports", Port { ctx: 0, seqno: 0 }), paused_cleared: false };
        // `u8 tail = READ_ONCE(*execlists->csb_write)` (:1841): only the low byte counts.
        let tail = csb_write & 0xff;
        // Linux would loop forever on a tail it can never reach (`while (head != tail)` with head wrapping at 6): refuse it.
        if tail >= GEN8_CSB_ENTRIES {
            return Err(SubmitError::CsbWritePtrOutOfRange { ptr: tail, entries: GEN8_CSB_ENTRIES });
        }
        let mut head = self.csb_head;
        if head == tail {
            return Ok(out); // :1842
        }
        self.csb_head = tail; // :1861
        loop {
            head += 1;
            if head == GEN8_CSB_ENTRIES {
                head = 0; // :1882
            }
            let mut csb = hwsp(head);
            if csb == u64::MAX {
                csb = mmio(head); // wa_csb_read
                if csb == u64::MAX {
                    return Err(SubmitError::CsbEntryStale { index: head });
                }
            }
            let promote = gen8_csb_parse(csb);
            out.events.push(CsbEvent { index: head, raw: csb, promote })?;
            if promote {
                if self.pending[0].is_none() {
                    return Err(SubmitError::CsbPromoteWithoutPending); // GEM_WARN_ON(!*execlists->pending) -> ERROR_CSB
                }
                out.paused_cleared = true; // ring_set_paused(engine, 0)
                // cancel the old in-flight ports, prepare for the switch
                for p in self.inflight.iter().skip(self.active).map_while(|p| *p) {
                    out.inactive.push(p)?;
                }
                // switch pending to inflight
                self.inflight = self.pending;
                self.active = 0;
                self.pending[0] = None;
                self.pending = [None; NUM_PORTS];
            } else {
                let Some(done) = self.inflight.get(self.active).copied().flatten() else {
                    return Err(SubmitError::CsbCompleteWithoutActive); // GEM_WARN_ON(!*execlists->active)
                };
                out.inactive.push(done)?; // port0 completed, advance to port1
                self.active += 1;
            }
            if head == tail {
                break;
            }
        }
        Ok(out)
    }

    /// After an engine reset the hardware restarts its CSB at entry 0 and nothing is in flight. Returns the ports that were
    /// outstanding (pending first, then in flight) so the caller can schedule them out. Pair with
    /// [`plan_reset_csb_pointers`].
    pub fn reset(&mut self) -> FixedVec<Port, 4> {
        let mut v = FixedVec::new("reset ports", Port { ctx: 0, seqno: 0 });
        for p in self.pending.iter().flatten().chain(self.inflight.iter().skip(self.active).map_while(|p| p.as_ref())) {
            let _ = v.push(*p); // at most 2 + 2 ports exist; capacity 4
        }
        *self = Execlists::new();
        v
    }
}

/// `write_desc` for a legacy ELSP (intel_execlists_submission.c:723-732) over both ports as `execlists_submit_ports` drives
/// it (:905-939): port 1 first, then port 0, each as the UPPER dword then the LOWER dword of the descriptor, all to
/// `RING_ELSP`. An empty port writes a zero descriptor. The fourth write starts the submission.
pub fn elsp_writes(mmio_base: u32, descs: [u64; NUM_PORTS]) -> [WritePlan; 4] {
    let elsp = r::ring_elsp(mmio_base);
    let mut w = [WritePlan { reg: elsp, value: 0 }; 4];
    let mut i = 0;
    let mut n = NUM_PORTS;
    while n > 0 {
        n -= 1;
        w[i] = WritePlan { reg: elsp, value: (descs[n] >> 32) as u32 };
        w[i + 1] = WritePlan { reg: elsp, value: descs[n] as u32 };
        i += 2;
    }
    w
}

/// `reset_csb_pointers` (intel_execlists_submission.c:2804-2844): pause cleared, `RING_CONTEXT_STATUS_PTR` written with both
/// pointers at `csb_size - 1` (twice, with posting reads, "once more for luck and our trusty paranoia"), the CSB write
/// pointer dword set to the same, and every entry poisoned so a missed hardware update is detectable.
pub fn plan_reset_csb_pointers(mmio_base: u32) -> CsbResetPlan {
    let reset_value = GEN8_CSB_ENTRIES - 1;
    let ptr = r::ring_context_status_ptr(mmio_base);
    let status = 0xffff << 16 | reset_value << 8 | reset_value;
    let mut steps = FixedVec::new("csb reset steps", Step::Mmio(Effect::FILL));
    let all = [
        Step::Hwsp(HwspStore::Dword { dword: I915_GEM_HWS_PREEMPT, value: 0 }), // ring_set_paused(engine, 0)
        Step::Mmio(Effect::Write(WritePlan { reg: ptr, value: status })),
        Step::Mmio(Effect::PostingRead(ptr)),
        Step::Hwsp(HwspStore::Dword { dword: I915_HWS_CSB_WRITE_INDEX, value: reset_value }),
        Step::Hwsp(HwspStore::PoisonCsb { entries: GEN8_CSB_ENTRIES }),
        Step::Mmio(Effect::Write(WritePlan { reg: ptr, value: status })),
        Step::Mmio(Effect::PostingRead(ptr)),
    ];
    for s in all {
        let _ = steps.push(s); // 7 steps, capacity 8
    }
    CsbResetPlan { steps }
}

/// `enable_execlists` (intel_execlists_submission.c:2928-2950) without the error-interrupt arming: mask the HWSP mirror of
/// the interrupt status, turn the run list on (`GFX_RUN_LIST_ENABLE`, a masked write), clear `STOP_RING`, point the ring at its
/// status page and post the write.
pub fn plan_enable_execlists(mmio_base: u32, status_page_ggtt: u32) -> Result<FixedVec<Effect, 5>, SubmitError> {
    if status_page_ggtt & 0xfff != 0 {
        return Err(SubmitError::GgttUnaligned { what: "status page", addr: status_page_ggtt });
    }
    let mut v = FixedVec::new("enable execlists", Effect::FILL);
    let hws_pga = r::ring_hws_pga(mmio_base);
    let all = [
        Effect::Write(WritePlan { reg: r::ring_hwstam(mmio_base), value: !0 }),
        Effect::Write(WritePlan { reg: r::ring_mode_gen7(mmio_base), value: masked_bit_enable(r::GFX_RUN_LIST_ENABLE) }),
        Effect::Write(WritePlan { reg: r::ring_mi_mode(mmio_base), value: masked_bit_disable(r::STOP_RING) }),
        Effect::Write(WritePlan { reg: hws_pga, value: status_page_ggtt }),
        Effect::PostingRead(hws_pga),
    ];
    for e in all {
        let _ = v.push(e);
    }
    Ok(v)
}
