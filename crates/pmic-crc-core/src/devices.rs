// SPDX-License-Identifier: GPL-2.0-only
//! Crystal Cove Bay Trail and Cherry Trail MFD sub-device tables.
//!
//! Mechanically ported from Linux `drivers/mfd/intel_soc_pmic_crc.c`.
//!
//! Copyright (C) 2012-2014, 2022 Intel Corporation. All rights reserved.
//! Original authors: Yang, Bin <bin.yang@intel.com> and Zhu, Lejun <lejun.zhu@linux.intel.com>.

/// Platform variant selected by Linux's `soc_intel_is_byt()` branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    BayTrail,
    CherryTrail,
}

/// One MFD child and its optional named top-level IRQ resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubDevice {
    /// Linux platform-device name.
    pub name: &'static str,
    /// `(IRQ-domain hardware number, resource name)` when Linux attaches a resource.
    pub irq: Option<(u8, &'static str)>,
}

/// Bay Trail's complete `crystal_cove_byt_dev[]` table.
pub const BAY_TRAIL_DEVICES: [SubDevice; 8] = [
    SubDevice {
        name: "crystal_cove_pwrsrc",
        irq: Some((0, "PWRSRC")),
    }, // intel_soc_pmic_crc.c:34-36,58-63
    SubDevice {
        name: "crystal_cove_thermal",
        irq: Some((1, "THERMAL")),
    }, // intel_soc_pmic_crc.c:38-40,64-68
    SubDevice {
        name: "crystal_cove_bcu",
        irq: Some((2, "BCU")),
    }, // intel_soc_pmic_crc.c:42-44,69-73
    SubDevice {
        name: "crystal_cove_adc",
        irq: Some((3, "ADC")),
    }, // intel_soc_pmic_crc.c:46-48,74-78
    SubDevice {
        name: "crystal_cove_charger",
        irq: Some((4, "CHGR")),
    }, // intel_soc_pmic_crc.c:50-52,79-83
    SubDevice {
        name: "crystal_cove_gpio",
        irq: Some((5, "GPIO")),
    }, // intel_soc_pmic_crc.c:54-56,84-88
    SubDevice {
        name: "byt_crystal_cove_pmic",
        irq: None,
    }, // intel_soc_pmic_crc.c:89-91
    SubDevice {
        name: "crystal_cove_pwm",
        irq: None,
    }, // intel_soc_pmic_crc.c:92-94
];

/// Cherry Trail's complete `crystal_cove_cht_dev[]` table.
pub const CHERRY_TRAIL_DEVICES: [SubDevice; 3] = [
    SubDevice {
        name: "crystal_cove_gpio",
        irq: Some((5, "GPIO")),
    }, // intel_soc_pmic_crc.c:54-56,97-102
    SubDevice {
        name: "cht_crystal_cove_pmic",
        irq: None,
    }, // intel_soc_pmic_crc.c:103-105
    SubDevice {
        name: "crystal_cove_pwm",
        irq: None,
    }, // intel_soc_pmic_crc.c:106-108
];

/// Select the complete Linux MFD child table for a platform.
pub fn devices(platform: Platform) -> &'static [SubDevice] {
    match platform {
        Platform::BayTrail => &BAY_TRAIL_DEVICES, // intel_soc_pmic_crc.c:175-178
        Platform::CherryTrail => &CHERRY_TRAIL_DEVICES, // intel_soc_pmic_crc.c:175-178
    }
}
