// SPDX-License-Identifier: GPL-2.0-only
//! Cherryview register offsets and fields, ported from Linux
//! `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//!
//! Copyright (C) 2014-2020 Intel Corporation. Original author Mika Westerberg;
//! based on work by Ning Li and Alan Cox.

pub const INTSTAT: u32 = 0x300; // pinctrl-cherryview.c:31
pub const INTMASK: u32 = 0x380; // pinctrl-cherryview.c:32

pub const FAMILY_PAD_REGS_OFF: u32 = 0x4400; // pinctrl-cherryview.c:34
pub const FAMILY_PAD_REGS_SIZE: u32 = 0x400; // pinctrl-cherryview.c:35
pub const MAX_FAMILY_PAD_GPIO_NO: u32 = 15; // pinctrl-cherryview.c:36
pub const GPIO_REGS_SIZE: u32 = 8; // pinctrl-cherryview.c:37

pub const PADCTRL0: u32 = 0x000; // pinctrl-cherryview.c:39
pub const PADCTRL0_INTSEL_SHIFT: u32 = 28; // pinctrl-cherryview.c:40
pub const PADCTRL0_INTSEL_MASK: u32 = 0xf000_0000; // pinctrl-cherryview.c:41
pub const PADCTRL0_TERM_UP: u32 = 1 << 23; // pinctrl-cherryview.c:42
pub const PADCTRL0_TERM_SHIFT: u32 = 20; // pinctrl-cherryview.c:43
pub const PADCTRL0_TERM_MASK: u32 = 0x0070_0000; // pinctrl-cherryview.c:44
pub const PADCTRL0_TERM_20K: u32 = 1; // pinctrl-cherryview.c:45
pub const PADCTRL0_TERM_5K: u32 = 2; // pinctrl-cherryview.c:46
pub const PADCTRL0_TERM_1K: u32 = 4; // pinctrl-cherryview.c:47
pub const PADCTRL0_PMODE_SHIFT: u32 = 16; // pinctrl-cherryview.c:48
pub const PADCTRL0_PMODE_MASK: u32 = 0x000f_0000; // pinctrl-cherryview.c:49
pub const PADCTRL0_GPIOEN: u32 = 1 << 15; // pinctrl-cherryview.c:50
pub const PADCTRL0_GPIOCFG_SHIFT: u32 = 8; // pinctrl-cherryview.c:51
pub const PADCTRL0_GPIOCFG_MASK: u32 = 0x0000_0700; // pinctrl-cherryview.c:52
pub const PADCTRL0_GPIOCFG_GPIO: u32 = 0; // pinctrl-cherryview.c:53
pub const PADCTRL0_GPIOCFG_GPO: u32 = 1; // pinctrl-cherryview.c:54
pub const PADCTRL0_GPIOCFG_GPI: u32 = 2; // pinctrl-cherryview.c:55
pub const PADCTRL0_GPIOCFG_HIZ: u32 = 3; // pinctrl-cherryview.c:56
pub const PADCTRL0_GPIOTXSTATE: u32 = 1 << 1; // pinctrl-cherryview.c:57
pub const PADCTRL0_GPIORXSTATE: u32 = 1 << 0; // pinctrl-cherryview.c:58

pub const PADCTRL1: u32 = 0x004; // pinctrl-cherryview.c:60
pub const PADCTRL1_CFGLOCK: u32 = 1 << 31; // pinctrl-cherryview.c:61
pub const PADCTRL1_INVRXTX_SHIFT: u32 = 4; // pinctrl-cherryview.c:62
pub const PADCTRL1_INVRXTX_MASK: u32 = 0x0000_00f0; // pinctrl-cherryview.c:63
pub const PADCTRL1_INVRXTX_TXDATA: u32 = 1 << 7; // pinctrl-cherryview.c:64
pub const PADCTRL1_INVRXTX_RXDATA: u32 = 1 << 6; // pinctrl-cherryview.c:65
pub const PADCTRL1_INVRXTX_TXENABLE: u32 = 1 << 5; // pinctrl-cherryview.c:66
pub const PADCTRL1_ODEN: u32 = 1 << 3; // pinctrl-cherryview.c:67
pub const PADCTRL1_INTWAKECFG_MASK: u32 = 0x7; // pinctrl-cherryview.c:68
pub const PADCTRL1_INTWAKECFG_FALLING: u32 = 1; // pinctrl-cherryview.c:69
pub const PADCTRL1_INTWAKECFG_RISING: u32 = 2; // pinctrl-cherryview.c:70
pub const PADCTRL1_INTWAKECFG_BOTH: u32 = 3; // pinctrl-cherryview.c:71
pub const PADCTRL1_INTWAKECFG_LEVEL: u32 = 4; // pinctrl-cherryview.c:72

pub const INVALID_HWIRQ: u32 = !0; // pinctrl-cherryview.c:79
pub const INTERRUPT_WIRES: usize = 16; // pinctrl-cherryview.c:83-88
pub const PINMODE_INVERT_OE: u32 = 1 << 15; // pinctrl-cherryview.c:91
