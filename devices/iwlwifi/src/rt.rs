// SPDX-License-Identifier: GPL-2.0-only
//! The production [`Host`]: coralos-rt's system calls and the windows the loader granted, nothing else. The ONLY module
//! with `unsafe` (the CPU's view of DMA memory, and `Box::leak` for the `'static` write list the rt window takes); the
//! rest of the crate is safe Rust over the [`Host`] seam.
//!
//! Slot numbering is the loader's convention for this device (coral-driver-metadata `wlan`): MMIO slot 0 is BAR0 (the
//! CSR block, the HBUS indirection, the flow handler, and the MSI-X table page above them), MMIO slot 1 the function's
//! PCI configuration page (READ only; the configuration authority owns the writes), IRQ slot 0 the MSI route.
//!
//! BAR0 starts READ-ONLY and is narrowed to a WRITE ALLOW-LIST built from the registers this driver writes
//! ([`BAR0_WRITE_RANGES`]): the CSR block, the HBUS indirection block and the flow handler. The MSI-X table and PBA above
//! 0x2000 are never writable, so a bug cannot reprogram the interrupt vectors the configuration authority owns.
#![allow(unsafe_code)]
use crate::host::{Host, Wake};
use alloc::boxed::Box;
use alloc::vec::Vec;
use coralos_hal::{Coherence, Dma, DmaRefusal, DmaRegion, Irq};
use coralos_rt::mmio::{Kernel, MmioError, ReadWindow, WriteRange, WriteWindow};
use coralos_rt::word::{WAIT_IRQ_FIRED, WAIT_TIMEOUT};
use coralos_rt::{RtDma, RtIrq};
use iwlwifi_pcie_nic_core::bus::RawBus;

pub const SLOT_BAR0: u32 = 0;
pub const SLOT_CFG: u32 = 1;
pub const IRQ_SLOT: u32 = 0;
/// The span of BAR0 the driver needs: the CSR block (0x000-0x3ff), HBUS (0x400-0x4ff) and the flow handler (0x1000-0x1fff).
pub const BAR0_NEED: u64 = 0x2000;
/// The writable parts of BAR0: (offset, length). CSR 0x000-0x3ff (interrupts, reset, GP control, ownership, the shadow
/// register control), HBUS 0x400-0x47f (the indirect periphery/SRAM accessors and the write-pointer doorbell), and the flow
/// handler 0x1000-0x1fff (upload channel, queue bases, load status, the RFH write-index trigger).
pub const BAR0_WRITE_RANGES: [(u32, u32); 3] = [(0x000, 0x400), (0x400, 0x080), (0x1000, 0x1000)];

pub struct RtHost {
    win: Option<WriteWindow>,
    ro: Option<ReadWindow>,
    irq: Option<RtIrq>,
    dma: RtDma,
    /// Writes the allow-list refused (a bug counter: nonzero is logged by [`RtHost::refused_writes`]'s caller).
    refused: u32,
}

fn leak_ranges(ranges: &[(u32, u32)]) -> Option<&'static [WriteRange]> {
    let mut v: Vec<WriteRange> = Vec::new();
    v.try_reserve_exact(ranges.len()).ok()?;
    v.extend(ranges.iter().map(|(o, l)| WriteRange { off: *o as u64, len: *l as u64 }));
    Some(Box::leak(v.into_boxed_slice()))
}

impl RtHost {
    /// Map BAR0 and narrow it to the allow-list. `None` when the loader granted no BAR0 window of [`BAR0_NEED`] bytes.
    pub fn new() -> Result<RtHost, MmioError> {
        let ranges = leak_ranges(&BAR0_WRITE_RANGES).ok_or(MmioError::NoWindow { slot: SLOT_BAR0 })?;
        let (win, ro) = match WriteWindow::open(&mut Kernel, SLOT_BAR0, BAR0_NEED, ranges) {
            Ok(w) => (Some(w), None),
            // the grant is read-only: keep reading, every write says NOT-ALLOWED (and the driver stops at its first refusal)
            Err(_) => (None, Some(ReadWindow::open(&mut Kernel, SLOT_BAR0, BAR0_NEED)?)),
        };
        // MSI is edge triggered: an ack re-arms nothing, so `level` is false.
        let irq = coralos_rt::irq_line(IRQ_SLOT).map(|line| RtIrq { line, level: false });
        Ok(RtHost { win, ro, irq, dma: RtDma, refused: 0 })
    }

    pub fn has_irq(&self) -> bool {
        self.irq.is_some()
    }

