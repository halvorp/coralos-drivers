// SPDX-License-Identifier: GPL-2.0-only
//! Intel DSM and AMD capability fix-ups.
//!
//! Ported from Linux `drivers/mmc/host/sdhci-acpi.c`: `intel_setup_host`
//! (lines 335-353), the Bay Trail timeout correction (lines 317-320), and
//! `sdhci_acpi_emmc_amd_probe_slot` (lines 610-664).
//!
//! Copyright (c) 2012, Intel Corporation.
//! Copyright holders of the Linux SDHCI and MMC subsystems.

/// Intel DSM advertises SDR25 support. // sdhci-acpi.c:98
pub const INTEL_DSM_HS_CAPS_SDR25: u32 = 1 << 0;
/// Intel DSM advertises DDR50 support. // sdhci-acpi.c:99
pub const INTEL_DSM_HS_CAPS_DDR50: u32 = 1 << 1;
/// Intel DSM advertises SDR50 support. // sdhci-acpi.c:100
pub const INTEL_DSM_HS_CAPS_SDR50: u32 = 1 << 2;
/// Intel DSM advertises SDR104 support. // sdhci-acpi.c:101
pub const INTEL_DSM_HS_CAPS_SDR104: u32 = 1 << 3;

/// MMC host capability for UHS SDR25. // include/linux/mmc/host.h:411
pub const MMC_CAP_UHS_SDR25: u32 = 1 << 17;
/// MMC host capability for UHS SDR50. // include/linux/mmc/host.h:412
pub const MMC_CAP_UHS_SDR50: u32 = 1 << 18;
/// MMC host capability for UHS SDR104. // include/linux/mmc/host.h:413
pub const MMC_CAP_UHS_SDR104: u32 = 1 << 19;
/// MMC host capability for UHS DDR50. // include/linux/mmc/host.h:414
pub const MMC_CAP_UHS_DDR50: u32 = 1 << 20;

/// eMMC DDR at 1.8 V. // include/linux/mmc/host.h:404
pub const MMC_CAP_1_8V_DDR: u32 = 1 << 12;
/// eMMC HS400 at 1.8 V. // include/linux/mmc/host.h:444
pub const MMC_CAP2_HS400_1_8V: u32 = 1 << 15;
/// Preset values cannot be used. // drivers/mmc/host/sdhci.h:499
pub const SDHCI_QUIRK2_PRESET_VALUE_BROKEN: u32 = 1 << 3;
/// Capabilities-1 advertises SDR104. // drivers/mmc/host/sdhci.h:275
pub const SDHCI_SUPPORT_SDR104: u32 = 0x0000_0002;
/// Capabilities-1 advertises DDR50. // drivers/mmc/host/sdhci.h:276
pub const SDHCI_SUPPORT_DDR50: u32 = 0x0000_0004;

/// Apply the Intel `_DSM` high-speed capability mask.
///
/// Linux clears each UHS mode absent from `hs_caps`; unrelated capability bits
/// remain untouched. // sdhci-acpi.c:335-353
pub const fn apply_intel_hs_caps(mut caps: u32, hs_caps: u32) -> u32 {
    if hs_caps & INTEL_DSM_HS_CAPS_SDR25 == 0 {
        caps &= !MMC_CAP_UHS_SDR25;
    }
    if hs_caps & INTEL_DSM_HS_CAPS_SDR50 == 0 {
        caps &= !MMC_CAP_UHS_SDR50;
    }
    if hs_caps & INTEL_DSM_HS_CAPS_DDR50 == 0 {
        caps &= !MMC_CAP_UHS_DDR50;
    }
    if hs_caps & INTEL_DSM_HS_CAPS_SDR104 == 0 {
        caps &= !MMC_CAP_UHS_SDR104;
    }
    caps
}

/// Bay Trail's corrected timeout clock, in kHz. // sdhci-acpi.c:320
pub const BYT_TIMEOUT_CLOCK_KHZ: u32 = 1000;

/// Apply the exact Bay Trail eMMC capability-register correction.
///
/// Only HID `80860F14`, UID `1`, and the two literal capability words match.
/// // sdhci-acpi.c:317-320
pub fn byt_timeout_clock_khz(hid: &str, uid: Option<&str>, caps: u32, caps1: u32) -> Option<u32> {
    if hid == "80860F14" && uid == Some("1") && caps == 0x446c_c8b2 && caps1 == 0x0000_0807 {
        Some(BYT_TIMEOUT_CLOCK_KHZ)
    } else {
        None
    }
}

/// Capability words after AMD's eMMC probe fix-ups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmdCapsFixup {
    pub caps: u32,
    pub caps2: u32,
    pub quirks2: u32,
}

/// Apply AMD eMMC capability overrides to values already read by SDHCI.
///
/// Linux assigns (rather than ORs) `caps` and `caps2` when their conditions
/// match, then always adds the broken-preset quirk. // sdhci-acpi.c:616-659
pub const fn apply_amd_emmc_caps(
    caps1: u32,
    mut caps: u32,
    mut caps2: u32,
    quirks2: u32,
) -> AmdCapsFixup {
    if caps1 & SDHCI_SUPPORT_DDR50 != 0 {
        caps = MMC_CAP_1_8V_DDR;
    }
    if caps1 & SDHCI_SUPPORT_SDR104 != 0 && caps & MMC_CAP_1_8V_DDR != 0 {
        caps2 = MMC_CAP2_HS400_1_8V;
    }
    AmdCapsFixup {
        caps,
        caps2,
        quirks2: quirks2 | SDHCI_QUIRK2_PRESET_VALUE_BROKEN,
    }
}
