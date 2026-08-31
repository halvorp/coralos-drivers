// SPDX-License-Identifier: GPL-2.0-only
//! ACPI HID/UID to SDHCI slot-policy mapping.
//!
//! Ported from Linux `drivers/mmc/host/sdhci-acpi.c`: slot declarations
//! (lines 355-396, 452-464, 667-676), `sdhci_acpi_uids` (lines 684-704),
//! and `sdhci_acpi_ids` (lines 706-722).
//!
//! Copyright (c) 2012, Intel Corporation.
//! Copyright holders of the Linux SDHCI and MMC subsystems.

/// Slot uses GPIO card detection. // sdhci-acpi.c:42
pub const SDHCI_ACPI_SD_CD: u32 = 1 << 0;
/// Slot can use runtime power management. // sdhci-acpi.c:43
pub const SDHCI_ACPI_RUNTIME_PM: u32 = 1 << 1;
/// Override the card-detect GPIO level. // sdhci-acpi.c:44
pub const SDHCI_ACPI_SD_CD_OVERRIDE_LEVEL: u32 = 1 << 2;

/// Can transfer on an 8-bit bus. // include/linux/mmc/host.h:399
pub const MMC_CAP_8_BIT_DATA: u32 = 1 << 6;
/// Aggressive runtime power management. // include/linux/mmc/host.h:400
pub const MMC_CAP_AGGRESSIVE_PM: u32 = 1 << 7;
/// Non-removable media. // include/linux/mmc/host.h:401
pub const MMC_CAP_NONREMOVABLE: u32 = 1 << 8;
/// Wait while the card is busy. // include/linux/mmc/host.h:402
pub const MMC_CAP_WAIT_WHILE_BUSY: u32 = 1 << 9;
/// eMMC DDR at 1.8 V. // include/linux/mmc/host.h:404
pub const MMC_CAP_1_8V_DDR: u32 = 1 << 12;
/// Card can be powered off after boot. // include/linux/mmc/host.h:408
pub const MMC_CAP_POWER_OFF_CARD: u32 = 1 << 14;
/// Commands may be sent during transfer. // include/linux/mmc/host.h:425
pub const MMC_CAP_CMD_DURING_TFR: u32 = 1 << 29;
/// Hardware eMMC reset. // include/linux/mmc/host.h:427
pub const MMC_CAP_HW_RESET: u32 = 1 << 31;
/// Preserve card power over suspend. // include/linux/mmc/pm.h:24
pub const MMC_PM_KEEP_POWER: u32 = 1 << 0;

/// DMA addresses must be 32-bit aligned. // drivers/mmc/host/sdhci.h:447
pub const SDHCI_QUIRK_32BIT_DMA_ADDR: u32 = 1 << 7;
/// DMA sizes must be 32-bit aligned. // drivers/mmc/host/sdhci.h:449
pub const SDHCI_QUIRK_32BIT_DMA_SIZE: u32 = 1 << 8;
/// ADMA sizes must be 32-bit aligned. // drivers/mmc/host/sdhci.h:451
pub const SDHCI_QUIRK_32BIT_ADMA_SIZE: u32 = 1 << 9;
/// Card detection is unreliable. // drivers/mmc/host/sdhci.h:463
pub const SDHCI_QUIRK_BROKEN_CARD_DETECTION: u32 = 1 << 15;
/// Controller has no LED. // drivers/mmc/host/sdhci.h:471
pub const SDHCI_QUIRK_NO_LED: u32 = 1 << 19;
/// No End Attribute in a NOP ADMA descriptor. // drivers/mmc/host/sdhci.h:485
pub const SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC: u32 = 1 << 26;

/// Host can be off while card remains on. // drivers/mmc/host/sdhci.h:495
pub const SDHCI_QUIRK2_HOST_OFF_CARD_ON: u32 = 1 << 0;
/// Platform cannot use 1.8 V. // drivers/mmc/host/sdhci.h:498
pub const SDHCI_QUIRK2_NO_1_8_V: u32 = 1 << 2;
/// Preset values are broken. // drivers/mmc/host/sdhci.h:499
pub const SDHCI_QUIRK2_PRESET_VALUE_BROKEN: u32 = 1 << 3;
/// Card-on requires bus-on. // drivers/mmc/host/sdhci.h:500
pub const SDHCI_QUIRK2_CARD_ON_NEEDS_BUS_ON: u32 = 1 << 4;
/// Stop command may report transfer complete. // drivers/mmc/host/sdhci.h:508
pub const SDHCI_QUIRK2_STOP_WITH_TC: u32 = 1 << 8;
/// Controller does not support 64-bit DMA. // drivers/mmc/host/sdhci.h:510
pub const SDHCI_QUIRK2_BROKEN_64_BIT_DMA: u32 = 1 << 9;
/// Capabilities bit 63 advertises HS400. // drivers/mmc/host/sdhci.h:514
pub const SDHCI_QUIRK2_CAPS_BIT63_FOR_HS400: u32 = 1 << 11;

