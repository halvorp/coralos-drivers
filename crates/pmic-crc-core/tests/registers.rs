// SPDX-License-Identifier: GPL-2.0-only
//! Literal register vectors from Linux `drivers/mfd/intel_soc_pmic_crc.c`.
//!
//! Copyright (C) 2012-2014, 2022 Intel Corporation. All rights reserved.
//! Original authors: Yang, Bin <bin.yang@intel.com> and Zhu, Lejun <lejun.zhu@linux.intel.com>.

use pmic_crc_core::registers::{
    IRQLVL1, IRQ_REGISTER_COUNT, MAX_REGISTER, MIRQLVL1, REGISTER_BITS, VALUE_BITS,
};

/// intel_soc_pmic_crc.c:21,23-24,112-115,132. Expected values are Linux literals.
#[test]
fn register_map_literals_match_linux() {
    assert_eq!(MAX_REGISTER, 0xc6);
    assert_eq!(IRQLVL1, 0x02);
    assert_eq!(MIRQLVL1, 0x0e);
    assert_eq!(REGISTER_BITS, 8);
    assert_eq!(VALUE_BITS, 8);
    assert_eq!(IRQ_REGISTER_COUNT, 1);
}
