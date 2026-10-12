// SPDX-License-Identifier: GPL-2.0-only
//! The Kaby Lake engine table against intel_engine_cs.c and the ring bases of i915_reg.h.
use i915_devinfo_core::{lookup, KBL_GT1, KBL_GT2, KBL_GT3};
use i915_gt_core::engine::*;
use i915_gt_core::GtError;

const RCS0: u32 = 0;
const BCS0: u32 = 1;
const VCS0: u32 = 10;
const VCS1: u32 = 11;
const VECS0: u32 = 18;

#[test]
fn gen9_engine_rows_are_linux_literals() {
    // (id, class, instance, name, mmio_base, reset_domain, context_size)
    // bases: RENDER_RING_BASE 0x2000, BLT 0x22000, GEN6_BSD 0x12000, GEN8_BSD2 0x1c000, VEBOX 0x1a000 (i915_reg.h:243-265)
    // reset domains: GEN6_GRDOM_RENDER 2, BLT 8, MEDIA 4, GEN8_GRDOM_MEDIA2 0x80, VECS 0x10 (intel_gt_regs.h:664-670)
    // context sizes: 22 pages (GEN9_LR_CONTEXT_RENDER_SIZE) / 2 pages (GEN8_LR_CONTEXT_OTHER_SIZE)
    let want = [
        (RCS0, EngineClass::Render, 0u8, "rcs0", 0x2000u32, 0x02u32, 22 * 4096u32),
        (BCS0, EngineClass::Copy, 0, "bcs0", 0x22000, 0x08, 2 * 4096),
        (VCS0, EngineClass::VideoDecode, 0, "vcs0", 0x12000, 0x04, 2 * 4096),
        (VCS1, EngineClass::VideoDecode, 1, "vcs1", 0x1c000, 0x80, 2 * 4096),
        (VECS0, EngineClass::VideoEnhance, 0, "vecs0", 0x1a000, 0x10, 2 * 4096),
    ];
    for (id, class, instance, name, base, dom, cs) in want {
        let e = engine_info(9, id).unwrap();
        assert_eq!(e, EngineInfo { id, class, instance, name, mmio_base: base, reset_domain: dom, context_size: cs }, "{name}");
    }
}

#[test]
fn reset_domains_are_five_distinct_single_bits() {
    let mut seen = 0u32;
    for id in [RCS0, BCS0, VCS0, VCS1, VECS0] {
        let d = engine_info(9, id).unwrap().reset_domain;
        assert_eq!(d.count_ones(), 1);
        assert_eq!(seen & d, 0, "engine {id} collides");
        seen |= d;
    }
    assert_eq!(seen & 1, 0, "none of them is GRDOM_FULL (bit 0)");
}

#[test]
fn render_bases_by_generation_follow_the_first_row_at_or_below() {
    // RCS0 has a single row from version 1; VCS0: 11 -> 0x1c0000, 6 -> 0x12000, 4 -> 0x4000 (intel_engine_cs.c:138-140)
    assert_eq!(engine_info(9, RCS0).unwrap().mmio_base, 0x2000);
    // Gen11+ is refused by name: its reset-domain and context-size tables are not ported
    assert_eq!(engine_info(11, VCS0), Err(GtError::UnsupportedGeneration { gen: 11 }));
    // VCS1 only exists from version 8: a Gen7 request has no row
    assert_eq!(engine_info(7, VCS1), Err(GtError::UnsupportedGeneration { gen: 7 }));
    assert_eq!(engine_info(9, 3), Err(GtError::NoSuchEngine { id: 3 }));
    assert_eq!(engine_info(9, 99).unwrap_err().name(), "NO-SUCH-ENGINE");
}

#[test]
fn engines_follow_the_platform_engine_mask() {
    let names = |info: &i915_devinfo_core::DeviceInfo| engines_for(info).unwrap().as_slice().iter().map(|e| e.name).collect::<Vec<_>>();
    assert_eq!(names(&KBL_GT1), ["rcs0", "bcs0", "vcs0", "vecs0"]);
    assert_eq!(names(&KBL_GT2), ["rcs0", "bcs0", "vcs0", "vecs0"]);
    assert_eq!(names(&KBL_GT3), ["rcs0", "bcs0", "vcs0", "vcs1", "vecs0"], "kbl_gt3 adds the second VDBOX");
    // the Pixelbook Go: 8086:591C
    let d = lookup(0x591c).unwrap();
    assert_eq!(names(&d.info), ["rcs0", "bcs0", "vcs0", "vecs0"]);
}

#[test]
fn a_mask_bit_the_table_does_not_carry_is_refused() {
    let mut info = KBL_GT2;
    info.engine_mask |= 1 << 3; // BCS2
    assert_eq!(engines_for(&info).unwrap_err(), GtError::NoSuchEngine { id: 3 });
}

#[test]
fn context_size_constants() {
    assert_eq!(GEN9_LR_CONTEXT_RENDER_SIZE, 0x16000);
    assert_eq!(GEN8_LR_CONTEXT_OTHER_SIZE, 0x2000);
    assert_eq!(PAGE_SIZE, 4096);
    assert_eq!(engine_info(8, RCS0), Err(GtError::UnsupportedGeneration { gen: 8 }), "Gen8 render context is 20 pages: not ported");
}

#[test]
fn engine_class_values_are_the_uapi_numbers() {
    assert_eq!(
        (EngineClass::Render as u8, EngineClass::Copy as u8, EngineClass::VideoDecode as u8, EngineClass::VideoEnhance as u8),
        (0, 1, 2, 3)
    );
}
