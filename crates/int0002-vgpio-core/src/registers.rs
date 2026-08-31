// SPDX-License-Identifier: GPL-2.0-only
//! GPE0a ports, masks, and pure register-update arithmetic.
//!
//! Ported mechanically from Linux
//! `drivers/platform/x86/intel/int0002_vgpio.c`.
//!
//! Original copyright holders:
//! - Copyright (C) 2017 Hans de Goede <hdegoede@redhat.com>
//! - Copyright (c) 2014 Intel Corporation

/// GPE0a status I/O port (`GPE0A_STS_PORT`).
pub const GPE0A_STS_PORT: u16 = 0x420; // int0002_vgpio.c:49
/// GPE0a enable I/O port (`GPE0A_EN_PORT`).
pub const GPE0A_EN_PORT: u16 = 0x428; // int0002_vgpio.c:50
/// PME bus 0 status bit (`GPE0A_PME_B0_STS_BIT`).
pub const GPE0A_PME_B0_STS_BIT: u32 = 1 << 13; // int0002_vgpio.c:47
/// PME bus 0 enable bit (`GPE0A_PME_B0_EN_BIT`).
pub const GPE0A_PME_B0_EN_BIT: u32 = 1 << 13; // int0002_vgpio.c:48

/// Value written to GPE0a status to acknowledge the PME interrupt.
///
/// Linux writes `GPE0A_PME_B0_STS_BIT` directly (int0002_vgpio.c:80-:83); this is a write-one-to-
/// clear value, not a read/modify/write operation.
pub const fn acknowledge_value() -> u32 {
    GPE0A_PME_B0_STS_BIT
}

/// Set the PME bus 0 enable bit while preserving every unrelated GPE enable bit.
///
/// This is Linux's `gpe_en_reg |= GPE0A_PME_B0_EN_BIT` (int0002_vgpio.c:93-:95), separated from
/// the port read and write so this crate never accesses hardware.
pub const fn enable_pme(current: u32) -> u32 {
    current | GPE0A_PME_B0_EN_BIT
}

/// Clear the PME bus 0 enable bit while preserving every unrelated GPE enable bit.
///
/// This is Linux's `gpe_en_reg &= ~GPE0A_PME_B0_EN_BIT` (int0002_vgpio.c:104-:106), separated from
/// the port read and write so this crate never accesses hardware.
pub const fn disable_pme(current: u32) -> u32 {
    current & !GPE0A_PME_B0_EN_BIT
}
