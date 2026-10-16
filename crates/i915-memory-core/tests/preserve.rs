// SPDX-License-Identifier: GPL-2.0-only
//! The GGTT created AROUND a live range (`Gtt::new_ggtt_preserving`): the firmware framebuffer the display engine is scanning out is
//! reserved before anything else, its entries are never written, the error-capture slot moves off it, and no bind may land on it.
//! Linux: i915_initial_plane.c `initial_plane_vma` (PIN_OFFSET_FIXED at the plane's base, :185) and intel_ggtt.c `init_ggtt`
//! (:914-922 error capture at 0 "or anywhere low", :943-949 "Clear any non-preallocated blocks", :952 the guard page).
mod common;
use common::*;
use i915_memory_core::*;

const SCRATCH: u64 = 0x9000;
/// The Pixelbook Go's firmware framebuffer: 1920x1080 XRGB8888 linear, stride 7680, at GGTT 0 - 7680 * 1080 = 0x7e9000 bytes.
const FB: Preallocated = Preallocated { start: 0, size: 0x7e_9000, what: "adopted-scanout" };
const FB_PAGES: u32 = 0x7e9;
const ENTRIES: u32 = 1 << 20;

fn req(pages: &[u64], size: u64, place: Placement) -> BindRequest<'_> {
    BindRequest { pages, size, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place }
}

fn ggtt_entry(w: &Write) -> u32 {
    match w.target {
        Target::Ggtt { mmio_offset } => (mmio_offset - 0x80_0000) / 8,
        t => panic!("a GGTT plan wrote {t:?}"),
    }
}

#[test]
fn the_adopted_framebuffer_entries_are_never_written_and_everything_else_is_scratch() {
    let (g, plan) = Gtt::new_ggtt_preserving(&kbl(), &geom_4g(), SCRATCH, &[FB]).unwrap();
    assert_eq!(plan.ops(), [Op::GgttFill { first: FB_PAGES, count: ENTRIES - FB_PAGES, value: 0x9001 }], "one fill, from the first page past the framebuffer to the guard page");
    assert!(plan.writes().all(|w| ggtt_entry(&w) >= FB_PAGES), "no store lands on an adopted entry");
    assert_eq!(plan.writes().count() as u32, ENTRIES - FB_PAGES);
    assert_eq!(plan.writes().last().map(|w| ggtt_entry(&w)), Some(ENTRIES - 1), "the guard page is still cleared");
    assert_eq!(g.live_nodes(), 2, "the framebuffer and the error-capture slot");
}

/// NEGATIVE CONTROL: the same GGTT with nothing kept DOES store over entry 0 - the test above can see a wipe of the live scanout.
#[test]
fn without_the_reservation_the_live_scanout_would_be_overwritten() {
    let (_, plan) = Gtt::new_ggtt_preserving(&kbl(), &geom_4g(), SCRATCH, &[]).unwrap();
    assert_eq!(plan.writes().next().map(|w| ggtt_entry(&w)), Some(0));
    assert_eq!(plan.ops(), Gtt::new_ggtt(&kbl(), &geom_4g(), SCRATCH).unwrap().1.ops(), "new_ggtt IS new_ggtt_preserving with nothing kept");
}

#[test]
fn the_error_capture_slot_moves_to_the_lowest_free_mappable_address_when_0_is_the_framebuffer() {
    let (g, _) = Gtt::new_ggtt_preserving(&kbl(), &geom_4g(), SCRATCH, &[FB]).unwrap();
    assert_eq!(g.reservation_at(0), Some("adopted-scanout"));
    assert_eq!(g.reservation_at(FB.size - 4096), Some("adopted-scanout"));
    assert_eq!(g.reservation_at(FB.size), Some(ERROR_CAPTURE), "DRM_MM_INSERT_LOW: right after the framebuffer");
    assert_eq!(g.reservation_at(FB.size + 2 * 4096), None);
    // with 0 free the slot stays at 0 (drm_mm_reserve_node succeeded)
    let mid = Preallocated { start: 0x10_0000, size: 0x20_0000, what: "adopted-scanout" };
    let (g, plan) = Gtt::new_ggtt_preserving(&kbl(), &geom_4g(), SCRATCH, &[mid]).unwrap();
    assert_eq!(g.reservation_at(0), Some(ERROR_CAPTURE));
    assert_eq!(plan.ops(), [Op::GgttFill { first: 0, count: 0x100, value: 0x9001 }, Op::GgttFill { first: 0x300, count: ENTRIES - 0x300, value: 0x9001 }], "the holes on both sides");
}

