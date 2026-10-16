// SPDX-License-Identifier: GPL-2.0-only
//! i915-memory-core: the Gen9 (KBL) Global GTT, the 4-level 48-bit PPGTT, PTE/PAT encoding, VMA placement and eviction, and
//! fence registers - as PURE PLANNERS. Nothing here touches MMIO, config space or memory: [`Gtt::plan_bind`] and
//! [`Gtt::plan_unbind`] return a [`BindPlan`]/[`UnbindPlan`] - an ordered list of [`Op`]s (table-entry writes, PTE runs,
//! scratch fills, GPU-retire waits) with the [`Flush`]es that must follow - and `devices/i915` is the only thing that executes them.
//! Page-table pages are never allocated here: a [`TablePageSource`] is asked for each one through a typed [`TablePageRequest`].
//!
//! Ported from Linux drivers/gpu/drm/i915 at v7.0-rc6: gt/intel_gtt.{c,h}, gt/intel_ggtt.c, gt/gen8_ppgtt.c, gt/intel_ppgtt.c,
//! gt/intel_ggtt_fencing.c, i915_vma.c and i915_gem_gtt.c (placement and eviction POLICY only), gem/i915_gem_stolen.c and
//! arch/x86/kernel/early-quirks.c (stolen sizing). Copyright (c) Intel Corporation and the i915 authors.
//!
//! WHAT IS CARRIED: Gen9 (SKL/KBL) only - 8-byte gen8 PTEs, full 48-bit PPGTT. Gen6/7/12/MTL encoders, 3-level 32-bit PPGTT,
//! 64K/2M huge pages, local memory, vGPU ballooning, GuC GGTT regions and the "bind via a blitter context" GGTT path are CUT
//! (each is a different platform row or a feature Gen9 lacks); see docs/I915-CONTRACTS.md.
//!
//! NO COMPILE-TIME CAP ON A POPULATION CLIENTS DRIVE (owner 2026-10-12, RUNTIME-TABLES-HAVE-NO-CAP; DECISIONS.md D10): the VMA
//! table, the page-table table, the spare table-page pool, a plan's op list and its victim list are `alloc` collections that
//! grow with memory. Every growth is fallible (`try_reserve`) and refuses with the named [`MemError::OutOfMemory`] BEFORE the
//! space or the plan has changed, so a refused bind leaves no half-created node or table. FIXED, because the hardware defines
//! it: 32 fence registers ([`fence`]), the 5-step fence sequence, the 4 PPGTT levels and 512 entries per table page.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod fence;
mod gtt;
mod nodes;
mod plan;
pub mod probe;
pub mod pte;
pub mod regs;

pub use gtt::{BindRequest, Gtt, GttKind, Preallocated, ERROR_CAPTURE, NoTablePages, PpgttInit, TablePage, TablePageRequest, TablePageSource, Walk};
pub use nodes::{Mode, Placement, VmaId};
pub use plan::{BindPlan, Flush, Op, RegWrite, Target, UnbindPlan, Write};
pub use pte::CacheLevel;

use i915_common_core::I915Error;

/// 4 KiB: `I915_GTT_PAGE_SIZE` (intel_gtt.h:49), the only page size Gen9 PPGTT/GGTT entries here use.
pub const GTT_PAGE: u64 = 4096;
/// Address bits of the full PPGTT (`ppgtt_size`, i915_pci.c:418).
pub const PPGTT_BITS: u32 = 48;
/// Entries per table page: `GEN8_PDES` = 4096 / 8 (gen8_ppgtt.c:139).
pub const ENTRIES_PER_TABLE: u64 = 512;

