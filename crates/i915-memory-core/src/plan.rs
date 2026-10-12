// SPDX-License-Identifier: GPL-2.0-only
//! The effect lists the planners return. A plan is DATA: an ordered list of [`Op`]s, expandable to individual [`Write`]s
//! ([`BindPlan::writes`]) and the [`Flush`]es that must follow them ([`BindPlan::flushes`]). The executor (`devices/i915`)
//! applies the ops in order, then the flushes, and nothing else; the planner has already updated its model on the assumption
//! that the plan WILL be applied (a plan that cannot be applied means the space must be rebuilt - there is no half-applied state
//! to roll back to in hardware).
//!
//! Ordering guarantees the executor may rely on: a table page is filled (`FillTable`) before the `TableEntry` that links it;
//! a victim's `AwaitRetire` precedes the scratch fills that unmap it; every victim is unmapped before the new range is written.

use crate::nodes::VmaId;
use crate::pte::{gen8_ggtt_pte_encode, gen8_pte_encode, CacheLevel};
use crate::regs::{GFX_FLSH_CNTL_EN, GFX_FLSH_CNTL_GEN6};
use crate::{GttKind, MemError, ENTRIES_PER_TABLE};
use alloc::vec::Vec;
use i915_common_core::Reg;


/// One step of a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Wait until the GPU no longer uses this VMA (`i915_vma_unbind` waits on its active tracker). Zero writes.
    AwaitRetire { vma: VmaId },
    /// Fill a whole table page (512 entries) with one constant entry (`fill_px`).
    FillTable { table: u64, entry: u64 },
    /// One entry of a table page (`set_pd_entry` / `clear_pd_entry`).
    TableEntry { table: u64, index: u16, value: u64 },
    /// `count` leaf PTEs of a page table from `index`, for `pages[page_offset .. page_offset + count]`.
    PteRun { table: u64, index: u16, count: u16, page_offset: u32 },
    /// `count` leaf PTEs of a page table from `index`, all `value` (scratch).
    PteFill { table: u64, index: u16, count: u16, value: u64 },
    /// `count` GGTT entries from entry `first`, for `pages[page_offset ..]`.
    GgttRun { first: u32, count: u32, page_offset: u32 },
    /// `count` GGTT entries from entry `first`, all `value` (scratch).
    GgttFill { first: u32, count: u32, value: u64 },
}

impl Op {
    /// 64-bit stores this op performs.
    pub const fn entries(&self) -> u32 {
        match *self {
            Op::AwaitRetire { .. } => 0,
            Op::FillTable { .. } => ENTRIES_PER_TABLE as u32,
            Op::TableEntry { .. } => 1,
            Op::PteRun { count, .. } | Op::PteFill { count, .. } => count as u32,
            Op::GgttRun { count, .. } | Op::GgttFill { count, .. } => count,
        }
    }
}

/// Where a 64-bit entry goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A GGTT entry: a store into BAR0 at this byte offset (GTTADR, 8 MiB in).
    Ggtt { mmio_offset: u32 },
    /// An entry of a DMA table page: the page's DEVICE address and the byte offset in it. The executor maps the CPU side from
    /// the page's `DmaGrant`; the planner never learns a CPU address.
    Table { device_addr: u64, byte_offset: u16 },
}

/// One 64-bit entry store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Write {
    pub target: Target,
    pub value: u64,
}

/// A 32-bit register write (the guard's allow-list decides whether the executor may issue it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegWrite {
    pub reg: Reg,
    pub value: u32,
}

/// What must happen after the entry stores, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flush {
    /// Flush the CPU cache lines of `bytes` at `byte_offset` in a table page so the GPU's page walker sees the stores
    /// (`drm_clflush_virt_range`, gen8_ppgtt.c:781).
    CpuCache { device_addr: u64, byte_offset: u16, bytes: u16 },
    /// Invalidate the GGTT TLB and drain the write-combining buffer of the entry stores: one uncached register write
    /// (`gen8_ggtt_invalidate`, intel_ggtt.c:217; the Gen9 non-LP path).
    GgttInvalidate(RegWrite),
    /// The PPGTT TLB must be invalidated by the next batch's flush (`MI_FLUSH_DW`/`PIPE_CONTROL` with TLB invalidate) - an
    /// engine-level step owned by `i915-gt-core`; the planner only states the requirement.
    PpgttTlb,
}

/// The op list of one plan. Its length follows the size of the bind (about three ops per fresh 2 MiB), a count the client
/// chooses, so it has no compile-time cap: it grows with memory and a refused growth is [`MemError::OutOfMemory`] (`plan ops`).
#[derive(Clone, Debug)]
pub struct Ops {
    arr: Vec<Op>,
}

