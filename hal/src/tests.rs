// Copied verbatim from CoralOS userland/coral-hal/src/tests.rs (2026-10-11).
//! Host tests of the DMA/IRQ seam's request rules and of the scripted fakes ports test against.

use crate::dma::{check_grant, check_request, Coherence, Dma, DmaRefusal, DMA_MAX_ALIGN};
use crate::fake::{FakeDma, FakeIrq};
use crate::irq::{Irq, IrqRefusal, IrqWait};

#[test]
fn the_request_rule_takes_a_power_of_two_up_to_a_page_and_names_every_refusal() {
    assert_eq!(DMA_MAX_ALIGN, 4096, "one 4 KiB page");
    for a in [1usize, 2, 64, 256, 1024, 4096] {
        assert_eq!(check_request(1, a), Ok(()), "align {a}");
    }
    for a in [0usize, 3, 96, 8192, 1 << 20] {
        assert_eq!(check_request(1, a), Err(DmaRefusal::BadAlign { align: a }), "align {a}");
    }
    assert_eq!(check_request(0, 4096), Err(DmaRefusal::ZeroLength));
    assert_eq!(check_grant(0x1000, 4096), Ok(()));
    assert_eq!(check_grant(0x1100, 256), Ok(()));
    assert_eq!(check_grant(0x1100, 4096), Err(DmaRefusal::Misplaced { phys: 0x1100, align: 4096 }));
}

#[test]
fn the_fake_grants_whole_aligned_pages_keeps_them_and_replays_scripted_refusals() {
    let mut d = FakeDma::new(0x10_0001);
    let a = d.alloc_coherent(100, 256, Coherence::Snooped).unwrap();
    assert_eq!(a.phys, 0x10_1000, "the base is rounded up to a page");
    assert_eq!(a.virt, 0x10_1000 + FakeDma::VIRT_OFFSET);
    assert_eq!(a.len, 100, "the length asked for, not the page");
    let b = d.alloc_coherent(4097, 4096, Coherence::NonSnooped).unwrap();
    assert_eq!(b.phys, 0x10_2000, "the next grant starts on the next page");
    assert_eq!(d.pages_held(), 3);
    d.refuse.push_back(DmaRefusal::OverQuota { len: 4096 });
    assert_eq!(d.alloc_coherent(4096, 4096, Coherence::Snooped), Err(DmaRefusal::OverQuota { len: 4096 }));
    assert_eq!(d.pages_held(), 3, "a refusal holds nothing");
    assert_eq!(d.alloc_coherent(0, 4096, Coherence::Snooped), Err(DmaRefusal::ZeroLength));
    assert_eq!(d.requests.len(), 3, "a request refused by the rule never reaches the host");
    assert_eq!(d.requests[1], (4097, 4096, Coherence::NonSnooped));
}

#[test]
fn a_level_line_stays_masked_until_acked_and_an_edge_line_does_not() {
    let mut l = FakeIrq::new(46, true);
    l.script.extend([Ok(IrqWait::Fired), Ok(IrqWait::Fired)]);
    assert_eq!(l.wait(10), Ok(IrqWait::Fired));
    assert!(l.masked());
    assert_eq!(l.wait(10), Ok(IrqWait::TimedOut), "masked: the second interrupt is not heard");
    assert_eq!(l.waits_while_masked, 1);
    assert_eq!(l.ack(), Ok(()));
    assert_eq!(l.wait(0), Ok(IrqWait::Fired), "after the ack the pending one arrives");
    assert_eq!(l.waits, [10, 10, 0]);

    let mut e = FakeIrq::new(0x41, false);
    e.script.extend([Ok(IrqWait::Fired), Ok(IrqWait::Fired), Err(IrqRefusal::NotHeld { line: 0x41 })]);
    assert_eq!(e.wait(5), Ok(IrqWait::Fired));
    assert_eq!(e.wait(5), Ok(IrqWait::Fired), "an edge line is never masked");
    assert_eq!(e.wait(5), Err(IrqRefusal::NotHeld { line: 0x41 }));
    assert_eq!(e.wait(5), Ok(IrqWait::TimedOut), "an exhausted script times out");
    assert_eq!(e.waits_while_masked, 0);
}

// --- the GPU-grade `Dma::alloc` (DmaRequest -> DmaGrant) ---------------------------------------------------------------
use crate::dma::{check_dma_width, DmaGrant, DmaRegion, DmaRequest};

fn req(size: usize, width: u8, budget: usize) -> DmaRequest {
    DmaRequest { size, align: 4096, addr_width: width, cache: Coherence::NonSnooped, budget }
}

