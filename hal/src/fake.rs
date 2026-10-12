// Copied verbatim from CoralOS userland/coral-hal/src/fake.rs (2026-10-11).
//! Scripted fakes of [`Dma`] and [`Irq`] for HOST tests of a driver port (feature `fake`; needs std). They model the
//! two host rules a port can get wrong without noticing on a desk: grants are whole, page-aligned pages that are
//! never returned, and a LEVEL line is masked on delivery until it is acked.

extern crate std;
use std::collections::VecDeque;
use std::vec::Vec;

use crate::dma::{check_dma_request, check_dma_width, check_grant, check_request, Coherence, Dma, DmaGrant, DmaRefusal, DmaRegion, DmaRequest};
use crate::irq::{Irq, IrqRefusal, IrqWait};

const PAGE: u64 = 4096;

/// A bump allocator over a pretend physical range. `virt = phys + VIRT_OFFSET`, so a test can tell the two apart.
#[derive(Debug)]
pub struct FakeDma {
    next_phys: u64,
    /// Refusals to return, in order, before any further grant (the request checks still run first).
    pub refuse: VecDeque<DmaRefusal>,
    /// Every request that reached the fake: `(len, align, coherence)`.
    pub requests: Vec<(usize, usize, Coherence)>,
    /// Every region granted, in order.
    pub granted: Vec<DmaRegion>,
}

impl FakeDma {
    pub const VIRT_OFFSET: u64 = 0x7000_0000_0000;
    /// Grants start at `phys_base` (rounded up to a page).
    pub fn new(phys_base: u64) -> Self {
        FakeDma { next_phys: phys_base.div_ceil(PAGE) * PAGE, refuse: VecDeque::new(), requests: Vec::new(), granted: Vec::new() }
    }
    /// Whole pages held so far — never fewer, since nothing is returned.
    pub fn pages_held(&self) -> u64 {
        self.granted.iter().map(|r| (r.len as u64).div_ceil(PAGE)).sum()
    }
}

impl Dma for FakeDma {
    fn alloc_coherent(&mut self, len: usize, align: usize, coherence: Coherence) -> Result<DmaRegion, DmaRefusal> {
        check_request(len, align)?;
        self.requests.push((len, align, coherence));
        if let Some(r) = self.refuse.pop_front() {
            return Err(r);
        }
        let phys = self.next_phys;
        check_grant(phys, align)?;
        self.next_phys += (len as u64).div_ceil(PAGE) * PAGE;
        let region = DmaRegion { virt: phys + Self::VIRT_OFFSET, phys, len };
        self.granted.push(region);
        Ok(region)
    }

    /// Checks the width BEFORE taking memory: a request the fake cannot place below `addr_width` holds nothing.
    fn alloc(&mut self, req: DmaRequest) -> Result<DmaGrant, DmaRefusal> {
        check_dma_request(&req)?;
        let probe = DmaRegion { virt: 0, phys: self.next_phys, len: req.size.div_ceil(PAGE as usize) * PAGE as usize };
        check_dma_width(&probe, req.addr_width)?;
        let r = self.alloc_coherent(req.size, req.align, req.cache)?;
        Ok(DmaGrant { cpu_mapping: r.virt, device_addr: r.phys, len: r.len })
    }
}

/// One line, driven by a script of wait outcomes. A `level` line is MASKED after it fires until [`Irq::ack`]: a wait
/// while masked returns `TimedOut` without consuming the script and is counted in `waits_while_masked` — the
/// "forgot to re-arm" defect, visible to the test. An exhausted script times out.
#[derive(Debug)]
pub struct FakeIrq {
    pub line: u32,
    pub level: bool,
    masked: bool,
    /// Outcomes to return, in order.
    pub script: VecDeque<Result<IrqWait, IrqRefusal>>,
    /// The `timeout_ms` of every wait.
    pub waits: Vec<u64>,
    pub acks: u32,
    pub waits_while_masked: u32,
}

impl FakeIrq {
    pub fn new(line: u32, level: bool) -> Self {
        FakeIrq { line, level, masked: false, script: VecDeque::new(), waits: Vec::new(), acks: 0, waits_while_masked: 0 }
    }
    pub fn masked(&self) -> bool {
        self.masked
    }
}

impl Irq for FakeIrq {
    fn wait(&mut self, timeout_ms: u64) -> Result<IrqWait, IrqRefusal> {
        self.waits.push(timeout_ms);
        if self.masked {
            self.waits_while_masked += 1;
            return Ok(IrqWait::TimedOut);
        }
        let r = self.script.pop_front().unwrap_or(Ok(IrqWait::TimedOut));
        if self.level && r == Ok(IrqWait::Fired) {
            self.masked = true;
        }
        r
    }
    fn ack(&mut self) -> Result<(), IrqRefusal> {
        self.acks += 1;
        self.masked = false;
        Ok(())
    }
}
