// SPDX-License-Identifier: GPL-2.0-only
//! The ONE seam between the device's logic and the machine it runs on: BAR0, DMA memory, time and the
//! blocking wait for the MSI. The production backing is [`crate::rt`] (coralos-rt: the windows the loader
//! granted); the host tests back it with a model of the card (tests/sim). Nothing above this trait
//! reaches hardware by any other road.
use alloc::string::String;
use coralos_hal::{DmaRefusal, DmaRegion};
use iwlwifi_pcie_nic_core::bus::RawBus;

/// What ended a [`Host::wait_irq`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    /// The MSI fired.
    Irq,
    /// The absolute deadline passed first.
    Deadline,
}

/// Everything the device needs from its host besides the registers ([`RawBus`]).
pub trait Host: RawBus {
    /// Zeroed, physically contiguous, `align`-aligned coherent memory (the card snoops the CPU caches).
    /// There is no free: the memory lives as long as the task (CoralOS has no DMA-free call).
    fn dma_alloc(&mut self, len: usize, align: usize) -> Result<DmaRegion, DmaRefusal>;
    /// The CPU's view of `region`, written. `off + data.len()` is within the region.
    fn dma_write(&mut self, region: &DmaRegion, off: usize, data: &[u8]);
    /// The CPU's view of `region`, read.
    fn dma_read(&mut self, region: &DmaRegion, off: usize, out: &mut [u8]);
    /// Monotonic milliseconds.
    fn now_ms(&mut self) -> u64;
    /// Block until the card's MSI fires or the absolute `deadline_ms` passes. An MSI that fired while
    /// the device was busy is not lost: it wakes the next wait at once.
    fn wait_irq(&mut self, deadline_ms: u64) -> Wake;
    /// The task log: one line, no newline.
    fn log(&mut self, line: &str);
}

/// A small formatting convenience so call sites read as one line each.
pub fn say<H: Host>(h: &mut H, args: core::fmt::Arguments<'_>) {
    use core::fmt::Write;
    let mut s = String::new();
    let _ = s.write_fmt(args);
    h.log(&s);
}
