// SPDX-License-Identifier: GPL-2.0-only
//! PCI BAR kind/address decode, sizing arithmetic, and slot walking.
//!
//! Ported from Linux `drivers/pci/probe.c` and
//! `include/uapi/linux/pci_regs.h`. Copyright Drew Eckhardt, Martin Mares,
//! and the Linux PCI authors.

use crate::regs::STD_NUM_BARS;

pub const BASE_ADDRESS_0: usize = 0x10; // pci_regs.h:96
pub const BASE_ADDRESS_SPACE: u32 = 0x01; // pci_regs.h:102
pub const BASE_ADDRESS_SPACE_IO: u32 = 0x01; // pci_regs.h:103
pub const BASE_ADDRESS_MEM_TYPE_MASK: u32 = 0x06; // pci_regs.h:105
pub const BASE_ADDRESS_MEM_TYPE_32: u32 = 0x00; // pci_regs.h:106
pub const BASE_ADDRESS_MEM_TYPE_1M: u32 = 0x02; // pci_regs.h:107
pub const BASE_ADDRESS_MEM_TYPE_64: u32 = 0x04; // pci_regs.h:108
pub const BASE_ADDRESS_MEM_PREFETCH: u32 = 0x08; // pci_regs.h:109
pub const BASE_ADDRESS_MEM_MASK: u32 = 0xffff_fff0; // pci_regs.h:110
pub const BASE_ADDRESS_IO_MASK: u32 = 0xffff_fffc; // pci_regs.h:111
pub const BAR_NAMES: [&str; STD_NUM_BARS] = [
    "BAR 0", // pci.c:802
    "BAR 1", // pci.c:803
    "BAR 2", // pci.c:804
    "BAR 3", // pci.c:805
    "BAR 4", // pci.c:806
    "BAR 5", // pci.c:807
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKind {
    Io,
    Memory32,
    Memory64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bar {
    pub index: u8,
    pub offset: u8,
    pub kind: BarKind,
    pub address: u64,
    pub prefetchable: bool,
    pub slots: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarError {
    ConfigTooShort {
        length: usize,
        required: usize,
    },
    BarIndexOutOfRange {
        index: u8,
        count: u8,
    },
    Memory64MissingUpperSlot {
        index: u8,
        count: u8,
    },
    ProbeValueAllOnes {
        value: u32,
    },
    SizeMaskIsZero {
        mask: u64,
    },
    InvalidSizeEncoding {
        base: u64,
        mask_value: u64,
        address_mask: u64,
    },
}

/// Decode one endpoint BAR. A 64-bit BAR consumes this slot and its successor.
pub fn decode_bar(config: &[u8], index: u8) -> Result<Bar, BarError> {
    if index as usize >= STD_NUM_BARS {
        return Err(BarError::BarIndexOutOfRange {
            index,
            count: STD_NUM_BARS as u8,
        });
    }
    let offset = BASE_ADDRESS_0 + index as usize * 4; // probe.c:367,378
    let low = read_u32(config, offset)?;
    if low == u32::MAX {
        return Err(BarError::ProbeValueAllOnes { value: low }); // probe.c:240-245
    }
    if low & BASE_ADDRESS_SPACE == BASE_ADDRESS_SPACE_IO {
        // probe.c:139-142
        return Ok(Bar {
            index,
            offset: offset as u8,
            kind: BarKind::Io,
            address: (low & BASE_ADDRESS_IO_MASK) as u64, // probe.c:250-253
            prefetchable: false,
            slots: 1,
        });
    }

    let prefetchable = low & BASE_ADDRESS_MEM_PREFETCH != 0; // probe.c:145-148
    let kind = match low & BASE_ADDRESS_MEM_TYPE_MASK {
        // probe.c:150-162
        BASE_ADDRESS_MEM_TYPE_64 => BarKind::Memory64,
        BASE_ADDRESS_MEM_TYPE_32 | BASE_ADDRESS_MEM_TYPE_1M => BarKind::Memory32,
        _ => BarKind::Memory32,
    };
    if kind == BarKind::Memory64 {
        if index as usize + 1 >= STD_NUM_BARS {
            return Err(BarError::Memory64MissingUpperSlot {
                index,
                count: STD_NUM_BARS as u8,
            });
        }
        let high = read_u32(config, offset + 4)?; // probe.c:267-273
        Ok(Bar {
            index,
            offset: offset as u8,
            kind,
            address: ((high as u64) << 32) | (low & BASE_ADDRESS_MEM_MASK) as u64,
            prefetchable,
            slots: 2,
        })
    } else {
        Ok(Bar {
            index,
            offset: offset as u8,
            kind,
            address: (low & BASE_ADDRESS_MEM_MASK) as u64, // probe.c:254-257
            prefetchable,
            slots: 1,
        })
    }
}

/// Fixed-capacity iterator over logical BARs; upper halves of 64-bit BARs are skipped.
pub struct Bars<'a> {
    config: &'a [u8],
    next: u8,
    finished: bool,
}

/// Walk all six standard endpoint BAR slots.
pub fn bars(config: &[u8]) -> Bars<'_> {
    Bars {
        config,
        next: 0,
        finished: false,
    }
}

impl Iterator for Bars<'_> {
    type Item = Result<Bar, BarError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished || self.next as usize >= STD_NUM_BARS {
            return None;
        }
        let index = self.next;
        match decode_bar(self.config, index) {
            Ok(bar) => {
                self.next += bar.slots; // probe.c:376-380
                Some(Ok(bar))
            }
            Err(error) => {
                self.finished = true;
                Some(Err(error))
            }
        }
    }
}

