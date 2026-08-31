// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for capability fix-ups.
//!
//! Ported from Linux `drivers/mmc/host/sdhci-acpi.c` by Intel Corporation and
//! the Linux SDHCI/MMC copyright holders.

use sdhci_acpi_core::caps::{
    self, apply_amd_emmc_caps, apply_intel_hs_caps, byt_timeout_clock_khz, AmdCapsFixup,
};

/// Every public mask is pinned independently to its Linux value and bit position.
#[test]
fn public_capability_constants_match_linux_literals() {
    assert_eq!(caps::INTEL_DSM_HS_CAPS_SDR25, 0x0000_0001); // sdhci-acpi.c:98
    assert_eq!(1_u32 << 0, 0x0000_0001); // sdhci-acpi.c:98
    assert_eq!(caps::INTEL_DSM_HS_CAPS_DDR50, 0x0000_0002); // sdhci-acpi.c:99
    assert_eq!(1_u32 << 1, 0x0000_0002); // sdhci-acpi.c:99
    assert_eq!(caps::INTEL_DSM_HS_CAPS_SDR50, 0x0000_0004); // sdhci-acpi.c:100
    assert_eq!(1_u32 << 2, 0x0000_0004); // sdhci-acpi.c:100
    assert_eq!(caps::INTEL_DSM_HS_CAPS_SDR104, 0x0000_0008); // sdhci-acpi.c:101
    assert_eq!(1_u32 << 3, 0x0000_0008); // sdhci-acpi.c:101

    assert_eq!(caps::MMC_CAP_UHS_SDR25, 0x0002_0000); // include/linux/mmc/host.h:411
    assert_eq!(1_u32 << 17, 0x0002_0000); // include/linux/mmc/host.h:411
    assert_eq!(caps::MMC_CAP_UHS_SDR50, 0x0004_0000); // include/linux/mmc/host.h:412
    assert_eq!(1_u32 << 18, 0x0004_0000); // include/linux/mmc/host.h:412
    assert_eq!(caps::MMC_CAP_UHS_SDR104, 0x0008_0000); // include/linux/mmc/host.h:413
    assert_eq!(1_u32 << 19, 0x0008_0000); // include/linux/mmc/host.h:413
    assert_eq!(caps::MMC_CAP_UHS_DDR50, 0x0010_0000); // include/linux/mmc/host.h:414
    assert_eq!(1_u32 << 20, 0x0010_0000); // include/linux/mmc/host.h:414

    assert_eq!(caps::MMC_CAP_1_8V_DDR, 0x0000_1000); // include/linux/mmc/host.h:404
    assert_eq!(1_u32 << 12, 0x0000_1000); // include/linux/mmc/host.h:404
    assert_eq!(caps::MMC_CAP2_HS400_1_8V, 0x0000_8000); // include/linux/mmc/host.h:444
    assert_eq!(1_u32 << 15, 0x0000_8000); // include/linux/mmc/host.h:444
    assert_eq!(caps::SDHCI_QUIRK2_PRESET_VALUE_BROKEN, 0x0000_0008); // drivers/mmc/host/sdhci.h:499
    assert_eq!(1_u32 << 3, 0x0000_0008); // drivers/mmc/host/sdhci.h:499
    assert_eq!(caps::SDHCI_SUPPORT_SDR104, 0x0000_0002); // drivers/mmc/host/sdhci.h:275
    assert_eq!(1_u32 << 1, 0x0000_0002); // drivers/mmc/host/sdhci.h:275
    assert_eq!(caps::SDHCI_SUPPORT_DDR50, 0x0000_0004); // drivers/mmc/host/sdhci.h:276
    assert_eq!(1_u32 << 2, 0x0000_0004); // drivers/mmc/host/sdhci.h:276

    assert_eq!(caps::BYT_TIMEOUT_CLOCK_KHZ, 1000); // sdhci-acpi.c:320
}

