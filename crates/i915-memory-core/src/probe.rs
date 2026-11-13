// SPDX-License-Identifier: GPL-2.0-only
//! Decoding of what the platform tells us about the Global GTT and stolen memory - config-space words and one register value
//! in, plain values out. No config or MMIO read happens here: the caller reads `SNB_GMCH_CTRL`, `INTEL_BSM`, BAR0/BAR2 sizes and
//! `GEN6_STOLEN_RESERVED` and passes the words.
//!
//! Ported from gt/intel_ggtt.c (`gen8_get_total_gtt_size`, `gen8_gmch_probe`, `ggtt_probe_common`, the 4 GiB / mappable clamps of
//! `i915_ggtt_probe_hw`), arch/x86/kernel/early-quirks.c (`gen9_stolen_size`, `gen3_stolen_base`) and gem/i915_gem_stolen.c
//! (`bdw_get_stolen_reserved`); for Cherryview (ruling D8) `chv_get_total_gtt_size` (intel_ggtt.c:1169-1178), `chv_stolen_size`
//! (early-quirks.c:426-445) and `chv_get_stolen_reserved` (i915_gem_stolen.c:352-383). [`GgttGeometry::probe_for`] and
//! [`decode_stolen_for`] pick the arm by the devinfo row, as Linux does by `IS_CHERRYVIEW`; any other row is refused by name.

use crate::regs::*;
use crate::MemError;
use i915_devinfo_core::{Platform, PlatformDescriptor};

/// Which probe arm a row takes: the Gen8/Gen9 BDW layout of GMCH_CTRL, or Cherryview's SNB layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    Gen9,
    Cherryview,
}

fn arm(desc: &PlatformDescriptor) -> Result<Arm, MemError> {
    match (desc.gen(), desc.platform()) {
        (9, _) => Ok(Arm::Gen9),
        (8, Platform::Cherryview) => Ok(Arm::Cherryview),
        (gen, _) => Err(MemError::UnsupportedGeneration { gen }),
    }
}

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

/// `chv_get_total_gtt_size` (intel_ggtt.c:1169-1178): Cherryview reads GGMS from the SNB field (bits 9:8) and the table is
/// `1 << (20 + GGMS)` bytes. GGMS 0 means "no GTT" (Linux returns 0; this port refuses by name).
pub fn chv_gtt_table_bytes(gmch_ctl: u16) -> Result<u64, MemError> {
    let ggms = (gmch_ctl >> SNB_GMCH_GGMS_SHIFT) & SNB_GMCH_GGMS_MASK; // intel_ggtt.c:1171-1172
    if ggms == 0 {
        return Err(MemError::GgttAbsent { gmch_ctl });
    }
    Ok(1u64 << (20 + ggms as u32)) // intel_ggtt.c:1174-1175
}

/// `chv_stolen_size` (early-quirks.c:426-445): bytes of stolen memory from the SNB GMS field (bits 7:3).
pub fn chv_stolen_size(gmch_ctl: u16) -> u64 {
    let gms = ((gmch_ctl >> SNB_GMCH_GMS_SHIFT) & SNB_GMCH_GMS_MASK) as u64; // early-quirks.c:432
    if gms < 0x11 {
        gms * 32 * MIB // early-quirks.c:439-440: 0x0 to 0x10, 32MB increments from 0
    } else if gms < 0x17 {
        (gms - 0x11) * 4 * MIB + 8 * MIB // :441-442: 0x11 to 0x16, 4MB increments from 8MB
    } else {
        (gms - 0x17) * 4 * MIB + 36 * MIB // :443-444: 0x17 to 0x1d, 4MB increments from 36MB
    }
}

