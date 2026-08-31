// SPDX-License-Identifier: GPL-2.0-only
//! Crystal Cove GPIO interrupt type, bank, mask and pending encodings.
//!
//! Mechanically ported from Linux `drivers/gpio/gpio-crystalcove.c`.
//!
//! Copyright (C) 2012, 2014 Intel Corporation. All rights reserved.
//! Original author: Yang, Bin <bin.yang@intel.com>.

use crate::regs::{off, PHYSICAL_GPIO_COUNT};

pub const UPDATE_IRQ_TYPE: u8 = 1 << 0; // gpio-crystalcove.c:24
pub const UPDATE_IRQ_MASK: u8 = 1 << 1; // gpio-crystalcove.c:25
pub const CTLI_INTCNT_DIS: u8 = 0; // gpio-crystalcove.c:39
pub const CTLI_INTCNT_NE: u8 = 1 << 1; // gpio-crystalcove.c:40
pub const CTLI_INTCNT_PE: u8 = 2 << 1; // gpio-crystalcove.c:41
pub const CTLI_INTCNT_BE: u8 = 3 << 1; // gpio-crystalcove.c:42
/// Mask passed to `regmap_update_bits` when changing interrupt detection.
pub const CTLI_INTCNT_MASK: u8 = CTLI_INTCNT_BE; // gpio-crystalcove.c:130

/// Names of every IRQ type accepted by `crystalcove_irq_type`, in Linux switch order.
pub const IRQ_TYPE_NAMES: [&str; 4] = [
    "IRQ_TYPE_NONE",
    "IRQ_TYPE_EDGE_BOTH",
    "IRQ_TYPE_EDGE_RISING",
    "IRQ_TYPE_EDGE_FALLING",
]; // gpio-crystalcove.c:194-204

/// GPIO IRQ trigger requests handled by Linux's `crystalcove_irq_type` switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqType {
    None,
    EdgeBoth,
    EdgeRising,
    EdgeFalling,
    /// A raw request not handled by the switch. Retained so the refusal can name the value.
    Unsupported(u32),
}

/// Why an IRQ register encoding was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqRefusal {
    /// Virtual GPIOs have no Crystal Cove GPIO interrupt registers.
    GpioOutOfRange { gpio: u8, maximum: u8 },
    /// Mirrors Linux's `default: return -EINVAL` for unsupported IRQ type flags.
    UnsupportedIrqType { irq_type: u32 },
}

fn physical_gpio(gpio: u8) -> Result<(), IrqRefusal> {
    if gpio >= PHYSICAL_GPIO_COUNT {
        return Err(IrqRefusal::GpioOutOfRange {
            gpio,
            maximum: PHYSICAL_GPIO_COUNT - 1,
        }); // gpio-crystalcove.c:190-191,236-240,251-255
    }
    Ok(())
}

/// Encode a trigger request into the CTLI interrupt-detect field.
pub fn irq_type_value(irq_type: IrqType) -> Result<u8, IrqRefusal> {
    match irq_type {
        IrqType::None => Ok(CTLI_INTCNT_DIS), // gpio-crystalcove.c:194-195
        IrqType::EdgeBoth => Ok(CTLI_INTCNT_BE), // gpio-crystalcove.c:197-198
        IrqType::EdgeRising => Ok(CTLI_INTCNT_PE), // gpio-crystalcove.c:200-201
        IrqType::EdgeFalling => Ok(CTLI_INTCNT_NE), // gpio-crystalcove.c:203-204
        IrqType::Unsupported(irq_type) => {
            Err(IrqRefusal::UnsupportedIrqType { irq_type }) // gpio-crystalcove.c:206-207
        }
    }
}

/// Encode the `(mask, value)` pair for updating CTLI interrupt detection.
pub fn irq_type_update(irq_type: IrqType) -> Result<(u8, u8), IrqRefusal> {
    Ok((CTLI_INTCNT_MASK, irq_type_value(irq_type)?)) // gpio-crystalcove.c:126-130
}

/// Select the S0 IRQ-mask register and pin bit for a physical GPIO.
pub fn irq_mask_register(gpio: u8) -> Result<(u8, u8), IrqRefusal> {
    physical_gpio(gpio)?;
    let register = if gpio < 8 {
        off::MGPIO0_IRQ_S0
    } else {
        off::MGPIO1_IRQ_S0
    };
    Ok((register, 1 << (gpio % 8))) // gpio-crystalcove.c:115-118
}

/// Encode Linux's mask/unmask update as `(register, mask, value)`.
///
/// `masked=true` writes the pin bit; `masked=false` clears it
/// (`gpio-crystalcove.c:120-123,247-248,260-261`).
pub fn irq_mask_update(gpio: u8, masked: bool) -> Result<(u8, u8, u8), IrqRefusal> {
    let (register, mask) = irq_mask_register(gpio)?;
    Ok((register, mask, if masked { mask } else { 0 }))
}

/// Select the interrupt-status register and pin bit for a physical GPIO.
pub fn irq_status_register(gpio: u8) -> Result<(u8, u8), IrqRefusal> {
    physical_gpio(gpio)?;
    let register = if gpio < 8 {
        off::GPIO0_IRQ
    } else {
        off::GPIO1_IRQ
    };
    Ok((register, 1 << (gpio % 8))) // gpio-crystalcove.c:315-327
}

/// Combine Linux's two 8-bit bank status reads into the 16-bit pending bitmap.
pub fn pending_bitmap(bank0: u8, bank1: u8) -> u16 {
    bank0 as u16 | ((bank1 as u16) << 8) // gpio-crystalcove.c:292
}
