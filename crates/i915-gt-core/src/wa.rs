// SPDX-License-Identifier: GPL-2.0-only
//! The workaround list: `struct i915_wa_list` and its `_wa_add` merge rule (gt/intel_workarounds.c:147-212), the `wa_*` builder
//! family (:216-334), the apply planner (`wa_list_apply`, :1757-1805) and the verify step (`wa_verify`, :1742-1755;
//! `wa_list_verify`, :1810-1836). Pure data in, effects out.
use crate::effect::{Effect, RmwPlan, WritePlan};
use crate::error::GtError;
use crate::fixed::FixedVec;
use i915_common_core::reg::try_masked_field;
use i915_common_core::Reg;

/// `struct i915_wa` (intel_workarounds_types.h): one register's accumulated programming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wa {
    pub reg: Reg,
    /// Bits cleared before `set` is applied.
    pub clr: u32,
    /// Bits (or the whole masked-register word) written.
    pub set: u32,
    /// The bits `wa_verify` compares on read-back.
    pub read: u32,
    /// `masked_reg`: the upper 16 bits of `set` are the write-enable mask.
    pub masked: bool,
    /// `is_mcr`: the register is multicast/replicated (steered by `GEN8_MCR_SELECTOR` on Gen9).
    pub mcr: bool,
}

impl Wa {
    pub const EMPTY: Wa = Wa { reg: Reg::new(0), clr: 0, set: 0, read: 0, masked: false, mcr: false };

    /// `wa_verify` (intel_workarounds.c:1742-1755): `(cur ^ set) & read == 0`.
    pub const fn holds(&self, cur: u32) -> bool {
        (cur ^ self.set) & self.read == 0
    }
}

/// KEPT FIXED (D10): `N` is the size of one platform's static workaround table (Linux `wal` for an engine/GT), known per platform - a
/// hardware sequence, not a client population.
/// A sorted, merged workaround list of at most `N` registers. `name`/`engine` are Linux's `wal->name` / `wal->engine_name`.
#[derive(Clone, Copy, Debug)]
pub struct WaList<const N: usize> {
    pub name: &'static str,
    pub engine: &'static str,
    list: FixedVec<Wa, N>,
    /// `wal->wa_count`: how many `wa_*` calls landed (a merged call still counts).
    pub wa_count: u32,
    /// How many times `_wa_add` hit its "Discarding overwritten w/a" branch (intel_workarounds.c:184-190).
    pub overwrites: u32,
}

impl<const N: usize> WaList<N> {
    pub const fn new(name: &'static str, engine: &'static str) -> Self {
        WaList { name, engine, list: FixedVec::new(name, Wa::EMPTY), wa_count: 0, overwrites: 0 }
    }
    pub fn entries(&self) -> &[Wa] {
        self.list.as_slice()
    }
    pub fn count(&self) -> usize {
        self.list.len()
    }
    pub fn find(&self, reg: Reg) -> Option<&Wa> {
        self.list.as_slice().iter().find(|w| w.reg == reg)
    }

    /// `_wa_add` (intel_workarounds.c:147-212): binary search by offset; a hit merges (set |= , clr |= , read |=) after the
    /// overwrite check, a miss inserts keeping the list sorted.
    pub fn add(&mut self, wa: Wa) -> Result<(), GtError> {
        let addr = wa.reg.offset();
        let (mut start, mut end) = (0usize, self.list.len());
        while start < end {
            let mid = start + (end - start) / 2;
            let m = self.list.as_slice()[mid].reg.offset();
            if m < addr {
                start = mid + 1;
            } else if m > addr {
                end = mid;
            } else {
                let e = self.list.get_mut(mid);
                // intel_workarounds.c:184: the new entry clears bits the old one set -> Linux logs and drops them from set.
                if (wa.clr | e.clr) != 0 && (wa.clr & !e.clr) == 0 {
                    self.overwrites += 1;
                    e.set &= !wa.clr;
                }
                self.wa_count += 1;
                e.set |= wa.set;
                e.clr |= wa.clr;
                e.read |= wa.read;
                return Ok(());
            }
        }
        self.list.insert(start, wa)?;
        self.wa_count += 1;
        Ok(())
    }

    // ---- the wa_* builder family (intel_workarounds.c:201-334) -------------------------------------------------------------

