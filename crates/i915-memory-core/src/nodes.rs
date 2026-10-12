// SPDX-License-Identifier: GPL-2.0-only
//! The address-range allocator and its eviction policy: a fixed-capacity, address-sorted node table.
//!
//! Policy ported from i915_gem_gtt.c (`i915_gem_gtt_insert`, :189 - BEST by default, HIGH for `PIN_HIGH`, LOW for `PIN_MAPPABLE`;
//! `i915_gem_gtt_reserve` for fixed offsets) and the victim selection of i915_gem_evict.c (`i915_gem_evict_something`: take
//! unpinned nodes oldest-first until a hole fits, then give back the ones that turned out not to be needed). The `drm_mm`
//! red-black tree is replaced by a sorted array: capacity `V` is a declared bound and [`MemError::VmaTableFull`] enforces it.
//! Linux's RANDOM replacement slot (:274) is deliberately not ported - a planner must be deterministic and testable.

use crate::plan::MAX_EVICT;
use crate::{MemError, GTT_PAGE};

/// A live node's handle. Ids are never reused within one [`crate::Gtt`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VmaId(pub u32);

/// How to pick the hole (`enum drm_mm_insert_mode` as `i915_gem_gtt_insert` chooses it, i915_gem_gtt.c:217-221).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// `DRM_MM_INSERT_BEST`: the smallest hole that fits (the default).
    Best,
    /// `DRM_MM_INSERT_LOW`: the lowest fitting address (`PIN_MAPPABLE`).
    Low,
    /// `DRM_MM_INSERT_HIGHEST`: the highest fitting address (`PIN_HIGH`).
    High,
    /// `PIN_OFFSET_FIXED`: exactly this offset of the (guard-less) data.
    Fixed(u64),
}

/// The pin flags that shape placement (gem/i915_gem_gtt.h:41-55 `PIN_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub mode: Mode,
    /// `PIN_OFFSET_BIAS`: nothing is placed below this address.
    pub bias: u64,
    /// `PIN_MAPPABLE`: stay inside the CPU-visible aperture (GGTT only).
    pub mappable: bool,
    /// `PIN_ZONE_4G`: stay below 4 GiB - 4 KiB.
    pub zone_4g: bool,
    /// May unpinned nodes be evicted (`!PIN_NOEVICT`)?
    pub evict: bool,
}

impl Placement {
    pub const fn best() -> Placement {
        Placement { mode: Mode::Best, bias: 0, mappable: false, zone_4g: false, evict: false }
    }
    pub const fn with_mode(mode: Mode) -> Placement {
        Placement { mode, bias: 0, mappable: false, zone_4g: false, evict: false }
    }
    pub const fn evicting(mut self) -> Placement {
        self.evict = true;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub id: u32,
    /// Start of the whole node, guard included.
    pub start: u64,
    /// Size of the whole node: `guard + data + guard`.
    pub size: u64,
    pub guard: u64,
    /// Whole pages bound at `start + guard`; the rest of the data area is scratch.
    pub bound_pages: u64,
    pub pinned: bool,
    /// A driver reservation (GGTT error-capture slot): never evicted, never unbound.
    pub reserved: bool,
    pub last_use: u64,
}

impl Node {
    const EMPTY: Node = Node { id: 0, start: 0, size: 0, guard: 0, bound_pages: 0, pinned: false, reserved: false, last_use: 0 };
    pub const fn end(&self) -> u64 {
        self.start + self.size
    }
    pub const fn data_start(&self) -> u64 {
        self.start + self.guard
    }
    pub const fn data_end(&self) -> u64 {
        self.start + self.size - self.guard
    }
    const fn evictable(&self) -> bool {
        !self.pinned && !self.reserved
    }
}

pub(crate) struct Nodes<const V: usize> {
    arr: [Node; V],
    len: usize,
    next_id: u32,
    clock: u64,
}

/// A chosen position plus the victims whose removal makes it free.
pub(crate) struct Choice {
    pub start: u64,
    pub victims: [usize; MAX_EVICT],
    pub n_victims: usize,
}

const fn align_up(v: u64, a: u64) -> Option<u64> {
    match v.checked_add(a - 1) {
        Some(x) => Some(x & !(a - 1)),
        None => None,
    }
}

impl<const V: usize> Nodes<V> {
    pub const fn new() -> Self {
        Nodes { arr: [Node::EMPTY; V], len: 0, next_id: 1, clock: 0 }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn get(&self, i: usize) -> &Node {
        &self.arr[i]
    }
    pub fn get_mut(&mut self, i: usize) -> &mut Node {
        &mut self.arr[i]
    }
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.arr[..self.len].iter().position(|n| n.id == id)
    }
    pub fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Insert keeping address order; returns the new id.
    pub fn insert(&mut self, mut n: Node) -> Result<u32, MemError> {
        if self.len == V {
            return Err(MemError::VmaTableFull { cap: V });
        }
        n.id = self.next_id;
        self.next_id += 1;
        n.last_use = self.tick();
        let at = self.arr[..self.len].iter().position(|m| m.start > n.start).unwrap_or(self.len);
        let mut i = self.len;
        while i > at {
            self.arr[i] = self.arr[i - 1];
            i -= 1;
        }
        self.arr[at] = n;
        self.len += 1;
        Ok(n.id)
    }

