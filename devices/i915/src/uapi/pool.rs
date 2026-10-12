// SPDX-License-Identifier: GPL-2.0-only
//! A recycling page pool over [`GpuMem`]. `coralos_hal::Dma` has no `free`: a grant, once taken from the host, is the device's for
//! good. So the device keeps what it is done with: [`PagePool::release`] puts a run back on a free list (merging neighbours) and
//! [`PagePool::alloc`] takes the best-fitting free run first, splitting it, and only asks the host for a new grant when none fits.
//! A recycled run is ZEROED before it is handed out again (a client must never read another's bytes: `I915_BO_ALLOC_USER`,
//! gem/i915_gem_create.c:124).
//!
//! RUNTIME-TABLES-HAVE-NO-CAP: the free list grows with memory, fallibly; a release that cannot record its run counts the pages as
//! `lost` (visible to the tests) instead of corrupting anything. An allocation refused for lack of DMA endowment or heap is the named
//! `OUT-OF-MEMORY`, raised before anything exists.
use super::Fail;
use crate::exec::dma::{CpuMem, GpuMem, ObjKind, PAGE};
use alloc::vec::Vec;
use coralos_hal::Dma;

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

#[derive(Debug, Default)]
pub struct PagePool {
    free: Vec<Run>,
    out: usize,
    lost: usize,
    fresh: usize,
}

impl PagePool {
    pub const fn new() -> PagePool {
        PagePool { free: Vec::new(), out: 0, lost: 0, fresh: 0 }
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
    /// Pages taken from the host in total (never shrinks: the HAL has no free).
    pub fn pages_from_host(&self) -> usize {
        self.fresh
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
        let g = gm.alloc.alloc(ObjKind::Data, pages * PAGE).map_err(Fail::from)?;
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
        self.put(r);
    }

    fn put(&mut self, r: Run) {
        // merge with a physically AND virtually adjacent neighbour (a run must stay one contiguous grant slice)
        for f in self.free.iter_mut() {
            if f.dev + f.bytes() as u64 == r.dev && f.cpu + f.bytes() as u64 == r.cpu {
                f.pages += r.pages;
                return;
            }
            if r.dev + r.bytes() as u64 == f.dev && r.cpu + r.bytes() as u64 == f.cpu {
                *f = Run { cpu: r.cpu, dev: r.dev, pages: f.pages + r.pages };
                return;
            }
        }
        if self.free.try_reserve(1).is_err() {
            self.lost += r.pages;
            return;
        }
        self.free.push(r);
    }
}