#[test]
fn alloc_keeps_cpu_mapping_and_device_addr_apart_and_never_assumes_identity() {
    let mut d = FakeDma::new(0x20_0000);
    let g = d.alloc(req(8192, 39, 1 << 20)).unwrap();
    assert_eq!(g, DmaGrant { cpu_mapping: 0x20_0000 + FakeDma::VIRT_OFFSET, device_addr: 0x20_0000, len: 8192 });
    assert_ne!(g.cpu_mapping, g.device_addr, "the fake maps CPU != device, so a caller that assumed identity fails");
    assert_eq!(d.requests[0], (8192, 4096, Coherence::NonSnooped), "the cache rule reaches the host");
}

#[test]
fn alloc_refuses_over_budget_bad_width_and_a_grant_beyond_the_width_by_name() {
    let mut d = FakeDma::new(0x1000);
    assert_eq!(d.alloc(req(4097, 39, 4096)), Err(DmaRefusal::OverQuota { len: 4097 }), "budget counts whole pages");
    assert_eq!(d.alloc(req(4096, 39, 4096)).map(|g| g.len), Ok(4096), "exactly the budget is allowed");
    for w in [0u8, 11, 65, 255] {
        assert_eq!(d.alloc(req(4096, w, 1 << 20)), Err(DmaRefusal::BadAddrWidth { width: w }));
    }
    assert_eq!(d.alloc(req(0, 39, 1 << 20)), Err(DmaRefusal::ZeroLength));
    let held = d.pages_held();
    // The next page is at 0x2000; a 13-bit device reaches 0x1fff only.
    assert_eq!(
        d.alloc(req(4096, 13, 1 << 20)),
        Err(DmaRefusal::BeyondAddrWidth { device_addr: 0x2000, len: 4096, width: 13 })
    );
    assert_eq!(d.pages_held(), held, "the fake refuses BEFORE taking memory");
}

#[test]
fn width_check_is_exact_at_the_boundary_and_the_default_alloc_applies_it_after_the_host() {
    let r = |phys, len| DmaRegion { virt: 0, phys, len };
    assert_eq!(check_dma_width(&r((1 << 39) - 4096, 4096), 39), Ok(()), "last byte is 2^39 - 1");
    assert!(check_dma_width(&r((1 << 39) - 4096, 4097), 39).is_err(), "one byte over");
    assert!(check_dma_width(&r(1 << 39, 1), 39).is_err());
    assert_eq!(check_dma_width(&r(u64::MAX - 4095, 4096), 64), Ok(()));
    assert!(check_dma_width(&r(u64::MAX - 10, 100), 64).is_err(), "address wrap is refused");
    // A host that only implements alloc_coherent still gets the width check from the default `alloc`.
    struct High;
    impl Dma for High {
        fn alloc_coherent(&mut self, len: usize, _a: usize, _c: Coherence) -> Result<DmaRegion, DmaRefusal> {
            Ok(DmaRegion { virt: 0x1000, phys: 1 << 40, len })
        }
    }
    assert_eq!(
        High.alloc(req(4096, 39, 4096)),
        Err(DmaRefusal::BeyondAddrWidth { device_addr: 1 << 40, len: 4096, width: 39 })
    );
}

#[test]
fn free_is_unsupported_by_default_and_the_fake_frees_exactly_what_it_granted_once() {
    use crate::dma::DmaGrant;
    // a host with only alloc_coherent (the trait default): free says so and frees nothing
    struct NoFree;
    impl Dma for NoFree {
        fn alloc_coherent(&mut self, len: usize, _a: usize, _c: Coherence) -> Result<DmaRegion, DmaRefusal> {
            Ok(DmaRegion { virt: 0x1000, phys: 0x1000, len })
        }
    }
    let g = DmaGrant { cpu_mapping: 0x1000, device_addr: 0x1000, len: 4096 };
    assert_eq!(NoFree.free(g), Err(DmaRefusal::Unsupported));

    let mut d = FakeDma::new(0x10_0000);
    let a = d.alloc(req(8192, 39, 1 << 20)).unwrap();
    let b = d.alloc(req(4096, 39, 1 << 20)).unwrap();
    assert_eq!(d.pages_held(), 3);
    assert_eq!(d.free(DmaGrant { len: 4096, ..a }), Err(DmaRefusal::NotGranted { device_addr: a.device_addr, len: 4096 }), "a part of a grant is not a grant");
    assert_eq!(d.free(DmaGrant { device_addr: a.device_addr + 1, ..a }), Err(DmaRefusal::NotGranted { device_addr: a.device_addr + 1, len: 8192 }));
    assert_eq!(d.pages_held(), 3, "a refused free holds nothing different");
    assert_eq!(d.free(a), Ok(()));
    assert_eq!(d.pages_held(), 1, "the freed grant is no longer held");
    assert_eq!(d.free(a), Err(DmaRefusal::NotGranted { device_addr: a.device_addr, len: 8192 }), "never twice");
    assert_eq!(d.freed, vec![a]);
    d.can_free = false;
    assert_eq!(d.free(b), Err(DmaRefusal::Unsupported), "the fake can pose as a host with no free call");
    assert_eq!(d.pages_held(), 1);
}
