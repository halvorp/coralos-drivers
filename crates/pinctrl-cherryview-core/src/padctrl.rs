// SPDX-License-Identifier: GPL-2.0-only
//! PADCTRL0/PADCTRL1 encode/decode logic, ported from Linux
//! `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//!
//! Copyright (C) 2014-2020 Intel Corporation. Original author Mika Westerberg;
//! based on work by Ning Li and Alan Cox.

use crate::regs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    MuxModeOutOfRange { mode: u8, maximum: u8 },
    UnsupportedPullUpOhms { ohms: u32 },
    UnsupportedPullDownOhms { ohms: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pull {
    Disabled,
    Up(u32),
    Down(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodedPull {
    Disabled,
    Up(u32),
    Down(u32),
    Unknown { up: bool, encoding: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drive {
    PushPull,
    OpenDrain,
}

/// Whether PADCTRL1 is configuration-locked (pinctrl-cherryview.c:609-612).
pub const fn is_locked(ctrl1: u32) -> bool {
    ctrl1 & regs::PADCTRL1_CFGLOCK != 0
}

/// Program native mux mode and optional output-enable inversion (pinctrl-cherryview.c:687-703).
pub fn encode_mux(
    ctrl0: u32,
    ctrl1: u32,
    mode: u8,
    invert_oe: bool,
) -> Result<(u32, u32), EncodeError> {
    if mode > 15 {
        return Err(EncodeError::MuxModeOutOfRange { mode, maximum: 15 });
    }
    let new0 = (ctrl0 & !(regs::PADCTRL0_GPIOEN | regs::PADCTRL0_PMODE_MASK))
        | ((mode as u32) << regs::PADCTRL0_PMODE_SHIFT);
    let mut new1 = ctrl1 & !regs::PADCTRL1_INVRXTX_MASK;
    if invert_oe {
        new1 |= regs::PADCTRL1_INVRXTX_TXENABLE;
    }
    Ok((new0, new1))
}

/// Enable GPIO mode; Linux converts Hi-Z into input first (pinctrl-cherryview.c:764-778).
pub const fn enable_gpio(ctrl0: u32) -> u32 {
    let mut value = ctrl0;
    if value & regs::PADCTRL0_GPIOCFG_MASK
        == regs::PADCTRL0_GPIOCFG_HIZ << regs::PADCTRL0_GPIOCFG_SHIFT
    {
        value = (value & !regs::PADCTRL0_GPIOCFG_MASK)
            | (regs::PADCTRL0_GPIOCFG_GPI << regs::PADCTRL0_GPIOCFG_SHIFT);
    }
    value | regs::PADCTRL0_GPIOEN
}

/// Set GPIO input/output direction (pinctrl-cherryview.c:807-811).
pub const fn encode_direction(ctrl0: u32, direction: Direction) -> u32 {
    let cfg = match direction {
        Direction::Input => regs::PADCTRL0_GPIOCFG_GPI,
        Direction::Output => regs::PADCTRL0_GPIOCFG_GPO,
    };
    (ctrl0 & !regs::PADCTRL0_GPIOCFG_MASK) | (cfg << regs::PADCTRL0_GPIOCFG_SHIFT)
}

/// Decode direction as Linux does: only GPO is output; every other encoding is input
/// (pinctrl-cherryview.c:1136-1142).
pub const fn decode_direction(ctrl0: u32) -> Direction {
    if (ctrl0 & regs::PADCTRL0_GPIOCFG_MASK) >> regs::PADCTRL0_GPIOCFG_SHIFT
        == regs::PADCTRL0_GPIOCFG_GPO
    {
        Direction::Output
    } else {
        Direction::Input
    }
}

/// Set the GPIO TX state bit (pinctrl-cherryview.c:1116-1123).
pub const fn encode_output_value(ctrl0: u32, high: bool) -> u32 {
    if high {
        ctrl0 | regs::PADCTRL0_GPIOTXSTATE
    } else {
        ctrl0 & !regs::PADCTRL0_GPIOTXSTATE
    }
}

/// Read TX state for GPO, RX state otherwise (pinctrl-cherryview.c:1101-1106).
pub const fn decode_gpio_value(ctrl0: u32) -> bool {
    let output = (ctrl0 & regs::PADCTRL0_GPIOCFG_MASK) >> regs::PADCTRL0_GPIOCFG_SHIFT
        == regs::PADCTRL0_GPIOCFG_GPO;
    ctrl0
        & if output {
            regs::PADCTRL0_GPIOTXSTATE
        } else {
            regs::PADCTRL0_GPIORXSTATE
        }
        != 0
}

/// Encode Linux's supported pull strengths (pinctrl-cherryview.c:918-967).
pub fn encode_pull(ctrl0: u32, pull: Pull) -> Result<u32, EncodeError> {
    let clean = ctrl0 & !(regs::PADCTRL0_TERM_MASK | regs::PADCTRL0_TERM_UP);
    match pull {
        Pull::Disabled => Ok(clean),
        Pull::Up(1_000) => Ok(clean
            | regs::PADCTRL0_TERM_UP
            | (regs::PADCTRL0_TERM_1K << regs::PADCTRL0_TERM_SHIFT)),
        Pull::Up(5_000) => Ok(clean
            | regs::PADCTRL0_TERM_UP
            | (regs::PADCTRL0_TERM_5K << regs::PADCTRL0_TERM_SHIFT)),
        Pull::Up(20_000) => Ok(clean
            | regs::PADCTRL0_TERM_UP
            | (regs::PADCTRL0_TERM_20K << regs::PADCTRL0_TERM_SHIFT)),
        Pull::Up(ohms) => Err(EncodeError::UnsupportedPullUpOhms { ohms }),
        Pull::Down(5_000) => Ok(clean | (regs::PADCTRL0_TERM_5K << regs::PADCTRL0_TERM_SHIFT)),
        Pull::Down(20_000) => Ok(clean | (regs::PADCTRL0_TERM_20K << regs::PADCTRL0_TERM_SHIFT)),
        Pull::Down(ohms) => Err(EncodeError::UnsupportedPullDownOhms { ohms }),
    }
}

/// Decode the termination field and preserve unknown hardware encodings rather than hiding them
/// (pinctrl-cherryview.c:841-878).
pub const fn decode_pull(ctrl0: u32) -> DecodedPull {
    let term = ((ctrl0 & regs::PADCTRL0_TERM_MASK) >> regs::PADCTRL0_TERM_SHIFT) as u8;
    let up = ctrl0 & regs::PADCTRL0_TERM_UP != 0;
    match (up, term) {
        (_, 0) => DecodedPull::Disabled,
        (true, 1) => DecodedPull::Up(20_000),
        (true, 2) => DecodedPull::Up(5_000),
        (true, 4) => DecodedPull::Up(1_000),
        (false, 1) => DecodedPull::Down(20_000),
        (false, 2) => DecodedPull::Down(5_000),
        _ => DecodedPull::Unknown { up, encoding: term },
    }
}

/// Set push-pull/open-drain (pinctrl-cherryview.c:979-986).
pub const fn encode_drive(ctrl1: u32, drive: Drive) -> u32 {
    match drive {
        Drive::PushPull => ctrl1 & !regs::PADCTRL1_ODEN,
        Drive::OpenDrain => ctrl1 | regs::PADCTRL1_ODEN,
    }
}

/// Decode the ODEN bit (pinctrl-cherryview.c:893-900).
pub const fn decode_drive(ctrl1: u32) -> Drive {
    if ctrl1 & regs::PADCTRL1_ODEN != 0 {
        Drive::OpenDrain
    } else {
        Drive::PushPull
    }
}
