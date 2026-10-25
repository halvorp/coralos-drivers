// SPDX-License-Identifier: GPL-2.0-only
//! [`Gtt`]: one address space (the GGTT or one full PPGTT) as a planner. It owns the VMA node table, the table-page tree (PPGTT)
//! and the scratch entries, and turns [`BindRequest`]s into [`BindPlan`]s.
//!
//! Ported from gt/gen8_ppgtt.c (`gen8_ppgtt_create`, `gen8_init_scratch`, `gen8_alloc_top_pd`, `__gen8_ppgtt_alloc`,
//! `__gen8_ppgtt_clear`, `gen8_ppgtt_insert_entry`), gt/intel_ppgtt.c (`__set_pd_entry`), gt/intel_ggtt.c (`init_ggtt`,
//! `gen8_ggtt_insert_entries`, `gen8_ggtt_clear_range`), and i915_vma.c `i915_vma_insert` (size, alignment, guard, range and
//! bias rules). Deliberate deviations from Linux, each stated where it happens: empty table pages are KEPT after an unbind (the
//! hal `Dma` seam has no free; `release_pd_entry` frees them in Linux); guard pages are written as scratch explicitly on every
//! bind in both spaces (Linux relies on `fill_px` of fresh tables for the PPGTT).

use crate::nodes::{Node, Nodes, Choice, Mode, Placement, VmaId};
use crate::plan::{BindPlan, Op, Ops, UnbindPlan};
use crate::probe::GgttGeometry;
use crate::pte::{gen8_ggtt_pte_encode, gen8_pde_encode, gen8_pte_encode, CacheLevel, PTE_LM, PTE_READ_ONLY};
use crate::{MemError, ENTRIES_PER_TABLE, GTT_PAGE, PPGTT_BITS};
use alloc::vec::Vec;
use i915_devinfo_core::{PlatformDescriptor, PpgttType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GttKind {
    Ggtt,
    Ppgtt,
}

/// A request for one page-table page, made to the caller. `level` is the table's level (0 = PT, 1 = PD, 2 = PDP; the PML4 is
/// supplied at construction) and `base_va` the first address the table covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TablePageRequest {
    pub level: u8,
    pub base_va: u64,
}

/// A granted table page: the DEVICE address (4 KiB aligned, inside the DMA mask). The caller keeps the CPU mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TablePage {
    pub device_addr: u64,
}

/// Where table pages come from. Implemented by the executor over `coralos_hal::Dma::alloc` (never identity-mapped).
pub trait TablePageSource {
    fn table_page(&mut self, req: TablePageRequest) -> Result<TablePage, MemError>;
}

impl<F: FnMut(TablePageRequest) -> Result<TablePage, MemError>> TablePageSource for F {
    fn table_page(&mut self, req: TablePageRequest) -> Result<TablePage, MemError> {
        self(req)
    }
}

/// A source that always refuses: for the GGTT, which needs no table pages, and for tests of the refusal path.
pub struct NoTablePages;
impl TablePageSource for NoTablePages {
    fn table_page(&mut self, req: TablePageRequest) -> Result<TablePage, MemError> {
        Err(MemError::TablePageRefused { level: req.level })
    }
}

/// The five pages a PPGTT needs at creation: the PML4 and the scratch chain (`gen8_init_scratch`: data page, scratch PT,
/// scratch PD, scratch PDP).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PpgttInit {
    pub pml4: u64,
    pub scratch: [u64; 4],
}

/// A GGTT range that is live before the driver starts and that [`Gtt::new_ggtt_preserving`] must keep exactly as found: its entries
/// are never written and no bind may land on it. `what` names the holder in every refusal (`"adopted-scanout"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preallocated {
    pub start: u64,
    pub size: u64,
    pub what: &'static str,
}

/// The name of the GGTT error-capture reservation (intel_ggtt.c:914, `ggtt->error_capture`).
pub const ERROR_CAPTURE: &str = "error-capture";