/// `chv_get_stolen_reserved` (i915_gem_stolen.c:352-383): `(base, size)` of the reserved part, or `None` when the enable bit is
/// clear (Linux leaves `reserved_base = stolen_top`). The size is the register's own field; an encoding outside the four is
/// impossible (a 2-bit field has four values), so Linux's `MISSING_CASE` default (8 MiB) cannot be reached.
pub fn chv_stolen_reserved(stolen_reserved: u32) -> Option<(u64, u64)> {
    if stolen_reserved & GEN6_STOLEN_RESERVED_ENABLE == 0 {
        return None; // :362-363
    }
    let base = (stolen_reserved & GEN6_STOLEN_RESERVED_ADDR_MASK) as u64; // :364
    let size = match stolen_reserved & GEN8_STOLEN_RESERVED_SIZE_MASK {
        GEN8_STOLEN_RESERVED_1M => MIB,     // :367-369
        GEN8_STOLEN_RESERVED_2M => 2 * MIB, // :370-372
        GEN8_STOLEN_RESERVED_4M => 4 * MIB, // :373-375
        _ => 8 * MIB,                       // GEN8_STOLEN_RESERVED_8M, :376-378
    };
    Some((base, size))
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

/// [`decode_stolen`] for the row's arm: Gen9 sizes with `gen9_stolen_size` and `bdw_get_stolen_reserved`; Cherryview with
/// `chv_stolen_size` and `chv_get_stolen_reserved`, then `init_reserved_stolen`'s checks (i915_gem_stolen.c:470-526): a reserved
/// base equal to the top is "no reserved stolen" (:503-504), a reserved range not inside stolen is refused by name (:506-524). The
/// base is `gen3_stolen_base` for both (early-quirks.c:339; `chv_early_ops`, :509-512).
pub fn decode_stolen_for(desc: &PlatformDescriptor, gmch_ctl: u16, bsm: u32, stolen_reserved: u32) -> Result<Stolen, MemError> {
    match arm(desc)? {
        Arm::Gen9 => decode_stolen(gmch_ctl, bsm, stolen_reserved),
        Arm::Cherryview => {
            let size = chv_stolen_size(gmch_ctl);
            let base = (bsm & INTEL_BSM_MASK) as u64;
            let top = base + size;
            let reserved = match chv_stolen_reserved(stolen_reserved) {
                None => None,
                Some((rbase, _)) if rbase == top => None,
                Some((rbase, rsize)) => {
                    // !reserved_base (:506) and !resource_contains (:516): both are a reservation outside the stolen range
                    if rbase == 0 || rbase < base || rbase.checked_add(rsize).map_or(true, |e| e > top) {
                        return Err(MemError::StolenReservedOutside { reserved_base: rbase, base, top });
                    }
                    Some((rbase, rsize))
                }
            };
            let usable = size - reserved.map_or(0, |(_, s)| s);
            Ok(Stolen { base, size, reserved, usable })
        }
    }
}

/// Decode stolen memory from GMCH_CTRL (size), BSM (base) and `GEN6_STOLEN_RESERVED` (reserved top). The Gen9 arm.
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
    /// `gen8_gmch_probe` + `ggtt_probe_common` + the clamps of `i915_ggtt_probe_hw`, for the Gen9 arm.
    /// `bar0_len` is the GTTMMADR size and `gmadr_len` the aperture BAR size.
    pub fn probe(gmch_ctl: u16, bar0_len: u64, gmadr_len: u64) -> Result<GgttGeometry, MemError> {
        Self::probe_table(gen8_gtt_table_bytes(gmch_ctl), bar0_len, gmadr_len, GEN7_NUM_FENCES)
    }

    /// [`GgttGeometry::probe`] for the row's arm: `gen8_gmch_probe` takes `chv_get_total_gtt_size` on Cherryview
    /// (intel_ggtt.c:1275-1278), and Cherryview has 16 fences, not 32 (`intel_ggtt_init_fences`, intel_ggtt_fencing.c:855-861).
    /// Both are Gen8+ parts: GTTMMADR is 16 MiB and the table starts at 8 MiB (`gen6_gttmmadr_size`, intel_ggtt.c:1187).
    pub fn probe_for(desc: &PlatformDescriptor, gmch_ctl: u16, bar0_len: u64, gmadr_len: u64) -> Result<GgttGeometry, MemError> {
        match arm(desc)? {
            Arm::Gen9 => Self::probe(gmch_ctl, bar0_len, gmadr_len),
            Arm::Cherryview => Self::probe_table(chv_gtt_table_bytes(gmch_ctl), bar0_len, gmadr_len, CHV_NUM_FENCES),
        }
    }

    fn probe_table(table: Result<u64, MemError>, bar0_len: u64, gmadr_len: u64, fences: u32) -> Result<GgttGeometry, MemError> {
        if bar0_len != GEN8_GTTMMADR_SIZE {
            // GEM_WARN_ON(pci_resource_len(...) != gen6_gttmmadr_size(i915)), intel_ggtt.c:1204
            return Err(MemError::Bar0Size { got: bar0_len, want: GEN8_GTTMMADR_SIZE });
        }
        let table = table?;
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
        Ok(GgttGeometry { total, entries: total / 4096, mappable_end, pte_base: GEN8_GTTADR_OFFSET, fences })
    }

    /// BAR0 byte offset of GGTT entry `i` (`gsm + i * 8`, intel_ggtt.c:460).
    pub const fn pte_offset(&self, i: u64) -> u32 {
        self.pte_base + (i * 8) as u32
    }
}