/// Why a memory operation refused. Every variant carries the values; [`MemError::name`] is the stable marker token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemError {
    /// The platform is not Gen9 (devinfo knows it, this crate does not drive it).
    UnsupportedGeneration { gen: u8 },
    /// `GMCH_CTRL.GGMS` = 0: the platform reports no GTT.
    GgttAbsent { gmch_ctl: u16 },
    /// GTTMMADR is not the 16 MiB a Gen8+ part has.
    Bar0Size { got: u64, want: u64 },
    /// GGMS names a page table larger than the BAR0 half that holds it.
    GgttOverflowsBar { table: u64 },
    /// `GEN6_STOLEN_RESERVED` names a base outside the stolen range.
    StolenReservedOutside { reserved_base: u64, base: u64, top: u64 },
    /// A PPGTT address with a bit at or above bit 48 (the 48-bit limit).
    Beyond48Bit { va: u64 },
    /// A range that ends past the space (or wraps).
    BeyondSpace { va: u64, len: u64, total: u64 },
    /// A value that must be a multiple of `align` is not.
    Misaligned { what: &'static str, value: u64, align: u64 },
    /// An alignment that is not a power of two >= 4096.
    BadAlignment { align: u64 },
    ZeroLength,
    /// More pages than the node reserves, or a node size that is not whole pages.
    PagesExceedNode { pages: u64, node_pages: u64 },
    /// A device address that does not fit the device's DMA mask (`dma_mask_size`).
    DeviceAddrBeyondMask { addr: u64, bits: u8 },
    /// No hole (and no evictable victims) for `size` bytes.
    NoSpace { size: u64 },
    /// A fixed placement collides with a live (or unevictable) node.
    RangeOccupied { start: u64, len: u64 },
    /// A fixed placement collides with a driver RESERVATION, named by what holds it (`by`: `"adopted-scanout"` - the firmware
    /// framebuffer the display is scanning out - or `"error-capture"`).
    RangeReserved { start: u64, len: u64, by: &'static str },
    /// An access to an address that is not inside a live bound range: unbound, unbound-then-reused, or a guard/scratch page.
    StaleMapping { va: u64 },
    /// An access that lands on a guard page of a live node.
    GuardPageAccess { va: u64 },
    /// An id that names no live node.
    NotBound { id: u32 },
    /// The node is pinned (in use by a request) or a driver reservation; it cannot be unbound or evicted.
    Pinned { id: u32 },
    /// Memory for a growing table or plan list (`what`: "vma table", "page-table table", "spare pool", "plan ops", "victims",
    /// "scratch") could not be had. Refused before anything changed: the space is exactly as it was.
    OutOfMemory { what: &'static str },
    /// The caller's [`TablePageSource`] refused, or gave an unusable page.
    TablePageRefused { level: u8 },
    /// A fence parameter outside what the registers can encode.
    BadFence { what: &'static str, value: u64 },
    /// Devinfo says this part has no full 48-bit PPGTT.
    NotFullPpgtt { size_bits: u8 },
    /// Two table pages (or a table page and a scratch page) share one device address.
    DuplicatePage { addr: u64 },
    /// A cache level / flag this generation cannot express (`PTE_LM` on Gen9).
    UnsupportedFlag { flag: u32 },
}

impl MemError {
    /// The stable token printed in markers: `[I915-MEMORY FAIL why=VA-BEYOND-48BIT ...]`.
    pub const fn name(&self) -> &'static str {
        match self {
            MemError::UnsupportedGeneration { .. } => "UNSUPPORTED-GENERATION",
            MemError::GgttAbsent { .. } => "GGTT-ABSENT",
            MemError::Bar0Size { .. } => "BAR0-SIZE",
            MemError::GgttOverflowsBar { .. } => "GGTT-OVERFLOWS-BAR",
            MemError::StolenReservedOutside { .. } => "STOLEN-RESERVED-OUTSIDE",
            MemError::Beyond48Bit { .. } => "VA-BEYOND-48BIT",
            MemError::BeyondSpace { .. } => "RANGE-BEYOND-SPACE",
            MemError::Misaligned { .. } => "MISALIGNED",
            MemError::BadAlignment { .. } => "BAD-ALIGNMENT",
            MemError::ZeroLength => "ZERO-LENGTH",
            MemError::PagesExceedNode { .. } => "PAGES-EXCEED-NODE",
            MemError::DeviceAddrBeyondMask { .. } => "DEVICE-ADDR-BEYOND-MASK",
            MemError::NoSpace { .. } => "NO-SPACE",
            MemError::RangeOccupied { .. } => "RANGE-OCCUPIED",
            MemError::RangeReserved { .. } => "RANGE-RESERVED",
            MemError::StaleMapping { .. } => "STALE-MAPPING",
            MemError::GuardPageAccess { .. } => "GUARD-PAGE-ACCESS",
            MemError::NotBound { .. } => "VMA-NOT-BOUND",
            MemError::Pinned { .. } => "VMA-PINNED",
            MemError::OutOfMemory { .. } => "OUT-OF-MEMORY",
            MemError::TablePageRefused { .. } => "TABLE-PAGE-REFUSED",
            MemError::BadFence { .. } => "BAD-FENCE",
            MemError::UnsupportedFlag { .. } => "UNSUPPORTED-FLAG",
            MemError::NotFullPpgtt { .. } => "NOT-FULL-PPGTT",
            MemError::DuplicatePage { .. } => "DUPLICATE-TABLE-PAGE",
        }
    }

    /// The family-wide error: `I915Error::BadValue` carrying this refusal's stable name (the contract's enum is frozen at
    /// `CONTRACT_ABI` 1; a later ABI bump may promote these to first-class variants).
    pub fn to_i915(self) -> I915Error {
        I915Error::BadValue { what: self.name(), value: 0 }
    }
}
