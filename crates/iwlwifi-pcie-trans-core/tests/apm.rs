// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for PCI ownership, APM up/down, SW reset and the poll helpers.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/trans.c` and
//! `drivers/net/wireless/intel/iwlwifi/iwl-io.c`. Copyright (C) 2007-2015, 2018-2024 Intel
//! Corporation, and the Linux iwlwifi authors. Register numbers are LITERALS.

mod common;
use common::{Op, Sim};
use iwlwifi_pcie_trans_core::apm::*;
use iwlwifi_pcie_trans_core::io::*;

#[test]
fn timing_constants_are_linux() {
    assert_eq!(IWL_POLL_INTERVAL, 10); // iwl-io.c:48
    assert_eq!(HW_READY_TIMEOUT, 50); // trans.c:522
    assert_eq!(PREPARE_LINK_PM_SETTLE_US, 1000); // trans.c:561
    assert_eq!(PREPARE_ROUNDS, 10); // trans.c:563
    assert_eq!(PREPARE_RETRY_US, 200); // trans.c:589-590
    assert_eq!(PREPARE_ROUND_BUDGET_US, 150_000); // trans.c:591
    assert_eq!(PREPARE_ROUND_GAP_US, 25_000); // trans.c:592
    assert_eq!(MAC_CLOCK_READY_TIMEOUT_US, 25_000); // trans.c:4354
    assert_eq!(STOP_MASTER_TIMEOUT_US, 100); // trans.c:431
    assert_eq!(SW_RESET_SETTLE_US, 5000); // trans.c:88
}

#[test]
fn polls_check_every_10_us_until_the_timeout() {
    let mut s = Sim::new();
    assert!(!poll_bits(&mut s, 0x100, 0x1, 50)); // iwl-io.c:55-60: checks at t=0,10,..,40
    assert_eq!(s.sleeps(10), 5);
    s.csr.insert(0x100, 0x3);
    assert!(poll_bits(&mut s, 0x100, 0x3, 50));
    assert_eq!(s.sleeps(10), 5, "a ready register is not slept on");
    // iwl_poll_prph_bit compares (read & mask) == (bits & mask). // iwl-io.c:156-169
    s.prph.insert(0xA0_0000, 0x0000_00F0);
    assert!(poll_prph_bit(&mut s, 0xA0_0000, 0x10, 0x30, 20) == false);
    assert!(poll_prph_bit(&mut s, 0xA0_0000, 0x30, 0x30, 20));
    assert!(poll_prph_bit(&mut s, 0xA0_0000, 0x0F, 0xF0, 20) == false);
}

#[test]
fn a_free_card_is_claimed_once_with_os_alive() {
    let mut s = Sim::new();
    assert_eq!(prepare_card_hw(&mut s), Ok(()));
    // trans.c:529-538: OWN_SET, its poll (no sleep: it stuck), OS_ALIVE.
    assert_eq!(s.ops, vec![Op::W(0x000, 0x0040_0000), Op::W(0x088, 0x20)]);
}

#[test]
fn an_owned_card_walks_the_ladder_and_a_never_free_one_is_refused_with_the_count() {
    let mut late = Sim::new();
    late.own_after = 3;
    assert_eq!(prepare_card_hw(&mut late), Ok(()));
    let w = late.writes();
    assert_eq!(w[1], Op::W(0x250, 0x8000_0000)); // trans.c:559-560
    assert_eq!(w[2], Op::W(0x000, 0x0800_0000)); // WAKE_ME, trans.c:567-568
    assert_eq!(*w.last().unwrap(), Op::W(0x088, 0x20));
    assert!(late.pos(Op::Sleep(1000)) < late.pos(Op::W(0x000, 0x0800_0000)));
    assert_eq!(late.sleeps(200), 1);

    let mut owned = Sim::new();
    owned.own_after = u32::MAX;
    assert_eq!(prepare_card_hw(&mut owned), Err(ApmError::CardNotReady { attempts: 1 + 10 * 750 }));
    assert_eq!(owned.sleeps(25_000), 10);
    assert!(!owned.ops.contains(&Op::W(0x088, 0x20)), "never announces OS_ALIVE");
}

#[test]
fn apm_init_sets_the_four_bits_then_init_done_then_waits_for_the_clock() {
    let mut s = Sim::new();
    assert_eq!(apm_init_9000(&mut s), Ok(()));
    assert_eq!(
        s.writes(),
        vec![
            Op::W(0x100, 0x0080_0000), // L1A_NO_L0S_RX, trans.c:251-252
            Op::W(0x240, 0xFFFF_0000), // HPET, trans.c:255
            Op::W(0x000, 0x0008_0000), // HAP_WAKE, trans.c:261-262
            Op::W(0x03C, 0x2),         // L0S_DISABLED, trans.c:214
            Op::W(0x024, 0x0800_0004), // INIT_DONE (radio bit preserved), trans.c:4341-4342
        ]
    );
    // No PRPH traffic: apmg_not_supported (cfg/9000.c:42).
    assert!(!s.ops.iter().any(|o| matches!(o, Op::P(..))));

    let mut asleep = Sim::new();
    asleep.clock_follows_init_done = false;
    assert_eq!(apm_init_9000(&mut asleep), Err(ApmError::MacClockNotReady { gp_cntrl: 0x0800_0004 }));
    assert_eq!(asleep.sleeps(10), 2500); // 25 ms of 10 µs polls
}

#[test]
fn sw_reset_settles_5ms_then_reclaims_only_when_asked() {
    let mut s = Sim::new();
    assert_eq!(sw_reset(&mut s, false), Ok(()));
    assert_eq!(s.ops, vec![Op::W(0x020, 0x80), Op::Sleep(5000)]); // trans.c:86-88
    let mut r = Sim::new();
    assert_eq!(sw_reset(&mut r, true), Ok(()));
    assert_eq!(r.ops, vec![Op::W(0x020, 0x80), Op::Sleep(5000), Op::W(0x000, 0x0040_0000), Op::W(0x088, 0x20)]);
}

#[test]
fn apm_stop_stops_the_master_resets_and_clears_init_done() {
    let mut s = Sim::new();
    s.csr.insert(0x024, 0x0800_0005); // INIT_DONE | MAC_CLOCK_READY, radio on
    assert!(apm_stop(&mut s));
    assert_eq!(
        s.ops,
        vec![
            Op::W(0x020, 0x200),        // STOP_MASTER, trans.c:428 (MASTER_DISABLED follows)
            Op::W(0x020, 0x300 | 0x80), // SW_RESET on top, trans.c:86-87
            Op::Sleep(5000),
            Op::W(0x024, 0x0800_0001),  // INIT_DONE cleared, trans.c:482
        ]
    );
    let mut stuck = Sim::new();
    stuck.master_follows_stop = false;
    assert!(!apm_stop(&mut stuck), "the stop-master timeout is reported");
    assert_eq!(stuck.sleeps(10), 10); // 100 µs
}
