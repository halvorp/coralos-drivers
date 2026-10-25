// SPDX-License-Identifier: GPL-2.0-only
//! The PCI COMMAND register the render and display DMA depend on: decoded, reported on the ring, and never written by this device.
//! Linux sets bus mastering in `i915_driver_hw_probe` (`pci_set_master`, i915_driver.c:533); this device holds no lever for it, so
//! the ring must say what the firmware left - a clear BME is the first thing to read when the GPU never retires a batch.
use coralos_rt::mmio::MmioError;
use i915_device::identity::{read_command, PciCommand, PCI_COMMAND, PCI_COMMAND_IO, PCI_COMMAND_MASTER, PCI_COMMAND_MEMORY};
mod common;
use common::*;

#[test]
fn the_command_bits_are_linux_pci_regs_h() {
    // include/uapi/linux/pci_regs.h: PCI_COMMAND 0x04; _IO 0x1; _MEMORY 0x2; _MASTER 0x4
    assert_eq!(PCI_COMMAND, 0x04);
    assert_eq!(PCI_COMMAND_IO, 0x1);
    assert_eq!(PCI_COMMAND_MEMORY, 0x2);
    assert_eq!(PCI_COMMAND_MASTER, 0x4);
}

#[test]
fn the_command_register_is_the_low_half_of_dword_one_and_the_status_half_is_ignored() {
    let mut w = Window::new(256);
    w.set(0x04, 0x0010_0407); // status: capability list; command: io+mem+bme, INTx disabled (PCI_COMMAND_INTX_DISABLE 0x400)
    let c = read_command(&mut w).unwrap();
    assert_eq!(c, PciCommand { raw: 0x0407 });
    assert!(c.io() && c.memory() && c.bus_master() && c.intx_disabled());
    assert_eq!(w.reads, vec![0x04]);

    let mut w = Window::new(256);
    w.set(0x04, 0xffff_0003);
    let c = read_command(&mut w).unwrap();
    assert!(c.io() && c.memory() && !c.bus_master(), "BME is bit 2 and only bit 2");
    assert!(!c.intx_disabled());
}

#[test]
fn an_unreadable_command_is_a_named_error() {
    let mut w = Window::new(256);
    w.fail_at = Some(0x04);
    assert!(matches!(read_command(&mut w), Err(i915_device::identity::Refusal::ConfigUnreadable(MmioError::OutOfRange { .. }))));
}
