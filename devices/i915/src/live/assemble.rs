// SPDX-License-Identifier: GPL-2.0-only
//! Bringing the uAPI service up from what the proof of life learned plus what only the HOST knows.
//!
//! The proof of life gives the platform row, the SSEU fuse readout and the command-stream timestamp frequency. The host (the loader's
//! endowment, the PCI configuration page, the kernel's accounting) gives the rest, in [`UapiFacts`]; nothing here is guessed from a
//! constant.
use crate::exec::dma::{CpuMem, GpuAlloc, GpuMem};
use crate::exec::Executor;
use crate::gt::Proof;
use crate::log::Log;
use crate::uapi::{bring_up_render, Fail, Hw, RenderConfig, UapiService};
use coralos_hal::{Dma, Mmio};
use i915_gt_core::emit::FlushQuirks;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::errno;

/// What the host supplies.
#[derive(Clone, Copy, Debug)]
pub struct UapiFacts {
    /// PCI revision id (the configuration page).
    pub pci_revision: u8,
    /// `GMCH_CTRL` (PCI configuration offset 0x50, 16 bits): the GGTT table size.
    pub gmch_ctl: u16,
    /// BAR0's length (the loader's window).
    pub bar0_len: u64,
    /// The aperture BAR's length (BAR2), which the loader reports; never inferred.
    pub gmadr_len: u64,
    /// The machine's memory, as the kernel accounts it.
    pub system_memory_bytes: u64,
    /// The DMA budget: the task's quota, in bytes.
    pub dma_endowment_bytes: usize,
    pub quirks: FlushQuirks,
    /// The engine's stall bound in milliseconds.
    pub stall_bound_ms: u64,
    /// The firmware display as startup judged it (D18): an adopted framebuffer's GGTT range is kept through bring-up.
    pub adoption: crate::display::Adoption,
}

/// The executor, the granted memory and the service, brought up.
pub struct Assembled<B: Mmio, D: Dma, M: CpuMem> {
    pub ex: Executor<B>,
    pub gm: GpuMem<D, M>,
    pub svc: UapiService,
}

/// Assemble: the executor over `bar`, the allocator over `dma` (budgeted by `facts.dma_endowment_bytes`), then `bring_up_render`.
/// A refusal is a named [`Fail`]; nothing is served on failure.
pub fn assemble<B: Mmio, D: Dma, M: CpuMem, L: Log>(bar: B, dma: D, mem: M, proof: &Proof, facts: &UapiFacts, log: &mut L) -> Result<Assembled<B, D, M>, Fail> {
    let mut ex = Executor::new(bar, &proof.desc).map_err(|e| Fail::new(errno::EIO, e.name()))?;
    let mut gm = GpuMem::new(GpuAlloc::new(dma, &proof.desc, facts.dma_endowment_bytes), mem);
    let dev = UapiDevice::new(proof.desc, facts.pci_revision, proof.sseu, proof.ts_hz, facts.system_memory_bytes).map_err(|e| Fail::new(errno::ENODEV, e.name()))?;
    let geom = GgttGeometry::probe(facts.gmch_ctl, facts.bar0_len, facts.gmadr_len).map_err(|e| Fail::new(errno::EIO, e.name()))?;
    let cfg = RenderConfig { quirks: facts.quirks, stall_bound_ms: facts.stall_bound_ms, keep: facts.adoption.keep() };
    let mut svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log, now_ms: 0 }, dev, &geom, cfg)?;
    svc.set_display(facts.adoption);
    Ok(Assembled { ex, gm, svc })
}
