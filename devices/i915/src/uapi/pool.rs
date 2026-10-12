// SPDX-License-Identifier: GPL-2.0-only
//! A recycling page pool over [`GpuMem`]. A grant taken from the host is the device's until the host takes it back
//! (`coralos_hal::Dma::free`, a provided method that says `Unsupported` where the host has no such call - rt's binding has none today).
//! So the device keeps what it is done with: [`PagePool::release`] puts a run back on a free list (merging neighbours) and
//! [`PagePool::alloc`] takes the best-fitting free run first, splitting it, and only asks the host for a new grant when none fits.
//!
//! RETURNING MEMORY: [`PagePool::trim`] hands whole host grants back while more pages sit idle than the LIVE SET justifies: the high-water
//! mark is the number of pages currently handed out (`pages_out`), not a constant - a busy device keeps as much slack as it uses, an idle
//! one keeps none. Only a grant the pool took whole and now holds whole (merged runs are matched back to the grants they cover) can go;
//! a partly-used grant stays. A host that answers `Unsupported` is asked once: the pool then keeps everything and says so
//! ([`PagePool::host_free`]).
//! A recycled run is ZEROED before it is handed out again (a client must never read another's bytes: `I915_BO_ALLOC_USER`,
//! gem/i915_gem_create.c:124).
//!
//! RUNTIME-TABLES-HAVE-NO-CAP: the free list grows with memory, fallibly; a release that cannot record its run counts the pages as
//! `lost` (visible to the tests) instead of corrupting anything. An allocation refused for lack of DMA endowment or heap is the named
//! `OUT-OF-MEMORY`, raised before anything exists.
use super::Fail;
use crate::exec::dma::{CpuMem, GpuMem, Grant, ObjKind, PAGE};
use crate::exec::ExecError;
use alloc::vec::Vec;
use coralos_hal::{Dma, DmaRefusal};

/// A run of whole pages: CPU mapping and DEVICE address kept apart (never derived from each other).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub cpu: u64,
    pub dev: u64,
    pub pages: usize,
}

impl Run {
    pub const fn bytes(&self) -> usize {
        self.pages * PAGE
    }
    /// The device address of page `i`.
    pub const fn page(&self, i: usize) -> u64 {
        self.dev + (i * PAGE) as u64
    }
}

/// Whether the host takes grants back, as far as the pool knows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostFree {
    /// Never asked.
    #[default]
    Unknown,
    /// The host returned a grant.
    Works,
    /// The host said `Unsupported` (no call): the pool keeps every page it takes.
    Unsupported,
}

/// One grant the pool took from the host (exactly as `GpuAlloc` recorded it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Origin {
    cpu: u64,
    dev: u64,
    pages: usize,
}

#[derive(Debug, Default)]
pub struct PagePool {
    free: Vec<Run>,
    origins: Vec<Origin>,
    out: usize,
    lost: usize,
    fresh: usize,
    returned: usize,
    host_free: HostFree,
    /// Pages were released since the last trim.
    dirty: bool,
}

impl PagePool {
    pub const fn new() -> PagePool {
        PagePool { free: Vec::new(), origins: Vec::new(), out: 0, lost: 0, fresh: 0, returned: 0, host_free: HostFree::Unknown, dirty: false }
    }
    /// Pages handed out and not yet released.
    pub fn pages_out(&self) -> usize {
        self.out
    }
    /// Pages sitting on the free list.
    pub fn pages_free(&self) -> usize {
        self.free.iter().map(|r| r.pages).sum()
    }
    /// Pages whose release could not be recorded (the free list could not grow).
    pub fn pages_lost(&self) -> usize {
        self.lost
    }
    /// Pages taken from the host in total.
    pub fn pages_from_host(&self) -> usize {
        self.fresh
    }
    /// Pages given back to the host in total (`pages_from_host() - pages_returned()` is what the host still counts against this device).
    pub fn pages_returned(&self) -> usize {
        self.returned
    }
    /// Whole host grants the pool holds (handed out, free or partly each).
    pub fn grants_held(&self) -> usize {
        self.origins.len()
    }
    pub fn host_free(&self) -> HostFree {
        self.host_free
    }
    /// The most idle pages worth keeping: the live set. Derived from the pool's own accounting every time it is asked.
    pub fn high_water(&self) -> usize {
        self.out
    }
    /// Is there slack above the high-water mark that a trim could return (and might the host take it)?
    pub fn wants_trim(&self) -> bool {
        self.dirty && self.host_free != HostFree::Unsupported && self.pages_free() > self.high_water()
    }

    /// `pages` zeroed pages, recycled when one fits.
    pub fn alloc<D: Dma, M: CpuMem>(&mut self, gm: &mut GpuMem<D, M>, pages: usize) -> Result<Run, Fail> {
        if pages == 0 {
            return Err(Fail::new(super::errno::EINVAL, "POOL-ZERO-PAGES"));
        }
        let best = self.free.iter().enumerate().filter(|(_, r)| r.pages >= pages).min_by_key(|(_, r)| r.pages).map(|(i, _)| i);
        if let Some(i) = best {
            let r = self.free[i];
            let take = Run { pages, ..r };
            if r.pages == pages {
                self.free.swap_remove(i);
            } else {
                self.free[i] = Run { cpu: r.cpu + (pages * PAGE) as u64, dev: r.dev + (pages * PAGE) as u64, pages: r.pages - pages };
            }
            self.zero(gm, take)?;
            self.out += pages;
            return Ok(take);
        }
        // room to remember the grant BEFORE the host is asked: a grant the pool could not name later could never be given back
        self.origins.try_reserve(1).map_err(|_| Fail::oom("pool-grants"))?;
        let g = gm.alloc.alloc(ObjKind::Data, pages * PAGE).map_err(Fail::from)?;
        self.origins.push(Origin { cpu: g.cpu_mapping, dev: g.device_addr, pages: g.len / PAGE }); // reserved above
        self.fresh += g.len / PAGE;
        let run = Run { cpu: g.cpu_mapping, dev: g.device_addr, pages: g.len / PAGE };
        // a host that rounded up gives us the tail: keep it on the free list (clean, but zeroed again on reuse)
        let take = Run { pages, ..run };
        if run.pages > pages {
            let tail = Run { cpu: run.cpu + (pages * PAGE) as u64, dev: run.dev + (pages * PAGE) as u64, pages: run.pages - pages };
            self.put(tail);
        }
        self.out += pages;
        Ok(take)
    }

