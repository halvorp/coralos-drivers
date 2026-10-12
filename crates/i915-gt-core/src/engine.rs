// SPDX-License-Identifier: GPL-2.0-only
//! The Kaby Lake engine table: `intel_engines[]` (gt/intel_engine_cs.c:63-243) restricted to the five engines a Gen9 GT can
//! have (RCS0, BCS0, VCS0, VCS1, VECS0), with `__engine_mmio_base` (:343-356), `get_reset_domain` (:396-445) and
//! `intel_engine_context_size` (:273-341) for Gen9. Engine ids are the bit positions of `enum intel_engine_id` that
//! i915-devinfo-core already carries.
use crate::error::GtError;
use crate::fixed::FixedVec;
use i915_devinfo_core::engine::{BCS0, RCS0, VCS0, VCS1, VECS0};
use i915_devinfo_core::DeviceInfo;
use i915_gt_regs_core as r;

/// `enum drm_i915_gem_engine_class` values (include/uapi/drm/i915_drm.h:157-175): RENDER 0, COPY 1, VIDEO 2, VIDEO_ENHANCE 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineClass {
    Render = 0,
    Copy = 1,
    VideoDecode = 2,
    VideoEnhance = 3,
}

/// `PAGE_SIZE` for the context-size constants (intel_engine_cs.c:43-50 are multiples of it).
pub const PAGE_SIZE: u32 = 4096;
/// `GEN9_LR_CONTEXT_RENDER_SIZE` (intel_engine_cs.c:47).
pub const GEN9_LR_CONTEXT_RENDER_SIZE: u32 = 22 * PAGE_SIZE; // intel_engine_cs.c:47
/// `GEN8_LR_CONTEXT_OTHER_SIZE` (intel_engine_cs.c:50).
pub const GEN8_LR_CONTEXT_OTHER_SIZE: u32 = 2 * PAGE_SIZE; // intel_engine_cs.c:50

/// One engine as `intel_engine_setup` builds it for a Gen9 part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineInfo {
    /// Bit position of `enum intel_engine_id` (`RCS0` = 0 ...).
    pub id: u32,
    pub class: EngineClass,
    pub instance: u8,
    /// `__sprint_engine_name`: "rcs0", "bcs0", "vcs0", "vcs1", "vecs0".
    pub name: &'static str,
    /// `engine->mmio_base`.
    pub mmio_base: u32,
    /// `engine->reset_domain`: the `GEN6_GDRST` bit that resets just this engine.
    pub reset_domain: u32,
    /// `intel_engine_context_size`: bytes of logical-ring-context image.
    pub context_size: u32,
}

/// One row of `struct engine_mmio_base` (intel_engine_cs.c:56-60): the base from this graphics version up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MmioBase {
    graphics_ver: u8,
    base: u32,
}

struct Row {
    id: u32,
    class: EngineClass,
    instance: u8,
    name: &'static str,
    /// "sorted in reverse graphics_ver order" (intel_engine_cs.c:56).
    bases: &'static [MmioBase],
}

static ROWS: [Row; 5] = [
    Row { id: RCS0, class: EngineClass::Render, instance: 0, name: "rcs0", bases: &[MmioBase { graphics_ver: 1, base: r::RENDER_RING_BASE }] }, // intel_engine_cs.c:64-70
    Row { id: BCS0, class: EngineClass::Copy, instance: 0, name: "bcs0", bases: &[MmioBase { graphics_ver: 6, base: r::BLT_RING_BASE }] },     // intel_engine_cs.c:71-77
    Row {
        id: VCS0,
        class: EngineClass::VideoDecode,
        instance: 0,
        name: "vcs0",
        bases: &[
            MmioBase { graphics_ver: 11, base: r::GEN11_BSD_RING_BASE }, // intel_engine_cs.c:138
            MmioBase { graphics_ver: 6, base: r::GEN6_BSD_RING_BASE },   // intel_engine_cs.c:139
            MmioBase { graphics_ver: 4, base: r::BSD_RING_BASE },        // intel_engine_cs.c:140
        ],
    }, // intel_engine_cs.c:134-142
    Row {
        id: VCS1,
        class: EngineClass::VideoDecode,
        instance: 1,
        name: "vcs1",
        bases: &[
            MmioBase { graphics_ver: 11, base: r::GEN11_BSD2_RING_BASE }, // intel_engine_cs.c:147
            MmioBase { graphics_ver: 8, base: r::GEN8_BSD2_RING_BASE },   // intel_engine_cs.c:148
        ],
    }, // intel_engine_cs.c:143-150
    Row {
        id: VECS0,
        class: EngineClass::VideoEnhance,
        instance: 0,
        name: "vecs0",
        bases: &[
            MmioBase { graphics_ver: 11, base: r::GEN11_VEBOX_RING_BASE }, // intel_engine_cs.c:197
            MmioBase { graphics_ver: 7, base: r::VEBOX_RING_BASE },        // intel_engine_cs.c:198
        ],
    }, // intel_engine_cs.c:193-200
];

