// SPDX-License-Identifier: GPL-2.0-only
//! `__gen9_fw_ranges`, `gen8_shadowed_regs`, `NEEDS_FORCE_WAKE` and the lookups that use them (intel_uncore.c:919-982, :1004-1010,
//! :1171-1206, :1274-1307). The Gen9 table is "watertight": sorted, no gaps, covering 0x0..=0x3ffff.
use crate::domain::{FORCEWAKE_GT, FORCEWAKE_MEDIA, FORCEWAKE_RENDER};

/// `struct intel_forcewake_range` (intel_uncore.h:119-124): `[start, end]` inclusive and the domains it needs. `0` = the "uncore
/// range": no forcewake required.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FwRange {
    pub start: u32,
    pub end: u32,
    pub domains: u32,
}

const RENDER: u32 = FORCEWAKE_RENDER;
const GT: u32 = FORCEWAKE_GT;
const MEDIA: u32 = FORCEWAKE_MEDIA;

/// `GEN_FW_RANGE(s, e, d)` (intel_uncore.c:1208).
const fn range(start: u32, end: u32, domains: u32) -> FwRange {
    FwRange { start, end, domains }
}

/// `__gen9_fw_ranges` (intel_uncore.c:1274-1307), every range, in Linux's order.
pub static GEN9_FW_RANGES: [FwRange; 32] = [
    range(0x0, 0xaff, GT), // intel_uncore.c:1275
    range(0xb00, 0x1fff, 0), // intel_uncore.c:1276 /* uncore range */
    range(0x2000, 0x26ff, RENDER), // intel_uncore.c:1277
    range(0x2700, 0x2fff, GT), // intel_uncore.c:1278
    range(0x3000, 0x3fff, RENDER), // intel_uncore.c:1279
    range(0x4000, 0x51ff, GT), // intel_uncore.c:1280
    range(0x5200, 0x7fff, RENDER), // intel_uncore.c:1281
    range(0x8000, 0x812f, GT), // intel_uncore.c:1282
    range(0x8130, 0x813f, MEDIA), // intel_uncore.c:1283
    range(0x8140, 0x815f, RENDER), // intel_uncore.c:1284
    range(0x8160, 0x82ff, GT), // intel_uncore.c:1285
    range(0x8300, 0x84ff, RENDER), // intel_uncore.c:1286
    range(0x8500, 0x87ff, GT), // intel_uncore.c:1287
    range(0x8800, 0x89ff, MEDIA), // intel_uncore.c:1288
    range(0x8a00, 0x8bff, GT), // intel_uncore.c:1289
    range(0x8c00, 0x8cff, RENDER), // intel_uncore.c:1290
    range(0x8d00, 0x93ff, GT), // intel_uncore.c:1291
    range(0x9400, 0x97ff, RENDER | MEDIA), // intel_uncore.c:1292
    range(0x9800, 0xafff, GT), // intel_uncore.c:1293
    range(0xb000, 0xb47f, RENDER), // intel_uncore.c:1294
    range(0xb480, 0xcfff, GT), // intel_uncore.c:1295
    range(0xd000, 0xd7ff, MEDIA), // intel_uncore.c:1296
    range(0xd800, 0xdfff, GT), // intel_uncore.c:1297
    range(0xe000, 0xe8ff, RENDER), // intel_uncore.c:1298
    range(0xe900, 0x11fff, GT), // intel_uncore.c:1299
    range(0x12000, 0x13fff, MEDIA), // intel_uncore.c:1300
    range(0x14000, 0x19fff, GT), // intel_uncore.c:1301
    range(0x1a000, 0x1e9ff, MEDIA), // intel_uncore.c:1302
    range(0x1ea00, 0x243ff, GT), // intel_uncore.c:1303
    range(0x24400, 0x247ff, RENDER), // intel_uncore.c:1304
    range(0x24800, 0x2ffff, GT), // intel_uncore.c:1305
    range(0x30000, 0x3ffff, MEDIA), // intel_uncore.c:1306
];

/// `gen8_shadowed_regs` (intel_uncore.c:1004-1010): registers written without forcewake (the hardware shadows the write).
/// `(start, end)` inclusive. Shadowing applies to WRITES only; a read of these still needs forcewake (intel_uncore.c:985-990).
pub static GEN8_SHADOWED_REGS: [(u32, u32); 5] = [
    (0x2030, 0x2030),   // intel_uncore.c:1005
    (0xA008, 0xA00C),   // intel_uncore.c:1006
    (0x12030, 0x12030), // intel_uncore.c:1007
    (0x1a030, 0x1a030), // intel_uncore.c:1008
    (0x22030, 0x22030), // intel_uncore.c:1009
];

/// `NEEDS_FORCE_WAKE(reg)` (intel_uncore.c:919-922): `reg < 0x40000 || reg >= 0x116000`.
pub const fn needs_force_wake(offset: u32) -> bool {
    offset < 0x40000 || offset >= 0x116000
}

/// `find_fw_domain` over the Gen9 table (intel_uncore.c:953-982): Linux's `BSEARCH` (intel_uncore.c:933-952), reproduced as a
/// binary search over the sorted table. An offset in no range (past 0x3ffff) needs nothing: `0`.
pub fn find_fw_domain(offset: u32) -> u32 {
    let (mut lo, mut hi) = (0usize, GEN9_FW_RANGES.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let e = &GEN9_FW_RANGES[mid];
        if offset < e.start {
            hi = mid;
        } else if offset > e.end {
            lo = mid + 1;
        } else {
            return e.domains;
        }
    }
    0
}

/// `is_shadowed` (intel_uncore.c:1171-1183).
pub fn is_shadowed(offset: u32) -> bool {
    GEN8_SHADOWED_REGS.iter().any(|&(s, e)| offset >= s && offset <= e)
}

/// `__fwtable_reg_read_fw_domains` (intel_uncore.c:1191-1197): the domains a READ of `offset` needs.
pub fn read_fw_domains(offset: u32) -> u32 {
    if needs_force_wake(offset) {
        find_fw_domain(offset)
    } else {
        0
    }
}

/// `__fwtable_reg_write_fw_domains` (intel_uncore.c:1199-1206): the domains a WRITE of `offset` needs (none if shadowed).
pub fn write_fw_domains(offset: u32) -> u32 {
    if needs_force_wake(offset) && !is_shadowed(offset) {
        find_fw_domain(offset)
    } else {
        0
    }
}
