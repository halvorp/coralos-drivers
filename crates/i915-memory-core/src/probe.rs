// SPDX-License-Identifier: GPL-2.0-only
//! Decoding of what the platform tells us about the Global GTT and stolen memory - config-space words and one register value
//! in, plain values out. No config or MMIO read happens here: the caller reads `SNB_GMCH_CTRL`, `INTEL_BSM`, BAR0/BAR2 sizes and
//! `GEN6_STOLEN_RESERVED` and passes the words.
//!
//! Ported from gt/intel_ggtt.c (`gen8_get_total_gtt_size`, `gen8_gmch_probe`, `ggtt_probe_common`, the 4 GiB / mappable clamps of
//! `i915_ggtt_probe_hw`), arch/x86/kernel/early-quirks.c (`gen9_stolen_size`, `gen3_stolen_base`) and gem/i915_gem_stolen.c
//! (`bdw_get_stolen_reserved`). Cherryview's `chv_get_total_gtt_size` is NOT ported: devinfo refuses Gen8.

use crate::regs::*;
use crate::MemError;

const MIB: u64 = 1 << 20;

/// `gen8_get_total_gtt_size` (intel_ggtt.c:1153): the GGTT *page-table* size in bytes from GMCH_CTRL. GGMS 0 means "no GTT"
/// (Linux would carry on with size 0; this port refuses by name).
pub fn gen8_gtt_table_bytes(gmch_ctl: u16) -> Result<u64, MemError> {
    let ggms = (gmch_ctl >> BDW_GMCH_GGMS_SHIFT) & BDW_GMCH_GGMS_MASK; // intel_ggtt.c:1155-1156
    if ggms == 0 {
        return Err(MemError::GgttAbsent { gmch_ctl });
    }
    Ok((1u64 << ggms) << 20) // intel_ggtt.c:1158,1166
}

/// `gen9_stolen_size` (early-quirks.c:447): bytes of stolen memory from GMS.
pub fn gen9_stolen_size(gmch_ctl: u16) -> u64 {
    let gms = ((gmch_ctl >> BDW_GMCH_GMS_SHIFT) & BDW_GMCH_GMS_MASK) as u64; // early-quirks.c:453
    if gms < 0xf0 {
        gms * 32 * MIB // early-quirks.c:458
    } else {
        (gms - 0xf0) * 4 * MIB + 4 * MIB // early-quirks.c:460
    }
}

/// Stolen memory as the planners see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stolen {
    pub base: u64,
    pub size: u64,
    /// The firmware-reserved top part (`GEN6_STOLEN_RESERVED`), `(base, size)`; `None` when the register says disabled.
    pub reserved: Option<(u64, u64)>,
    /// `size` minus the reserved part: what the driver may use (`i915_gem_init_stolen`, i915_gem_stolen.c:573).
    pub usable: u64,
}

/// Decode stolen memory from GMCH_CTRL (size), BSM (base) and `GEN6_STOLEN_RESERVED` (reserved top).
pub fn decode_stolen(gmch_ctl: u16, bsm: u32, stolen_reserved: u32) -> Result<Stolen, MemError> {
    let size = gen9_stolen_size(gmch_ctl);
    let base = (bsm & INTEL_BSM_MASK) as u64; // gen3_stolen_base, early-quirks.c:339
    let top = base + size;
    let mut reserved = None;
    // bdw_get_stolen_reserved (i915_gem_stolen.c:385-402)
    if stolen_reserved & GEN6_STOLEN_RESERVED_ENABLE != 0 && stolen_reserved & GEN6_STOLEN_RESERVED_ADDR_MASK != 0 {
        let rbase = (stolen_reserved & GEN6_STOLEN_RESERVED_ADDR_MASK) as u64;
        if rbase < base || rbase > top {
            // init_reserved_stolen would carve a region outside the stolen range; name it instead of underflowing.
            return Err(MemError::StolenReservedOutside { reserved_base: rbase, base, top });
        }
        // init_reserved_stolen: `reserved_base == stolen_top` means nothing is reserved (i915_gem_stolen.c:507)
        if rbase != top {
            reserved = Some((rbase, top - rbase));
        }
    }
    let usable = match reserved {
        Some((_, s)) => size - s,
        None => size,
    };
    Ok(Stolen { base, size, reserved, usable })
}

/// What the platform words say about the GGTT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GgttGeometry {
    /// Bytes of GGTT address space (`vm.total`), clamped to 4 GiB as Linux does.
    pub total: u64,
    /// Entries (`total / 4096`).
    pub entries: u64,
    /// Bytes of the aperture the CPU can map (`mappable_end`), clamped to `total`.
    pub mappable_end: u64,
    /// BAR0 offset of PTE 0: `GEN8_GTTADR_OFFSET` (8 MiB).
    pub pte_base: u32,
    pub fences: u32,
}

impl GgttGeometry {
    /// `gen8_gmch_probe` + `ggtt_probe_common` + the clamps of `i915_ggtt_probe_hw`.
    /// `bar0_len` is the GTTMMADR size and `gmadr_len` the aperture BAR size.
    pub fn probe(gmch_ctl: u16, bar0_len: u64, gmadr_len: u64) -> Result<GgttGeometry, MemError> {
        if bar0_len != GEN8_GTTMMADR_SIZE {
            // GEM_WARN_ON(pci_resource_len(...) != gen6_gttmmadr_size(i915)), intel_ggtt.c:1204
            return Err(MemError::Bar0Size { got: bar0_len, want: GEN8_GTTMMADR_SIZE });
        }
        let table = gen8_gtt_table_bytes(gmch_ctl)?;
        if table > GEN8_GTTMMADR_SIZE - GEN8_GTTADR_OFFSET as u64 {
            return Err(MemError::GgttOverflowsBar { table });
        }
        let mut total = table / 8 * 4096; // intel_ggtt.c:1284
        let mut mappable_end = gmadr_len;
        if total - 1 > u32::MAX as u64 {
            total = 1 << 32; // intel_ggtt.c:1536
            mappable_end = mappable_end.min(total);
        }
        if mappable_end > total {
            mappable_end = total; // intel_ggtt.c:1546
        }
        Ok(GgttGeometry { total, entries: total / 4096, mappable_end, pte_base: GEN8_GTTADR_OFFSET, fences: GEN7_NUM_FENCES })
    }

    /// BAR0 byte offset of GGTT entry `i` (`gsm + i * 8`, intel_ggtt.c:460).
    pub const fn pte_offset(&self, i: u64) -> u32 {
        self.pte_base + (i * 8) as u32
    }
}
