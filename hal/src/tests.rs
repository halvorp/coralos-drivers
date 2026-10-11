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
