// SPDX-License-Identifier: GPL-2.0-only
//! INCREMENT P2 on a simulated GPU: the memory planners' plans EXECUTED. GGTT entries land in the GGTT window and nowhere else, PPGTT
//! entries land in granted DMA table pages found by DEVICE address (never identity), `AwaitRetire` is waited on in order, and the
//! `[I915-MEMORY ...]` marker appears only when the step really happened.
//!
//! With the feature OFF (the shipped default) every one of those stores is REFUSED by name - the `refused_*` tests at the end run in
//! the default build and prove it.
mod exec_common;
use exec_common::*;
use i915_device::exec::dma::{ObjKind, PAGE};
use i915_device::exec::memory::{emit_memory_marker, msi_delivered, stale_access_denied, MemoryProof, AWAIT_RETIRE_POLLS};
use i915_device::exec::ExecError;
use i915_memory_core::probe::GgttGeometry;
use i915_memory_core::{BindRequest, CacheLevel, Gtt, Mode, NoTablePages, Placement, PpgttInit, Target, VmaId};

const GMCH_2M: u16 = 1 << 6; // GGMS = 1: a 2 MiB GGTT = 262144 entries = 1 GiB of address space
const ENTRIES: u64 = 262_144;

fn geom() -> GgttGeometry {
    GgttGeometry::probe(GMCH_2M, 16 << 20, 256 << 20).unwrap()
}

fn req<'a>(pages: &'a [u64], place: Placement) -> BindRequest<'a> {
    BindRequest { pages, size: pages.len() as u64 * 4096, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place }
}

#[cfg(feature = "p2-memory")]
mod p2 {
    use super::*;
    use i915_device::exec::allow::GgttWindow;

    #[test]
    fn ggtt_creation_and_bind_write_the_ggtt_window_only_then_one_invalidate() {
        let (chip, _mem, mut ex, mut gm) = built();
        let mut log = Lines::default();
        ex.set_ggtt_window(GgttWindow::for_table(geom().pte_base, geom().entries).unwrap()).unwrap();
        let scratch = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
        let (mut gtt, create) = Gtt::<16, 4>::new_ggtt(&kbl(), &geom(), scratch.device_addr).unwrap();
        let rep = ex.run_bind_step(&mut log, &mut gm, &create, true, &mut |_| true).unwrap();
        assert_eq!(rep.ggtt_entries as u64, ENTRIES);
        assert_eq!(rep.ggtt_invalidates, 1);
        {
            let s = chip.0.borrow();
            assert_eq!(s.ggtt_writes as u64, ENTRIES * 2, "two dword stores per 8-byte entry");
            assert!(s.ggtt_write_offsets.iter().all(|o| (0x80_0000..0x80_0000 + 0x20_0000).contains(o)), "every store inside the probed table");
        }
        assert_eq!(chip.all_writes(), [(0x101008, 1)], "the ONLY register written is the GGTT invalidate, with GFX_FLSH_CNTL_EN");
        assert_eq!(ex.verify_ggtt(&create), None);
        assert!(log.has("[I915-STEP begun id=0x2001 name=GGTT-BIND"));
        assert!(log.has("[I915-STEP done id=0x2001 name=GGTT-BIND"));

        // bind three pages: PTE = page | PRESENT at BAR0 + 8 MiB + 8 * entry
        let pages = [0x10_0000u64, 0x30_0000, 0x20_0000];
        let plan = gtt.plan_bind(&mut NoTablePages, &req(&pages, Placement::best())).unwrap();
        assert_eq!(plan.offset(), 0x2000, "after the 2-page error-capture slot");
        let rep = ex.run_bind_step(&mut log, &mut gm, &plan, true, &mut |_| true).unwrap();
        assert_eq!(rep.ggtt_entries, 3);
        assert_eq!(chip.ggtt_entry(0x80_0010), 0x10_0001);
        assert_eq!(chip.ggtt_entry(0x80_0018), 0x30_0001);
        assert_eq!(chip.ggtt_entry(0x80_0020), 0x20_0001);
        assert_eq!(ex.verify_ggtt(&plan), None);
        assert_eq!(chip.writes_to(0x101008), [1, 1], "one invalidate per bind");
    }