/// What a bind needs.
#[derive(Clone, Copy, Debug)]
pub struct BindRequest<'a> {
    /// The device address of each 4 KiB page to map, in order. Each must be page-aligned and inside the DMA mask.
    pub pages: &'a [u64],
    /// Bytes of address space for the data (whole pages, at least `pages.len() * 4096`); any excess maps scratch.
    pub size: u64,
    /// Power of two >= 4096; 0 means 4096.
    pub alignment: u64,
    /// Scratch bytes on EACH side of the data (`PIN_OFFSET_GUARD`); rounded up to `alignment` as `i915_vma_insert` does.
    pub guard: u64,
    pub cache: CacheLevel,
    /// `PTE_READ_ONLY` (ignored by the GGTT, as `gen8_ggtt_insert_entries` ignores it) | `PTE_LM` (refused on Gen9).
    pub flags: u32,
    pub place: Placement,
}

#[derive(Clone, Copy, Debug)]
struct Table {
    level: u8,
    base: u64,
    addr: u64,
}

/// The result of [`Gtt::walk`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walk {
    /// Entry index at each level: `[PML4, PDP, PD, PT]` (bits 47:39, 38:30, 29:21, 20:12).
    pub index: [u16; 4],
    /// Device address of the table page at each level, `[PML4, PDP, PD, PT]`, `None` where the tree has no page yet.
    pub table: [Option<u64>; 4],
}

/// One address space. The VMA table, the page-table table and the spare pool are growable `alloc` collections (no compile-time
/// capacity: how many buffers a client binds, and so how many table pages they need, is the client's choice); each growth is
/// fallible and refused as [`MemError::OutOfMemory`] before the space changes.
pub struct Gtt {
    kind: GttKind,
    total: u64,
    alloc_end: u64,
    mappable_end: u64,
    dma_bits: u8,
    nodes: Nodes,
    /// `scratch[0]->encode` for the GGTT; `enc[0]` for the PPGTT.
    enc: [u64; 4],
    tables: Vec<Table>,
    /// Table pages parked by a rolled-back plan, reused before the source is asked again. Its capacity is kept at or above
    /// `spare.len() + tables.len()` (see [`Gtt::reserve_table_slot`]) so a rollback can never fail to park a page.
    spare: Vec<u64>,
    /// The PML4 and scratch-chain pages (PPGTT): a table page may never alias them. FIXED by hardware: a 4-level tree has one
    /// PML4 and `gen8_init_scratch` makes exactly 4 scratch pages (gen8_ppgtt.c:826).
    fixed: [u64; 5],
}

const fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) & !(a - 1)
}

impl Gtt {
    fn blank(kind: GttKind, total: u64, alloc_end: u64, mappable_end: u64, dma_bits: u8) -> Self {
        Gtt {
            kind,
            total,
            alloc_end,
            mappable_end,
            dma_bits,
            nodes: Nodes::new(),
            enc: [0; 4],
            tables: Vec::new(),
            spare: Vec::new(),
            fixed: [u64::MAX; 5],
        }
    }

    fn check_page(&self, addr: u64) -> Result<(), MemError> {
        if addr % GTT_PAGE != 0 {
            return Err(MemError::Misaligned { what: "device page", value: addr, align: GTT_PAGE });
        }
        if self.dma_bits < 64 && (addr + GTT_PAGE - 1) >> self.dma_bits != 0 {
            return Err(MemError::DeviceAddrBeyondMask { addr, bits: self.dma_bits });
        }
        Ok(())
    }

    fn require_gen9(desc: &PlatformDescriptor) -> Result<u8, MemError> {
        if desc.info.graphics_ver != 9 {
            return Err(MemError::UnsupportedGeneration { gen: desc.info.graphics_ver });
        }
        Ok(desc.info.dma_mask_size)
    }