/// `__engine_mmio_base` (intel_engine_cs.c:343-356): the first row whose version is `<=` the platform's. `None` when no row
/// qualifies (Linux: `GEM_BUG_ON(i == MAX_MMIO_BASES)`).
fn mmio_base(bases: &[MmioBase], graphics_ver: u8) -> Option<u32> {
    bases.iter().find(|b| graphics_ver >= b.graphics_ver).map(|b| b.base)
}

/// `get_reset_domain` for `ver < 11` (intel_engine_cs.c:432-443). The `ver >= 11` table is a different set of bits and is
/// not ported: a Gen11+ request is refused by name.
fn reset_domain(graphics_ver: u8, id: u32) -> Result<u32, GtError> {
    if graphics_ver >= 11 {
        return Err(GtError::UnsupportedGeneration { gen: graphics_ver });
    }
    match id {
        x if x == RCS0 => Ok(r::GEN6_GRDOM_RENDER), // intel_engine_cs.c:435
        x if x == BCS0 => Ok(r::GEN6_GRDOM_BLT),    // intel_engine_cs.c:436
        x if x == VCS0 => Ok(r::GEN6_GRDOM_MEDIA),  // intel_engine_cs.c:437
        x if x == VCS1 => Ok(r::GEN8_GRDOM_MEDIA2), // intel_engine_cs.c:438
        x if x == VECS0 => Ok(r::GEN6_GRDOM_VECS),  // intel_engine_cs.c:439
        _ => Err(GtError::NoSuchEngine { id }),
    }
}

/// `intel_engine_context_size` for Gen9 (intel_engine_cs.c:291-292 `case 9`, and the non-render classes' `GEN8_LR_CONTEXT_OTHER_SIZE`, :339).
fn context_size(graphics_ver: u8, class: EngineClass) -> Result<u32, GtError> {
    if graphics_ver != 9 {
        return Err(GtError::UnsupportedGeneration { gen: graphics_ver });
    }
    Ok(match class {
        EngineClass::Render => GEN9_LR_CONTEXT_RENDER_SIZE,
        _ => GEN8_LR_CONTEXT_OTHER_SIZE,
    })
}

/// `intel_engine_setup` for one id.
pub fn engine_info(graphics_ver: u8, id: u32) -> Result<EngineInfo, GtError> {
    let row = ROWS.iter().find(|r| r.id == id).ok_or(GtError::NoSuchEngine { id })?;
    let base = mmio_base(row.bases, graphics_ver).ok_or(GtError::UnsupportedGeneration { gen: graphics_ver })?;
    Ok(EngineInfo {
        id,
        class: row.class,
        instance: row.instance,
        name: row.name,
        mmio_base: base,
        reset_domain: reset_domain(graphics_ver, id)?,
        context_size: context_size(graphics_ver, row.class)?,
    })
}

/// Every engine in `info.engine_mask` (`.platform_engine_mask`), in `enum intel_engine_id` order, built for `info.graphics_ver`.
pub fn engines_for(info: &DeviceInfo) -> Result<FixedVec<EngineInfo, 5>, GtError> {
    let mut v = FixedVec::<EngineInfo, 5>::new("engines", engine_info(9, RCS0).unwrap_or(FILLER));
    for row in ROWS.iter() {
        if info.engine_mask & (1 << row.id) != 0 {
            v.push(engine_info(info.graphics_ver, row.id)?)?;
        }
    }
    // Any mask bit outside the five ids is an engine this table does not carry: refuse it by name.
    let known = ROWS.iter().fold(0u32, |m, r| m | (1 << r.id));
    let extra = info.engine_mask & !known;
    if extra != 0 {
        return Err(GtError::NoSuchEngine { id: extra.trailing_zeros() });
    }
    Ok(v)
}

const FILLER: EngineInfo = EngineInfo {
    id: 0,
    class: EngineClass::Render,
    instance: 0,
    name: "",
    mmio_base: 0,
    reset_domain: 0,
    context_size: 0,
};
