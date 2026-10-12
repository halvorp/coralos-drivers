// SPDX-License-Identifier: GPL-2.0-only
//! The refusals of the GT core. Every one names WHAT refused and WHY (PORT_CONVENTIONS.md section 5); [`GtError::name`] is the
//! stable token a device prints in its marker lines.
use i915_common_core::Reg;

/// Which wait of the reset sequence ran out of its bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetStage {
    /// `gen8_engine_reset_prepare`: RESET_CTL READY_TO_RESET (or CAT_ERROR clear) never came back (intel_reset.c:560-598).
    EngineReadyToReset,
    /// `gen6_hw_domain_reset`: GDRST never cleared the requested domain bits (intel_reset.c:289-298).
    GdrstAck,
}

/// Why a GT-core operation refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GtError {
    /// A fixed-capacity list is full (Linux: `kmalloc` failure, "No space for workaround init!", intel_workarounds.c:162).
    ListFull { list: &'static str, capacity: u32 },
    /// A masked-register workaround was given a mask or value that does not fit the 16-bit masked encoding.
    BadMaskedValue { reg: Reg, mask: u32, value: u32 },
    /// More than `RING_MAX_NONPRIV_SLOTS` whitelist entries (intel_workarounds.c:1867).
    NonprivSlotsFull { limit: u32 },
    /// A whitelist entry carries flag bits `RING_FORCE_TO_NONPRIV_MASK_VALID` does not allow, or the INVALID access mode
    /// (`is_nonpriv_flags_valid`, intel_workarounds.c:1846-1859).
    NonprivFlagsInvalid { flags: u32 },
    /// A workaround register no longer holds the programmed bits (`wa_verify`, intel_workarounds.c:1742-1755).
    WaLost { list: &'static str, reg: Reg, got: u32, relevant: u32, expected: u32 },
    /// A command emitter ran out of room in the caller's dword buffer.
    CsOverflow { need: u32, have: u32 },
    /// The request is for a graphics generation this crate does not port.
    UnsupportedGeneration { gen: u8 },
    /// An engine id the Kaby Lake table does not carry.
    NoSuchEngine { id: u32 },
    /// A MOCS L3CC entry has the skip-caching bits set (`WaDisableSkipCaching`, intel_mocs.c:540-548): Linux programs NO MOCS.
    MocsSkipCaching { index: u32, l3cc: u16 },
    /// A MOCS table with more entries than the hardware has.
    MocsTableTooLarge { size: u32, n_entries: u32 },
    /// A context workaround needs the register's current value and the caller supplied none.
    CtxWaNeedsRead { reg: Reg },
    /// A reset wait ran out of its bound: the register never took the wanted value in `polls` reads.
    ResetTimeout { stage: ResetStage, reg: Reg, mask: u32, want: u32, got: u32, polls: u32 },
    /// The reset planner was driven out of order (an observation that does not answer the pending step).
    ResetOutOfOrder { expected: &'static str },
    /// A GGTT write offset the command cannot encode (Linux `GEM_BUG_ON`: gen8_engine_cs.h:95,132,134).
    BadGgttOffset { offset: u32, why: &'static str },
    /// The fuse decode left no enabled slice, so there is no slice/subslice pair to steer MCR reads to (`GEM_BUG_ON(slice >= ..)`,
    /// intel_workarounds.c:1160).
    NoEnabledSlice,
    /// The first enabled slice has no enabled subslice (`GEM_BUG_ON(!subslice)`, intel_workarounds.c:1163).
    NoEnabledSubslice { slice: u32 },
    /// A bound of zero was supplied (a wait that may not poll can never succeed).
    ZeroBound { what: &'static str },
    /// A GT interrupt bank index outside 0..=3.
    BadIrqBank { bank: u32 },
    /// A requested slice / subslice / EU count does not fit its RPCS field (`GEM_BUG_ON(val & ~mask)`, intel_sseu.c:731,741,751,757).
    RpcsFieldOverflow { field: &'static str, value: u32 },
}

impl GtError {
    pub const fn name(&self) -> &'static str {
        match self {
            GtError::ListFull { .. } => "LIST-FULL",
            GtError::BadMaskedValue { .. } => "BAD-MASKED-VALUE",
            GtError::NonprivSlotsFull { .. } => "NONPRIV-SLOTS-FULL",
            GtError::NonprivFlagsInvalid { .. } => "NONPRIV-FLAGS-INVALID",
            GtError::WaLost { .. } => "WA-LOST",
            GtError::CsOverflow { .. } => "CS-OVERFLOW",
            GtError::UnsupportedGeneration { .. } => "UNSUPPORTED-GENERATION",
            GtError::NoSuchEngine { .. } => "NO-SUCH-ENGINE",
            GtError::MocsSkipCaching { .. } => "MOCS-SKIP-CACHING",
            GtError::MocsTableTooLarge { .. } => "MOCS-TABLE-TOO-LARGE",
            GtError::CtxWaNeedsRead { .. } => "CTX-WA-NEEDS-READ",
            GtError::ResetTimeout { stage: ResetStage::EngineReadyToReset, .. } => "RESET-READY-TIMEOUT",
            GtError::ResetTimeout { stage: ResetStage::GdrstAck, .. } => "GDRST-ACK-TIMEOUT",
            GtError::ResetOutOfOrder { .. } => "RESET-OUT-OF-ORDER",
            GtError::BadGgttOffset { .. } => "BAD-GGTT-OFFSET",
            GtError::NoEnabledSlice => "NO-ENABLED-SLICE",
            GtError::NoEnabledSubslice { .. } => "NO-ENABLED-SUBSLICE",
            GtError::ZeroBound { .. } => "ZERO-BOUND",
            GtError::BadIrqBank { .. } => "BAD-IRQ-BANK",
            GtError::RpcsFieldOverflow { .. } => "RPCS-FIELD-OVERFLOW",
        }
    }
}