    #[test]
    fn a_store_outside_the_window_is_refused_and_the_plan_stops_before_its_invalidate() {
        let (chip, _mem, mut ex, mut gm) = built();
        let mut log = Lines::default();
        ex.set_ggtt_window(GgttWindow { base: 0x80_0000, bytes: 0x1000 }).unwrap(); // 512 entries
        let scratch = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
        let (_g, create) = Gtt::<16, 4>::new_ggtt(&kbl(), &geom(), scratch.device_addr).unwrap();
        let e = ex.run_bind_step(&mut log, &mut gm, &create, true, &mut |_| true).unwrap_err();
        assert_eq!(e, ExecError::GgttStoreRefused { offset: 0x80_1000 });
        assert_eq!(chip.0.borrow().ggtt_writes, 1024, "entries 0..512 only");
        assert!(chip.all_writes().is_empty(), "no invalidate after a refused store");
        assert_eq!(ex.stats().refused, 1);
        assert!(log.has("[I915-STEP failed id=0x2001 name=GGTT-BIND seq=1 why=GGTT-STORE-NOT-ALLOWED]"));
    }

    #[test]
    fn ppgtt_entries_land_in_granted_pages_found_by_device_address_not_identity() {
        let (_chip, mem, mut ex, mut gm) = built();
        let mut log = Lines::default();
        let pml4 = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
        let scratch = [(); 4].map(|_| gm.alloc.alloc(ObjKind::Data, PAGE).unwrap());
        let (mut gtt, create) = Gtt::<16, 32>::new_ppgtt(&kbl(), &PpgttInit { pml4: pml4.device_addr, scratch: scratch.map(|g| g.device_addr) }).unwrap();
        let rep = ex.run_bind_step(&mut log, &mut gm, &create, false, &mut |_| true).unwrap();
        assert_eq!(rep.table_entries, 4 * 512);
        assert_eq!(rep.ggtt_entries, 0, "a PPGTT plan never touches BAR0");
        assert_ne!(pml4.cpu_mapping, pml4.device_addr, "the fake host maps CPU != device");
        // every PML4 entry is the top scratch PDP's PDE; read through the CPU mapping
        let pde = gtt.scratch_chain()[3];
        for i in [0u64, 1, 511] {
            assert_eq!(mem.get(pml4.cpu_mapping + i * 8) as u64 | (mem.get(pml4.cpu_mapping + i * 8 + 4) as u64) << 32, pde);
        }
        assert_eq!(mem.get(pml4.device_addr), 0, "the device address is NOT a CPU address: nothing was stored there");

        // bind: the planner asks gm for table pages (DMA grants), and the stores follow
        let data = gm.alloc.alloc(ObjKind::Data, 2 * PAGE).unwrap();
        let before = gm.alloc.grants().count();
        let pages = [data.page(0), data.page(1)];
        let plan = gtt.plan_bind(&mut gm, &req(&pages, Placement::with_mode(Mode::Fixed(0x7000_0000_1000)))).unwrap();
        assert_eq!(plan.new_tables(), 3, "a PDP, a PD and a PT for a fresh range");
        assert_eq!(gm.alloc.grants().count(), before + 3);
        let rep = ex.run_bind_step(&mut log, &mut gm, &plan, false, &mut |_| true).unwrap();
        assert!(rep.ppgtt_tlb_pending, "the PPGTT TLB step is the next batch's, and the executor says so");
        assert_eq!(rep.ggtt_invalidates, 0);
        // the leaf PTEs: find the PT table page and read its entry for va 0x7000_0000_1000 (index 1)
        let walk = gtt.walk(0x7000_0000_1000).unwrap();
        let pt = walk.table[3].unwrap();
        let cpu = gm.alloc.cpu_of(pt, 4096).unwrap();
        assert_ne!(cpu, pt);
        assert_eq!(mem.get(cpu + 8) as u64 & !0xfff, data.page(0), "PTE 1 names the first data page");
        assert_eq!(mem.get(cpu + 16) as u64 & !0xfff, data.page(1));
        assert!(mem.flushes().iter().any(|(c, b)| *c == cpu + 8 && *b == 16), "the touched span was flushed for the page walker");
        assert!(!log.has("FAIL"));
    }

    #[test]
    fn an_entry_whose_table_page_is_not_a_grant_is_refused_by_name() {
        let (_chip, _mem, mut ex, mut gm) = built();
        let pml4 = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
        let s = [(); 4].map(|_| gm.alloc.alloc(ObjKind::Data, PAGE).unwrap());
        // a PPGTT planned over pages this device never was granted
        let (_g, plan) = Gtt::<16, 4>::new_ppgtt(&kbl(), &PpgttInit { pml4: 0x5000_0000, scratch: [0x5000_1000, 0x5000_2000, 0x5000_3000, 0x5000_4000] }).unwrap();
        let e = ex.run_bind(&mut gm, &plan, &mut |_| true).unwrap_err();
        assert_eq!(e, ExecError::OutsideGrant { device_addr: 0x5000_2000, len: 8 });
        assert_eq!(e.name(), "OUTSIDE-DMA-GRANT");
        let _ = (pml4, s);
    }

