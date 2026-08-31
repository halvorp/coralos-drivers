// SPDX-License-Identifier: GPL-2.0-only
//! Register offsets, bits, masks, counts, and interrupt names ported from Linux
//! `drivers/thermal/intel/intel_soc_dts_iosf.c` and `intel_soc_dts_iosf.h`.
//!
//! Copyright (c) 2015, Intel Corporation.

/// IOSF mailbox register offsets from `intel_soc_dts_iosf.c`.
pub mod offset {
    pub const ENABLE: u32 = 0xB0; // intel_soc_dts_iosf.c:17
    pub const TEMP: u32 = 0xB1; // intel_soc_dts_iosf.c:18
    pub const PTPS: u32 = 0xB2; // intel_soc_dts_iosf.c:20
    pub const PTTS: u32 = 0xB3; // intel_soc_dts_iosf.c:21
    pub const PTTSS: u32 = 0xB4; // intel_soc_dts_iosf.c:22
    pub const PTMC: u32 = 0x80; // intel_soc_dts_iosf.c:23
    pub const TE_AUX0: u32 = 0xB5; // intel_soc_dts_iosf.c:24
    pub const TE_AUX1: u32 = 0xB6; // intel_soc_dts_iosf.c:25
}

/// Register bits and masks from `intel_soc_dts_iosf.c`.
pub mod bit {
    pub const AUX0_ENABLE: u32 = 1 << 0; // intel_soc_dts_iosf.c:27 BIT(0)
    pub const AUX1_ENABLE: u32 = 1 << 1; // intel_soc_dts_iosf.c:28 BIT(1)
    pub const CPU_MODULE0_ENABLE: u32 = 1 << 16; // intel_soc_dts_iosf.c:29 BIT(16)
    pub const CPU_MODULE1_ENABLE: u32 = 1 << 17; // intel_soc_dts_iosf.c:30 BIT(17)
    pub const TE_SCI_ENABLE: u32 = 1 << 9; // intel_soc_dts_iosf.c:31 BIT(9)
    pub const TE_SMI_ENABLE: u32 = 1 << 10; // intel_soc_dts_iosf.c:32 BIT(10)
    pub const TE_MSI_ENABLE: u32 = 1 << 11; // intel_soc_dts_iosf.c:33 BIT(11)
    pub const TE_APICA_ENABLE: u32 = 1 << 14; // intel_soc_dts_iosf.c:34 BIT(14)
    pub const PTMC_APIC_DEASSERT: u32 = 1 << 4; // intel_soc_dts_iosf.c:35 BIT(4)
}

/// DTS encoding of TjMax. // intel_soc_dts_iosf.c:37-38
pub const TJMAX_ENCODING: u8 = 0x7F; // intel_soc_dts_iosf.c:38
/// Mask for the two OSPM trip sticky-status bits. // intel_soc_dts_iosf.c:40-41
pub const TRIP_MASK: u32 = 0x03; // intel_soc_dts_iosf.c:41
/// Number of DTS sensors. // intel_soc_dts_iosf.h:12-13
pub const SENSOR_COUNT: usize = 2; // intel_soc_dts_iosf.h:13
/// Names of both sensors from Linux's `DTS0 and DTS 1` comment. // intel_soc_dts_iosf.h:12
pub const SENSOR_NAMES: [&str; SENSOR_COUNT] = ["DTS0", "DTS1"]; // intel_soc_dts_iosf.h:12
/// Number of the four hardware trips made available to OSPM. // intel_soc_dts_iosf.h:15-16
pub const TRIP_COUNT: usize = 2; // intel_soc_dts_iosf.h:16
/// Stable names for the two indexed OSPM trips reset by Linux. // intel_soc_dts_iosf.c:293-294
pub const TRIP_NAMES: [&str; TRIP_COUNT] = ["trip0", "trip1"]; // intel_soc_dts_iosf.c:293-294

/// Linux interrupt-selection value and its exact C enumerator name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptType {
    pub value: u8,
    pub name: &'static str,
}

/// All `intel_soc_dts_interrupt_type` values, in Linux declaration order.
pub const INTERRUPT_TYPES: [InterruptType; 5] = [
    InterruptType {
        value: 0,
        name: "INTEL_SOC_DTS_INTERRUPT_NONE",
    }, // intel_soc_dts_iosf.h:19
    InterruptType {
        value: 1,
        name: "INTEL_SOC_DTS_INTERRUPT_APIC",
    }, // intel_soc_dts_iosf.h:20
    InterruptType {
        value: 2,
        name: "INTEL_SOC_DTS_INTERRUPT_MSI",
    }, // intel_soc_dts_iosf.h:21
    InterruptType {
        value: 3,
        name: "INTEL_SOC_DTS_INTERRUPT_SCI",
    }, // intel_soc_dts_iosf.h:22
    InterruptType {
        value: 4,
        name: "INTEL_SOC_DTS_INTERRUPT_SMI",
    }, // intel_soc_dts_iosf.h:23
];
