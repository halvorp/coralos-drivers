// SPDX-License-Identifier: GPL-2.0-only
//! Literal Linux vectors for INT0002 identity, GPIO count, and wake-source decoding.
//!
//! Ported from Linux `drivers/platform/x86/intel/int0002_vgpio.c`.
//! Copyright (C) 2017 Hans de Goede; Copyright (c) 2014 Intel Corporation.

use int0002_vgpio_core::wake::{
    decode_wake_source, is_wake_source, WakeSource, ACPI_DEVICE_IDS, DRIVER_NAME, GPIO_COUNT,
    IRQ_REQUEST_NAME, VALID_IRQ_MASK, VIRTUAL_GPIO_PIN, WAKE_SOURCES,
};

/// int0002_vgpio.c:42,209,273-:276. Linux has one non-sentinel ACPI ID; pin both its count and
/// its literal name rather than generating an expectation from the production table.
#[test]
fn driver_and_acpi_names_match_linux() {
    assert_eq!(DRIVER_NAME, "INT0002 Virtual GPIO");
    assert_eq!(IRQ_REQUEST_NAME, "INT0002");
    assert_eq!(ACPI_DEVICE_IDS.len(), 1);
    assert_eq!(ACPI_DEVICE_IDS, ["INT0002"]);
}

/// int0002_vgpio.c:44-:45,137,199-:201. Linux exposes offsets 0, 1, and 2, but only the named pin
/// 2 is a valid IRQ source. Pin both the source count and its name.
#[test]
fn the_gpio_count_and_sole_named_wake_source_match_linux() {
    assert_eq!(VIRTUAL_GPIO_PIN, 2);
    assert_eq!(GPIO_COUNT, 3);
    assert_eq!(VALID_IRQ_MASK, 0x0000_0004);
    assert_eq!(WAKE_SOURCES.len(), 1);
    assert_eq!(
        WAKE_SOURCES,
        [WakeSource {
            name: "GPE0A_PME_B0_VIRT_GPIO_PIN",
            gpio: 2
        }]
    );
}

/// int0002_vgpio.c:133-:141. Bit 13 dispatches GPIO 2; zero or unrelated bits return IRQ_NONE.
#[test]
fn parent_irq_status_decodes_to_the_virtual_gpio() {
    assert_eq!(decode_wake_source(0), None);
    assert_eq!(decode_wake_source(0x0000_1000), None, "bit 12 is unrelated");
    assert_eq!(
        decode_wake_source(0x0000_2000),
        Some(WakeSource {
            name: "GPE0A_PME_B0_VIRT_GPIO_PIN",
            gpio: 2
        })
    );
    assert_eq!(
        decode_wake_source(0xffff_ffff),
        Some(WakeSource {
            name: "GPE0A_PME_B0_VIRT_GPIO_PIN",
            gpio: 2
        })
    );
}

/// int0002_vgpio.c:144-:150. The ACPI wake check consults only PME bus 0 status bit 13.
#[test]
fn wake_check_uses_only_the_linux_status_bit() {
    assert!(!is_wake_source(0));
    assert!(!is_wake_source(0xffff_dfff));
    assert!(is_wake_source(0x0000_2000));
    assert!(is_wake_source(0xffff_ffff));
}