    pub fn irq_line(&self) -> Option<u32> {
        self.irq.as_ref().map(|i| i.line)
    }

    pub fn refused_writes(&self) -> u32 {
        self.refused
    }

    /// Block for any endpoint in the 128-bit set, this card's MSI, or `timeout_ms` (0 = none): the raw rt word. The closed
    /// wrapper serves wifi-manager from this one wait.
    pub fn wait_any(&mut self, eps_lo: u64, eps_hi: u64, timeout_ms: u64) -> u64 {
        coralos_rt::wait_any_irq(eps_lo, eps_hi, timeout_ms, self.irq.as_ref().map_or(0, |i| i.line))
    }

    /// Whether `word` (from [`RtHost::wait_any`]) is this card's interrupt.
    pub fn is_irq_word(word: u64) -> bool {
        word == WAIT_IRQ_FIRED
    }

    fn rd(&self, off: u32) -> u32 {
        let r = match (&self.win, &self.ro) {
            (Some(w), _) => w.read32(off as u64),
            (None, Some(r)) => r.read32(off as u64),
            _ => return u32::MAX,
        };
        r.unwrap_or(u32::MAX) // all-ones is what a dead link reads, and the cores treat it so
    }
}

impl RawBus for RtHost {
    fn read32(&mut self, off: u32) -> u32 {
        self.rd(off)
    }
    fn write32(&mut self, off: u32, val: u32) {
        match &mut self.win {
            Some(w) => {
                if w.write32(off as u64, val).is_err() {
                    self.refused += 1;
                }
            }
            None => self.refused += 1,
        }
    }
    fn write8(&mut self, off: u32, val: u8) {
        match &mut self.win {
            Some(w) => {
                if w.write8(off as u64, val).is_err() {
                    self.refused += 1;
                }
            }
            None => self.refused += 1,
        }
    }
    fn sleep_us(&mut self, us: u32) {
        // A bounded sleep: one wait on nothing, rounded up to a millisecond (the kernel's granularity). Sub-millisecond
        // settles are register handshakes the card completes within the system call's own latency.
        if us >= 1000 {
            coralos_rt::wait_any_irq(0, 0, (us as u64).div_ceil(1000), 0);
        }
    }
}

impl Host for RtHost {
    fn dma_alloc(&mut self, len: usize, align: usize) -> Result<DmaRegion, DmaRefusal> {
        // a PCIe bus-master's DMA snoops the CPU caches: write-back memory, no cache maintenance
        self.dma.alloc_coherent(len, align, Coherence::Snooped)
    }

    fn dma_write(&mut self, region: &DmaRegion, off: usize, data: &[u8]) {
        if off.checked_add(data.len()).is_none_or(|e| e > region.len) {
            return;
        }
        // SAFETY: `region` came from `dma_alloc` (the kernel mapped `len` bytes at `virt` for this task) and the range was checked above.
        unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), (region.virt as *mut u8).add(off), data.len()) };
        // publish the stores before the card is told (a doorbell or a channel enable follows)
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    fn dma_read(&mut self, region: &DmaRegion, off: usize, out: &mut [u8]) {
        if off.checked_add(out.len()).is_none_or(|e| e > region.len) {
            return;
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        // SAFETY: as `dma_write`.
        unsafe { core::ptr::copy_nonoverlapping((region.virt as *const u8).add(off), out.as_mut_ptr(), out.len()) };
    }

    fn now_ms(&mut self) -> u64 {
        coralos_rt::ticks()
    }

    fn wait_irq(&mut self, deadline_ms: u64) -> Wake {
        let now = coralos_rt::ticks();
        if now >= deadline_ms {
            return Wake::Deadline;
        }
        let remaining = (deadline_ms - now).max(1);
        match self.irq.as_mut() {
            Some(i) => match coralos_rt::wait_any_irq(0, 0, remaining, i.line) {
                WAIT_IRQ_FIRED => {
                    let _ = i.ack(); // MSI: edge, a no-op by construction
                    Wake::Irq
                }
                WAIT_TIMEOUT => Wake::Deadline,
                // a denied or unrouted line degrades to polling: the driver reads CSR_INT after every 1 ms tick
                _ => Wake::Irq,
            },
            None => {
                // No MSI route was granted: poll. One millisecond per wake, until the deadline.
                coralos_rt::wait_any_irq(0, 0, remaining.min(1), 0);
                Wake::Irq
            }
        }
    }

    fn log(&mut self, line: &str) {
        coralos_rt::write(b"[iwlwifi] ");
        coralos_rt::write(line.as_bytes());
        coralos_rt::write(b"\n");
    }
}