    /// The Global GTT (`gen8_gmch_probe` + `ggtt_probe_common` + `init_ggtt`) with nothing preallocated: every entry is pointed at
    /// the scratch page and the last page is kept as a guard (`init_ggtt`, intel_ggtt.c:868-953). See [`Gtt::new_ggtt_preserving`].
    pub fn new_ggtt(desc: &PlatformDescriptor, geom: &GgttGeometry, scratch_page: u64) -> Result<(Self, UnbindPlan), MemError> {
        Self::new_ggtt_preserving(desc, geom, scratch_page, &[])
    }

    /// The Global GTT around ranges that are LIVE before the driver starts - the firmware's framebuffer the display engine is scanning
    /// out (i915_initial_plane.c `initial_plane_vma`: pinned `PIN_OFFSET_FIXED` at the base the plane reads, :185) - and that must
    /// therefore survive the init exactly as found. In Linux order (`init_ggtt`, intel_ggtt.c:868-953):
    ///  1. each `keep` range becomes a RESERVED node named by its `what` (never evicted, never unbound; a fixed bind over it is
    ///     [`MemError::RangeReserved`]);
    ///  2. the error-capture slot (2 pages) is reserved at 0 when 0 is free, else at the lowest free address in the mappable aperture
    ///     (`drm_mm_reserve_node` then `drm_mm_insert_node_in_range(.., DRM_MM_INSERT_LOW)`, :914-922: "Since 0 may already be in use
    ///     by the system (e.g. the BIOS framebuffer), we let the reservation fail quietly"), and none at all when nothing fits;
    ///  3. every entry NOT inside a `keep` range is pointed at scratch - the error-capture slot (`scratch_range`, :926), the holes
    ///     ("Clear any non-preallocated blocks", :943-949) and the guard page (:952). A kept range's entries are never written.
    /// A `keep` range must be page-aligned, non-empty, below the guard page and disjoint from the others; each refusal is named.
    pub fn new_ggtt_preserving(desc: &PlatformDescriptor, geom: &GgttGeometry, scratch_page: u64, keep: &[Preallocated]) -> Result<(Self, UnbindPlan), MemError> {
        let bits = Self::require_gen9(desc)?;
        let mut g = Self::blank(GttKind::Ggtt, geom.total, geom.total - GTT_PAGE, geom.mappable_end, bits);
        g.check_page(scratch_page)?;
        // ggtt->vm.scratch[0]->encode = pte_encode(scratch dma, I915_CACHE_NONE, 0), intel_ggtt.c:1236
        g.enc[0] = gen8_ggtt_pte_encode(scratch_page);
        for k in keep {
            if k.size == 0 {
                return Err(MemError::ZeroLength);
            }
            if k.start % GTT_PAGE != 0 || k.size % GTT_PAGE != 0 {
                return Err(MemError::Misaligned { what: k.what, value: if k.start % GTT_PAGE != 0 { k.start } else { k.size }, align: GTT_PAGE });
            }
            if k.start.checked_add(k.size).map_or(true, |e| e > g.alloc_end) {
                return Err(MemError::BeyondSpace { va: k.start, len: k.size, total: g.alloc_end });
            }
            // i915_gem_gtt_reserve semantics: the exact range or nothing; two kept ranges that overlap are one range named twice
            let c = g.nodes.choose(k.size, GTT_PAGE, 0, g.alloc_end, Mode::Fixed(k.start), false)?;
            debug_assert!(c.victims.is_empty());
            g.nodes.insert(Node { id: 0, start: k.start, size: k.size, guard: 0, bound_pages: 0, pinned: false, reserved: Some(k.what), last_use: 0 })?;
        }
        if geom.mappable_end >= 2 * GTT_PAGE {
            let size = 2 * GTT_PAGE;
            let at = match g.nodes.choose(size, GTT_PAGE, 0, geom.mappable_end, Mode::Fixed(0), false) {
                Ok(c) => Some(c.start),
                Err(MemError::RangeReserved { .. } | MemError::RangeOccupied { .. }) => g.nodes.choose(size, GTT_PAGE, 0, geom.mappable_end, Mode::Low, false).ok().map(|c| c.start),
                Err(e) => return Err(e),
            };
            if let Some(start) = at {
                g.nodes.insert(Node { id: 0, start, size, guard: 0, bound_pages: 0, pinned: false, reserved: Some(ERROR_CAPTURE), last_use: 0 })?;
            }
        }
        // every entry outside the kept ranges, in address order, last page (the guard) included
        let mut ops = Ops::new();
        let mut kept: Vec<(u64, u64)> = Vec::new();
        kept.try_reserve_exact(keep.len()).map_err(|_| MemError::OutOfMemory { what: "plan ops" })?;
        kept.extend(keep.iter().map(|k| (k.start / GTT_PAGE, (k.start + k.size) / GTT_PAGE)));
        kept.sort_unstable();
        let mut next = 0u64;
        for (first, end) in kept.iter().copied().chain(core::iter::once((geom.entries, geom.entries))) {
            if first > next {
                ops.push(Op::GgttFill { first: next as u32, count: (first - next) as u32, value: g.enc[0] })?;
            }
            next = next.max(end);
        }
        let p = g.plan(ops, VmaId(0), 0, 0, 0, &[], CacheLevel::None, 0, Vec::new(), 0);
        Ok((g, p))
    }

