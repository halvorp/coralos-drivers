// Provenance: first mirrored from CoralOS userland/coral-hal/src/dma.rs (2026-10-11, DECISIONS.md D3); this file is now its own source of truth - `Dma::alloc` (commit 77c0858) is an addition made here.
//! The [`Dma`] seam — coherent DMA memory a driver hands to its device (the first consumer: the Intel 9260 iwlwifi
//! port's TX/RX rings, ICT table, keep-warm page and firmware sections).
//!
//! ONE CALL, NO FREE. A host OS backs [`Dma::alloc_coherent`] with fresh, zeroed, physically-contiguous pages; a host
//! test backs it with the scripted fake (`fake::FakeDma`). There is deliberately no `free`: CoralOS has no call that
//! returns a DMA grant before the task ends (the grant lives in the task's DMA window and goes back with the task), so
//! a `free` here would be a nominal call that frees nothing. A driver allocates its rings once at bring-up and reuses
//! them, as Linux's iwlwifi does across firmware restarts.
//!
//! THE CACHE RULE is [`Coherence`]: the driver says whether its DEVICE snoops CPU caches, and the host picks the CPU
//! memory type from that — never the other way round.

/// Whether the device's DMA snoops the CPU caches — the one fact that decides how the CPU must map the region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coherence {
    /// The device SNOOPS CPU caches: PCIe bus-master DMA through the root complex (iwlwifi, xHCI, NVMe, r8169). The
    /// region is mapped WRITE-BACK and needs no cache maintenance; the driver orders its writes before the doorbell with
    /// a store fence. (Linux `dma_alloc_coherent` on x86.)
    Snooped,
    /// The device does NOT snoop: e.g. a GPU engine with no last-level cache shared with the CPU (Cherry Trail). The
    /// region is mapped WRITE-COMBINING, so a CPU write never sits in a cache line the device cannot see; a store fence
    /// publishes the writes before the doorbell. A host that cannot map WC refuses ([`DmaRefusal::CacheTypeUnavailable`])
    /// rather than answer with a write-back page. (Linux `intel_gt_coherent_map_type`: WB with an LLC, WC without.)
    NonSnooped,
}

/// The largest alignment a request may name: one 4 KiB page. A host's grants are page-aligned, so any power of two up to
/// a page is met by construction; iwlwifi's strictest (the 4 KiB ICT table) is exactly a page.
pub const DMA_MAX_ALIGN: usize = 4096;

/// A granted region: the CPU's address of it, the address the DEVICE is programmed with, and the length asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaRegion {
    pub virt: u64,
    pub phys: u64,
    pub len: usize,
}

/// Why nothing was granted. Every variant names what refused and the value it refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaRefusal {
    /// A zero-byte region was asked for.
    ZeroLength,
    /// `align` is not a power of two, or is above [`DMA_MAX_ALIGN`].
    BadAlign { align: usize },
    /// The host granted a run whose physical address breaks the asked alignment (a host defect, named, never used).
    Misplaced { phys: u64, align: usize },
    /// More than the task's DMA budget.
    OverQuota { len: usize },
    /// No physically-contiguous run of that length.
    OutOfMemory { len: usize },
    /// [`Coherence::NonSnooped`] on a CPU the host cannot map write-combining.
    CacheTypeUnavailable,
    /// A [`DmaRequest`] named an `addr_width` outside `12..=64` bits.
    BadAddrWidth { width: u8 },
    /// The host granted memory whose device address does not fit the request's `addr_width` (named, never used; a host
    /// that cannot place below the width must refuse instead).
    BeyondAddrWidth { device_addr: u64, len: usize, width: u8 },
    /// Any other refusal word the host returned, as itself.
    Host(u64),
}

/// Coherent DMA memory for one driver.
pub trait Dma {
    /// `len` bytes (the host rounds up to whole pages), zeroed and physically contiguous, its physical address a
    /// multiple of `align`, mapped for the CPU as `coherence` requires. Nothing is held on a refusal.
    fn alloc_coherent(&mut self, len: usize, align: usize, coherence: Coherence) -> Result<DmaRegion, DmaRefusal>;