    #[test]
    fn await_retire_is_waited_on_in_order_and_bounded() {
        let (chip, _mem, mut ex, mut gm) = built();
        let mut log = Lines::default();
        ex.set_ggtt_window(GgttWindow::for_table(geom().pte_base, geom().entries).unwrap()).unwrap();
        let scratch = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
        let (mut gtt, create) = Gtt::<16, 4>::new_ggtt(&kbl(), &geom(), scratch.device_addr).unwrap();
        ex.run_bind(&mut gm, &create, &mut |_| true).unwrap();
        let bind = gtt.plan_bind(&mut NoTablePages, &req(&[0x10_0000], Placement::best())).unwrap();
        ex.run_bind(&mut gm, &bind, &mut |_| true).unwrap();
        let entry = gtt.offset_of(bind.vma()).unwrap().0;
        let off = 0x80_0000 + (entry / 4096 * 8) as u32;
        assert_eq!(chip.ggtt_entry(off), 0x10_0001);
        // unbind: the victim has NOT retired for 3 asks, then does; the scratch fill follows only then
        let unbind = gtt.plan_unbind(bind.vma()).unwrap();
        let mut asks = 0u32;
        let rep = ex.run_bind(&mut gm, &unbind, &mut |v: VmaId| {
            assert_eq!(v, bind.vma());
            asks += 1;
            assert_eq!(chip.ggtt_entry(off), 0x10_0001, "still bound while waiting: the scratch fill comes AFTER the wait");
            asks > 3
        })
        .unwrap();
        assert_eq!((asks, rep.retire_waits), (4, 1));
        assert_eq!(chip.ggtt_entry(off), gtt.scratch_entry());
        let _ = &mut log;
    }

    #[test]
    fn a_victim_that_never_retires_stops_the_plan_before_the_scratch_fill() {
        let (chip, _mem, mut ex, mut gm) = built();
        ex.set_ggtt_window(GgttWindow::for_table(geom().pte_base, geom().entries).unwrap()).unwrap();
        let scratch = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
        let (mut gtt, create) = Gtt::<16, 4>::new_ggtt(&kbl(), &geom(), scratch.device_addr).unwrap();
        ex.run_bind(&mut gm, &create, &mut |_| true).unwrap();
        let bind = gtt.plan_bind(&mut NoTablePages, &req(&[0x10_0000], Placement::best())).unwrap();
        ex.run_bind(&mut gm, &bind, &mut |_| true).unwrap();
        let off = 0x80_0000 + (gtt.offset_of(bind.vma()).unwrap().0 / 4096 * 8) as u32;
        let unbind = gtt.plan_unbind(bind.vma()).unwrap();
        let mut asks = 0u32;
        let e = ex.run_bind(&mut gm, &unbind, &mut |_| {
            asks += 1;
            false
        })
        .unwrap_err();
        assert_eq!(e, ExecError::AwaitRetireTimeout { vma: bind.vma().0, polls: AWAIT_RETIRE_POLLS });
        assert_eq!(asks, AWAIT_RETIRE_POLLS);
        assert_eq!(chip.ggtt_entry(off), 0x10_0001, "the entry was never unmapped under a live GPU use");
    }
}

// ---- the marker, the denial and the MSI ----------------------------------------------------------------------------------------

#[test]
fn stale_access_is_denied_by_the_planner_model_after_an_unbind() {
    let init = PpgttInit { pml4: 0x10000, scratch: [0x11000, 0x12000, 0x13000, 0x14000] };
    let (mut g, _) = Gtt::<16, 32>::new_ppgtt(&kbl(), &init).unwrap();
    let mut next = 0x100000u64;
    let mut src = |_r: i915_memory_core::TablePageRequest| {
        next += 0x1000;
        Ok(i915_memory_core::TablePage { device_addr: next })
    };
    let plan = g.plan_bind(&mut src, &req(&[0x40_0000], Placement::with_mode(Mode::Fixed(0x1000)))).unwrap();
    assert!(!stale_access_denied(&g, 0x1000, 4), "a live mapping is reachable");
    let _ = g.plan_unbind(plan.vma()).unwrap();
    assert!(stale_access_denied(&g, 0x1000, 4), "after the unbind the access is STALE");
    assert!(stale_access_denied(&g, 1 << 48, 4), "beyond 48 bits");
}