    /// The reservation, if any, that holds `va` - `Some(what)` for an address inside a kept range or the error-capture slot.
    pub fn reservation_at(&self, va: u64) -> Option<&'static str> {
        let (i, _) = self.nodes.find_va(va)?;
        self.nodes.get(i).reserved
    }

    /// A full 48-bit PPGTT (`gen8_ppgtt_create`, gen8_ppgtt.c:1002): scratch chain + PML4, all as fills.
    pub fn new_ppgtt(desc: &PlatformDescriptor, init: &PpgttInit) -> Result<(Self, UnbindPlan), MemError> {
        let bits = Self::require_gen9(desc)?;
        if desc.info.ppgtt_type != PpgttType::Full || desc.info.ppgtt_size as u32 != PPGTT_BITS {
            return Err(MemError::NotFullPpgtt { size_bits: desc.info.ppgtt_size });
        }
        let mut g = Self::blank(GttKind::Ppgtt, 1u64 << PPGTT_BITS, 1u64 << PPGTT_BITS, 0, bits);
        let all = [init.pml4, init.scratch[0], init.scratch[1], init.scratch[2], init.scratch[3]];
        for (i, a) in all.iter().enumerate() {
            g.check_page(*a)?;
            if all[..i].contains(a) {
                return Err(MemError::DuplicatePage { addr: *a });
            }
        }
        // gen8_init_scratch (gen8_ppgtt.c:826): has_read_only is true on Gen9 (:1025), so the scratch PTE is read-only.
        g.enc[0] = gen8_pte_encode(init.scratch[0], CacheLevel::None, PTE_READ_ONLY); // :856
        for i in 1..4 {
            g.enc[i] = gen8_pde_encode(init.scratch[i], CacheLevel::None); // :878
        }
        let mut ops = Ops::new();
        for i in 1..4 {
            ops.push(Op::FillTable { table: init.scratch[i], entry: g.enc[i - 1] })?; // fill_px(obj, scratch[i-1]->encode), :877
        }
        ops.push(Op::FillTable { table: init.pml4, entry: g.enc[3] })?; // gen8_alloc_top_pd: fill_page_dma(.., scratch[top]->encode), :948
        g.fixed = all;
        g.reserve_table_slot()?;
        g.tables.push(Table { level: 3, base: 0, addr: init.pml4 });
        let p = g.plan(ops, VmaId(0), 0, 0, 0, &[], CacheLevel::None, 0, Vec::new(), 0);
        Ok((g, p))
    }

    #[allow(clippy::too_many_arguments)]
    fn plan<'a>(
        &self,
        ops: Ops,
        vma: VmaId,
        offset: u64,
        node_start: u64,
        node_size: u64,
        pages: &'a [u64],
        cache: CacheLevel,
        pte_flags: u32,
        evicted: Vec<VmaId>,
        new_tables: u32,
    ) -> BindPlan<'a> {
        BindPlan { kind: self.kind, vma, offset, node_start, node_size, pages, cache, pte_flags, ops, evicted, new_tables }
    }

    pub fn kind(&self) -> GttKind {
        self.kind
    }
    pub fn total(&self) -> u64 {
        self.total
    }
    /// The entry an unbound page holds (`scratch[0]->encode`).
    pub fn scratch_entry(&self) -> u64 {
        self.enc[0]
    }
    /// The `scratch[level]->encode` chain (PPGTT): entry 0 is the scratch PTE, 1..=3 the scratch PDEs.
    pub fn scratch_chain(&self) -> [u64; 4] {
        self.enc
    }
    pub fn live_nodes(&self) -> usize {
        self.nodes.len()
    }
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }
    pub fn spare_pages(&self) -> usize {
        self.spare.len()
    }

    /// Mark a node in use by a request (it will not be evicted or unbound) or free again.
    pub fn set_pinned(&mut self, id: VmaId, pinned: bool) -> Result<(), MemError> {
        let i = self.nodes.index_of(id.0).ok_or(MemError::NotBound { id: id.0 })?;
        self.nodes.get_mut(i).pinned = pinned;
        Ok(())
    }

    /// LRU bookkeeping: the node was used now.
    pub fn touch(&mut self, id: VmaId) -> Result<(), MemError> {
        let i = self.nodes.index_of(id.0).ok_or(MemError::NotBound { id: id.0 })?;
        let t = self.nodes.tick();
        self.nodes.get_mut(i).last_use = t;
        Ok(())
    }

    /// `(offset the GPU uses, data bytes bound, guard)` of a live node.
    pub fn offset_of(&self, id: VmaId) -> Option<(u64, u64, u64)> {
        let i = self.nodes.index_of(id.0)?;
        let n = self.nodes.get(i);
        Some((n.data_start(), n.bound_pages * GTT_PAGE, n.guard))
    }

    fn check_va(&self, va: u64) -> Result<(), MemError> {
        if self.kind == GttKind::Ppgtt && va >> PPGTT_BITS != 0 {
            return Err(MemError::Beyond48Bit { va });
        }
        Ok(())
    }

    /// The 48-bit rule for a range: it must start below 2^48 and end at or below it. The refusal names the FIRST offending address.
    fn check_range48(&self, va: u64, len: u64) -> Result<(), MemError> {
        self.check_va(va)?;
        if self.kind == GttKind::Ppgtt && va.checked_add(len).map_or(true, |e| e > 1u64 << PPGTT_BITS) {
            return Err(MemError::Beyond48Bit { va: 1u64 << PPGTT_BITS });
        }
        Ok(())
    }

    /// The table indices of `va` and which of its tables exist (`gen8_pd_index`, gen8_ppgtt.c:140-141).
    pub fn walk(&self, va: u64) -> Result<Walk, MemError> {
        if self.kind != GttKind::Ppgtt {
            return Err(MemError::UnsupportedFlag { flag: 0 });
        }
        self.check_va(va)?;
        let mut index = [0u16; 4];
        let mut table = [None; 4];
        for l in (0..4u8).rev() {
            index[3 - l as usize] = ((va >> (12 + 9 * l as u32)) & (ENTRIES_PER_TABLE - 1)) as u16;
            let base = if l == 3 { 0 } else { va & !((1u64 << (12 + 9 * (l as u32 + 1))) - 1) };
            table[3 - l as usize] = self.find(l, base).map(|i| self.tables[i].addr);
        }
        Ok(Walk { index, table })
    }

    fn find(&self, level: u8, base: u64) -> Option<usize> {
        self.tables.iter().position(|t| t.level == level && t.base == base)
    }

    /// The access check a device applies before honouring a GPU address: `va..va+len` must lie wholly inside the BOUND data of
    /// one live node. Anything else - unbound, unbound-then-reused, hole, scratch tail, reservation - is a stale mapping that
    /// the hardware would resolve to the scratch page; here it is refused by name.
    pub fn check_access(&self, va: u64, len: u64) -> Result<VmaId, MemError> {
        if len == 0 {
            return Err(MemError::ZeroLength);
        }
        self.check_range48(va, len)?;
        let last = va.checked_add(len - 1).ok_or(MemError::BeyondSpace { va, len, total: self.total })?;
        if last >= self.total {
            return Err(MemError::BeyondSpace { va, len, total: self.total });
        }
        let (i, in_data) = self.nodes.find_va(va).ok_or(MemError::StaleMapping { va })?;
        let n = self.nodes.get(i);
        if n.reserved.is_some() {
            return Err(MemError::StaleMapping { va });
        }
        if !in_data {
            return if va < n.data_start() || va >= n.data_end() { Err(MemError::GuardPageAccess { va }) } else { Err(MemError::StaleMapping { va }) };
        }
        let bound_end = n.data_start() + n.bound_pages * GTT_PAGE;
        if last >= bound_end {
            return Err(MemError::StaleMapping { va: bound_end });
        }
        Ok(VmaId(n.id))
    }

    // ---- planning -------------------------------------------------------------------------------------------------------

    /// A page for a new table: the spare pool first, then the caller. Validated, never a duplicate.
    fn get_page<S: TablePageSource>(&mut self, src: &mut S, level: u8, base: u64) -> Result<u64, MemError> {
        let addr = match self.spare.pop() {
            Some(a) => a,
            None => src.table_page(TablePageRequest { level, base_va: base })?.device_addr,
        };
        if self.check_page(addr).is_err() || self.tables.iter().any(|t| t.addr == addr) || self.fixed.contains(&addr) {
            return Err(MemError::TablePageRefused { level });
        }
        Ok(addr)
    }

    /// Make sure PDP, PD and PT pages exist for `va`, emitting fill + link ops for each new one (`__gen8_ppgtt_alloc`).
    fn ensure_tables<S: TablePageSource>(&mut self, src: &mut S, ops: &mut Ops, va: u64, made: &mut u32) -> Result<u64, MemError> {
        let mut parent = self.tables[0].addr; // the PML4
        for l in (0..3u8).rev() {
            let base = va & !((1u64 << (12 + 9 * (l as u32 + 1))) - 1);
            let at = match self.find(l, base) {
                Some(i) => i,
                None => {
                    self.reserve_table_slot()?;
                    let addr = self.get_page(src, l, base)?;
                    self.tables.push(Table { level: l, base, addr });
                    *made += 1;
                    ops.push(Op::FillTable { table: addr, entry: self.enc[l as usize] })?; // fill_px(pt, scratch[lvl]->encode), gen8_ppgtt.c:351
                    let idx = ((va >> (12 + 9 * (l as u32 + 1))) & (ENTRIES_PER_TABLE - 1)) as u16;
                    // __set_pd_entry: encode(px_dma(to), I915_CACHE_LLC), intel_ppgtt.c:109
                    ops.push(Op::TableEntry { table: parent, index: idx, value: gen8_pde_encode(addr, CacheLevel::Llc) })?;
                    self.tables.len() - 1
                }
            };
            parent = self.tables[at].addr;
        }
        Ok(parent)
    }

    /// Emit the ops that map `npages` pages at `va`: `data` is `Some(first_page_index)` for real pages, `None` for scratch.
    fn emit<S: TablePageSource>(&mut self, src: &mut S, ops: &mut Ops, mut va: u64, npages: u64, data: Option<u64>, made: &mut u32) -> Result<(), MemError> {
        let mut done = 0u64;
        match self.kind {
            GttKind::Ggtt => {
                if npages == 0 {
                    return Ok(());
                }
                let first = (va / GTT_PAGE) as u32;
                match data {
                    Some(p) => ops.push(Op::GgttRun { first, count: npages as u32, page_offset: p as u32 }),
                    None => ops.push(Op::GgttFill { first, count: npages as u32, value: self.enc[0] }),
                }
            }
            GttKind::Ppgtt => {
                while done < npages {
                    let idx = (va >> 12) & (ENTRIES_PER_TABLE - 1);
                    let n = (npages - done).min(ENTRIES_PER_TABLE - idx);
                    let pt = self.ensure_tables(src, ops, va, made)?;
                    match data {
                        Some(p) => ops.push(Op::PteRun { table: pt, index: idx as u16, count: n as u16, page_offset: (p + done) as u32 })?,
                        None => ops.push(Op::PteFill { table: pt, index: idx as u16, count: n as u16, value: self.enc[0] })?,
                    }
                    va += n * GTT_PAGE;
                    done += n;
                }
                Ok(())
            }
        }
    }

    /// Make room for one more table AND keep the spare pool able to hold every table (so [`Gtt::rollback_tables`] is infallible:
    /// a granted page is never dropped). Called before the page is taken from the source, so a refusal here loses nothing.
    fn reserve_table_slot(&mut self) -> Result<(), MemError> {
        let oom = MemError::OutOfMemory { what: "page-table table" };
        self.tables.try_reserve(1).map_err(|_| oom)?;
        self.spare.try_reserve(self.tables.len() + 1).map_err(|_| MemError::OutOfMemory { what: "spare pool" })
    }

    /// Forget the tables made since `keep` and park their pages in the spare pool. Cannot fail: capacity was reserved by
    /// [`Gtt::reserve_table_slot`] as each table was made.
    fn rollback_tables(&mut self, keep: usize) {
        while self.tables.len() > keep {
            let t = self.tables.pop().expect("len > keep");
            self.spare.push(t.addr);
        }
    }

    /// Plan a bind (`i915_vma_insert` + `ppgtt_bind_vma` / `intel_ggtt_bind_vma`). On success the node is live in the model and
    /// victims (if `place.evict`) are gone from it; on ANY error nothing changed except that table pages already obtained are
    /// parked in the spare pool for the next plan.
    pub fn plan_bind<'a, S: TablePageSource>(&mut self, src: &mut S, req: &BindRequest<'a>) -> Result<BindPlan<'a>, MemError> {
        let npages = req.pages.len() as u64;
        if npages == 0 || req.size == 0 {
            return Err(MemError::ZeroLength);
        }
        if req.size % GTT_PAGE != 0 {
            return Err(MemError::Misaligned { what: "size", value: req.size, align: GTT_PAGE });
        }
        if npages * GTT_PAGE > req.size {
            return Err(MemError::PagesExceedNode { pages: npages, node_pages: req.size / GTT_PAGE });
        }
        if req.flags & PTE_LM != 0 {
            return Err(MemError::UnsupportedFlag { flag: PTE_LM });
        }
        let align = if req.alignment == 0 { GTT_PAGE } else { req.alignment };
        if !align.is_power_of_two() || align < GTT_PAGE {
            return Err(MemError::BadAlignment { align });
        }
        for p in req.pages {
            self.check_page(*p)?;
        }
        let guard = align_up(req.guard, align); // i915_vma_insert, i915_vma.c:823
        let node_size = req.size.checked_add(2 * guard).ok_or(MemError::NoSpace { size: req.size })?;

        let mut hi = self.alloc_end;
        if req.place.mappable && self.kind == GttKind::Ggtt {
            hi = hi.min(self.mappable_end); // i915_vma.c:829
        }
        if req.place.zone_4g {
            hi = hi.min((1u64 << 32) - GTT_PAGE); // i915_vma.c:831
        }
        self.check_va(req.place.bias)?;
        let lo = req.place.bias;
        if let Mode::Fixed(off) = req.place.mode {
            self.check_range48(off, req.size)?;
            if off % align != 0 {
                return Err(MemError::Misaligned { what: "fixed offset", value: off, align });
            }
            if off.checked_add(req.size).map_or(true, |e| e > hi) {
                return Err(MemError::BeyondSpace { va: off, len: req.size, total: hi });
            }
            if off < guard || off + req.size > hi - guard.min(hi) {
                return Err(MemError::NoSpace { size: req.size }); // i915_vma.c:865
            }
        } else if hi < 2 * guard || req.size > hi - 2 * guard {
            return Err(MemError::NoSpace { size: req.size }); // "larger than the aperture", i915_vma.c:842
        }
        let mode = match req.place.mode {
            Mode::Fixed(off) => Mode::Fixed(off - guard),
            m => m,
        };
        let Choice { start, victims } = self.nodes.choose(node_size, align, lo, hi, mode, req.place.evict)?;
        // The node's slot is obtained now, before any table or victim is touched, so the commit below cannot fail.
        self.nodes.reserve(1)?;
        let mut evicted: Vec<VmaId> = Vec::new();
        evicted.try_reserve_exact(victims.len()).map_err(|_| MemError::OutOfMemory { what: "victims" })?;

        let t0 = self.tables.len();
        let mut ops = Ops::new();
        let mut made = 0u32;
        let r = (|| -> Result<(), MemError> {
            for &vi in &victims {
                let n = *self.nodes.get(vi);
                evicted.push(VmaId(n.id));
                ops.push(Op::AwaitRetire { vma: VmaId(n.id) })?;
                self.emit(src, &mut ops, n.start, n.size / GTT_PAGE, None, &mut made)?;
            }
            // guard | data | scratch tail | guard
            self.emit(src, &mut ops, start, guard / GTT_PAGE, None, &mut made)?;
            self.emit(src, &mut ops, start + guard, npages, Some(0), &mut made)?;
            let tail = (req.size / GTT_PAGE) - npages;
            self.emit(src, &mut ops, start + guard + npages * GTT_PAGE, tail, None, &mut made)?;
            self.emit(src, &mut ops, start + guard + req.size, guard / GTT_PAGE, None, &mut made)
        })();
        if let Err(e) = r {
            self.rollback_tables(t0);
            return Err(e);
        }
        // commit: remove victims (descending index keeps the rest valid), insert the node
        let mut order = victims;
        order.sort_unstable_by(|a, b| b.cmp(a));
        for &vi in &order {
            self.nodes.remove(vi);
        }
        let id = self.nodes.insert(Node { id: 0, start, size: node_size, guard, bound_pages: npages, pinned: false, reserved: None, last_use: 0 })?;
        let offset = start + guard;
        Ok(self.plan(ops, VmaId(id), offset, start, node_size, req.pages, req.cache, req.flags, evicted, made))
    }

    /// Plan the unbind of `id`: wait for the GPU to retire it, point its whole range (guard included) back at scratch, drop the
    /// node. Refused while pinned (in use by a request) and for driver reservations.
    pub fn plan_unbind(&mut self, id: VmaId) -> Result<UnbindPlan, MemError> {
        let i = self.nodes.index_of(id.0).ok_or(MemError::NotBound { id: id.0 })?;
        let n = *self.nodes.get(i);
        if n.pinned || n.reserved.is_some() {
            return Err(MemError::Pinned { id: id.0 });
        }
        let mut ops = Ops::new();
        ops.push(Op::AwaitRetire { vma: id })?;
        let mut made = 0u32;
        // an unbind never creates tables: every page of a bound node already has its path
        self.emit(&mut NoTablePages, &mut ops, n.start, n.size / GTT_PAGE, None, &mut made)?;
        self.nodes.remove(i);
        Ok(self.plan(ops, id, n.data_start(), n.start, n.size, &[], CacheLevel::None, 0, Vec::new(), 0))
    }
}