/// Linux `pci_size`: derive the power-of-two aperture from the value read
/// after an all-ones sizing probe.
pub fn bar_size(base: u64, mask_value: u64, address_mask: u64) -> Result<u64, BarError> {
    let significant = address_mask & mask_value; // probe.c:112-116
    if significant == 0 {
        return Err(BarError::SizeMaskIsZero { mask: significant });
    }
    let size = significant & significant.wrapping_neg(); // probe.c:118-122
    if base == mask_value && ((base | (size - 1)) & address_mask) != address_mask {
        return Err(BarError::InvalidSizeEncoding {
            base,
            mask_value,
            address_mask,
        }); // probe.c:124-129
    }
    Ok(size)
}

/// Size a decoded BAR from the raw low/high values returned by an all-ones probe.
pub fn size_bar(bar: Bar, mask_low: u32, mask_high: Option<u32>) -> Result<u64, BarError> {
    if mask_low == u32::MAX {
        return Err(BarError::ProbeValueAllOnes { value: mask_low }); // probe.c:231-238
    }
    match bar.kind {
        BarKind::Io => bar_size(
            bar.address,
            (mask_low & BASE_ADDRESS_IO_MASK) as u64,
            BASE_ADDRESS_IO_MASK as u64,
        ), // probe.c:250-253,279
        BarKind::Memory32 => bar_size(
            bar.address,
            (mask_low & BASE_ADDRESS_MEM_MASK) as u64,
            BASE_ADDRESS_MEM_MASK as u64,
        ), // probe.c:254-257,279
        BarKind::Memory64 => {
            let high = mask_high.ok_or(BarError::Memory64MissingUpperSlot {
                index: bar.index,
                count: STD_NUM_BARS as u8,
            })?;
            bar_size(
                bar.address,
                ((high as u64) << 32) | (mask_low & BASE_ADDRESS_MEM_MASK) as u64,
                0xffff_ffff_ffff_fff0,
            ) // probe.c:267-279
        }
    }
}

fn read_u32(config: &[u8], offset: usize) -> Result<u32, BarError> {
    let required = offset + 4;
    if config.len() < required {
        return Err(BarError::ConfigTooShort {
            length: config.len(),
            required,
        });
    }
    Ok(u32::from_le_bytes([
        config[offset],
        config[offset + 1],
        config[offset + 2],
        config[offset + 3],
    ]))
}