/// sdhci-acpi.c:340-350 — DSM bits 0,2,1,3 retain MMC bits 17,18,20,19 respectively.
#[test]
fn intel_dsm_keeps_exactly_the_advertised_uhs_modes() {
    let all_uhs = 0x001e_0000; // include/linux/mmc/host.h:411-414
    assert_eq!(apply_intel_hs_caps(all_uhs, 0x0000_000f), 0x001e_0000);
    assert_eq!(apply_intel_hs_caps(all_uhs, 0x0000_0001), 0x0002_0000);
    assert_eq!(apply_intel_hs_caps(all_uhs, 0x0000_0002), 0x0010_0000);
    assert_eq!(apply_intel_hs_caps(all_uhs, 0x0000_0004), 0x0004_0000);
    assert_eq!(apply_intel_hs_caps(all_uhs, 0x0000_0008), 0x0008_0000);
    assert_eq!(apply_intel_hs_caps(all_uhs, 0x0000_0000), 0x0000_0000);
}

/// sdhci-acpi.c:340-350 uses `&= ~bit`, so unrelated MMC caps cannot be erased.
#[test]
fn intel_dsm_fixup_preserves_unrelated_capabilities() {
    assert_eq!(
        apply_intel_hs_caps(0x8000_0040 | 0x001e_0000, 0),
        0x8000_0040
    );
}

/// sdhci-acpi.c:317-320 — all four predicates and Linux's literal 1000 kHz result are pinned.
#[test]
fn bay_trail_timeout_correction_requires_the_exact_signature() {
    assert_eq!(
        byt_timeout_clock_khz("80860F14", Some("1"), 0x446c_c8b2, 0x0000_0807),
        Some(1000)
    );
    assert_eq!(
        byt_timeout_clock_khz("80860F14", Some("2"), 0x446c_c8b2, 0x0000_0807),
        None
    );
    assert_eq!(
        byt_timeout_clock_khz("80860F14", Some("1"), 0x446c_c8b3, 0x0000_0807),
        None
    );
    assert_eq!(
        byt_timeout_clock_khz("80860F14", Some("1"), 0x446c_c8b2, 0x0000_0806),
        None
    );
    assert_eq!(
        byt_timeout_clock_khz("80865ACC", Some("1"), 0x446c_c8b2, 0x0000_0807),
        None
    );
}

/// sdhci-acpi.c:616-622,659 — DDR50 assigns caps bit 12; SDR104 then assigns caps2 bit 15;
/// every path ORs quirks2 bit 3.
#[test]
fn amd_ddr50_and_sdr104_enable_the_literal_emmc_caps() {
    assert_eq!(
        apply_amd_emmc_caps(0x0000_0006, 0xdead_beef, 0xcafe_babe, 0x0000_0200),
        AmdCapsFixup {
            caps: 0x0000_1000,
            caps2: 0x0000_8000,
            quirks2: 0x0000_0208
        }
    );
}

/// sdhci-acpi.c:617-622 uses assignments, not ORs. DDR50 without SDR104 replaces only caps;
/// SDR104 without a DDR capability leaves both incoming words unchanged.
#[test]
fn amd_fixup_conditions_and_assignment_semantics_match_linux() {
    assert_eq!(
        apply_amd_emmc_caps(0x0000_0004, 0xaaaa_aaaa, 0xbbbb_bbbb, 0),
        AmdCapsFixup {
            caps: 0x0000_1000,
            caps2: 0xbbbb_bbbb,
            quirks2: 0x0000_0008
        }
    );
    assert_eq!(
        apply_amd_emmc_caps(0x0000_0002, 0x0000_0000, 0x1234_5678, 0),
        AmdCapsFixup {
            caps: 0x0000_0000,
            caps2: 0x1234_5678,
            quirks2: 0x0000_0008
        }
    );
    // Existing 1.8 V DDR is sufficient for the second condition even without DDR50 in caps1.
    assert_eq!(
        apply_amd_emmc_caps(0x0000_0002, 0x0000_1000, 0x1234_5678, 0),
        AmdCapsFixup {
            caps: 0x0000_1000,
            caps2: 0x0000_8000,
            quirks2: 0x0000_0008
        }
    );
}
