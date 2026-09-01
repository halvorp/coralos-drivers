// SPDX-License-Identifier: GPL-2.0-only
//! GPIO-to-interrupt-line mapping and interrupt type encoding, ported from Linux
//! `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//!
//! Copyright (C) 2014-2020 Intel Corporation. Original author Mika Westerberg;
//! based on work by Ning Li and Alan Cox.

use crate::regs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    EdgeRising,
    EdgeFalling,
    EdgeBoth,
    LevelHigh,
    LevelLow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingError {
    InterruptLineOutOfRange {
        line: u8,
        line_count: u8,
    },
    CommunityLineCountOutOfRange {
        line_count: u8,
        maximum: u8,
    },
    InterruptLineConflictOnLockedPin {
        line: u8,
        owner: u32,
        requested_pin: u32,
    },
    NoFreeInterruptLine {
        requested_pin: u32,
        line_count: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappingUpdate {
    pub padctrl0: u32,
    pub line: u8,
    pub changed: bool,
}

/// Extract PADCTRL0 IntSel (pinctrl-cherryview.c:1177-1179).
pub const fn interrupt_line(ctrl0: u32) -> u8 {
    ((ctrl0 & regs::PADCTRL0_INTSEL_MASK) >> regs::PADCTRL0_INTSEL_SHIFT) as u8
}

/// Replace PADCTRL0 IntSel, naming values that cannot fit the four-bit field.
pub fn encode_interrupt_line(ctrl0: u32, line: u8) -> Result<u32, MappingError> {
    if line >= regs::INTERRUPT_WIRES as u8 {
        return Err(MappingError::InterruptLineOutOfRange {
            line,
            line_count: regs::INTERRUPT_WIRES as u8,
        });
    }
    Ok((ctrl0 & !regs::PADCTRL0_INTSEL_MASK) | ((line as u32) << regs::PADCTRL0_INTSEL_SHIFT))
}

/// Encode IntWakeCfg and RX inversion exactly as `chv_gpio_irq_type`
/// (pinctrl-cherryview.c:1351-1369).
pub const fn encode_trigger(ctrl1: u32, trigger: Trigger) -> u32 {
    let clean = ctrl1 & !(regs::PADCTRL1_INTWAKECFG_MASK | regs::PADCTRL1_INVRXTX_MASK);
    clean
        | match trigger {
            Trigger::EdgeRising => regs::PADCTRL1_INTWAKECFG_RISING,
            Trigger::EdgeFalling => regs::PADCTRL1_INTWAKECFG_FALLING,
            Trigger::EdgeBoth => regs::PADCTRL1_INTWAKECFG_BOTH,
            Trigger::LevelHigh => regs::PADCTRL1_INTWAKECFG_LEVEL,
            Trigger::LevelLow => regs::PADCTRL1_INTWAKECFG_LEVEL | regs::PADCTRL1_INVRXTX_RXDATA,
        }
}

/// Interrupt acknowledge word: one bit selected by IntSel (pinctrl-cherryview.c:1177-1180).
pub const fn acknowledge_word(ctrl0: u32) -> u32 {
    1 << interrupt_line(ctrl0)
}

/// Update INTMASK: Linux clears a bit to mask, sets it to unmask (pinctrl-cherryview.c:1190-1199).
pub const fn update_interrupt_mask(
    mask_word: u32,
    line: u8,
    masked: bool,
) -> Result<u32, MappingError> {
    if line >= regs::INTERRUPT_WIRES as u8 {
        return Err(MappingError::InterruptLineOutOfRange {
            line,
            line_count: regs::INTERRUPT_WIRES as u8,
        });
    }
    if masked {
        Ok(mask_word & !(1 << line))
    } else {
        Ok(mask_word | (1 << line))
    }
}

/// Claim a pin's BIOS-selected line or, on conflict, search downward from the highest usable line
/// as Linux does (pinctrl-cherryview.c:1279-1319).
pub fn map_gpio_to_interrupt_line(
    lines: &mut [u32; regs::INTERRUPT_WIRES],
    line_count: u8,
    pin: u32,
    ctrl0: u32,
    locked: bool,
) -> Result<MappingUpdate, MappingError> {
    if line_count > regs::INTERRUPT_WIRES as u8 {
        return Err(MappingError::CommunityLineCountOutOfRange {
            line_count,
            maximum: regs::INTERRUPT_WIRES as u8,
        });
    }
    let selected = interrupt_line(ctrl0);
    if selected >= line_count {
        return Err(MappingError::InterruptLineOutOfRange {
            line: selected,
            line_count,
        });
    }
    let owner = lines[selected as usize];
    if owner == pin {
        return Ok(MappingUpdate {
            padctrl0: ctrl0,
            line: selected,
            changed: false,
        });
    }
    if owner == regs::INVALID_HWIRQ {
        lines[selected as usize] = pin;
        return Ok(MappingUpdate {
            padctrl0: ctrl0,
            line: selected,
            changed: false,
        });
    }
    if locked {
        return Err(MappingError::InterruptLineConflictOnLockedPin {
            line: selected,
            owner,
            requested_pin: pin,
        });
    }
    let free = (0..line_count)
        .rev()
        .find(|&line| lines[line as usize] == regs::INVALID_HWIRQ)
        .ok_or(MappingError::NoFreeInterruptLine {
            requested_pin: pin,
            line_count,
        })?;
    let value = encode_interrupt_line(ctrl0, free)?;
    lines[free as usize] = pin;
    Ok(MappingUpdate {
        padctrl0: value,
        line: free,
        changed: true,
    })
}
