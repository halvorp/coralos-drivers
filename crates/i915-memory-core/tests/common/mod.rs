// SPDX-License-Identifier: GPL-2.0-only
#![allow(dead_code)]
use i915_devinfo_core::{lookup, PlatformDescriptor};
use i915_memory_core::probe::GgttGeometry;
use i915_memory_core::{MemError, TablePage, TablePageRequest, TablePageSource};

/// 8086:591C, Kaby Lake GT2 ULX (the Pixelbook Go).
pub fn kbl() -> PlatformDescriptor {
    lookup(0x591c).expect("KBL row")
}

/// GMCH_CTRL with GGMS = 3 (8 MiB of GGTT = 4 GiB) and GMS = 0.
pub const GMCH_8M: u16 = 3 << 6;

pub fn geom_4g() -> GgttGeometry {
    GgttGeometry::probe(GMCH_8M, 16 << 20, 256 << 20).unwrap()
}

/// A table-page source that hands out sequential pages from `next` and remembers every request.
pub struct Pool {
    pub next: u64,
    pub asked: Vec<TablePageRequest>,
    pub limit: usize,
}

impl Pool {
    pub fn new(next: u64) -> Pool {
        Pool { next, asked: Vec::new(), limit: usize::MAX }
    }
}

impl TablePageSource for Pool {
    fn table_page(&mut self, req: TablePageRequest) -> Result<TablePage, MemError> {
        if self.asked.len() >= self.limit {
            return Err(MemError::TablePageRefused { level: req.level });
        }
        self.asked.push(req);
        let a = self.next;
        self.next += 0x1000;
        Ok(TablePage { device_addr: a })
    }
}

pub const PML4: u64 = 0x10000;
pub const S: [u64; 4] = [0x11000, 0x12000, 0x13000, 0x14000];
pub const POOL0: u64 = 0x100000;
