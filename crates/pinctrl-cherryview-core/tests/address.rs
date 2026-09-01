// SPDX-License-Identifier: GPL-2.0-only
//! Address vectors from Linux `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//! Copyright (C) 2014-2020 Intel Corporation; Mika Westerberg, Ning Li, Alan Cox.

use pinctrl_cherryview_core::address::pad_register_offset;
use pinctrl_cherryview_core::regs::{PADCTRL0, PADCTRL1};

#[test]
fn family_and_pad_address_math_matches_linux() {
    // FAMILY_PAD_REGS_OFF=0x4400, family size=0x400, 15 pads/family, stride=8; :34-37, :587-592.
    assert_eq!(pad_register_offset(0, PADCTRL0), 0x4400);
    assert_eq!(pad_register_offset(14, PADCTRL1), 0x4474);
    assert_eq!(pad_register_offset(15, PADCTRL0), 0x4800);
    assert_eq!(pad_register_offset(16, PADCTRL1), 0x480c);
    assert_eq!(pad_register_offset(97, PADCTRL0), 0x5c38);
}
