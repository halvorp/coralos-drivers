// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for the HID/UID map and each slot's policy words.
//!
//! Ported from Linux `drivers/mmc/host/sdhci-acpi.c` by Intel Corporation and
//! the Linux SDHCI/MMC copyright holders.

use sdhci_acpi_core::slots::{
    self, is_supported_hid, lookup_slot, slot_policy, HidUidSlot, SlotKind, SlotLookup, SlotPolicy,
    ACPI_IDS, HID_UID_SLOTS,
};

/// Every public mask is pinned independently to its Linux value and bit position.
#[test]
fn public_slot_constants_match_linux_literals() {
    assert_eq!(slots::SDHCI_ACPI_SD_CD, 0x0000_0001); // sdhci-acpi.c:42
    assert_eq!(1_u32 << 0, 0x0000_0001); // sdhci-acpi.c:42
    assert_eq!(slots::SDHCI_ACPI_RUNTIME_PM, 0x0000_0002); // sdhci-acpi.c:43
    assert_eq!(1_u32 << 1, 0x0000_0002); // sdhci-acpi.c:43
    assert_eq!(slots::SDHCI_ACPI_SD_CD_OVERRIDE_LEVEL, 0x0000_0004); // sdhci-acpi.c:44
    assert_eq!(1_u32 << 2, 0x0000_0004); // sdhci-acpi.c:44

    assert_eq!(slots::MMC_CAP_8_BIT_DATA, 0x0000_0040); // include/linux/mmc/host.h:399
    assert_eq!(1_u32 << 6, 0x0000_0040); // include/linux/mmc/host.h:399
    assert_eq!(slots::MMC_CAP_AGGRESSIVE_PM, 0x0000_0080); // include/linux/mmc/host.h:400
    assert_eq!(1_u32 << 7, 0x0000_0080); // include/linux/mmc/host.h:400
    assert_eq!(slots::MMC_CAP_NONREMOVABLE, 0x0000_0100); // include/linux/mmc/host.h:401
    assert_eq!(1_u32 << 8, 0x0000_0100); // include/linux/mmc/host.h:401
    assert_eq!(slots::MMC_CAP_WAIT_WHILE_BUSY, 0x0000_0200); // include/linux/mmc/host.h:402
    assert_eq!(1_u32 << 9, 0x0000_0200); // include/linux/mmc/host.h:402
    assert_eq!(slots::MMC_CAP_1_8V_DDR, 0x0000_1000); // include/linux/mmc/host.h:404
    assert_eq!(1_u32 << 12, 0x0000_1000); // include/linux/mmc/host.h:404
    assert_eq!(slots::MMC_CAP_POWER_OFF_CARD, 0x0000_4000); // include/linux/mmc/host.h:408
    assert_eq!(1_u32 << 14, 0x0000_4000); // include/linux/mmc/host.h:408
    assert_eq!(slots::MMC_CAP_CMD_DURING_TFR, 0x2000_0000); // include/linux/mmc/host.h:425
    assert_eq!(1_u32 << 29, 0x2000_0000); // include/linux/mmc/host.h:425
    assert_eq!(slots::MMC_CAP_HW_RESET, 0x8000_0000); // include/linux/mmc/host.h:427
    assert_eq!(1_u32 << 31, 0x8000_0000); // include/linux/mmc/host.h:427
    assert_eq!(slots::MMC_PM_KEEP_POWER, 0x0000_0001); // include/linux/mmc/pm.h:24
    assert_eq!(1_u32 << 0, 0x0000_0001); // include/linux/mmc/pm.h:24

    assert_eq!(slots::SDHCI_QUIRK_32BIT_DMA_ADDR, 0x0000_0080); // drivers/mmc/host/sdhci.h:447
    assert_eq!(1_u32 << 7, 0x0000_0080); // drivers/mmc/host/sdhci.h:447
    assert_eq!(slots::SDHCI_QUIRK_32BIT_DMA_SIZE, 0x0000_0100); // drivers/mmc/host/sdhci.h:449
    assert_eq!(1_u32 << 8, 0x0000_0100); // drivers/mmc/host/sdhci.h:449
    assert_eq!(slots::SDHCI_QUIRK_32BIT_ADMA_SIZE, 0x0000_0200); // drivers/mmc/host/sdhci.h:451
    assert_eq!(1_u32 << 9, 0x0000_0200); // drivers/mmc/host/sdhci.h:451
    assert_eq!(slots::SDHCI_QUIRK_BROKEN_CARD_DETECTION, 0x0000_8000); // drivers/mmc/host/sdhci.h:463
    assert_eq!(1_u32 << 15, 0x0000_8000); // drivers/mmc/host/sdhci.h:463
    assert_eq!(slots::SDHCI_QUIRK_NO_LED, 0x0008_0000); // drivers/mmc/host/sdhci.h:471
    assert_eq!(1_u32 << 19, 0x0008_0000); // drivers/mmc/host/sdhci.h:471
    assert_eq!(slots::SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC, 0x0400_0000); // drivers/mmc/host/sdhci.h:485
    assert_eq!(1_u32 << 26, 0x0400_0000); // drivers/mmc/host/sdhci.h:485

    assert_eq!(slots::SDHCI_QUIRK2_HOST_OFF_CARD_ON, 0x0000_0001); // drivers/mmc/host/sdhci.h:495
    assert_eq!(1_u32 << 0, 0x0000_0001); // drivers/mmc/host/sdhci.h:495
    assert_eq!(slots::SDHCI_QUIRK2_NO_1_8_V, 0x0000_0004); // drivers/mmc/host/sdhci.h:498
    assert_eq!(1_u32 << 2, 0x0000_0004); // drivers/mmc/host/sdhci.h:498
    assert_eq!(slots::SDHCI_QUIRK2_PRESET_VALUE_BROKEN, 0x0000_0008); // drivers/mmc/host/sdhci.h:499
    assert_eq!(1_u32 << 3, 0x0000_0008); // drivers/mmc/host/sdhci.h:499
    assert_eq!(slots::SDHCI_QUIRK2_CARD_ON_NEEDS_BUS_ON, 0x0000_0010); // drivers/mmc/host/sdhci.h:500
    assert_eq!(1_u32 << 4, 0x0000_0010); // drivers/mmc/host/sdhci.h:500
    assert_eq!(slots::SDHCI_QUIRK2_STOP_WITH_TC, 0x0000_0100); // drivers/mmc/host/sdhci.h:508
    assert_eq!(1_u32 << 8, 0x0000_0100); // drivers/mmc/host/sdhci.h:508
    assert_eq!(slots::SDHCI_QUIRK2_BROKEN_64_BIT_DMA, 0x0000_0200); // drivers/mmc/host/sdhci.h:510
    assert_eq!(1_u32 << 9, 0x0000_0200); // drivers/mmc/host/sdhci.h:510
    assert_eq!(slots::SDHCI_QUIRK2_CAPS_BIT63_FOR_HS400, 0x0000_0800); // drivers/mmc/host/sdhci.h:514
    assert_eq!(1_u32 << 11, 0x0000_0800); // drivers/mmc/host/sdhci.h:514
}

