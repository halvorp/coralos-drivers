// SPDX-License-Identifier: GPL-2.0-only
//! Register literals from Linux `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//! Copyright (C) 2014-2020 Intel Corporation; Mika Westerberg, Ning Li, Alan Cox.

use pinctrl_cherryview_core::regs::*;

#[test]
fn every_cherryview_register_constant_matches_linux() {
    assert_eq!(INTSTAT, 0x300); // pinctrl-cherryview.c:31
    assert_eq!(INTMASK, 0x380); // pinctrl-cherryview.c:32
    assert_eq!(FAMILY_PAD_REGS_OFF, 0x4400); // pinctrl-cherryview.c:34
    assert_eq!(FAMILY_PAD_REGS_SIZE, 0x400); // pinctrl-cherryview.c:35
    assert_eq!(MAX_FAMILY_PAD_GPIO_NO, 15); // pinctrl-cherryview.c:36
    assert_eq!(GPIO_REGS_SIZE, 8); // pinctrl-cherryview.c:37
    assert_eq!(PADCTRL0, 0x000); // pinctrl-cherryview.c:39
    assert_eq!(PADCTRL0_INTSEL_SHIFT, 28); // pinctrl-cherryview.c:40
    assert_eq!(PADCTRL0_INTSEL_MASK, 0xf000_0000); // pinctrl-cherryview.c:41
    assert_eq!(PADCTRL0_TERM_UP, 0x0080_0000); // pinctrl-cherryview.c:42
    assert_eq!(PADCTRL0_TERM_SHIFT, 20); // pinctrl-cherryview.c:43
    assert_eq!(PADCTRL0_TERM_MASK, 0x0070_0000); // pinctrl-cherryview.c:44
    assert_eq!(PADCTRL0_TERM_20K, 1); // pinctrl-cherryview.c:45
    assert_eq!(PADCTRL0_TERM_5K, 2); // pinctrl-cherryview.c:46
    assert_eq!(PADCTRL0_TERM_1K, 4); // pinctrl-cherryview.c:47
    assert_eq!(PADCTRL0_PMODE_SHIFT, 16); // pinctrl-cherryview.c:48
    assert_eq!(PADCTRL0_PMODE_MASK, 0x000f_0000); // pinctrl-cherryview.c:49
    assert_eq!(PADCTRL0_GPIOEN, 0x0000_8000); // pinctrl-cherryview.c:50
    assert_eq!(PADCTRL0_GPIOCFG_SHIFT, 8); // pinctrl-cherryview.c:51
    assert_eq!(PADCTRL0_GPIOCFG_MASK, 0x0000_0700); // pinctrl-cherryview.c:52
    assert_eq!(PADCTRL0_GPIOCFG_GPIO, 0); // pinctrl-cherryview.c:53
    assert_eq!(PADCTRL0_GPIOCFG_GPO, 1); // pinctrl-cherryview.c:54
    assert_eq!(PADCTRL0_GPIOCFG_GPI, 2); // pinctrl-cherryview.c:55
    assert_eq!(PADCTRL0_GPIOCFG_HIZ, 3); // pinctrl-cherryview.c:56
    assert_eq!(PADCTRL0_GPIOTXSTATE, 0x2); // pinctrl-cherryview.c:57
    assert_eq!(PADCTRL0_GPIORXSTATE, 0x1); // pinctrl-cherryview.c:58
    assert_eq!(PADCTRL1, 0x004); // pinctrl-cherryview.c:60
    assert_eq!(PADCTRL1_CFGLOCK, 0x8000_0000); // pinctrl-cherryview.c:61
    assert_eq!(PADCTRL1_INVRXTX_SHIFT, 4); // pinctrl-cherryview.c:62
    assert_eq!(PADCTRL1_INVRXTX_MASK, 0xf0); // pinctrl-cherryview.c:63
    assert_eq!(PADCTRL1_INVRXTX_TXDATA, 0x80); // pinctrl-cherryview.c:64
    assert_eq!(PADCTRL1_INVRXTX_RXDATA, 0x40); // pinctrl-cherryview.c:65
    assert_eq!(PADCTRL1_INVRXTX_TXENABLE, 0x20); // pinctrl-cherryview.c:66
    assert_eq!(PADCTRL1_ODEN, 0x8); // pinctrl-cherryview.c:67
    assert_eq!(PADCTRL1_INTWAKECFG_MASK, 0x7); // pinctrl-cherryview.c:68
    assert_eq!(PADCTRL1_INTWAKECFG_FALLING, 1); // pinctrl-cherryview.c:69
    assert_eq!(PADCTRL1_INTWAKECFG_RISING, 2); // pinctrl-cherryview.c:70
    assert_eq!(PADCTRL1_INTWAKECFG_BOTH, 3); // pinctrl-cherryview.c:71
    assert_eq!(PADCTRL1_INTWAKECFG_LEVEL, 4); // pinctrl-cherryview.c:72
    assert_eq!(INVALID_HWIRQ, 0xffff_ffff); // pinctrl-cherryview.c:79
    assert_eq!(INTERRUPT_WIRES, 16); // pinctrl-cherryview.c:83-88
    assert_eq!(PINMODE_INVERT_OE, 0x8000); // pinctrl-cherryview.c:91
}