    /// `wa_add` / `wa_mcr_add`.
    pub fn wa_add(&mut self, reg: Reg, mcr: bool, clr: u32, set: u32, read: u32, masked: bool) -> Result<(), GtError> {
        self.add(Wa { reg, clr, set, read, masked, mcr })
    }
    /// `wa_write_clr_set`: read = clear | set.
    pub fn write_clr_set(&mut self, reg: Reg, mcr: bool, clear: u32, set: u32) -> Result<(), GtError> {
        self.wa_add(reg, mcr, clear, set, clear | set, false)
    }
    /// `wa_write`: clear everything, set `set` (intel_workarounds.c:258-260).
    pub fn write(&mut self, reg: Reg, mcr: bool, set: u32) -> Result<(), GtError> {
        self.write_clr_set(reg, mcr, !0, set)
    }
    /// `wa_write_or`: clear and set the same bits.
    pub fn write_or(&mut self, reg: Reg, mcr: bool, set: u32) -> Result<(), GtError> {
        self.write_clr_set(reg, mcr, set, set)
    }
    /// `wa_write_clr`.
    pub fn write_clr(&mut self, reg: Reg, mcr: bool, clr: u32) -> Result<(), GtError> {
        self.write_clr_set(reg, mcr, clr, 0)
    }
    /// `wa_masked_en` = `_MASKED_BIT_ENABLE(val)`, read = val.
    pub fn masked_en(&mut self, reg: Reg, mcr: bool, val: u32) -> Result<(), GtError> {
        let set = try_masked_field(val, val).ok_or(GtError::BadMaskedValue { reg, mask: val, value: val })?;
        self.wa_add(reg, mcr, 0, set, val, true)
    }
    /// `wa_masked_dis` = `_MASKED_BIT_DISABLE(val)`, read = val.
    pub fn masked_dis(&mut self, reg: Reg, mcr: bool, val: u32) -> Result<(), GtError> {
        let set = try_masked_field(val, 0).ok_or(GtError::BadMaskedValue { reg, mask: val, value: 0 })?;
        self.wa_add(reg, mcr, 0, set, val, true)
    }
    /// `wa_masked_field_set` = `_MASKED_FIELD(mask, val)`, read = mask.
    pub fn masked_field_set(&mut self, reg: Reg, mcr: bool, mask: u32, val: u32) -> Result<(), GtError> {
        let set = try_masked_field(mask, val).ok_or(GtError::BadMaskedValue { reg, mask, value: val })?;
        self.wa_add(reg, mcr, 0, set, mask, true)
    }
}

// ---- the planners --------------------------------------------------------------------------------------------------------

/// Forcewake domains `wal_get_fw_for_rmw` would hold (intel_workarounds.c:131-145): the union of every entry's read and write
/// domains. Uses the Gen9 `fw_ranges` of i915-uncore-core.
pub fn fw_domains_for_rmw<const N: usize>(wal: &WaList<N>) -> u32 {
    let mut fw = 0;
    for wa in wal.entries() {
        fw |= i915_uncore_core::read_fw_domains(wa.reg.offset()) | i915_uncore_core::write_fw_domains(wa.reg.offset());
    }
    fw
}

/// What `wa_list_apply` does (intel_workarounds.c:1757-1802), as a plan: take `forcewake`, then per entry either a plain write
/// (`clr == 0`: Linux writes `val = set` unconditionally) or a read-modify-write that writes only when the value changes.
#[derive(Clone, Debug)]
pub struct WaApplyPlan<const N: usize> {
    pub forcewake: u32,
    pub effects: FixedVec<Effect, N>,
}

pub fn plan_apply<const N: usize>(wal: &WaList<N>) -> Result<WaApplyPlan<N>, GtError> {
    let mut effects = FixedVec::<Effect, N>::new("wa apply plan", Effect::FILL);
    for wa in wal.entries() {
        if wa.clr != 0 {
            effects.push(Effect::Rmw(RmwPlan { reg: wa.reg, clear: wa.clr, set: wa.set, skip_if_unchanged: true }))?;
        } else {
            effects.push(Effect::Write(WritePlan { reg: wa.reg, value: wa.set }))?;
        }
    }
    Ok(WaApplyPlan { forcewake: fw_domains_for_rmw(wal), effects })
}

/// `wa_list_verify` (intel_workarounds.c:1810-1836) over register values the caller read: every entry must read back the bits
/// it programmed. Returns the number of entries checked, or the FIRST lost workaround by name (Linux counts them all and
/// prints each; a driver wants to know which register, so the first is returned and `lost_count` gives the total).
pub fn verify<const N: usize>(wal: &WaList<N>, mut read: impl FnMut(Reg) -> u32) -> Result<usize, GtError> {
    let mut first: Option<GtError> = None;
    for wa in wal.entries() {
        let cur = read(wa.reg);
        if !wa.holds(cur) && first.is_none() {
            first = Some(GtError::WaLost { list: wal.name, reg: wa.reg, got: cur, relevant: cur & wa.read, expected: wa.set & wa.read });
        }
    }
    match first {
        Some(e) => Err(e),
        None => Ok(wal.count()),
    }
}

/// How many entries of `wal` do NOT hold against `read` (the count Linux's `ok &= wa_verify(..)` loop would report).
pub fn lost_count<const N: usize>(wal: &WaList<N>, mut read: impl FnMut(Reg) -> u32) -> usize {
    wal.entries().iter().filter(|wa| !wa.holds(read(wa.reg))).count()
}