/// sdhci-acpi.c:684-703 — 18 non-terminating rows, written out independently of production data.
#[test]
fn hid_uid_table_count_names_and_slots_match_linux() {
    let expected = [
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
    assert_eq!(HID_UID_SLOTS.len(), 18);
    assert_eq!(HID_UID_SLOTS, expected);
}

/// sdhci-acpi.c:706-721 — 14 driver bind IDs, in Linux's literal order.
#[test]
fn acpi_id_count_and_names_match_linux() {
    let expected = [
        "80865ACA", "80865ACC", "80865AD0", "80860F14", "80860F16", "INT33BB", "INT33C6",
        "INT3436", "INT344D", "PNP0D40", "QCOM8051", "QCOM8052", "AMDI0040", "AMDI0041",
    ];
    assert_eq!(ACPI_IDS.len(), 14);
    assert_eq!(ACPI_IDS, expected);
}

/// sdhci-acpi.c:355-396,452-464,667-676. Values are literal expansions of Linux's bit constants.
#[test]
fn all_six_named_slot_policies_match_linux_literals() {
    let expected = [
        (
            SlotKind::IntelEmmc,
            SlotPolicy {
                quirks: 0x0408_0000,
                quirks2: 0x0000_0908,
                caps: 0xa000_1340,
                caps2: 0,
                pm_caps: 0,
                flags: 0x0000_0002,
            },
        ),
        (
            SlotKind::IntelSdio,
            SlotPolicy {
                quirks: 0x0408_8000,
                quirks2: 0x0000_0001,
                caps: 0x0000_4300,
                caps2: 0,
                pm_caps: 0x0000_0001,
                flags: 0x0000_0002,
            },
        ),
        (
            SlotKind::IntelSd,
            SlotPolicy {
                quirks: 0x0408_0000,
                quirks2: 0x0000_0110,
                caps: 0x0000_0280,
                caps2: 0,
                pm_caps: 0,
                flags: 0x0000_0007,
            },
        ),
        (
            SlotKind::QcomSd3v,
            SlotPolicy {
                quirks: 0x0000_8000,
                quirks2: 0x0000_0004,
                caps: 0x0000_0100,
                caps2: 0,
                pm_caps: 0,
                flags: 0,
            },
        ),
        (
            SlotKind::QcomSd,
            SlotPolicy {
                quirks: 0x0000_8000,
                quirks2: 0,
                caps: 0x0000_0100,
                caps2: 0,
                pm_caps: 0,
                flags: 0,
            },
        ),
        (
            SlotKind::AmdEmmc,
            SlotPolicy {
                quirks: 0x0000_0380,
                quirks2: 0x0000_0200,
                caps: 0x0000_0140,
                caps2: 0,
                pm_caps: 0,
                flags: 0,
            },
        ),
    ];
    assert_eq!(expected.len(), 6);
    for (kind, policy) in expected {
        assert_eq!(slot_policy(kind), policy, "wrong policy for {kind:?}");
    }
}

/// sdhci-acpi.c:685-690,698,806-815 — UID rows are exact while NULL UIDs are HID wildcards.
#[test]
fn lookup_obeys_linux_uid_matching_and_names_refusals() {
    assert_eq!(
        lookup_slot("80860F14", Some("1")),
        SlotLookup::Policy(SlotKind::IntelEmmc)
    );
    assert_eq!(
        lookup_slot("80860F14", Some("2")),
        SlotLookup::Policy(SlotKind::IntelSdio)
    );
    assert_eq!(
        lookup_slot("80860F14", Some("3")),
        SlotLookup::Policy(SlotKind::IntelSd)
    );
    assert_eq!(
        lookup_slot("80860F14", Some("4")),
        SlotLookup::NoMatchingHidUid
    );
    assert_eq!(
        lookup_slot("80865ACC", Some("firmware-uid")),
        SlotLookup::Policy(SlotKind::IntelEmmc)
    );
    assert_eq!(lookup_slot("PNP0D40", None), SlotLookup::DefaultPolicy);
    assert_eq!(lookup_slot("UNKNOWN", None), SlotLookup::NoMatchingHidUid);
}

/// sdhci-acpi.c:706-722 — binding IDs are a separate list from slot overrides.
#[test]
fn supported_hid_uses_the_literal_binding_table() {
    assert!(is_supported_hid("80865ACC"));
    assert!(
        is_supported_hid("PNP0D40"),
        "known default-policy HID still binds"
    );
    assert!(
        !is_supported_hid("PNP0FFF"),
        "slot-table row is absent from Linux's bind table"
    );
    assert!(!is_supported_hid("UNKNOWN"));
}