#[test]
fn msi_is_ok_only_when_the_line_fired_and_re_armed() {
    use coralos_hal::fake::FakeIrq;
    use coralos_hal::IrqWait;
    let mut irq = FakeIrq::new(11, true);
    irq.script.push_back(Ok(IrqWait::Fired));
    assert!(msi_delivered(&mut irq, 10));
    let mut irq = FakeIrq::new(11, true);
    irq.script.push_back(Ok(IrqWait::TimedOut));
    assert!(!msi_delivered(&mut irq, 10));
}

#[test]
fn the_memory_marker_prints_only_when_every_fact_holds() {
    let d = kbl();
    let full = MemoryProof { ggtt_readback: true, ppgtt_bound: true, stale_denied: true, msi: Some(true) };
    let mut log = Lines::default();
    assert!(emit_memory_marker(&mut log, &d, &full));
    assert_eq!(log.0, ["[I915-MEMORY va_bits=48 stale_access=denied msi=ok]"], "exactly the plan's marker, va_bits from the platform");
    for broken in [
        MemoryProof { ggtt_readback: false, ..full },
        MemoryProof { ppgtt_bound: false, ..full },
        MemoryProof { stale_denied: false, ..full },
        MemoryProof { msi: Some(false), ..full },
        MemoryProof { msi: None, ..full },
    ] {
        let mut log = Lines::default();
        assert!(!emit_memory_marker(&mut log, &d, &broken));
        assert_eq!(log.0.len(), 1);
        assert!(log.0[0].starts_with("[I915-MEMORY INCOMPLETE "), "{:?}", log.0);
        assert!(!log.0[0].starts_with("[I915-MEMORY va_bits"), "an incomplete run never prints the marker itself");
    }
}

// ---- allocator ---------------------------------------------------------------------------------------------------------------

#[test]
fn every_dma_request_carries_the_platform_width_non_snooped_and_the_remaining_budget() {
    let (_mem, mut gm) = memory();
    let g = gm.alloc.alloc_ring(4096).unwrap();
    let h = gm.alloc.alloc_hwsp().unwrap();
    let c = gm.alloc.alloc_context_image(22 * 4096).unwrap();
    assert_eq!(gm.alloc.addr_width(), 39, "dma_mask_size of Kaby Lake (i915_pci.c:416), from the descriptor");
    assert_eq!(gm.alloc.dma().requests.len(), 3);
    assert!(gm.alloc.dma().requests.iter().all(|(_, align, coh)| *align == 4096 && *coh == coralos_hal::Coherence::NonSnooped));
    assert_eq!(gm.alloc.dma().requests[2].0, 22 * 4096, "one contiguous grant for the whole context object");
    assert_eq!(gm.alloc.left(), ENDOWMENT - 4096 - 4096 - 22 * 4096);
    for gr in [g, h, c] {
        assert_ne!(gr.cpu_mapping, gr.device_addr, "cpu_mapping kept beside device_addr, never assumed identical");
        assert_eq!(gr.device_addr % 4096, 0);
        assert!(gr.device_addr + gr.len as u64 <= 1 << 39);
    }
    assert_eq!(gm.alloc.cpu_of(c.device_addr + 0x1234, 4).unwrap(), c.cpu_mapping + 0x1234);
    assert_eq!(gm.alloc.cpu_of(c.device_addr + c.len as u64, 4).unwrap_err().name(), "OUTSIDE-DMA-GRANT", "one past the grant");
    assert_eq!(gm.alloc.cpu_of(c.device_addr + c.len as u64 - 2, 4).unwrap_err().name(), "OUTSIDE-DMA-GRANT", "an access straddling the end");
}

#[test]
fn the_endowment_is_a_hard_limit_the_allocator_enforces_before_asking_the_host() {
    let mut gm = GpuMemSmall::build(3 * PAGE);
    gm.alloc.alloc(ObjKind::Data, 2 * PAGE).unwrap();
    let e = gm.alloc.alloc(ObjKind::Data, 2 * PAGE).unwrap_err();
    assert_eq!(e, ExecError::EndowmentExhausted { want: 2 * PAGE, left: PAGE });
    assert_eq!(gm.alloc.dma().requests.len(), 1, "the host was not asked");
    gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
    assert_eq!(gm.alloc.left(), 0);
    assert_eq!(gm.alloc.alloc(ObjKind::Data, 1).unwrap_err().name(), "DMA-ENDOWMENT-EXHAUSTED");
}