    /// The GPU-grade form (additive; the iwlwifi-era call above is unchanged): a [`DmaRequest`] that also bounds the
    /// device's address width and the caller's byte budget, answered with a [`DmaGrant`] whose CPU mapping and device
    /// address are SEPARATE fields - a caller never derives one from the other (no identity assumption).
    /// The default implementation applies the shared request rules, refuses over-budget BEFORE asking the host, calls
    /// [`Dma::alloc_coherent`], and refuses a grant that does not fit `addr_width`. A host that can place below a width
    /// overrides it so the check happens before memory is taken (`fake::FakeDma` does).
    fn alloc(&mut self, req: DmaRequest) -> Result<DmaGrant, DmaRefusal> {
        check_dma_request(&req)?;
        let region = self.alloc_coherent(req.size, req.align, req.cache)?;
        check_dma_width(&region, req.addr_width)?;
        Ok(DmaGrant { cpu_mapping: region.virt, device_addr: region.phys, len: region.len })
    }
}

/// A request for device memory with its address-width limit and byte budget (the plan's
/// `DmaRequest{size,align,addr_width,cache,budget}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaRequest {
    /// Bytes wanted (the host rounds up to whole pages).
    pub size: usize,
    /// Power of two, at most [`DMA_MAX_ALIGN`].
    pub align: usize,
    /// The device's DMA address width in bits (`12..=64`); the whole grant must lie below `1 << addr_width`.
    /// (Linux `dma_set_mask`; KBL i915: 39 bits, `dma_mask_size` in i915_pci.c.)
    pub addr_width: u8,
    pub cache: Coherence,
    /// The most bytes (rounded up to whole pages) this request may take; more is [`DmaRefusal::OverQuota`].
    pub budget: usize,
}

/// A granted region with the two addresses kept apart: what the CPU uses and what the DEVICE is programmed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaGrant {
    pub cpu_mapping: u64,
    pub device_addr: u64,
    pub len: usize,
}

/// The pure checks of [`Dma::alloc`]: [`check_request`], a sane width, and the budget.
pub fn check_dma_request(req: &DmaRequest) -> Result<(), DmaRefusal> {
    check_request(req.size, req.align)?;
    if !(12..=64).contains(&req.addr_width) {
        return Err(DmaRefusal::BadAddrWidth { width: req.addr_width });
    }
    let pages_bytes = req.size.div_ceil(4096).saturating_mul(4096);
    if pages_bytes > req.budget {
        return Err(DmaRefusal::OverQuota { len: req.size });
    }
    Ok(())
}

/// The check on a grant: its last byte lies below `1 << width`.
pub fn check_dma_width(region: &DmaRegion, width: u8) -> Result<(), DmaRefusal> {
    let last = region.phys.checked_add(region.len.max(1) as u64 - 1);
    let fits = match last {
        Some(l) => width >= 64 || l >> width == 0,
        None => false,
    };
    if fits {
        Ok(())
    } else {
        Err(DmaRefusal::BeyondAddrWidth { device_addr: region.phys, len: region.len, width })
    }
}

/// The request checks every host makes before it asks for memory: a non-zero length and a power-of-two alignment no
/// larger than a page.
pub fn check_request(len: usize, align: usize) -> Result<(), DmaRefusal> {
    if len == 0 {
        return Err(DmaRefusal::ZeroLength);
    }
    if !align.is_power_of_two() || align > DMA_MAX_ALIGN {
        return Err(DmaRefusal::BadAlign { align });
    }
    Ok(())
}

/// The check every host makes on what it was granted: the physical address meets the alignment.
pub fn check_grant(phys: u64, align: usize) -> Result<(), DmaRefusal> {
    if phys % align as u64 != 0 {
        return Err(DmaRefusal::Misplaced { phys, align });
    }
    Ok(())
}
