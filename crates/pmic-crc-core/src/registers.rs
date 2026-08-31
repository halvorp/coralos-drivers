// SPDX-License-Identifier: GPL-2.0-only
//! Crystal Cove PMIC register-map geometry.
//!
//! Mechanically ported from Linux `drivers/mfd/intel_soc_pmic_crc.c`.
//!
//! Copyright (C) 2012-2014, 2022 Intel Corporation. All rights reserved.
//! Original authors: Yang, Bin <bin.yang@intel.com> and Zhu, Lejun <lejun.zhu@linux.intel.com>.

/// Highest register accepted by Linux's Crystal Cove regmap.
pub const MAX_REGISTER: u8 = 0xc6; // intel_soc_pmic_crc.c:21
/// Level-one interrupt status register.
pub const IRQLVL1: u8 = 0x02; // intel_soc_pmic_crc.c:23
/// Level-one interrupt mask register.
pub const MIRQLVL1: u8 = 0x0e; // intel_soc_pmic_crc.c:24
/// Address width used by the regmap, in bits.
pub const REGISTER_BITS: u8 = 8; // intel_soc_pmic_crc.c:112
/// Value width used by the regmap, in bits.
pub const VALUE_BITS: u8 = 8; // intel_soc_pmic_crc.c:113
/// Number of register banks represented by the level-one IRQ chip.
pub const IRQ_REGISTER_COUNT: u8 = 1; // intel_soc_pmic_crc.c:132
