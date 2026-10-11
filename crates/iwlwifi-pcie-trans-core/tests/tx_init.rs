// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for tx_init's register program, tx_stop and rx_stop.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/tx.c`, `pcie/gen1_2/rx.c` and
//! `iwl-scd.h`. Copyright (C) 2003-2014, 2018-2025 Intel Corporation, and the Linux iwlwifi authors.
//! Register numbers are LITERALS.

mod common;
use common::{Op, Sim};
use iwlwifi_pcie_trans_core::tx_init::*;

fn rings() -> Vec<u64> {
    (0..31u64).map(|q| 0x1_2340_0000 + q * 0x1000).collect()
}

#[test]
fn constants_are_linux() {
    assert_eq!(TX_STOP_FH_TIMEOUT_US, 5000); // tx.c:594
    assert_eq!(RX_STOP_TIMEOUT_US, 1000); // rx.c:155
}

#[test]
fn tx_init_writes_fifos_off_keep_warm_31_ring_bases_then_scd_gp_ctrl() {
    let mut s = Sim::new();
    s.prph.insert(0xa02c00 + 0x1a8, 0x0000_0100); // an unrelated SCD_GP_CTRL bit survives
    tx_init(&mut s, 0x0_8765_4320, &rings()).unwrap();
    let mut want = vec![
        Op::P(0xa02c10, 0),                   // SCD_TXFACT ← 0, iwl-scd.h:41-44; tx.c:955
        Op::W(0x197C, 0x0876_5432),           // FH_KW_MEM_ADDR_REG ← kw >> 4, tx.c:958-959
    ];
    let cbbc = |q: u64| match q {
        0..=15 => 0x19D0 + 4 * q,
        16..=19 => 0x1BF0 + 4 * (q - 16),
        _ => 0x1B20 + 4 * (q - 20),
    };
    for q in 0..31u64 {
        want.push(Op::W(cbbc(q) as u32, ((0x1_2340_0000 + q * 0x1000) >> 8) as u32)); // tx.c:987-988
    }
    want.push(Op::P(0xa02da8, 0x0004_0100)); // AUTO_ACTIVE_MODE (bit 18), tx.c:991
    want.push(Op::P(0xa02da8, 0x0004_0101)); // ENABLE_31_QUEUES (bit 0): 31 > 20, tx.c:992-994
    assert_eq!(s.ops, want);
}

#[test]
fn tx_init_refuses_a_bad_address_or_count_before_any_write() {
    let mut s = Sim::new();
    assert_eq!(tx_init(&mut s, 0x1000, &rings()[..30]), Err(TxInitError::QueueCount { got: 30, need: 31 }));
    assert_eq!(tx_init(&mut s, 0x1008, &rings()), Err(TxInitError::KeepWarm { dma: 0x1008 }));
    assert_eq!(tx_init(&mut s, 1 << 36, &rings()), Err(TxInitError::KeepWarm { dma: 1 << 36 }));
    let mut r = rings();
    r[20] = 0x1_0000_0080;
    assert_eq!(tx_init(&mut s, 0x1000, &r), Err(TxInitError::TfdRing { queue: 20, dma: 0x1_0000_0080 }));
    r[20] = 1 << 40;
    assert_eq!(tx_init(&mut s, 0x1000, &r), Err(TxInitError::TfdRing { queue: 20, dma: 1 << 40 }));
    assert!(s.ops.is_empty(), "nothing programmed");
}

#[test]
fn tx_stop_closes_the_fifos_and_all_8_channels_then_waits_for_idle() {
    let mut s = Sim::new();
    assert_eq!(tx_stop(&mut s), Ok(()));
    assert_eq!(
        s.ops,
        vec![
            Op::P(0xa02c10, 0), // tx.c:615
            Op::W(0x1D00, 0), Op::W(0x1D20, 0), Op::W(0x1D40, 0), Op::W(0x1D60, 0),
            Op::W(0x1D80, 0), Op::W(0x1DA0, 0), Op::W(0x1DC0, 0), Op::W(0x1DE0, 0), // tx.c:588-591
        ]
    );
    let mut busy = Sim::new();
    busy.tx_status = 0x007F_0000; // channel 7 never idles
    assert_eq!(tx_stop(&mut busy), Err(0x007F_0000));
    assert_eq!(busy.sleeps(10), 500); // 5 ms
}

#[test]
fn rx_stop_turns_rfh_dma_off_and_waits_for_rxf_dma_idle() {
    let mut s = Sim::new();
    assert_eq!(rx_stop(&mut s), Ok(()));
    assert_eq!(s.ops, vec![Op::P(0xA09820, 0)]); // rx.c:153
    let mut busy = Sim::new();
    busy.rfh_status = 0x7FFF_FFFF;
    assert_eq!(rx_stop(&mut busy), Err(0x7FFF_FFFF));
    assert_eq!(busy.sleeps(10), 100); // 1 ms
}