impl Ops {
    pub const fn new() -> Ops {
        Ops { arr: Vec::new() }
    }
    pub fn push(&mut self, op: Op) -> Result<(), MemError> {
        self.arr.try_reserve(1).map_err(|_| MemError::OutOfMemory { what: "plan ops" })?;
        self.arr.push(op);
        Ok(())
    }
    pub fn as_slice(&self) -> &[Op] {
        &self.arr
    }
}

/// The result of [`crate::Gtt::plan_bind`] (and, with no pages, of [`crate::Gtt::plan_unbind`]).
#[derive(Clone, Debug)]
pub struct BindPlan<'a> {
    pub(crate) kind: GttKind,
    pub(crate) vma: VmaId,
    pub(crate) offset: u64,
    pub(crate) node_start: u64,
    pub(crate) node_size: u64,
    pub(crate) pages: &'a [u64],
    pub(crate) cache: CacheLevel,
    pub(crate) pte_flags: u32,
    pub(crate) ops: Ops,
    pub(crate) evicted: Vec<VmaId>,
    pub(crate) new_tables: u32,
}

/// The result of [`crate::Gtt::plan_unbind`]: a plan with no pages.
pub type UnbindPlan = BindPlan<'static>;

impl<'a> BindPlan<'a> {
    pub fn vma(&self) -> VmaId {
        self.vma
    }
    /// The address the GPU uses for the first byte of the data (`i915_vma_offset`): past the leading guard.
    pub fn offset(&self) -> u64 {
        self.offset
    }
    /// The whole reserved range, guard pages included.
    pub fn node(&self) -> (u64, u64) {
        (self.node_start, self.node_size)
    }
    pub fn ops(&self) -> &[Op] {
        self.ops.as_slice()
    }
    /// VMAs this plan evicted; each has its `AwaitRetire` + scratch fill in [`BindPlan::ops`] ahead of the new range.
    pub fn evicted(&self) -> &[VmaId] {
        &self.evicted
    }
    /// Table pages this plan consumed from the caller's [`crate::TablePageSource`] (or the spare pool).
    pub fn new_tables(&self) -> u32 {
        self.new_tables
    }

    fn write_of(&self, op: &Op, i: u32) -> Write {
        let page = |off: u32| self.pages[(off + i) as usize];
        match *op {
            Op::AwaitRetire { .. } => unreachable!("AwaitRetire has no entries"),
            Op::FillTable { table, entry } => Write { target: Target::Table { device_addr: table, byte_offset: (i * 8) as u16 }, value: entry },
            Op::TableEntry { table, index, value } => Write { target: Target::Table { device_addr: table, byte_offset: index * 8 }, value },
            Op::PteRun { table, index, page_offset, .. } => Write {
                target: Target::Table { device_addr: table, byte_offset: (index + i as u16) * 8 },
                value: gen8_pte_encode(page(page_offset), self.cache, self.pte_flags),
            },
            Op::PteFill { table, index, value, .. } => Write { target: Target::Table { device_addr: table, byte_offset: (index + i as u16) * 8 }, value },
            Op::GgttRun { first, page_offset, .. } => Write {
                target: Target::Ggtt { mmio_offset: crate::regs::GEN8_GTTADR_OFFSET + (first + i) * 8 },
                value: gen8_ggtt_pte_encode(page(page_offset)),
            },
            Op::GgttFill { first, value, .. } => Write { target: Target::Ggtt { mmio_offset: crate::regs::GEN8_GTTADR_OFFSET + (first + i) * 8 }, value },
        }
    }

    /// Every 64-bit store of the plan, in order, expanded lazily (nothing is allocated).
    pub fn writes(&self) -> impl Iterator<Item = Write> + '_ {
        self.ops().iter().flat_map(move |op| (0..op.entries()).map(move |i| self.write_of(op, i)))
    }

    /// The flushes to run after all stores: a CPU-cache flush per touched table span, then the one TLB step for the space.
    pub fn flushes(&self) -> impl Iterator<Item = Flush> + '_ {
        let tail = match self.kind {
            GttKind::Ggtt => Flush::GgttInvalidate(RegWrite { reg: GFX_FLSH_CNTL_GEN6, value: GFX_FLSH_CNTL_EN }),
            GttKind::Ppgtt => Flush::PpgttTlb,
        };
        self.ops()
            .iter()
            .filter_map(|op| match *op {
                Op::FillTable { table, .. } => Some(Flush::CpuCache { device_addr: table, byte_offset: 0, bytes: 4096 }),
                Op::TableEntry { table, index, .. } => Some(Flush::CpuCache { device_addr: table, byte_offset: index * 8, bytes: 8 }),
                Op::PteRun { table, index, count, .. } | Op::PteFill { table, index, count, .. } => {
                    Some(Flush::CpuCache { device_addr: table, byte_offset: index * 8, bytes: count * 8 })
                }
                _ => None,
            })
            .chain(core::iter::once(tail))
    }
}
