// SPDX-License-Identifier: GPL-2.0-only
//! The gain ceiling: the derivation, the table, and the property that NO input produces a gain above it.
use max98373_core::gain::*;

/// ChromeOS's CRAS curve, a few anchor points typed by hand from `etc/cras/avs_max98373` (db_at_100 .. db_at_0).
#[test]
fn the_curve_matches_the_anchor_points_of_the_owners_file() {
    assert_eq!(CRAS_CURVE_CDB.len(), 101);
    for (step, cdb) in [(100usize, 0i16), (99, -50), (98, -100), (97, -200), (90, -500), (80, -850), (70, -1150), (69, -1200), (68, -1250), (50, -1800), (30, -2550), (10, -3700), (1, -4500), (0, -4600)] {
        assert_eq!(CRAS_CURVE_CDB[step], cdb, "db_at_{step}");
    }
    // a volume curve never gets louder as the step goes down
    for s in 1..=100 {
        assert!(CRAS_CURVE_CDB[s - 1] <= CRAS_CURVE_CDB[s], "step {s}");
    }
}

#[test]
fn the_curve_is_the_file_line_for_line_when_the_file_is_here() {
    let d = std::env::var("CORAL_AVS_CRAS_DIR").unwrap_or_else(|_| "/home/peter/Projects/coral_handoffs/atlas/recovery/configs/etc/cras".to_string());
    let p = std::path::Path::new(&d).join("avs_max98373");
    if !p.exists() {
        assert!(std::env::var("CORAL_AVS_REQUIRE_BLOBS").is_err(), "avs_max98373 is required (CORAL_AVS_REQUIRE_BLOBS) and is not at {}", p.display());
        eprintln!("avs_max98373 (CRAS): SKIPPED (set CORAL_AVS_CRAS_DIR)");
        return;
    }
    let mut seen = 0;
    for l in std::fs::read_to_string(p).unwrap().lines() {
        let t = l.trim();
        if let Some(rest) = t.strip_prefix("db_at_") {
            let (step, v) = rest.split_once('=').unwrap();
            let (step, v): (usize, i16) = (step.trim().parse().unwrap(), v.trim().parse().unwrap());
            assert_eq!(CRAS_CURVE_CDB[step], v, "db_at_{step}");
            seen += 1;
        }
    }
    assert_eq!(seen, 101);
}

#[test]
fn the_ceiling_is_the_highest_cras_step_at_or_below_minus_twelve_db() {
    assert_eq!(CEILING_CDB, -1200);
    let highest = (0..=100usize).rev().find(|s| CRAS_CURVE_CDB[*s] <= -1200).unwrap();
    assert_eq!(CEILING_STEP, highest);
    assert_eq!(CEILING_STEP, 69);
    // strictly below ChromeOS's own maximum (0 dB at step 100), and below the topology's DSP Volume control maximum 0x3FFFFFFF (-6.02 dB)
    assert!(CEILING_CDB < CRAS_CURVE_CDB[100]);
    assert!(CEILING_LINEAR < 0x3FFF_FFFF);
}

fn lin(db: f64) -> f64 {
    0x7FFF_FFFFu32 as f64 * 10f64.powf(db / 20.0)
}

#[test]
fn the_ceiling_and_every_table_entry_are_ten_to_the_db_over_twenty() {
    let c = lin(-12.0);
    assert!((CEILING_LINEAR as f64 - c).abs() <= 1.0, "{} vs {}", CEILING_LINEAR, c);
    assert_eq!(GAIN_Q31.len(), STEPS);
    assert_eq!(GAIN_Q31[0], CEILING_LINEAR);
    for (i, g) in GAIN_Q31.iter().enumerate() {
        let want = lin(-12.0 - 0.75 * i as f64);
        assert!((*g as f64 - want).abs() <= 1.0, "step {i}: {} vs {}", g, want);
        if i > 0 {
            assert!(*g < GAIN_Q31[i - 1], "strictly decreasing at {i}");
        }
        assert!(*g > 0 && *g <= CEILING_LINEAR);
    }
    // 63 steps of 0.75 dB = 47.25 dB below the ceiling
    let span_db = 20.0 * (GAIN_Q31[0] as f64 / GAIN_Q31[STEPS - 1] as f64).log10();
    assert!((span_db - 47.25).abs() < 0.001, "{span_db}");
}

#[test]
fn the_derivation_numbers_hold() {
    // 9.65 V full scale (Output Voltage 5, UCM) at -12.00 dB: 2.43 V peak; V^2 / 2R
    let vpk = 9.65 * 10f64.powf(-12.0 / 20.0);
    assert!((vpk - 2.4245).abs() < 0.001, "{vpk}");
    assert!(vpk * vpk / (2.0 * 8.0) < 0.38 && vpk * vpk / (2.0 * 4.0) < 0.74);
    // and at ChromeOS's own maximum it is not: this is the point of the ceiling
    let full = 9.65f64;
    assert!(full * full / (2.0 * 8.0) > 5.8 && full * full / (2.0 * 4.0) > 11.6);
}

#[test]
fn no_level_and_no_mute_flag_can_produce_a_gain_above_the_ceiling() {
    // every u16 level, both mute states: exhaustive, so a property test could add nothing
    let mut max_seen = 0u32;
    for level in 0..=u16::MAX {
        for mute in [false, true] {
            let g = linear_for(level, mute);
            assert!(g <= CEILING_LINEAR, "level {level} mute {mute} -> {g}");
            if mute || level == 0 {
                assert_eq!(g, 0, "silence is silence");
            }
            max_seen = max_seen.max(g);
        }
    }
    assert_eq!(max_seen, CEILING_LINEAR, "the ceiling is reachable, at the top level and above");
    assert_eq!(linear_for(LEVEL_MAX, false), CEILING_LINEAR);
    assert_eq!(linear_for(u16::MAX, false), CEILING_LINEAR, "a level above the scale is the scale");
    assert!(linear_for(1, false) > 0 && linear_for(1, false) == GAIN_Q31[STEPS - 1]);
}

#[test]
fn the_level_map_is_monotonic() {
    let mut prev = 0;
    for level in 0..=LEVEL_MAX {
        let g = linear_for(level, false);
        assert!(g >= prev, "level {level}");
        prev = g;
    }
}

#[test]
fn clamp_is_the_second_line_of_defence() {
    assert_eq!(clamp(0), 0);
    assert_eq!(clamp(CEILING_LINEAR), CEILING_LINEAR);
    assert_eq!(clamp(CEILING_LINEAR + 1), CEILING_LINEAR);
    assert_eq!(clamp(u32::MAX), CEILING_LINEAR);
    assert_eq!(clamp(0x7FFF_FFFF), CEILING_LINEAR);
    // a pseudo-random sweep through the whole u32 range
    let mut x = 0x1234_5678u32;
    for _ in 0..100_000 {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        assert!(clamp(x) <= CEILING_LINEAR);
    }
}
