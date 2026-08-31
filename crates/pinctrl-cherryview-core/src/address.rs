// SPDX-License-Identifier: GPL-2.0-only
//! Community/family/pad address math, ported from Linux
//! `drivers/pinctrl/intel/pinctrl-cherryview.c` (`chv_padreg`).
//!
//! Copyright (C) 2014-2020 Intel Corporation. Original author Mika Westerberg;
//! based on work by Ning Li and Alan Cox.

use crate::regs;

/// Offset of a pin register from the community MMIO base.
///
/// Linux divides the GPIO offset into families of 15, places families `0x400` apart, and pads
/// eight bytes apart (pinctrl-cherryview.c:583-592).
pub const fn pad_register_offset(pin: u32, register: u32) -> u32 {
    let family = pin / regs::MAX_FAMILY_PAD_GPIO_NO;
    let pad = pin % regs::MAX_FAMILY_PAD_GPIO_NO;
    regs::FAMILY_PAD_REGS_OFF
        + regs::FAMILY_PAD_REGS_SIZE * family
        + regs::GPIO_REGS_SIZE * pad
        + register
}
