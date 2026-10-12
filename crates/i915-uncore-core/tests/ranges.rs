// SPDX-License-Identifier: GPL-2.0-only
//! `__gen9_fw_ranges` pinned entry by entry against intel_uncore.c:1275-1306, plus the lookup rules.
use i915_uncore_core::*;

const R: u32 = 1;
const G: u32 = 2;
const M: u32 = 4;

/// Hand-copied from intel_uncore.c:1275-1306 (GEN_FW_RANGE(start, end, domains)); 0 = "uncore range".
const WANT: [(u32, u32, u32); 32] = [
    (0x0, 0xaff, G),
    (0xb00, 0x1fff, 0),
    (0x2000, 0x26ff, R),
    (0x2700, 0x2fff, G),
    (0x3000, 0x3fff, R),
    (0x4000, 0x51ff, G),
    (0x5200, 0x7fff, R),
    (0x8000, 0x812f, G),
    (0x8130, 0x813f, M),
    (0x8140, 0x815f, R),
    (0x8160, 0x82ff, G),
    (0x8300, 0x84ff, R),
    (0x8500, 0x87ff, G),
    (0x8800, 0x89ff, M),
    (0x8a00, 0x8bff, G),
    (0x8c00, 0x8cff, R),
    (0x8d00, 0x93ff, G),
    (0x9400, 0x97ff, R | M),
    (0x9800, 0xafff, G),
    (0xb000, 0xb47f, R),
    (0xb480, 0xcfff, G),
    (0xd000, 0xd7ff, M),
    (0xd800, 0xdfff, G),
    (0xe000, 0xe8ff, R),
    (0xe900, 0x11fff, G),
    (0x12000, 0x13fff, M),
    (0x14000, 0x19fff, G),
    (0x1a000, 0x1e9ff, M),
    (0x1ea00, 0x243ff, G),
    (0x24400, 0x247ff, R),
    (0x24800, 0x2ffff, G),
    (0x30000, 0x3ffff, M),
];

#[test]
fn every_range_is_pinned_to_linux() {
    assert_eq!(GEN9_FW_RANGES.len(), 32);
    for (i, (s, e, d)) in WANT.iter().enumerate() {
        let r = GEN9_FW_RANGES[i];
        assert_eq!((r.start, r.end, r.domains), (*s, *e, *d), "entry {i}");
    }
}

#[test]
fn the_table_is_sorted_watertight_and_covers_0_to_0x3ffff() {
    assert_eq!(GEN9_FW_RANGES[0].start, 0);
    assert_eq!(GEN9_FW_RANGES[31].end, 0x3ffff);
    for w in GEN9_FW_RANGES.windows(2) {
        assert_eq!(w[0].end + 1, w[1].start, "gap or overlap after {:#x}", w[0].end);
    }
    for r in GEN9_FW_RANGES.iter() {
        assert!(r.start <= r.end);
        assert_eq!(r.domains & !7, 0);
    }
}

#[test]
fn each_range_edge_resolves_to_its_own_domain_and_one_past_to_the_next() {
    for (i, (s, e, d)) in WANT.iter().enumerate() {
        assert_eq!(find_fw_domain(*s), *d, "start of entry {i}");
        assert_eq!(find_fw_domain(*e), *d, "end of entry {i}");
        if let Some((s2, _, d2)) = WANT.get(i + 1) {
            assert_eq!(find_fw_domain(*e + 1), *d2);
            assert_eq!(*e + 1, *s2);
        }
        if i > 0 {
            assert_eq!(find_fw_domain(*s - 1), WANT[i - 1].2, "just below entry {i}");
        }
    }
    assert_eq!(find_fw_domain(0x40000), 0, "past the table");
    assert_eq!(find_fw_domain(u32::MAX), 0);
}

#[test]
fn composite_range_is_the_or_of_its_named_parts() {
    // trap 1: 0x9400..=0x97ff is RENDER | MEDIA (intel_uncore.c:1292)
    assert_eq!(find_fw_domain(0x9400), FORCEWAKE_RENDER | FORCEWAKE_MEDIA);
    assert_eq!(find_fw_domain(0x9400), R | M);
    assert_eq!(find_fw_domain(0x97ff), R | M);
    // zero-valued (trap 2): the uncore ranges need no forcewake, and are distinct from every domain
    assert_eq!(find_fw_domain(0xb00), 0);
    assert_eq!(find_fw_domain(0x1fff), 0);
    assert_ne!(find_fw_domain(0xb00), find_fw_domain(0xaff));
    assert_eq!((FORCEWAKE_RENDER, FORCEWAKE_GT, FORCEWAKE_MEDIA), (R, G, M));
}

#[test]
fn the_registers_the_proof_of_life_reads_land_in_the_domains_linux_says() {
    assert_eq!(read_fw_domains(0x9120), G); // GEN8_FUSE2 in 0x8d00-0x93ff
    assert_eq!(read_fw_domains(0x9134), G); // GEN8_EU_DISABLE0
    assert_eq!(read_fw_domains(0x9138), G);
    assert_eq!(read_fw_domains(0x913c), G);
    assert_eq!(read_fw_domains(0xd00), 0); // RPM_CONFIG0 in the uncore range 0xb00-0x1fff
    assert_eq!(read_fw_domains(0xa26c), G); // CTC_MODE in 0x9800-0xafff
    assert_eq!(read_fw_domains(0x145998), 0); // GEN6_RP_STATE_CAP: NEEDS_FORCE_WAKE says yes (>= 0x116000) but the Gen9 table ends at 0x3ffff
    assert_eq!(read_fw_domains(0x44074), 0); // GEN9_TIMESTAMP_OVERRIDE: 0x40000..0x116000 needs none
    assert_eq!(read_fw_domains(0xd84), 0); // the ack registers are in the uncore range too
    assert_eq!(read_fw_domains(0xd88), 0);
    assert_eq!(read_fw_domains(0x130044), 0); // FORCEWAKE_ACK_GT_GEN9: above 0x116000 but past the table (find_fw_domain -> 0)
}

#[test]
fn needs_force_wake_has_a_gap_between_0x40000_and_0x116000() {
    assert!(needs_force_wake(0));
    assert!(needs_force_wake(0x3ffff));
    assert!(!needs_force_wake(0x40000));
    assert!(!needs_force_wake(0x115fff));
    assert!(needs_force_wake(0x116000));
    assert!(needs_force_wake(0x145998));
}

#[test]
fn shadowed_registers_are_written_without_forcewake_but_still_read_with_it() {
    for (lo, hi) in [(0x2030u32, 0x2030u32), (0xA008, 0xA00C), (0x12030, 0x12030), (0x1a030, 0x1a030), (0x22030, 0x22030)] {
        for off in [lo, hi] {
            assert!(is_shadowed(off), "{off:#x}");
            assert_eq!(write_fw_domains(off), 0, "{off:#x} write needs nothing");
            assert_eq!(read_fw_domains(off), find_fw_domain(off), "{off:#x} read still needs forcewake");
        }
        assert!(!is_shadowed(lo - 1));
        assert!(!is_shadowed(hi + 1));
    }
    assert_ne!(read_fw_domains(0x2030), 0, "0x2030 is in the RENDER range");
    assert_eq!(read_fw_domains(0x2030), R);
    assert_eq!(write_fw_domains(0x2034), R, "a neighbour of a shadowed register is not shadowed");
}