/// The six non-default slot policies Linux names in `sdhci-acpi.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    IntelEmmc,
    IntelSdio,
    IntelSd,
    QcomSd3v,
    QcomSd,
    AmdEmmc,
}

/// Pure policy fields copied from `struct sdhci_acpi_slot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotPolicy {
    pub quirks: u32,
    pub quirks2: u32,
    pub caps: u32,
    pub caps2: u32,
    pub pm_caps: u32,
    pub flags: u32,
}

/// Policy for a named Linux slot declaration. // sdhci-acpi.c:355-396,452-464,667-676
pub const fn slot_policy(kind: SlotKind) -> SlotPolicy {
    match kind {
        SlotKind::IntelEmmc => SlotPolicy {
            quirks: SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC | SDHCI_QUIRK_NO_LED,
            quirks2: SDHCI_QUIRK2_PRESET_VALUE_BROKEN
                | SDHCI_QUIRK2_STOP_WITH_TC
                | SDHCI_QUIRK2_CAPS_BIT63_FOR_HS400,
            caps: MMC_CAP_8_BIT_DATA
                | MMC_CAP_NONREMOVABLE
                | MMC_CAP_HW_RESET
                | MMC_CAP_1_8V_DDR
                | MMC_CAP_CMD_DURING_TFR
                | MMC_CAP_WAIT_WHILE_BUSY,
            caps2: 0,
            pm_caps: 0,
            flags: SDHCI_ACPI_RUNTIME_PM,
        },
        SlotKind::IntelSdio => SlotPolicy {
            quirks: SDHCI_QUIRK_BROKEN_CARD_DETECTION
                | SDHCI_QUIRK_NO_LED
                | SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC,
            quirks2: SDHCI_QUIRK2_HOST_OFF_CARD_ON,
            caps: MMC_CAP_NONREMOVABLE | MMC_CAP_POWER_OFF_CARD | MMC_CAP_WAIT_WHILE_BUSY,
            caps2: 0,
            pm_caps: MMC_PM_KEEP_POWER,
            flags: SDHCI_ACPI_RUNTIME_PM,
        },
        SlotKind::IntelSd => SlotPolicy {
            quirks: SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC | SDHCI_QUIRK_NO_LED,
            quirks2: SDHCI_QUIRK2_CARD_ON_NEEDS_BUS_ON | SDHCI_QUIRK2_STOP_WITH_TC,
            caps: MMC_CAP_WAIT_WHILE_BUSY | MMC_CAP_AGGRESSIVE_PM,
            caps2: 0,
            pm_caps: 0,
            flags: SDHCI_ACPI_SD_CD | SDHCI_ACPI_SD_CD_OVERRIDE_LEVEL | SDHCI_ACPI_RUNTIME_PM,
        },
        SlotKind::QcomSd3v => SlotPolicy {
            quirks: SDHCI_QUIRK_BROKEN_CARD_DETECTION,
            quirks2: SDHCI_QUIRK2_NO_1_8_V,
            caps: MMC_CAP_NONREMOVABLE,
            caps2: 0,
            pm_caps: 0,
            flags: 0,
        },
        SlotKind::QcomSd => SlotPolicy {
            quirks: SDHCI_QUIRK_BROKEN_CARD_DETECTION,
            quirks2: 0,
            caps: MMC_CAP_NONREMOVABLE,
            caps2: 0,
            pm_caps: 0,
            flags: 0,
        },
        SlotKind::AmdEmmc => SlotPolicy {
            quirks: SDHCI_QUIRK_32BIT_DMA_ADDR
                | SDHCI_QUIRK_32BIT_DMA_SIZE
                | SDHCI_QUIRK_32BIT_ADMA_SIZE,
            quirks2: SDHCI_QUIRK2_BROKEN_64_BIT_DMA,
            caps: MMC_CAP_8_BIT_DATA | MMC_CAP_NONREMOVABLE,
            caps2: 0,
            pm_caps: 0,
            flags: 0,
        },
    }
}

