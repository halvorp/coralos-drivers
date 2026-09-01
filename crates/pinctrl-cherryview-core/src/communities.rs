// SPDX-License-Identifier: GPL-2.0-only
//! Cherryview community/bank corpus, ported from Linux
//! `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//!
//! Copyright (C) 2014-2020 Intel Corporation. Original author Mika Westerberg;
//! based on work by Ning Li and Alan Cox.

/// One hardware pad bank (`INTEL_GPP`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bank {
    pub name: &'static str,
    pub first_pin: u16,
    pub last_pin: u16,
    pub gpio_base: u16,
}

/// One Cherryview GPIO community.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Community {
    pub name: &'static str,
    pub uid: &'static str,
    pub acpi_space_id: u8,
    pub interrupt_lines: u8,
    pub banks: &'static [Bank],
}

pub const SOUTHWEST_BANKS: [Bank; 7] = [
    Bank {
        name: "southwest_gpp0",
        first_pin: 0,
        last_pin: 7,
        gpio_base: 0,
    }, // pinctrl-cherryview.c:255
    Bank {
        name: "southwest_gpp1",
        first_pin: 15,
        last_pin: 22,
        gpio_base: 15,
    }, // pinctrl-cherryview.c:256
    Bank {
        name: "southwest_gpp2",
        first_pin: 30,
        last_pin: 37,
        gpio_base: 30,
    }, // pinctrl-cherryview.c:257
    Bank {
        name: "southwest_gpp3",
        first_pin: 45,
        last_pin: 52,
        gpio_base: 45,
    }, // pinctrl-cherryview.c:258
    Bank {
        name: "southwest_gpp4",
        first_pin: 60,
        last_pin: 67,
        gpio_base: 60,
    }, // pinctrl-cherryview.c:259
    Bank {
        name: "southwest_gpp5",
        first_pin: 75,
        last_pin: 82,
        gpio_base: 75,
    }, // pinctrl-cherryview.c:260
    Bank {
        name: "southwest_gpp6",
        first_pin: 90,
        last_pin: 97,
        gpio_base: 90,
    }, // pinctrl-cherryview.c:261
];

pub const NORTH_BANKS: [Bank; 5] = [
    Bank {
        name: "north_gpp0",
        first_pin: 0,
        last_pin: 8,
        gpio_base: 0,
    }, // pinctrl-cherryview.c:351
    Bank {
        name: "north_gpp1",
        first_pin: 15,
        last_pin: 27,
        gpio_base: 15,
    }, // pinctrl-cherryview.c:352
    Bank {
        name: "north_gpp2",
        first_pin: 30,
        last_pin: 41,
        gpio_base: 30,
    }, // pinctrl-cherryview.c:353
    Bank {
        name: "north_gpp3",
        first_pin: 45,
        last_pin: 56,
        gpio_base: 45,
    }, // pinctrl-cherryview.c:354
    Bank {
        name: "north_gpp4",
        first_pin: 60,
        last_pin: 72,
        gpio_base: 60,
    }, // pinctrl-cherryview.c:355
];

pub const EAST_BANKS: [Bank; 2] = [
    Bank {
        name: "east_gpp0",
        first_pin: 0,
        last_pin: 11,
        gpio_base: 0,
    }, // pinctrl-cherryview.c:403
    Bank {
        name: "east_gpp1",
        first_pin: 15,
        last_pin: 26,
        gpio_base: 15,
    }, // pinctrl-cherryview.c:404
];

pub const SOUTHEAST_BANKS: [Bank; 6] = [
    Bank {
        name: "southeast_gpp0",
        first_pin: 0,
        last_pin: 7,
        gpio_base: 0,
    }, // pinctrl-cherryview.c:523
    Bank {
        name: "southeast_gpp1",
        first_pin: 15,
        last_pin: 26,
        gpio_base: 15,
    }, // pinctrl-cherryview.c:524
    Bank {
        name: "southeast_gpp2",
        first_pin: 30,
        last_pin: 35,
        gpio_base: 30,
    }, // pinctrl-cherryview.c:525
    Bank {
        name: "southeast_gpp3",
        first_pin: 45,
        last_pin: 52,
        gpio_base: 45,
    }, // pinctrl-cherryview.c:526
    Bank {
        name: "southeast_gpp4",
        first_pin: 60,
        last_pin: 69,
        gpio_base: 60,
    }, // pinctrl-cherryview.c:527
    Bank {
        name: "southeast_gpp5",
        first_pin: 75,
        last_pin: 85,
        gpio_base: 75,
    }, // pinctrl-cherryview.c:528
];

pub const COMMUNITIES: [Community; 4] = [
    Community {
        name: "southwest",
        uid: "1",
        acpi_space_id: 0x91,
        interrupt_lines: 8,
        banks: &SOUTHWEST_BANKS,
    }, // pinctrl-cherryview.c:268-280
    Community {
        name: "north",
        uid: "2",
        acpi_space_id: 0x92,
        interrupt_lines: 8,
        banks: &NORTH_BANKS,
    }, // pinctrl-cherryview.c:362-370
    Community {
        name: "east",
        uid: "3",
        acpi_space_id: 0x93,
        interrupt_lines: 16,
        banks: &EAST_BANKS,
    }, // pinctrl-cherryview.c:407-415
    Community {
        name: "southeast",
        uid: "4",
        acpi_space_id: 0x94,
        interrupt_lines: 16,
        banks: &SOUTHEAST_BANKS,
    }, // pinctrl-cherryview.c:531-543
];

/// Find a community by the ACPI UID used in Linux's four SoC data records.
pub fn community_by_uid(uid: &str) -> Option<&'static Community> {
    COMMUNITIES.iter().find(|community| community.uid == uid)
}

/// Translate a GPIO offset through a community's frozen bank layout.
pub fn gpio_to_pin(community: &Community, gpio: u16) -> Option<u16> {
    community.banks.iter().find_map(|bank| {
        let size = bank.last_pin - bank.first_pin + 1;
        (gpio >= bank.gpio_base && gpio < bank.gpio_base + size)
            .then_some(bank.first_pin + gpio - bank.gpio_base)
    })
}
