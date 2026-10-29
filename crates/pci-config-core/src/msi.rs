// SPDX-License-Identifier: GPL-2.0-only
//! The MSI capability's layout (the capability found by [`crate::capability::find_capability`] with [`PCI_CAP_ID_MSI`]).
//!
//! Ported from Linux `include/uapi/linux/pci_regs.h`. Copyright 1994 Drew Eckhardt. Copyright 1997--1999 Martin Mares and
//! the Linux PCI authors.

pub const PCI_CAP_ID_MSI: u8 = 0x05; // pci_regs.h:223
/// Offsets from the capability's own position.
pub const PCI_MSI_FLAGS: usize = 0x02; // pci_regs.h:315
pub const PCI_MSI_FLAGS_ENABLE: u16 = 0x0001; // pci_regs.h:316
pub const PCI_MSI_FLAGS_64BIT: u16 = 0x0080; // pci_regs.h:319
pub const PCI_MSI_ADDRESS_LO: usize = 0x04; // pci_regs.h:322
pub const PCI_MSI_DATA_32: usize = 0x08; // pci_regs.h:324
pub const PCI_MSI_DATA_64: usize = 0x0c; // pci_regs.h:327
