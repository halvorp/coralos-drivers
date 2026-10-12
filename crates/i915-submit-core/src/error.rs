// SPDX-License-Identifier: GPL-2.0-only
//! The refusals of the submission core. Every one names WHAT refused and WHY (PORT_CONVENTIONS.md section 5);
//! [`SubmitError::name`] is the stable token a device prints in its marker lines. Where Linux has a `GEM_BUG_ON` on a
//! precondition, the port refuses by name instead; where Linux blocks (`wait_for_space`), the port refuses and lets the
//! caller retire and retry.
use i915_gt_core::GtError;

/// Why a submission-core operation refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    /// A refusal from `i915-gt-core` (command emitters, fixed lists).
    Gt(GtError),
    /// Only Gen8 and Gen9 context images are carried; Gen9 is the target, Gen8 is data-only.
    UnsupportedGeneration { gen: u8 },
    /// `intel_engine_create_ring`: size is not a power of two in 4 KiB..=2 MiB (`RING_CTL_SIZE(size) & ~RING_NR_PAGES`).
    BadRingSize { size: u32 },
    /// `intel_ring_begin`: packets must be qword aligned (`GEM_BUG_ON(num_dwords & 1)`).
    RingOddDwords { dwords: u32 },
    /// `intel_ring_begin`: the packet plus the reserved space can never fit (`GEM_BUG_ON(total_bytes > effective_size)`).
    RingRequestTooLarge { need: u32, effective: u32 },
    /// `intel_ring_begin` -> `wait_for_space`: not enough free ring space. Retire completed requests and retry.
    RingFull { need: u32, space: u32 },
    /// An emitter wrote a different number of dwords than the ring reservation (`intel_ring_advance`'s `GEM_BUG_ON`).
    RingEmitMismatch { reserved: u32, wrote: u32 },
    /// `intel_ring_offset_valid`: not strictly inside the ring, or not qword aligned.
    RingOffsetInvalid { pos: u32, size: u32 },
    /// `assert_ring_tail_valid`: the tail would advance into the head's cacheline while behind it.
    RingTailInvalid { tail: u32, head: u32 },
    /// A register-state offset table is malformed (an LRI not followed by `count` registers, a bad register offset).
    LrcTableMalformed { table: &'static str, at: u32 },
    /// A register-state write fell outside the 4 KiB state page.
    LrcOverrun { index: u32, limit: u32 },
    /// `i915_ggtt_offset(ce->state)` is not page aligned, so it cannot be an LRCA (`CTX_GTT_ADDRESS_MASK`).
    LrcaUnaligned { lrca: u32 },
    /// A ring / wa-batch GGTT address that must be page aligned is not.
    GgttUnaligned { what: &'static str, addr: u32 },
    /// The wa batch buffer scratch address must leave room for the COHERENTL3_WA field (offset 256).
    ScratchOverflow { scratch: u32 },
    /// `process_csb`: the CSB write pointer is not a valid entry index (Linux would spin forever on it).
    CsbWritePtrOutOfRange { ptr: u32, entries: u32 },
    /// `csb_read`: the entry is still the all-ones "unwritten" poison and the MMIO fallback did not produce one either.
    CsbEntryStale { index: u32 },
    /// `process_csb`: the hardware promoted a pending submission but none was outstanding (`ERROR_CSB`).
    CsbPromoteWithoutPending,
    /// `process_csb`: the hardware completed a port but nothing was in flight (`ERROR_CSB`).
    CsbCompleteWithoutActive,
    /// `execlists_dequeue_irq` is only entered with nothing pending; a submission is still waiting for its ack.
    ElspPendingUnacked,
    /// A second context table slot was asked for beyond the engine's capacity.
    ContextTableFull { capacity: u32 },
    /// A request table slot was asked for beyond the engine's capacity.
    RequestTableFull { capacity: u32 },
    /// No such context.
    NoSuchContext { id: u32 },
    /// A port names a request the table no longer holds (an internal inconsistency, never silently skipped).
    NoSuchRequest { ctx: u32, seqno: u32 },
    /// The context still has requests or is in flight (`intel_context_is_active`).
    ContextBusy { id: u32 },
    /// The 63 hardware context tags (`GENMASK(BITS_PER_LONG - 2, 0)`) are all in use.
    TagsExhausted,
    /// The timeline's HWSP seqno slot is not usable (outside the driver area, odd, or bit 5 set).
    SeqnoSlotInvalid { addr: u32, why: &'static str },
    /// The u32 timeline seqno would wrap (`__intel_timeline_get_seqno` rotates the HWSP slot; not ported).
    SeqnoWrap { seqno: u32 },
    /// A bound of zero was supplied (a watchdog that may never fire is not a bound).
    ZeroBound { what: &'static str },
    /// The oldest request made no progress for the bound: declared hung by name.
    Hung { ctx: u32, seqno: u32, stalled: u64, bound: u64 },
    /// An ELSP submission was never acknowledged by a CSB event within the bound.
    ElspNotAcked { waited: u64, bound: u64 },
}

impl From<GtError> for SubmitError {
    fn from(e: GtError) -> Self {
        SubmitError::Gt(e)
    }
}

impl SubmitError {
    pub const fn name(&self) -> &'static str {
        match self {
            SubmitError::Gt(e) => e.name(),
            SubmitError::UnsupportedGeneration { .. } => "UNSUPPORTED-GENERATION",
            SubmitError::BadRingSize { .. } => "RING-BAD-SIZE",
            SubmitError::RingOddDwords { .. } => "RING-ODD-DWORDS",
            SubmitError::RingRequestTooLarge { .. } => "RING-REQUEST-TOO-LARGE",
            SubmitError::RingFull { .. } => "RING-FULL",
            SubmitError::RingEmitMismatch { .. } => "RING-EMIT-MISMATCH",
            SubmitError::RingOffsetInvalid { .. } => "RING-OFFSET-INVALID",
            SubmitError::RingTailInvalid { .. } => "RING-TAIL-INVALID",
            SubmitError::LrcTableMalformed { .. } => "LRC-TABLE-MALFORMED",
            SubmitError::LrcOverrun { .. } => "LRC-OVERRUN",
            SubmitError::LrcaUnaligned { .. } => "LRCA-UNALIGNED",
            SubmitError::GgttUnaligned { .. } => "GGTT-UNALIGNED",
            SubmitError::ScratchOverflow { .. } => "SCRATCH-OVERFLOW",
            SubmitError::CsbWritePtrOutOfRange { .. } => "CSB-WRITE-PTR-OUT-OF-RANGE",
            SubmitError::CsbEntryStale { .. } => "CSB-ENTRY-STALE",
            SubmitError::CsbPromoteWithoutPending => "CSB-PROMOTE-WITHOUT-PENDING",
            SubmitError::CsbCompleteWithoutActive => "CSB-COMPLETE-WITHOUT-ACTIVE",
            SubmitError::ElspPendingUnacked => "ELSP-PENDING-UNACKED",
            SubmitError::ContextTableFull { .. } => "CONTEXT-TABLE-FULL",
            SubmitError::RequestTableFull { .. } => "REQUEST-TABLE-FULL",
            SubmitError::NoSuchContext { .. } => "NO-SUCH-CONTEXT",
            SubmitError::NoSuchRequest { .. } => "NO-SUCH-REQUEST",
            SubmitError::ContextBusy { .. } => "CONTEXT-BUSY",
            SubmitError::TagsExhausted => "CONTEXT-TAGS-EXHAUSTED",
            SubmitError::SeqnoSlotInvalid { .. } => "SEQNO-SLOT-INVALID",
            SubmitError::SeqnoWrap { .. } => "SEQNO-WRAP",
            SubmitError::ZeroBound { .. } => "ZERO-BOUND",
            SubmitError::Hung { .. } => "ENGINE-HUNG",
            SubmitError::ElspNotAcked { .. } => "ELSP-NOT-ACKED",
        }
    }
}
