// SPDX-License-Identifier: GPL-2.0-only
//! Literal top-level IRQ vectors from Linux `drivers/mfd/intel_soc_pmic_crc.c`.
//!
//! Copyright (C) 2012-2014, 2022 Intel Corporation. All rights reserved.
//! Original authors: Yang, Bin <bin.yang@intel.com> and Zhu, Lejun <lejun.zhu@linux.intel.com>.

use pmic_crc_core::irq::{
    asserted_irqs, irq_mask, irq_name, undecoded_level_bits, IrqRefusal, DOMAIN_BUS_TOKEN, IRQS,
    IRQ_CHIP_NAME,
};

/// intel_soc_pmic_crc.c:26-32,118-126. This expectation is frozen and independent of `IRQS`.
const LINUX_IRQS: [(u8, u8, &str); 7] = [
    (0, 0x01, "PWRSRC"),
    (1, 0x02, "THRM"),
    (2, 0x04, "BCU"),
    (3, 0x08, "ADC"),
    (4, 0x10, "CHGR"),
    (5, 0x20, "GPIO"),
    (6, 0x40, "VHDMIOCP"),
];

/// Linux defines exactly seven top-level IRQs; count, names, numbers and masks are all pinned.
#[test]
fn irq_count_names_numbers_and_masks_match_linux() {
    let ours: Vec<(u8, u8, &str)> = IRQS
        .iter()
        .map(|descriptor| (descriptor.index, descriptor.mask, descriptor.name))
        .collect();
    assert_eq!(IRQS.len(), 7);
    assert_eq!(ours, LINUX_IRQS);
}

/// intel_soc_pmic_crc.c:26-32,118-126: hardware IRQ n maps to BIT(n).
#[test]
fn irq_mask_encodes_every_linux_irq_and_names_refusal() {
    assert_eq!(irq_mask(0), Ok(0x01));
    assert_eq!(irq_mask(1), Ok(0x02));
    assert_eq!(irq_mask(2), Ok(0x04));
    assert_eq!(irq_mask(3), Ok(0x08));
    assert_eq!(irq_mask(4), Ok(0x10));
    assert_eq!(irq_mask(5), Ok(0x20));
    assert_eq!(irq_mask(6), Ok(0x40));
    assert_eq!(
        irq_mask(7),
        Err(IrqRefusal::IrqOutOfRange { irq: 7, maximum: 6 })
    );
}

/// intel_soc_pmic_crc.c:26-32: symbolic top-level names are available by IRQ number.
#[test]
fn irq_name_decodes_every_linux_irq_and_names_refusal() {
    assert_eq!(irq_name(0), Ok("PWRSRC"));
    assert_eq!(irq_name(1), Ok("THRM"));
    assert_eq!(irq_name(2), Ok("BCU"));
    assert_eq!(irq_name(3), Ok("ADC"));
    assert_eq!(irq_name(4), Ok("CHGR"));
    assert_eq!(irq_name(5), Ok("GPIO"));
    assert_eq!(irq_name(6), Ok("VHDMIOCP"));
    assert_eq!(
        irq_name(0xff),
        Err(IrqRefusal::IrqOutOfRange {
            irq: 0xff,
            maximum: 6
        })
    );
}

/// intel_soc_pmic_crc.c:118-126: set IRQLVL1 bits decode in the table's hardware-number order.
#[test]
fn asserted_level_bits_decode_to_named_irqs() {
    let names: Vec<&str> = asserted_irqs(0x51)
        .map(|descriptor| descriptor.name)
        .collect();
    assert_eq!(names, ["PWRSRC", "CHGR", "VHDMIOCP"]);
    assert_eq!(asserted_irqs(0).count(), 0);
    assert_eq!(asserted_irqs(0x7f).count(), 7);
}

/// The Linux table defines BIT(0)..BIT(6), not bit 7; an unknown status must not disappear.
#[test]
fn undefined_level_bit_is_preserved() {
    assert_eq!(undecoded_level_bits(0x00), 0x00);
    assert_eq!(undecoded_level_bits(0x7f), 0x00);
    assert_eq!(undecoded_level_bits(0x80), 0x80);
    assert_eq!(undecoded_level_bits(0xff), 0x80);
}

/// intel_soc_pmic_crc.c:129,205-207. These names prevent domain confusion with child irqchips.
#[test]
fn irq_chip_and_domain_token_names_match_linux() {
    assert_eq!(IRQ_CHIP_NAME, "Crystal Cove");
    assert_eq!(DOMAIN_BUS_TOKEN, "DOMAIN_BUS_NEXUS");
}
