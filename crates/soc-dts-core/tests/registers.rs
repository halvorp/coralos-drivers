// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux literals for the SoC DTS register corpus. Ported from
//! `drivers/thermal/intel/intel_soc_dts_iosf.c` and `intel_soc_dts_iosf.h`.
//!
//! Copyright (c) 2015, Intel Corporation.

use soc_dts_core::registers::{
    bit, offset, INTERRUPT_TYPES, SENSOR_COUNT, SENSOR_NAMES, TJMAX_ENCODING, TRIP_COUNT,
    TRIP_MASK, TRIP_NAMES,
};

/// `intel_soc_dts_iosf.c:17-25`. The expected values are Linux literals, not derived from the
/// production constants.
#[test]
fn all_eight_register_offsets_match_linux() {
    let got = [
        ("ENABLE", offset::ENABLE),
        ("TEMP", offset::TEMP),
        ("PTPS", offset::PTPS),
        ("PTTS", offset::PTTS),
        ("PTTSS", offset::PTTSS),
        ("PTMC", offset::PTMC),
        ("TE_AUX0", offset::TE_AUX0),
        ("TE_AUX1", offset::TE_AUX1),
    ];
    let expected = [
        ("ENABLE", 0xB0),  // intel_soc_dts_iosf.c:17
        ("TEMP", 0xB1),    // intel_soc_dts_iosf.c:18
        ("PTPS", 0xB2),    // intel_soc_dts_iosf.c:20
        ("PTTS", 0xB3),    // intel_soc_dts_iosf.c:21
        ("PTTSS", 0xB4),   // intel_soc_dts_iosf.c:22
        ("PTMC", 0x80),    // intel_soc_dts_iosf.c:23
        ("TE_AUX0", 0xB5), // intel_soc_dts_iosf.c:24
        ("TE_AUX1", 0xB6), // intel_soc_dts_iosf.c:25
    ];
    assert_eq!(got.len(), 8);
    assert_eq!(got, expected);
}

/// `intel_soc_dts_iosf.c:27-35,38,41`.
#[test]
fn all_bits_masks_and_encoding_literals_match_linux() {
    let got = [
        ("AUX0_ENABLE", bit::AUX0_ENABLE),
        ("AUX1_ENABLE", bit::AUX1_ENABLE),
        ("CPU_MODULE0_ENABLE", bit::CPU_MODULE0_ENABLE),
        ("CPU_MODULE1_ENABLE", bit::CPU_MODULE1_ENABLE),
        ("TE_SCI_ENABLE", bit::TE_SCI_ENABLE),
        ("TE_SMI_ENABLE", bit::TE_SMI_ENABLE),
        ("TE_MSI_ENABLE", bit::TE_MSI_ENABLE),
        ("TE_APICA_ENABLE", bit::TE_APICA_ENABLE),
        ("PTMC_APIC_DEASSERT", bit::PTMC_APIC_DEASSERT),
    ];
    let expected = [
        ("AUX0_ENABLE", 1 << 0),         // intel_soc_dts_iosf.c:27
        ("AUX1_ENABLE", 1 << 1),         // intel_soc_dts_iosf.c:28
        ("CPU_MODULE0_ENABLE", 1 << 16), // intel_soc_dts_iosf.c:29
        ("CPU_MODULE1_ENABLE", 1 << 17), // intel_soc_dts_iosf.c:30
        ("TE_SCI_ENABLE", 1 << 9),       // intel_soc_dts_iosf.c:31
        ("TE_SMI_ENABLE", 1 << 10),      // intel_soc_dts_iosf.c:32
        ("TE_MSI_ENABLE", 1 << 11),      // intel_soc_dts_iosf.c:33
        ("TE_APICA_ENABLE", 1 << 14),    // intel_soc_dts_iosf.c:34
        ("PTMC_APIC_DEASSERT", 1 << 4),  // intel_soc_dts_iosf.c:35
    ];
    assert_eq!(got.len(), 9);
    assert_eq!(got, expected);
    assert_eq!(TJMAX_ENCODING, 0x7F); // intel_soc_dts_iosf.c:38
    assert_eq!(TRIP_MASK, 0x03); // intel_soc_dts_iosf.c:41
}

/// Linux defines exactly two sensors and two OSPM trips (`intel_soc_dts_iosf.h:12-16`). Names are
/// frozen as well as counts so a swapped or duplicated item cannot hide behind the same length.
#[test]
fn sensor_and_trip_counts_and_names_are_pinned() {
    const LINUX_SENSOR_NAMES: [&str; 2] = ["DTS0", "DTS1"]; // intel_soc_dts_iosf.h:12-13
    const LINUX_TRIP_NAMES: [&str; 2] = ["trip0", "trip1"]; // intel_soc_dts_iosf.c:293-294
    assert_eq!(SENSOR_COUNT, 2); // intel_soc_dts_iosf.h:13
    assert_eq!(TRIP_COUNT, 2); // intel_soc_dts_iosf.h:16
    assert_eq!(SENSOR_NAMES, LINUX_SENSOR_NAMES);
    assert_eq!(TRIP_NAMES, LINUX_TRIP_NAMES);
}

/// Every enumerator at `intel_soc_dts_iosf.h:18-24`, with literal names and implicit values.
#[test]
fn all_five_interrupt_types_are_pinned_by_name_and_value() {
    let got: Vec<(u8, &str)> = INTERRUPT_TYPES
        .iter()
        .map(|item| (item.value, item.name))
        .collect();
    let expected = vec![
        (0, "INTEL_SOC_DTS_INTERRUPT_NONE"), // intel_soc_dts_iosf.h:19
        (1, "INTEL_SOC_DTS_INTERRUPT_APIC"), // intel_soc_dts_iosf.h:20
        (2, "INTEL_SOC_DTS_INTERRUPT_MSI"),  // intel_soc_dts_iosf.h:21
        (3, "INTEL_SOC_DTS_INTERRUPT_SCI"),  // intel_soc_dts_iosf.h:22
        (4, "INTEL_SOC_DTS_INTERRUPT_SMI"),  // intel_soc_dts_iosf.h:23
    ];
    assert_eq!(INTERRUPT_TYPES.len(), 5);
    assert_eq!(got, expected);
}
