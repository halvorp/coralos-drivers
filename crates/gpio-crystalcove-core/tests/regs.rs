// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for bank/offset decoding ported from Linux
//! `drivers/gpio/gpio-crystalcove.c`.
//!
//! Copyright (C) 2012, 2014 Intel Corporation. All rights reserved.
//! Original author: Yang, Bin <bin.yang@intel.com>.

use gpio_crystalcove_core::regs::{
    control_register, off, ControlRegister, RegisterRefusal, CONTROL_REGISTER_NAMES,
    EXPORTED_GPIO_COUNT, PANEL_GPIO, PHYSICAL_GPIO_COUNT,
};

/// gpio-crystalcove.c:27-37. Expected values are Linux literals, not derived from production data.
#[test]
fn register_offsets_match_linux_literals() {
    assert_eq!(off::GPIO0_IRQ, 0x0b);
    assert_eq!(off::GPIO1_IRQ, 0x0c);
    assert_eq!(off::MGPIO0_IRQ_S0, 0x19);
    assert_eq!(off::MGPIO1_IRQ_S0, 0x1a);
    assert_eq!(off::MGPIO0_IRQ_SX, 0x1b);
    assert_eq!(off::MGPIO1_IRQ_SX, 0x1c);
    assert_eq!(off::GPIO0_P0_CTLO, 0x2b);
    assert_eq!(off::GPIO0_P0_CTLI, 0x33);
    assert_eq!(off::GPIO1_P0_CTLO, 0x3b);
    assert_eq!(off::GPIO1_P0_CTLI, 0x43);
    assert_eq!(off::GPIO_PANEL_CTL, 0x52);
}

/// gpio-crystalcove.c:21-22 and :354. Both Linux GPIO counts are pinned.
#[test]
fn physical_and_exported_gpio_counts_match_linux() {
    assert_eq!(PHYSICAL_GPIO_COUNT, 16);
    assert_eq!(EXPORTED_GPIO_COUNT, 95);
}

/// gpio-crystalcove.c:60-62 defines exactly two enum entries, with these names and this order.
/// The expected list is frozen literally and is not generated from `CONTROL_REGISTER_NAMES`.
#[test]
fn control_register_count_and_names_match_linux() {
    const LINUX_NAMES: [&str; 2] = ["CTRL_IN", "CTRL_OUT"];
    assert_eq!(CONTROL_REGISTER_NAMES.len(), 2);
    assert_eq!(CONTROL_REGISTER_NAMES, LINUX_NAMES);
}

/// gpio-crystalcove.c:100-112. These literal vectors cross both bank and pin boundaries.
#[test]
fn physical_control_registers_decode_by_bank_and_offset() {
    assert_eq!(control_register(0, ControlRegister::Input), Ok(0x33));
    assert_eq!(control_register(7, ControlRegister::Input), Ok(0x3a));
    assert_eq!(control_register(8, ControlRegister::Input), Ok(0x43));
    assert_eq!(control_register(15, ControlRegister::Input), Ok(0x4a));

    assert_eq!(control_register(0, ControlRegister::Output), Ok(0x2b));
    assert_eq!(control_register(7, ControlRegister::Output), Ok(0x32));
    assert_eq!(control_register(8, ControlRegister::Output), Ok(0x3b));
    assert_eq!(control_register(15, ControlRegister::Output), Ok(0x42));
}

/// gpio-crystalcove.c:87-96 supports only ACPI virtual GPIO 0x5e, mapped to literal register 0x52.
#[test]
fn only_panel_virtual_gpio_is_supported() {
    assert_eq!(PANEL_GPIO, 0x5e);
    assert_eq!(control_register(0x5e, ControlRegister::Input), Ok(0x52));
    assert_eq!(control_register(0x5e, ControlRegister::Output), Ok(0x52));

    assert_eq!(
        control_register(16, ControlRegister::Output),
        Err(RegisterRefusal::UnsupportedVirtualGpio {
            gpio: 16,
            first_virtual: 16,
            only_supported: 0x5e,
        })
    );
    assert_eq!(
        control_register(93, ControlRegister::Input),
        Err(RegisterRefusal::UnsupportedVirtualGpio {
            gpio: 93,
            first_virtual: 16,
            only_supported: 0x5e,
        })
    );
}
