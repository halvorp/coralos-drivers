// SPDX-License-Identifier: GPL-2.0-only
//! Literal MFD sub-device vectors from Linux `drivers/mfd/intel_soc_pmic_crc.c`.
//!
//! Copyright (C) 2012-2014, 2022 Intel Corporation. All rights reserved.
//! Original authors: Yang, Bin <bin.yang@intel.com> and Zhu, Lejun <lejun.zhu@linux.intel.com>.

use pmic_crc_core::devices::{
    devices, Platform, SubDevice, BAY_TRAIL_DEVICES, CHERRY_TRAIL_DEVICES,
};

/// intel_soc_pmic_crc.c:58-95. Deliberately literal, never generated from `BAY_TRAIL_DEVICES`.
const LINUX_BAY_TRAIL_DEVICES: [(&str, Option<(u8, &str)>); 8] = [
    ("crystal_cove_pwrsrc", Some((0, "PWRSRC"))),
    ("crystal_cove_thermal", Some((1, "THERMAL"))),
    ("crystal_cove_bcu", Some((2, "BCU"))),
    ("crystal_cove_adc", Some((3, "ADC"))),
    ("crystal_cove_charger", Some((4, "CHGR"))),
    ("crystal_cove_gpio", Some((5, "GPIO"))),
    ("byt_crystal_cove_pmic", None),
    ("crystal_cove_pwm", None),
];

/// intel_soc_pmic_crc.c:97-109. Deliberately literal and independent of the production table.
const LINUX_CHERRY_TRAIL_DEVICES: [(&str, Option<(u8, &str)>); 3] = [
    ("crystal_cove_gpio", Some((5, "GPIO"))),
    ("cht_crystal_cove_pmic", None),
    ("crystal_cove_pwm", None),
];

fn flatten(device: &SubDevice) -> (&str, Option<(u8, &str)>) {
    (device.name, device.irq)
}

/// Linux's Bay Trail table defines exactly eight devices; count, names and resources are pinned.
#[test]
fn bay_trail_device_count_names_and_resources_match_linux() {
    let ours: Vec<(&str, Option<(u8, &str)>)> = BAY_TRAIL_DEVICES.iter().map(flatten).collect();
    assert_eq!(BAY_TRAIL_DEVICES.len(), 8);
    assert_eq!(ours, LINUX_BAY_TRAIL_DEVICES);
}

/// Linux's Cherry Trail table defines exactly three devices; count, names and resources are pinned.
#[test]
fn cherry_trail_device_count_names_and_resources_match_linux() {
    let ours: Vec<(&str, Option<(u8, &str)>)> = CHERRY_TRAIL_DEVICES.iter().map(flatten).collect();
    assert_eq!(CHERRY_TRAIL_DEVICES.len(), 3);
    assert_eq!(ours, LINUX_CHERRY_TRAIL_DEVICES);
}

/// intel_soc_pmic_crc.c:175-178 selects Bay Trail only when `soc_intel_is_byt()` is true.
#[test]
fn platform_selects_the_complete_linux_table() {
    let byt: Vec<(&str, Option<(u8, &str)>)> =
        devices(Platform::BayTrail).iter().map(flatten).collect();
    let cht: Vec<(&str, Option<(u8, &str)>)> =
        devices(Platform::CherryTrail).iter().map(flatten).collect();
    assert_eq!(byt, LINUX_BAY_TRAIL_DEVICES);
    assert_eq!(cht, LINUX_CHERRY_TRAIL_DEVICES);
}