/// One literal entry of Linux's HID/UID slot table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HidUidSlot {
    pub hid: &'static str,
    pub uid: Option<&'static str>,
    pub slot: Option<SlotKind>,
}

/// The 18 non-terminating entries in `sdhci_acpi_uids`. // sdhci-acpi.c:684-703
pub const HID_UID_SLOTS: [HidUidSlot; 18] = [
    HidUidSlot {
        hid: "80865ACA",
        uid: None,
        slot: Some(SlotKind::IntelSd),
    },
    HidUidSlot {
        hid: "80865ACC",
        uid: None,
        slot: Some(SlotKind::IntelEmmc),
    },
    HidUidSlot {
        hid: "80865AD0",
        uid: None,
        slot: Some(SlotKind::IntelSdio),
    },
    HidUidSlot {
        hid: "80860F14",
        uid: Some("1"),
        slot: Some(SlotKind::IntelEmmc),
    },
    HidUidSlot {
        hid: "80860F14",
        uid: Some("2"),
        slot: Some(SlotKind::IntelSdio),
    },
    HidUidSlot {
        hid: "80860F14",
        uid: Some("3"),
        slot: Some(SlotKind::IntelSd),
    },
    HidUidSlot {
        hid: "80860F16",
        uid: None,
        slot: Some(SlotKind::IntelSd),
    },
    HidUidSlot {
        hid: "INT33BB",
        uid: Some("2"),
        slot: Some(SlotKind::IntelSdio),
    },
    HidUidSlot {
        hid: "INT33BB",
        uid: Some("3"),
        slot: Some(SlotKind::IntelSd),
    },
    HidUidSlot {
        hid: "INT33C6",
        uid: None,
        slot: Some(SlotKind::IntelSdio),
    },
    HidUidSlot {
        hid: "INT3436",
        uid: None,
        slot: Some(SlotKind::IntelSdio),
    },
    HidUidSlot {
        hid: "INT344D",
        uid: None,
        slot: Some(SlotKind::IntelSdio),
    },
    HidUidSlot {
        hid: "PNP0FFF",
        uid: Some("3"),
        slot: Some(SlotKind::IntelSd),
    },
    HidUidSlot {
        hid: "PNP0D40",
        uid: None,
        slot: None,
    },
    HidUidSlot {
        hid: "QCOM8051",
        uid: None,
        slot: Some(SlotKind::QcomSd3v),
    },
    HidUidSlot {
        hid: "QCOM8052",
        uid: None,
        slot: Some(SlotKind::QcomSd),
    },
    HidUidSlot {
        hid: "AMDI0040",
        uid: None,
        slot: Some(SlotKind::AmdEmmc),
    },
    HidUidSlot {
        hid: "AMDI0041",
        uid: None,
        slot: Some(SlotKind::AmdEmmc),
    },
];

/// The 14 non-terminating ACPI match-table HIDs. // sdhci-acpi.c:706-721
pub const ACPI_IDS: [&str; 14] = [
    "80865ACA", "80865ACC", "80865AD0", "80860F14", "80860F16", "INT33BB", "INT33C6", "INT3436",
    "INT344D", "PNP0D40", "QCOM8051", "QCOM8052", "AMDI0040", "AMDI0041",
];

/// Named result of the HID/UID table walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotLookup {
    /// Linux selected one of its named slot policies.
    Policy(SlotKind),
    /// Linux listed the HID but attached no slot overrides (`PNP0D40`).
    DefaultPolicy,
    /// No HID/UID row matched, so no per-slot policy can be applied.
    NoMatchingHidUid,
}

/// Find the first slot matching Linux's `acpi_dev_hid_uid_match` table walk.
///
/// A table UID of `None` is a HID-only wildcard; a UID string requires an exact
/// match. The result names both kinds of refusal instead of collapsing a known
/// default-policy HID and an unknown HID into a bare `None`.
/// // sdhci-acpi.c:806-815
pub fn lookup_slot(hid: &str, uid: Option<&str>) -> SlotLookup {
    match HID_UID_SLOTS
        .iter()
        .find(|entry| entry.hid == hid && entry.uid.map_or(true, |expected| uid == Some(expected)))
    {
        Some(entry) => match entry.slot {
            Some(kind) => SlotLookup::Policy(kind),
            None => SlotLookup::DefaultPolicy,
        },
        None => SlotLookup::NoMatchingHidUid,
    }
}

/// Whether Linux binds the SDHCI ACPI driver to this HID. // sdhci-acpi.c:706-722
pub fn is_supported_hid(hid: &str) -> bool {
    ACPI_IDS.contains(&hid)
}