#[test]
fn no_error_capture_slot_when_the_kept_range_fills_the_aperture() {
    let geom = geom_4g();
    let all = Preallocated { start: 0, size: geom.mappable_end, what: "adopted-scanout" };
    let (g, _) = Gtt::new_ggtt_preserving(&kbl(), &geom, SCRATCH, &[all]).unwrap();
    assert_eq!(g.live_nodes(), 1, "Linux remains silent when no space is found for the error capture (intel_ggtt.c:903-911)");
}

#[test]
fn a_bind_over_the_adopted_range_is_refused_by_name_and_no_placement_lands_on_it() {
    let (mut g, _) = Gtt::new_ggtt_preserving(&kbl(), &geom_4g(), SCRATCH, &[FB]).unwrap();
    let pg = [0x10_0000u64; 1];
    let fx = |o| Placement::with_mode(Mode::Fixed(o));
    let refused = Some(MemError::RangeReserved { start: 0x1000, len: 4096, by: "adopted-scanout" });
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(0x1000))).err(), refused);
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(0x1000).evicting())).err(), refused, "a reservation is never a victim");
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(0x1000))).err().map(|e| e.name()), Some("RANGE-RESERVED"));
    let low = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, Placement::with_mode(Mode::Low))).unwrap();
    assert!(low.offset() >= FB.size + 2 * 4096, "LOW starts past the framebuffer and the error-capture slot: {:#x}", low.offset());
    let best = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, Placement::best())).unwrap();
    assert!(best.offset() >= FB.size, "{:#x}", best.offset());
    assert_eq!(g.check_access(0x2000, 4).err(), Some(MemError::StaleMapping { va: 0x2000 }), "the GPU may not address the framebuffer through a VMA");
    assert_eq!(g.plan_unbind(VmaId(1)).err(), Some(MemError::Pinned { id: 1 }), "id 1 is the framebuffer's reservation: never unbound");
}

#[test]
fn bad_kept_ranges_are_refused_by_name() {
    let k = |start, size| Preallocated { start, size, what: "adopted-scanout" };
    let make = |keep: &[Preallocated]| Gtt::new_ggtt_preserving(&kbl(), &geom_4g(), SCRATCH, keep).err();
    assert_eq!(make(&[k(0x800, 0x1000)]), Some(MemError::Misaligned { what: "adopted-scanout", value: 0x800, align: 4096 }));
    assert_eq!(make(&[k(0, 0x1800)]), Some(MemError::Misaligned { what: "adopted-scanout", value: 0x1800, align: 4096 }));
    assert_eq!(make(&[k(0, 0)]), Some(MemError::ZeroLength));
    let end = (4u64 << 30) - 4096;
    assert_eq!(make(&[k(end, 4096)]), Some(MemError::BeyondSpace { va: end, len: 4096, total: end }), "the guard page cannot be kept");
    assert_eq!(make(&[k(0, 0x2000), k(0x1000, 0x2000)]), Some(MemError::RangeReserved { start: 0x1000, len: 0x2000, by: "adopted-scanout" }), "two kept ranges may not overlap");
    assert!(make(&[k(0, 0x2000), k(0x2000, 0x2000)]).is_none(), "adjacent is fine");
}
