// SPDX-License-Identifier: GPL-2.0-only
//! Identifiers. None is ever reused while a holder could still present it: that is what turns "use after close" and "use of
//! another session's name" into named refusals instead of silent aliasing.

/// One incarnation of one session (`struct drm_file`). Allocated from a counter that never wraps in practice (u64) and is
/// never reused, so a closed session's id can never name a later session (KBL-I915-PORT-PLAN section 2: "sole authority for
/// GEM/session incarnations").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

/// An object in the global slab: `generation << 32 | index`. The generation changes whenever the slot is reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjId(pub u64);

impl ObjId {
    pub(crate) const fn new(gen: u32, idx: u32) -> ObjId {
        ObjId((gen as u64) << 32 | idx as u64)
    }
    pub(crate) const fn slot(self) -> u32 {
        self.0 as u32
    }
    pub const fn generation(self) -> u32 {
        (self.0 >> 32) as u32
    }
    /// The slot index, for diagnostics that carry a `u32`.
    pub const fn index(self) -> u32 {
        self.0 as u32
    }
}

/// A VM in the global VM table (not the user-visible VM id, which is per session).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VmId(pub u32);

/// A request, from creation to retirement. Never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub u64);

/// Bits of a user handle that index its session's table. A handle is a `u32` on the wire (`drm_i915_gem_exec_object2::handle`,
/// `GEM_CLOSE`...), so its encoding - not a table size - bounds one session at `2^20 - 1` live handles; the other 12 bits are the
/// slot's generation (a slot is retired after 4095 reuses). Exhausting the index space is `HANDLE-SPACE-EXHAUSTED`.
pub const HANDLE_IDX_BITS: u32 = 20;
pub(crate) const HANDLE_IDX_MASK: u32 = (1 << HANDLE_IDX_BITS) - 1;
/// The highest generation a handle slot may carry (`2^12 - 1`).
pub(crate) const HANDLE_MAX_GEN: u32 = (1 << (32 - HANDLE_IDX_BITS)) - 1;

/// A user-visible object handle is `generation << 20 | (index + 1)`: never zero, and a slot's next occupant gets a new generation.
pub(crate) const fn handle_of(gen: u32, idx: u32) -> u32 {
    gen << HANDLE_IDX_BITS | (idx + 1)
}