#[test]
fn a_grant_above_the_39_bit_mask_is_refused_by_name_and_a_table_page_request_reports_it() {
    use i915_memory_core::{TablePageRequest, TablePageSource};
    let mut gm = i915_device::exec::dma::GpuMem::new(i915_device::exec::dma::GpuAlloc::new(coralos_hal::fake::FakeDma::new(1 << 39), &kbl(), ENDOWMENT), Mem::default());
    let e = gm.alloc.alloc(ObjKind::Ring, 4096).unwrap_err();
    assert_eq!(e.name(), "DMA-BEYOND-ADDR-WIDTH");
    assert_eq!(gm.alloc.grants().count(), 0, "nothing was recorded");
    assert_eq!(gm.table_page(TablePageRequest { level: 0, base_va: 0 }).unwrap_err().name(), "TABLE-PAGE-REFUSED");
}

struct GpuMemSmall;
impl GpuMemSmall {
    fn build(endowment: usize) -> Gm {
        i915_device::exec::dma::GpuMem::new(i915_device::exec::dma::GpuAlloc::new(coralos_hal::fake::FakeDma::new(PHYS_BASE), &kbl(), endowment), Mem::default())
    }
}

// ---- with the feature OFF (the shipped default) every one of those stores is refused by name ------------------------------------------

#[test]
fn refused_without_the_feature_a_ggtt_bind_stores_nothing_and_says_why() {
    let chip = Chip::new();
    let (_mem, mut gm) = memory();
    // the shipped grant, explicitly: valid in any feature mode
    static FW: [i915_common_core::Reg; 3] = i915_uncore_core::WRITE_ALLOWLIST;
    let mut ex = i915_device::exec::Executor::with_grant(chip.clone(), &kbl(), &FW, None).unwrap();
    let mut log = Lines::default();
    let scratch = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
    let (_g, create) = Gtt::<16, 4>::new_ggtt(&kbl(), &geom(), scratch.device_addr).unwrap();
    let e = ex.run_bind_step(&mut log, &mut gm, &create, true, &mut |_| true).unwrap_err();
    assert_eq!(e.name(), "GGTT-STORE-NOT-ALLOWED");
    assert_eq!(chip.0.borrow().ggtt_writes, 0);
    assert!(chip.all_writes().is_empty());
    assert_eq!(ex.stats().refused, 1);
    assert!(log.has("why=GGTT-STORE-NOT-ALLOWED"));
}

#[test]
fn refused_without_the_feature_the_ggtt_invalidate_register_is_not_writable() {
    let chip = Chip::new();
    static FW: [i915_common_core::Reg; 3] = i915_uncore_core::WRITE_ALLOWLIST;
    let mut ex = i915_device::exec::Executor::with_grant(chip.clone(), &kbl(), &FW, None).unwrap();
    let r = i915_common_core::Reg::new(0x101008);
    let d = ex.domains_for([r]);
    let e = ex.awake(d, |ex| ex.wr(r, 1)).unwrap_err();
    assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
    assert!(chip.all_writes().is_empty());
}

#[cfg(not(feature = "p2-memory"))]
#[test]
fn the_default_build_executor_has_no_ggtt_window_and_three_registers() {
    let (chip, _mem, mut ex, _gm) = built();
    assert_eq!(ex.ggtt_window(), None);
    assert_eq!(ex.allow_list().len(), 3);
    assert!(ex.ggtt_store(0x80_0000, 1).is_err());
    assert_eq!(chip.0.borrow().ggtt_writes, 0);
}

#[allow(dead_code)]
fn _unused(_: Target) {}

#[test]
fn the_grant_table_has_no_compile_time_cap_it_grows_with_memory() {
    // RUNTIME-TABLES-HAVE-NO-CAP: 3000 grants (the old fixed table held 1536) - bounded only by the endowment and the heap.
    let (_mem, mut gm) = memory();
    for _ in 0..3000 {
        gm.alloc.alloc(ObjKind::TablePage, PAGE).unwrap();
    }
    assert_eq!(gm.alloc.grants().count(), 3000);
    let last = *gm.alloc.grants().last().unwrap();
    assert_eq!(gm.alloc.cpu_of(last.device_addr, 8).unwrap(), last.cpu_mapping);
    assert_eq!(gm.alloc.left(), ENDOWMENT - 3000 * PAGE);
}
