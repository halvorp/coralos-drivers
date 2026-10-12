// SPDX-License-Identifier: GPL-2.0-only
//! GPU memory over `coralos_hal::Dma::alloc`: table pages, rings, context images, the hardware status page and batch buffers.
//!
//! THE DMA CONTRACT (docs/I915-CONTRACTS.md section 6): CPU address and device address are two different numbers. Every grant is
//! recorded as `(cpu_mapping, device_addr, len)` and a planner's DEVICE address is turned into a CPU address ONLY by looking the
//! grant up ([`GpuAlloc::cpu_of`]) - nothing here adds an offset, assumes a window base, or treats one as the other. A device
//! address that no grant contains is `OUTSIDE-DMA-GRANT`, never a guess.
//!
//! EVERY request is `DmaRequest { size, align: 4096, addr_width: dma_mask_size (39 on Kaby Lake), cache: NonSnooped, budget }`.
//! `NonSnooped` (write-combining) is the recommended class for GPU page tables, rings and context memory, so a CPU store is never
//! stranded in a line the page walker cannot see. `budget` is what is LEFT of the device's endowment, so the host is never asked for
//! more than the device was given; the allocator refuses first (`DMA-ENDOWMENT-EXHAUSTED`).
//!
//! The CPU side of a grant is reached through [`CpuMem`]: the lib is `forbid(unsafe_code)`, so the host (the device ELF's runtime,
//! or a test) supplies the loads, stores and cache flush for a `cpu_mapping` address.
use super::ExecError;
use alloc::vec::Vec;
use coralos_hal::{Coherence, Dma, DmaGrant, DmaRefusal, DmaRequest};
use i915_devinfo_core::PlatformDescriptor;
use i915_memory_core::{MemError, TablePage, TablePageRequest, TablePageSource};

/// 4 KiB: the only page size a Gen9 GGTT/PPGTT entry names.
pub const PAGE: usize = 4096;
// RUNTIME-TABLES-HAVE-NO-CAP: the grant table tracks a population that grows at run time (every table page, ring and context object),
// so it is a growable `Vec` with FALLIBLE growth. What bounds it is the device's DMA endowment (checked first, `DMA-ENDOWMENT-EXHAUSTED`)
// and the heap (`GRANT-TABLE-FULL` when the table cannot grow - named, before the grant is taken from the host).

/// What a grant is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjKind {
    TablePage,
    Ring,
    ContextImage,
    Hwsp,
    Batch,
    Data,
}

/// A recorded grant: both addresses side by side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grant {
    pub kind: ObjKind,
    pub cpu_mapping: u64,
    pub device_addr: u64,
    pub len: usize,
}

impl Grant {
    /// The device address of the `i`-th 4 KiB page of this grant (what `BindRequest::pages` names).
    pub const fn page(&self, i: usize) -> u64 {
        self.device_addr + (i * PAGE) as u64
    }
    pub const fn pages(&self) -> usize {
        self.len / PAGE
    }
}

/// The host's CPU access to granted memory. `cpu` is always a `cpu_mapping`-derived address the allocator validated.
pub trait CpuMem {
    fn load32(&mut self, cpu: u64) -> u32;
    fn store32(&mut self, cpu: u64, v: u32);
    /// Make the CPU's stores to `[cpu, cpu + bytes)` visible to the device (`drm_clflush_virt_range` on a write-back mapping; a
    /// store fence on a write-combining one).
    fn flush(&mut self, cpu: u64, bytes: u32);
}

/// The allocator.
pub struct GpuAlloc<D: Dma> {
    dma: D,
    addr_width: u8,
    left: usize,
    grants: Vec<Grant>,
}

impl<D: Dma> GpuAlloc<D> {
    /// `endowment_bytes`: the device's DMA budget (whole pages). `addr_width` comes from the platform, not from a constant.
    pub fn new(dma: D, desc: &PlatformDescriptor, endowment_bytes: usize) -> Self {
        GpuAlloc { dma, addr_width: desc.info.dma_mask_size, left: endowment_bytes / PAGE * PAGE, grants: Vec::new() }
    }

    pub fn addr_width(&self) -> u8 {
        self.addr_width
    }
    /// Endowment still unspent, in bytes.
    pub fn left(&self) -> usize {
        self.left
    }
    pub fn grants(&self) -> impl Iterator<Item = &Grant> {
        self.grants.iter()
    }
    pub fn dma(&self) -> &D {
        &self.dma
    }

    /// One zeroed, page-aligned, physically contiguous grant of `size` bytes (rounded up to whole pages), below `1 << addr_width`.
    pub fn alloc(&mut self, kind: ObjKind, size: usize) -> Result<Grant, ExecError> {
        let rounded = size.div_ceil(PAGE) * PAGE;
        if rounded == 0 {
            return Err(ExecError::Dma(coralos_hal::DmaRefusal::ZeroLength));
        }
        if rounded > self.left {
            return Err(ExecError::EndowmentExhausted { want: rounded, left: self.left });
        }
        // room for the record BEFORE the host is asked: a grant that could not be recorded would be unreachable memory
        if self.grants.try_reserve(1).is_err() {
            return Err(ExecError::GrantTableFull { have: self.grants.len() });
        }
        let req = DmaRequest { size: rounded, align: PAGE, addr_width: self.addr_width, cache: Coherence::NonSnooped, budget: self.left };
        let DmaGrant { cpu_mapping, device_addr, len } = self.dma.alloc(req)?;
        let g = Grant { kind, cpu_mapping, device_addr, len };
        self.left = self.left.saturating_sub(len.div_ceil(PAGE) * PAGE);
        self.grants.push(g); // cannot reallocate: reserved above
        Ok(g)
    }

