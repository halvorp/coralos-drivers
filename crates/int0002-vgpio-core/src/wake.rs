// SPDX-License-Identifier: GPL-2.0-only
//! INT0002 GPIO identity and wake-source decoding from a sampled GPE0a status word.
//!
//! Ported mechanically from Linux
//! `drivers/platform/x86/intel/int0002_vgpio.c`.
//!
//! Original copyright holders:
//! - Copyright (C) 2017 Hans de Goede <hdegoede@redhat.com>
//! - Copyright (c) 2014 Intel Corporation

use crate::registers::GPE0A_PME_B0_STS_BIT;

/// Linux platform-driver and irq-chip name (`DRV_NAME`).
pub const DRIVER_NAME: &str = "INT0002 Virtual GPIO"; // int0002_vgpio.c:42
/// Name used when Linux requests the shared parent IRQ.
pub const IRQ_REQUEST_NAME: &str = "INT0002"; // int0002_vgpio.c:209
/// The virtual GPIO pin tied to the GPE (`GPE0A_PME_B0_VIRT_GPIO_PIN`).
pub const VIRTUAL_GPIO_PIN: u32 = 2; // int0002_vgpio.c:44-:45
/// Number of GPIO offsets exposed by Linux: virtual pin 2 plus one.
pub const GPIO_COUNT: u32 = VIRTUAL_GPIO_PIN + 1; // int0002_vgpio.c:199-:201
/// Valid IRQ offsets after Linux clears offsets 0 through 1 from the validity bitmap.
pub const VALID_IRQ_MASK: u32 = 1 << VIRTUAL_GPIO_PIN; // int0002_vgpio.c:162-:167
/// Non-sentinel ACPI IDs accepted by the Linux driver.
pub const ACPI_DEVICE_IDS: [&str; 1] = ["INT0002"]; // int0002_vgpio.c:273-:276

/// One wake source defined by INT0002.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakeSource {
    /// Linux macro name, retained so the sole source remains identifiable when counts are pinned.
    pub name: &'static str,
    /// Virtual GPIO offset delivered to the GPIO IRQ domain.
    pub gpio: u32,
}

/// Every wake source Linux defines for INT0002.
pub const WAKE_SOURCES: [WakeSource; 1] = [WakeSource {
    name: "GPE0A_PME_B0_VIRT_GPIO_PIN",
    gpio: 2,
}]; // int0002_vgpio.c:44-:45,137

/// Decode a sampled GPE0a status word into the virtual GPIO wake source.
///
/// Linux rejects the shared parent IRQ when bit 13 is clear, and otherwise dispatches virtual GPIO
/// pin 2 (int0002_vgpio.c:128-:141). Unrelated status bits never make this IRQ ours.
pub const fn decode_wake_source(gpe0a_status: u32) -> Option<WakeSource> {
    if gpe0a_status & GPE0A_PME_B0_STS_BIT == 0 {
        None
    } else {
        Some(WAKE_SOURCES[0])
    }
}

/// Whether a sampled GPE0a status word identifies INT0002 as the wake source.
///
/// This is the pure decode used by Linux's ACPI wakeup handler
/// (int0002_vgpio.c:144-:150), with the port read left to the caller.
pub const fn is_wake_source(gpe0a_status: u32) -> bool {
    gpe0a_status & GPE0A_PME_B0_STS_BIT != 0
}
