// SPDX-License-Identifier: GPL-2.0-only
//! Identifiers. None is ever reused while a holder could still present it: that is what turns "use after close" and "use of
//! another session's name" into named refusals instead of silent aliasing.

/// One incarnation of one session (`struct drm_file`). Allocated from a counter that never wraps in practice (u64) and is
/// never reused, so a closed session's id can never name a later session (KBL-I915-PORT-PLAN section 2: "sole authority for
/// GEM/session incarnations").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

/// An object in the global arena: `generation << 16 | slot`. The generation changes whenever the slot is reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjId(pub u32);

impl ObjId {
    pub(crate) const fn new(gen: u16, slot: u16) -> ObjId {
        ObjId((gen as u32) << 16 | slot as u32)
    }
    pub(crate) const fn slot(self) -> usize {
        (self.0 & 0xffff) as usize
    }
    pub const fn generation(self) -> u16 {
        (self.0 >> 16) as u16
    }
}

/// A VM in the global VM arena (not the user-visible VM id, which is per session).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VmId(pub u8);

/// A request, from creation to retirement. Never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub u64);

/// A user-visible object handle is `generation << 16 | (slot + 1)`: never zero, and a slot's next occupant gets a new generation.
pub(crate) const fn handle_of(gen: u16, slot: usize) -> u32 {
    (gen as u32) << 16 | (slot as u32 + 1)
}
