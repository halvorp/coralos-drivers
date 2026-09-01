// SPDX-License-Identifier: GPL-2.0-only
//! Bounded standard PCI capability-list walking.
//!
//! Ported from Linux `drivers/pci/pci.c`, `drivers/pci/pci.h`, and
//! `include/uapi/linux/pci_regs.h`. Copyright Drew Eckhardt, Martin Mares,
//! Frederic Potter, David Mosberger-Tang, and the Linux PCI authors.

use crate::regs::{self, status};

pub const CAP_LIST_ID: usize = 0; // pci_regs.h:221
pub const CAP_LIST_NEXT: usize = 1; // pci_regs.h:244
pub const FIND_CAP_TTL: usize = 48; // pci.h:18

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    pub id: u8,
    pub offset: u8,
}

/// Every refusal names the malformed input and the applicable bound/rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityError {
    ConfigTooShort {
        length: usize,
        required: usize,
    },
    UnsupportedHeaderType {
        header_type: u8,
    },
    PointerBelowStandardHeader {
        pointer: u8,
        minimum: u8,
    },
    EntryOutOfBounds {
        pointer: u8,
        length: usize,
        required_end: usize,
    },
    InvalidCapabilityId {
        pointer: u8,
        id: u8,
    },
    LoopGuardExhausted {
        limit: usize,
        pointer: u8,
    },
}

/// Locate a capability from the status/header fields in the supplied config image.
pub fn find_capability(
    config: &[u8],
    wanted_id: u8,
) -> Result<Option<Capability>, CapabilityError> {
    require(config, 0x10)?;
    let stat = read_u16(config, 0x06)?; // pci_regs.h:53; pci.c:441-443
    if stat & status::CAP_LIST == 0 {
        return Ok(None);
    }
    let header = config[0x0e] & regs::HEADER_TYPE_MASK; // pci_regs.h:78-82; pci.c:504-506
    match header {
        regs::HEADER_TYPE_NORMAL | regs::HEADER_TYPE_BRIDGE => {
            find_from_pointer(config, 0x34, wanted_id) // pci.c:445-450; pci_regs.h:122
        }
        regs::HEADER_TYPE_CARDBUS => find_from_pointer(config, 0x14, wanted_id), // pci.c:449-450
        other => Err(CapabilityError::UnsupportedHeaderType { header_type: other }),
    }
}

/// Locate the next matching capability after an existing capability.
pub fn find_next_capability(
    config: &[u8],
    current_offset: u8,
    wanted_id: u8,
) -> Result<Option<Capability>, CapabilityError> {
    let pointer_offset = current_offset as usize + CAP_LIST_NEXT; // pci.c:429-432
    find_from_pointer(config, pointer_offset, wanted_id)
}

fn find_from_pointer(
    config: &[u8],
    pointer_offset: usize,
    wanted_id: u8,
) -> Result<Option<Capability>, CapabilityError> {
    require(config, pointer_offset + 1)?;
    let mut pointer = config[pointer_offset]; // pci.h:143

    for _ in 0..FIND_CAP_TTL {
        // pci.h:137,145
        if pointer == 0 {
            return Ok(None);
        }
        if pointer < regs::STD_HEADER_SIZE {
            return Err(CapabilityError::PointerBelowStandardHeader {
                pointer,
                minimum: regs::STD_HEADER_SIZE,
            }); // pci.h:146-147
        }
        pointer &= !3; // pci.h:149
        let end = pointer as usize + 2;
        if end > config.len() {
            return Err(CapabilityError::EntryOutOfBounds {
                pointer,
                length: config.len(),
                required_end: end,
            });
        }
        let id = config[pointer as usize]; // pci.h:150-152
        if id == 0xff {
            return Err(CapabilityError::InvalidCapabilityId { pointer, id }); // pci.h:153-154
        }
        if id == wanted_id {
            return Ok(Some(Capability {
                id,
                offset: pointer,
            })); // pci.h:156-160
        }
        pointer = config[pointer as usize + CAP_LIST_NEXT]; // pci.h:163-164
    }

    Err(CapabilityError::LoopGuardExhausted {
        limit: FIND_CAP_TTL,
        pointer,
    }) // pci.h:131,137,145
}

fn require(config: &[u8], required: usize) -> Result<(), CapabilityError> {
    if config.len() < required {
        Err(CapabilityError::ConfigTooShort {
            length: config.len(),
            required,
        })
    } else {
        Ok(())
    }
}

fn read_u16(config: &[u8], offset: usize) -> Result<u16, CapabilityError> {
    require(config, offset + 2)?;
    Ok(u16::from_le_bytes([config[offset], config[offset + 1]]))
}