    fn zero<D: Dma, M: CpuMem>(&self, gm: &mut GpuMem<D, M>, r: Run) -> Result<(), Fail> {
        for i in 0..r.bytes() / 4 {
            gm.store32(r.dev + i as u64 * 4, 0)?;
        }
        gm.flush(r.dev, r.bytes() as u32)?;
        Ok(())
    }

    /// Give a run back.
    pub fn release(&mut self, r: Run) {
        self.out = self.out.saturating_sub(r.pages);
        self.dirty = true;
        self.put(r);
    }

    /// Return idle host grants while more pages sit free than [`PagePool::high_water`]. Returns the pages given back. A grant goes only
    /// when a free run covers all of it; the free list is split (room reserved first) before the host is asked, so a refusal or a full
    /// heap changes nothing. `DMA-FREE-UNSUPPORTED` from the host ends the attempt for good (the pool keeps its memory).
    pub fn trim<D: Dma, M: CpuMem>(&mut self, gm: &mut GpuMem<D, M>) -> usize {
        self.dirty = false;
        let mut given = 0;
        while self.host_free != HostFree::Unsupported && self.pages_free() > self.high_water() {
            let Some((oi, fi)) = self.freeable() else { break };
            if self.free.try_reserve(1).is_err() {
                break; // a split may need one more row; none to spare
            }
            let o = self.origins[oi];
            let g = Grant { kind: ObjKind::Data, cpu_mapping: o.cpu, device_addr: o.dev, len: o.pages * PAGE };
            match gm.alloc.release(g) {
                Ok(()) => {}
                Err(ExecError::Dma(DmaRefusal::Unsupported)) => {
                    self.host_free = HostFree::Unsupported;
                    break;
                }
                Err(_) => break, // a host refusal other than "no call": keep the grant, try again at the next trim
            }
            self.host_free = HostFree::Works;
            self.origins.swap_remove(oi);
            self.cut(fi, o);
            self.returned += o.pages;
            given += o.pages;
        }
        given
    }

    /// An origin wholly inside one free run: `(origin index, free index)`.
    fn freeable(&self) -> Option<(usize, usize)> {
        self.origins.iter().enumerate().find_map(|(oi, o)| {
            let fi = self.free.iter().position(|f| {
                f.dev <= o.dev && f.cpu <= o.cpu && o.dev - f.dev == o.cpu - f.cpu && o.dev + (o.pages * PAGE) as u64 <= f.dev + f.bytes() as u64
            })?;
            Some((oi, fi))
        })
    }

    /// Remove the origin's pages from free run `fi`, keeping the head and the tail (room for the extra row was reserved).
    fn cut(&mut self, fi: usize, o: Origin) {
        let f = self.free[fi];
        let head = ((o.dev - f.dev) / PAGE as u64) as usize;
        let tail = f.pages - head - o.pages;
        match (head, tail) {
            (0, 0) => {
                self.free.swap_remove(fi);
            }
            (0, _) => self.free[fi] = Run { cpu: o.cpu + (o.pages * PAGE) as u64, dev: o.dev + (o.pages * PAGE) as u64, pages: tail },
            (_, 0) => self.free[fi].pages = head,
            _ => {
                self.free[fi].pages = head;
                self.free.push(Run { cpu: o.cpu + (o.pages * PAGE) as u64, dev: o.dev + (o.pages * PAGE) as u64, pages: tail }); // reserved by the caller
            }
        }
    }

    fn put(&mut self, r: Run) {
        // merge with a physically AND virtually adjacent neighbour (a run must stay one contiguous grant slice)
        let mut r = r;
        // ... and inside ONE host grant: a run that spanned two grants could not be flushed or zeroed (the CPU side looks a range up in a
        // single grant), and could never be handed back whole
        let origins = &self.origins;
        let grant_of = |dev: u64| origins.iter().position(|o| o.dev <= dev && dev < o.dev + (o.pages * PAGE) as u64);
        let adjacent = |lo: &Run, hi: &Run| lo.dev + lo.bytes() as u64 == hi.dev && lo.cpu + lo.bytes() as u64 == hi.cpu && grant_of(lo.dev) == grant_of(hi.dev);
        // a run that bridges two free runs absorbs BOTH (otherwise a grant covered by the union could never be recognised as whole)
        let (mut i, mut absorbed) = (0, false);
        while i < self.free.len() {
            let f = self.free[i];
            if adjacent(&f, &r) {
                r = Run { pages: f.pages + r.pages, ..f };
                self.free.swap_remove(i);
                (i, absorbed) = (0, true);
            } else if adjacent(&r, &f) {
                r = Run { pages: f.pages + r.pages, ..r };
                self.free.swap_remove(i);
                (i, absorbed) = (0, true);
            } else {
                i += 1;
            }
        }
        // absorbing a neighbour freed a row, so the push cannot reallocate; otherwise the row must be reserved, or the pages are `lost`
        if !absorbed && self.free.try_reserve(1).is_err() {
            self.lost += r.pages;
            return;
        }
        self.free.push(r);
    }
}
