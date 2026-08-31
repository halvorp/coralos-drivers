// SPDX-License-Identifier: GPL-2.0-only
//! Crystal Cove top-level IRQ-domain and IRQLVL1 bit decoding.
//!
//! Mechanically ported from Linux `drivers/mfd/intel_soc_pmic_crc.c`.
//!
//! Copyright (C) 2012-2014, 2022 Intel Corporation. All rights reserved.
//! Original authors: Yang, Bin <bin.yang@intel.com> and Zhu, Lejun <lejun.zhu@linux.intel.com>.

/// Linux's nexus token distinguishes this top-level domain from the GPIO and charger domains.
pub const DOMAIN_BUS_TOKEN: &str = "DOMAIN_BUS_NEXUS"; // intel_soc_pmic_crc.c:205-207
/// Linux regmap IRQ-chip name.
pub const IRQ_CHIP_NAME: &str = "Crystal Cove"; // intel_soc_pmic_crc.c:129

/// One Linux regmap IRQ entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqDescriptor {
    /// IRQ-domain hardware number.
    pub index: u8,
    /// IRQLVL1 bit mask.
    pub mask: u8,
    /// Linux symbolic IRQ name.
    pub name: &'static str,
}

/// All top-level Crystal Cove IRQs, in Linux table order.
pub const IRQS: [IrqDescriptor; 7] = [
    IrqDescriptor {
        index: 0,
        mask: 0x01,
        name: "PWRSRC",
    }, // intel_soc_pmic_crc.c:26,119
    IrqDescriptor {
        index: 1,
        mask: 0x02,
        name: "THRM",
    }, // intel_soc_pmic_crc.c:27,120
    IrqDescriptor {
        index: 2,
        mask: 0x04,
        name: "BCU",
    }, // intel_soc_pmic_crc.c:28,121
    IrqDescriptor {
        index: 3,
        mask: 0x08,
        name: "ADC",
    }, // intel_soc_pmic_crc.c:29,122
    IrqDescriptor {
        index: 4,
        mask: 0x10,
        name: "CHGR",
    }, // intel_soc_pmic_crc.c:30,123
    IrqDescriptor {
        index: 5,
        mask: 0x20,
        name: "GPIO",
    }, // intel_soc_pmic_crc.c:31,124
    IrqDescriptor {
        index: 6,
        mask: 0x40,
        name: "VHDMIOCP",
    }, // intel_soc_pmic_crc.c:32,125
];

/// Why a hardware IRQ number could not be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqRefusal {
    /// Linux defines hardware IRQs 0 through 6 only.
    IrqOutOfRange { irq: u8, maximum: u8 },
}

/// Return the IRQLVL1 mask for one IRQ-domain hardware number.
pub fn irq_mask(irq: u8) -> Result<u8, IrqRefusal> {
    IRQS.get(irq as usize)
        .map(|descriptor| descriptor.mask)
        .ok_or(IrqRefusal::IrqOutOfRange {
            irq,
            maximum: 6, // intel_soc_pmic_crc.c:32,118-126
        })
}

/// Return the Linux symbolic name for one IRQ-domain hardware number.
pub fn irq_name(irq: u8) -> Result<&'static str, IrqRefusal> {
    IRQS.get(irq as usize)
        .map(|descriptor| descriptor.name)
        .ok_or(IrqRefusal::IrqOutOfRange {
            irq,
            maximum: 6, // intel_soc_pmic_crc.c:32,118-126
        })
}

/// Iterate over every asserted top-level IRQ in hardware-number order.
///
/// Bit 7 has no entry in Linux's seven-entry `crystal_cove_irqs[]` table and is therefore retained
/// by [`undecoded_level_bits`] rather than silently assigned a meaning.
pub fn asserted_irqs(level: u8) -> impl Iterator<Item = &'static IrqDescriptor> {
    IRQS.iter()
        .filter(move |descriptor| level & descriptor.mask != 0)
}

/// Bits in an IRQLVL1 value that Linux's IRQ table does not decode.
pub fn undecoded_level_bits(level: u8) -> u8 {
    level & 0x80 // intel_soc_pmic_crc.c:118-126 (only BIT(0)..BIT(6) are defined)
}
