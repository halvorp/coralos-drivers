// SPDX-License-Identifier: GPL-2.0-only
//! Crystal Cove GPIO bank/offset decoding and register constants.
//!
//! Mechanically ported from Linux `drivers/gpio/gpio-crystalcove.c`.
//!
//! Copyright (C) 2012, 2014 Intel Corporation. All rights reserved.
//! Original author: Yang, Bin <bin.yang@intel.com>.

/// Number of physical GPIOs that support ordinary control and interrupts.
pub const PHYSICAL_GPIO_COUNT: u8 = 16; // gpio-crystalcove.c:21
/// Number of GPIO numbers exported by Linux, including ACPI virtual GPIOs.
pub const EXPORTED_GPIO_COUNT: u8 = 95; // gpio-crystalcove.c:22
/// The sole virtual GPIO Linux maps: ACPI GPIO number `0x5e`, panel control.
pub const PANEL_GPIO: u8 = 0x5e; // gpio-crystalcove.c:93

/// Names of Linux's two `ctrl_register` entries, in enum order.
pub const CONTROL_REGISTER_NAMES: [&str; 2] = ["CTRL_IN", "CTRL_OUT"]; // gpio-crystalcove.c:60-62

/// Register offsets.
pub mod off {
    pub const GPIO0_IRQ: u8 = 0x0b; // gpio-crystalcove.c:27
    pub const GPIO1_IRQ: u8 = 0x0c; // gpio-crystalcove.c:28
    pub const MGPIO0_IRQ_S0: u8 = 0x19; // gpio-crystalcove.c:29
    pub const MGPIO1_IRQ_S0: u8 = 0x1a; // gpio-crystalcove.c:30
    pub const MGPIO0_IRQ_SX: u8 = 0x1b; // gpio-crystalcove.c:31
    pub const MGPIO1_IRQ_SX: u8 = 0x1c; // gpio-crystalcove.c:32
    pub const GPIO0_P0_CTLO: u8 = 0x2b; // gpio-crystalcove.c:33
    pub const GPIO0_P0_CTLI: u8 = 0x33; // gpio-crystalcove.c:34
    pub const GPIO1_P0_CTLO: u8 = 0x3b; // gpio-crystalcove.c:35
    pub const GPIO1_P0_CTLI: u8 = 0x43; // gpio-crystalcove.c:36
    pub const GPIO_PANEL_CTL: u8 = 0x52; // gpio-crystalcove.c:37
}

/// Which per-pin control-register family to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRegister {
    Input,
    Output,
}

/// Why a GPIO control-register address could not be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterRefusal {
    /// Linux only supports panel control among virtual GPIOs.
    UnsupportedVirtualGpio {
        gpio: u8,
        first_virtual: u8,
        only_supported: u8,
    },
}

/// Decode a GPIO number and control-register family to a PMIC register address.
///
/// Physical GPIOs 0..=7 use bank 0 and 8..=15 use bank 1; the pin offset is `gpio % 8`
/// (`gpio-crystalcove.c:101-112`). GPIO `0x5e` maps to panel control
/// (`gpio-crystalcove.c:87-96`). Every other virtual GPIO is refused by name instead of imitating
/// Linux's later silent success paths.
pub fn control_register(gpio: u8, register: ControlRegister) -> Result<u8, RegisterRefusal> {
    if gpio >= PHYSICAL_GPIO_COUNT {
        if gpio == PANEL_GPIO {
            return Ok(off::GPIO_PANEL_CTL); // gpio-crystalcove.c:93-94
        }
        return Err(RegisterRefusal::UnsupportedVirtualGpio {
            gpio,
            first_virtual: PHYSICAL_GPIO_COUNT,
            only_supported: PANEL_GPIO,
        }); // gpio-crystalcove.c:87-97
    }

    let base = match (register, gpio < 8) {
        (ControlRegister::Input, true) => off::GPIO0_P0_CTLI, // gpio-crystalcove.c:100-102
        (ControlRegister::Input, false) => off::GPIO1_P0_CTLI, // gpio-crystalcove.c:103-104
        (ControlRegister::Output, true) => off::GPIO0_P0_CTLO, // gpio-crystalcove.c:105-107
        (ControlRegister::Output, false) => off::GPIO1_P0_CTLO, // gpio-crystalcove.c:108-109
    };
    Ok(base + gpio % 8) // gpio-crystalcove.c:112
}