    pub fn remove(&mut self, idx: usize) -> Node {
        let n = self.arr[idx];
        for i in idx..self.len - 1 {
            self.arr[i] = self.arr[i + 1];
        }
        self.len -= 1;
        n
    }

    /// The node whose DATA range holds `va`, or whose guard does (second value).
    pub fn find_va(&self, va: u64) -> Option<(usize, bool)> {
        self.arr[..self.len].iter().position(|n| va >= n.start && va < n.end()).map(|i| {
            let n = &self.arr[i];
            (i, va >= n.data_start() && va < n.data_start() + n.bound_pages * GTT_PAGE)
        })
    }

    /// First hole in `[lo, hi)` for `size` bytes at `align`, ignoring nodes flagged in `gone`.
    fn find_hole(&self, size: u64, align: u64, lo: u64, hi: u64, mode: Mode, gone: &[bool; V]) -> Option<u64> {
        let mut best: Option<(u64, u64)> = None; // (hole length, start)
        let mut high: Option<u64> = None;
        let mut prev_end = lo;
        let consider = |gap_lo: u64, gap_hi: u64, best: &mut Option<(u64, u64)>, high: &mut Option<u64>| -> bool {
            if gap_hi <= gap_lo || gap_hi - gap_lo < size {
                return false;
            }
            match mode {
                Mode::High => {
                    let cand = (gap_hi - size) & !(align - 1);
                    if cand >= gap_lo {
                        *high = Some(cand); // later (higher) gaps overwrite
                    }
                    false
                }
                _ => match align_up(gap_lo, align) {
                    Some(cand) if cand.checked_add(size).map_or(false, |e| e <= gap_hi) => {
                        if matches!(mode, Mode::Low) {
                            *best = Some((gap_hi - gap_lo, cand));
                            return true; // lowest fit: stop
                        }
                        if best.map_or(true, |(l, _)| gap_hi - gap_lo < l) {
                            *best = Some((gap_hi - gap_lo, cand));
                        }
                        false
                    }
                    _ => false,
                },
            }
        };
        for (i, n) in self.arr[..self.len].iter().enumerate() {
            if gone[i] {
                continue;
            }
            let gap_hi = n.start.min(hi);
            if consider(prev_end.max(lo), gap_hi, &mut best, &mut high) {
                return best.map(|b| b.1);
            }
            prev_end = prev_end.max(n.end());
        }
        if consider(prev_end.max(lo), hi, &mut best, &mut high) {
            return best.map(|b| b.1);
        }
        match mode {
            Mode::High => high,
            _ => best.map(|b| b.1),
        }
    }

    /// Place `size` bytes (guard-inclusive) at `align` inside `[lo, hi)`. `evict` allows unpinned nodes to be victims.
    pub fn choose(&self, size: u64, align: u64, lo: u64, hi: u64, mode: Mode, evict: bool) -> Result<Choice, MemError> {
        let mut gone = [false; V];
        let mut victims = [0usize; MAX_EVICT];
        let mut nv = 0usize;
        if let Mode::Fixed(start) = mode {
            // i915_gem_gtt_reserve + i915_gem_evict_for_node: every overlapping node must go, and each must be evictable.
            let end = start + size;
            for (i, n) in self.arr[..self.len].iter().enumerate() {
                if n.start < end && start < n.end() {
                    if !evict || !n.evictable() {
                        return Err(MemError::RangeOccupied { start, len: size });
                    }
                    if nv == MAX_EVICT {
                        return Err(MemError::PlanTooLarge { what: "victims", cap: MAX_EVICT });
                    }
                    victims[nv] = i;
                    nv += 1;
                }
            }
            return Ok(Choice { start, victims, n_victims: nv });
        }
        if let Some(start) = self.find_hole(size, align, lo, hi, mode, &gone) {
            return Ok(Choice { start, victims, n_victims: 0 });
        }
        if !evict {
            return Err(MemError::NoSpace { size });
        }
        // Oldest-first until a hole opens (i915_gem_evict_something, scan list in LRU order).
        let mut order = [0usize; V];
        let mut picked = 0usize;
        let found = loop {
            let mut oldest: Option<usize> = None;
            for (i, n) in self.arr[..self.len].iter().enumerate() {
                if gone[i] || !n.evictable() {
                    continue;
                }
                if oldest.map_or(true, |o| n.last_use < self.arr[o].last_use) {
                    oldest = Some(i);
                }
            }
            let Some(o) = oldest else { return Err(MemError::NoSpace { size }) };
            gone[o] = true;
            order[picked] = o;
            picked += 1;
            if let Some(s) = self.find_hole(size, align, lo, hi, mode, &gone) {
                break s;
            }
        };
        // Give back every victim the hole does not actually need (drm_mm_scan_remove_block order: newest first).
        let mut start = found;
        for k in (0..picked).rev() {
            gone[order[k]] = false;
            match self.find_hole(size, align, lo, hi, mode, &gone) {
                Some(s) => start = s,
                None => gone[order[k]] = true,
            }
        }
        for (i, g) in gone[..self.len].iter().enumerate() {
            if *g {
                if nv == MAX_EVICT {
                    return Err(MemError::PlanTooLarge { what: "victims", cap: MAX_EVICT });
                }
                victims[nv] = i;
                nv += 1;
            }
        }
        Ok(Choice { start, victims, n_victims: nv })
    }
}