    /// Give a whole grant back to the host ([`Dma::free`]): it must be one this allocator made and still holds, exactly as recorded. On
    /// `Ok` the grant leaves the table (a device address inside it is `OUTSIDE-DMA-GRANT` from now on) and its pages return to the
    /// endowment. On `Err` nothing changed - in particular `DMA-FREE-UNSUPPORTED` when the host has no free call.
    pub fn release(&mut self, g: Grant) -> Result<(), ExecError> {
        let i = self.grants.iter().position(|r| *r == g).ok_or(ExecError::Dma(DmaRefusal::NotGranted { device_addr: g.device_addr, len: g.len }))?;
        self.dma.free(DmaGrant { cpu_mapping: g.cpu_mapping, device_addr: g.device_addr, len: g.len })?;
        self.grants.swap_remove(i);
        self.left += g.len.div_ceil(PAGE) * PAGE;
        Ok(())
    }

    pub fn alloc_ring(&mut self, bytes: usize) -> Result<Grant, ExecError> {
        self.alloc(ObjKind::Ring, bytes)
    }
    /// A context object: `context_size` bytes, the register state at page 1 (`LRC_STATE_PN`).
    pub fn alloc_context_image(&mut self, context_size: u32) -> Result<Grant, ExecError> {
        self.alloc(ObjKind::ContextImage, context_size as usize)
    }
    /// The engine's hardware status page (one page: the CSB, the seqno dwords).
    pub fn alloc_hwsp(&mut self) -> Result<Grant, ExecError> {
        self.alloc(ObjKind::Hwsp, PAGE)
    }

    /// The grant that contains `[device_addr, device_addr + len)` entirely.
    pub fn find(&self, device_addr: u64, len: u64) -> Option<&Grant> {
        let end = device_addr.checked_add(len)?;
        self.grants().find(|g| device_addr >= g.device_addr && end <= g.device_addr + g.len as u64)
    }

    /// The CPU address of `device_addr`, looked up through the grant that holds `len` bytes there.
    pub fn cpu_of(&self, device_addr: u64, len: u64) -> Result<u64, ExecError> {
        let g = self.find(device_addr, len).ok_or(ExecError::OutsideGrant { device_addr, len })?;
        Ok(g.cpu_mapping + (device_addr - g.device_addr))
    }
}

/// Table pages for the memory planners: each is a 4 KiB `NonSnooped` grant; the planner learns only the DEVICE address.
impl<D: Dma> TablePageSource for GpuAlloc<D> {
    fn table_page(&mut self, req: TablePageRequest) -> Result<TablePage, MemError> {
        match self.alloc(ObjKind::TablePage, PAGE) {
            Ok(g) => Ok(TablePage { device_addr: g.device_addr }),
            Err(_) => Err(MemError::TablePageRefused { level: req.level }),
        }
    }
}

/// The allocator and the host's CPU access to what it granted, together: every load and store is by DEVICE address and is
/// validated against the grant table first.
pub struct GpuMem<D: Dma, M: CpuMem> {
    pub alloc: GpuAlloc<D>,
    pub mem: M,
}

impl<D: Dma, M: CpuMem> GpuMem<D, M> {
    pub fn new(alloc: GpuAlloc<D>, mem: M) -> Self {
        GpuMem { alloc, mem }
    }
    pub fn load32(&mut self, device_addr: u64) -> Result<u32, ExecError> {
        let cpu = self.alloc.cpu_of(device_addr, 4)?;
        Ok(self.mem.load32(cpu))
    }
    pub fn store32(&mut self, device_addr: u64, v: u32) -> Result<(), ExecError> {
        let cpu = self.alloc.cpu_of(device_addr, 4)?;
        self.mem.store32(cpu, v);
        Ok(())
    }
    /// A 64-bit entry as two dword stores, high first (the present bit is in the low dword).
    pub fn store64(&mut self, device_addr: u64, v: u64) -> Result<(), ExecError> {
        let cpu = self.alloc.cpu_of(device_addr, 8)?;
        self.mem.store32(cpu + 4, (v >> 32) as u32);
        self.mem.store32(cpu, v as u32);
        Ok(())
    }
    pub fn load64(&mut self, device_addr: u64) -> Result<u64, ExecError> {
        let cpu = self.alloc.cpu_of(device_addr, 8)?;
        let hi = self.mem.load32(cpu + 4) as u64;
        let lo = self.mem.load32(cpu) as u64;
        Ok(hi << 32 | lo)
    }
    pub fn flush(&mut self, device_addr: u64, bytes: u32) -> Result<(), ExecError> {
        let cpu = self.alloc.cpu_of(device_addr, bytes as u64)?;
        self.mem.flush(cpu, bytes);
        Ok(())
    }
}

impl<D: Dma, M: CpuMem> TablePageSource for GpuMem<D, M> {
    fn table_page(&mut self, req: TablePageRequest) -> Result<TablePage, MemError> {
        self.alloc.table_page(req)
    }
}
