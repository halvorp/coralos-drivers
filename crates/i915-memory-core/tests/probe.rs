// SPDX-License-Identifier: GPL-2.0-only
//! GMCH_CTRL / BSM / stolen-reserved decoding (intel_ggtt.c:1153, early-quirks.c:447, i915_gem_stolen.c:385).
use i915_memory_core::probe::*;
use i915_memory_core::MemError;

const MIB: u64 = 1 << 20;

#[test]
fn ggms_selects_2_4_8_mib_of_pte_and_zero_is_refused() {
    assert_eq!(gen8_gtt_table_bytes(1 << 6), Ok(2 * MIB));
    assert_eq!(gen8_gtt_table_bytes(2 << 6), Ok(4 * MIB));
    assert_eq!(gen8_gtt_table_bytes(3 << 6), Ok(8 * MIB));
    assert_eq!(gen8_gtt_table_bytes(0), Err(MemError::GgttAbsent { gmch_ctl: 0 }));
    // bits outside 7:6 never leak in (GMS is 15:8, bit 5:0 are other fields)
    assert_eq!(gen8_gtt_table_bytes(0xff00 | (2 << 6) | 0x3f), Ok(4 * MIB));
    assert_eq!(gen8_gtt_table_bytes(0xff00 | 0x3f), Err(MemError::GgttAbsent { gmch_ctl: 0xff3f }));
}

#[test]
fn geometry_total_is_entries_times_4k_clamped_to_4g() {
    // 8 MiB / 8 B = 1 Mi entries * 4 KiB = 4 GiB; 4 MiB -> 2 GiB; 2 MiB -> 1 GiB
    let g = GgttGeometry::probe(3 << 6, 16 * MIB, 256 * MIB).unwrap();
    assert_eq!((g.total, g.entries, g.mappable_end, g.pte_base, g.fences), (4 << 30, 1 << 20, 256 * MIB, 0x80_0000, 32));
    assert_eq!(GgttGeometry::probe(2 << 6, 16 * MIB, 256 * MIB).unwrap().total, 2 << 30);
    assert_eq!(GgttGeometry::probe(1 << 6, 16 * MIB, 256 * MIB).unwrap().total, 1 << 30);
    // the aperture is clamped to the GTT (intel_ggtt.c:1541-1546)
    assert_eq!(GgttGeometry::probe(1 << 6, 16 * MIB, 8 << 30).unwrap().mappable_end, 1 << 30);
    // a wrong BAR0 is refused (intel_ggtt.c:1204)
    assert_eq!(GgttGeometry::probe(3 << 6, 4 * MIB, 256 * MIB), Err(MemError::Bar0Size { got: 4 * MIB, want: 16 * MIB }));
    assert_eq!(GgttGeometry::probe(0, 16 * MIB, 256 * MIB), Err(MemError::GgttAbsent { gmch_ctl: 0 }));
}

#[test]
fn pte_offsets_start_8_mib_into_bar0() {
    let g = GgttGeometry::probe(3 << 6, 16 * MIB, 256 * MIB).unwrap();
    assert_eq!(g.pte_offset(0), 0x80_0000);
    assert_eq!(g.pte_offset(1), 0x80_0008);
    assert_eq!(g.pte_offset(0x1000), 0x80_0000 + 0x8000);
    assert_eq!(g.pte_offset(g.entries - 1), 0xff_fff8); // the last PTE is the last 8 bytes of the 16 MiB BAR
}

#[test]
fn stolen_size_follows_the_gen9_gms_ladder() {
    let gms = |g: u16| gen9_stolen_size(g << 8);
    assert_eq!(gms(0x00), 0);
    assert_eq!(gms(0x01), 32 * MIB);
    assert_eq!(gms(0x02), 64 * MIB);
    assert_eq!(gms(0xef), 0xef * 32 * MIB); // top of the 32 MiB ladder
    assert_eq!(gms(0xf0), 4 * MIB); // 4 MiB ladder starts at 4 MiB
    assert_eq!(gms(0xf1), 8 * MIB);
    assert_eq!(gms(0xff), 64 * MIB); // (0xf * 4) + 4
}

#[test]
fn stolen_base_masks_the_low_20_bits_and_reserved_is_decoded() {
    // GMS = 4 -> 128 MiB at 0x7b800000 -> top 0x83800000
    let ctl = 4 << 8;
    let s = decode_stolen(ctl, 0x7b80_0001, 0).unwrap();
    assert_eq!((s.base, s.size, s.reserved, s.usable), (0x7b80_0000, 128 * MIB, None, 128 * MIB));
    // enabled, base 0x82800000 -> reserved top 16 MiB
    let s = decode_stolen(ctl, 0x7b80_0001, 0x8280_0001).unwrap();
    assert_eq!(s.reserved, Some((0x8280_0000, 16 * MIB)));
    assert_eq!(s.usable, 112 * MIB);
    // enable bit clear -> none; address zero -> none; base == top -> none (i915_gem_stolen.c:395,398,507)
    assert_eq!(decode_stolen(ctl, 0x7b80_0001, 0x8280_0000).unwrap().reserved, None);
    assert_eq!(decode_stolen(ctl, 0x7b80_0001, 0x0000_0001).unwrap().reserved, None);
    assert_eq!(decode_stolen(ctl, 0x7b80_0001, 0x8380_0001).unwrap().reserved, None);
    // a reserved base outside the stolen range is refused rather than underflowing
    assert_eq!(
        decode_stolen(ctl, 0x7b80_0001, 0x8480_0001),
        Err(MemError::StolenReservedOutside { reserved_base: 0x8480_0000, base: 0x7b80_0000, top: 0x8380_0000 })
    );
    assert_eq!(decode_stolen(ctl, 0x7b80_0001, 0x7a80_0001).unwrap_err().name(), "STOLEN-RESERVED-OUTSIDE");
}
