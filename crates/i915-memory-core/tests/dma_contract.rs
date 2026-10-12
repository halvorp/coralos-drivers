// SPDX-License-Identifier: GPL-2.0-only
//! The DMA contract end to end: table pages come from `coralos_hal::Dma::alloc` (DmaRequest -> DmaGrant), the planner only ever
//! sees DEVICE addresses, and a fake host that maps CPU != device proves nothing assumes identity.
mod common;
use common::*;
use coralos_hal::fake::FakeDma;
use coralos_hal::{Coherence, Dma, DmaRequest};
use i915_memory_core::*;

#[test]
fn table_pages_are_requested_through_dma_alloc_and_targets_carry_device_addresses_only() {
    let mut dma = FakeDma::new(0x4000_0000);
    let mut grab = |bits_budget: usize| {
        let g = dma.alloc(DmaRequest { size: 4096, align: 4096, addr_width: 39, cache: Coherence::NonSnooped, budget: bits_budget }).unwrap();
        g
    };
    // the five creation pages
    let pml4 = grab(1 << 20);
    let sc = [grab(1 << 20), grab(1 << 20), grab(1 << 20), grab(1 << 20)];
    let init = PpgttInit { pml4: pml4.device_addr, scratch: [sc[0].device_addr, sc[1].device_addr, sc[2].device_addr, sc[3].device_addr] };
    let (mut g, init_plan) = Gtt::<16, 64>::new_ppgtt(&kbl(), &init).unwrap();
    assert!(init_plan.writes().all(|w| matches!(w.target, Target::Table { device_addr, .. } if device_addr < FakeDma::VIRT_OFFSET)));

    let mut granted = vec![];
    let mut src = |req: TablePageRequest| {
        let grant = dma
            .alloc(DmaRequest { size: 4096, align: 4096, addr_width: 39, cache: Coherence::NonSnooped, budget: 4096 })
            .map_err(|_| MemError::TablePageRefused { level: req.level })?;
        granted.push(grant);
        Ok(TablePage { device_addr: grant.device_addr })
    };
    let pages = [0x2000_0000u64];
    let plan = g
        .plan_bind(
            &mut src,
            &BindRequest { pages: &pages, size: 4096, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place: Placement::best() },
        )
        .unwrap();
    assert_eq!(granted.len(), 3);
    for w in plan.writes() {
        let Target::Table { device_addr, .. } = w.target else { panic!("a PPGTT plan has table targets only") };
        assert!(
            device_addr == pml4.device_addr || granted.iter().any(|gr| gr.device_addr == device_addr),
            "{device_addr:#x} is a device address the DMA seam granted"
        );
        assert!(!granted.iter().any(|gr| gr.cpu_mapping == device_addr), "never a CPU address");
    }
    for gr in &granted {
        assert_ne!(gr.cpu_mapping, gr.device_addr, "the fake maps CPU != device: identity would have been caught");
    }
}

#[test]
fn a_dma_host_that_refuses_surfaces_as_a_named_table_page_refusal() {
    let mut dma = FakeDma::new(0x4000_0000);
    let (mut g, _) = Gtt::<16, 64>::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap();
    dma.refuse.push_back(coralos_hal::DmaRefusal::OverQuota { len: 4096 });
    let mut src = |req: TablePageRequest| {
        dma.alloc(DmaRequest { size: 4096, align: 4096, addr_width: 39, cache: Coherence::NonSnooped, budget: 4096 })
            .map(|g| TablePage { device_addr: g.device_addr })
            .map_err(|_| MemError::TablePageRefused { level: req.level })
    };
    let e = g
        .plan_bind(
            &mut src,
            &BindRequest { pages: &[0x1000], size: 4096, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place: Placement::best() },
        )
        .err();
    assert_eq!(e, Some(MemError::TablePageRefused { level: 2 }));
    assert_eq!(g.table_count(), 1);
}
