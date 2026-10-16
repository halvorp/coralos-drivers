// SPDX-License-Identifier: GPL-2.0-only
//! The MSI cause decode and the handler's acknowledge/re-enable rules. Linux `pcie/gen1_2/rx.c:1717-1727, :1890-2150`,
//! `pcie/gen1_2/internal.h:827-850, :895-980`.
mod common;
use common::{Op, Rec};
use iwlwifi_pcie_nic_core::irq::*;
use iwlwifi_pcie_trans_core::hw::TransState;

const INT: u32 = 0x008;
const INT_MASK: u32 = 0x00c;
const FH_INT: u32 = 0x010;

fn st(mask: u32) -> TransState {
    TransState { inta_mask: mask, ..TransState::default() }
}

#[test]
fn every_cause_bit_decodes_to_its_own_flag_and_nothing_else() {
    let cases: [(u32, &str); 12] = [
        (1 << 29, "hw_err"),
        (1 << 26, "scd"),
        (1 << 0, "alive"),
        (1 << 2, "reset_done"),
        (1 << 7, "rf_kill"),
        (1 << 6, "ct_kill"),
        (1 << 25, "sw_err"),
        (1 << 1, "wakeup"),
        (1 << 31, "fh_rx"),
        (1 << 3, "sw_rx"),
        (1 << 28, "rx_periodic"),
        (1 << 27, "fh_tx"),
    ];
    for (bit, name) in cases {
        let c = decode(bit);
        let flags = [
            ("hw_err", c.hw_err),
            ("scd", c.scd),
            ("alive", c.alive),
            ("reset_done", c.reset_done),
            ("rf_kill", c.rf_kill),
            ("ct_kill", c.ct_kill),
            ("sw_err", c.sw_err),
            ("wakeup", c.wakeup),
            ("fh_rx", c.fh_rx),
            ("sw_rx", c.sw_rx),
            ("rx_periodic", c.rx_periodic),
            ("fh_tx", c.fh_tx),
        ];
        let set: Vec<&str> = flags.iter().filter(|f| f.1).map(|f| f.0).collect();
        assert_eq!(set, [name], "bit {bit:#x}");
        assert_eq!(c.unhandled, 0);
        assert_eq!(c.inta, bit);
    }
    assert_eq!(decode(1 << 4).unhandled, 1 << 4, "an undocumented bit is reported as unhandled (rx.c:2123)");
    assert!(decode(1 << 31).wants_rx() && decode(1 << 3).wants_rx() && decode(1 << 28).wants_rx());
    assert!(!decode(1 << 27).wants_rx() && !decode(1 << 0).wants_rx());
}

#[test]
fn the_prologue_masks_acknowledges_with_the_complement_of_the_mask_and_returns_the_causes() {
    let mut io = Rec::default();
    io.csr.insert(INT, 0x8000_0001 | 0x40); // FH_RX | ALIVE | CT_KILL (CT_KILL is not in the ALIVE|FH_RX mask)
    let mask = 0x8000_0001;
    match service(&mut io, &st(mask)) {
        Cause::Some(c) => {
            assert_eq!(c.inta, 0x8000_0001, "inta &= inta_mask (rx.c:1923)");
            assert!(c.fh_rx && c.alive && !c.ct_kill);
        }
        c => panic!("{c:?}"),
    }
    // CSR_INT <- inta | ~inta_mask: also acks the causes the card raises despite the mask (rx.c:1952-1964)
    assert_eq!(io.ops, [Op::CsrW(INT, 0x8000_0001 | !0x8000_0001)]);
}

#[test]
fn nothing_unmasked_is_nothing_and_writes_nothing() {
    let mut io = Rec::default();
    io.csr.insert(INT, 0x40);
    assert_eq!(service(&mut io, &st(0x8000_0001)), Cause::Nothing); // rx.c:1928-1940
    assert!(io.ops.is_empty());
}

#[test]
fn an_all_ones_read_is_a_link_that_is_gone_even_when_the_mask_is_partial() {
    // DEVIATION from rx.c:1943 (which tests the masked value): the raw read is tested.
    let mut io = Rec::default();
    io.csr.insert(INT, 0xFFFF_FFFF);
    assert_eq!(service(&mut io, &st(0xBA00_008F)), Cause::HardwareGone { inta: 0xFFFF_FFFF });
    io.csr.insert(INT, 0xa5a5_a5a3); // iwl_trans_is_hw_error_value: (v & ~0xf) == 0xa5a5a5a0
    assert_eq!(service(&mut io, &st(0xBA00_008F)), Cause::HardwareGone { inta: 0xa5a5_a5a3 });
    assert!(io.ops.is_empty(), "nothing acknowledged on a dead link");
}

#[test]
fn the_rx_arm_acks_the_fh_bits_and_rearms_the_periodic_one_shot_only_for_a_real_rx() {
    let mut io = Rec::default();
    ack_rx(&mut io, &decode((1 << 31) | (1 << 3)));
    // rx.c:2063-2064: CSR_FH_INT_STATUS <- CSR_FH_INT_RX_MASK (0x40030000); :2083-2084 periodic off; :2093-2095 periodic on
    assert_eq!(io.ops, [Op::CsrW(FH_INT, 0x4003_0000), Op::W8(0x005, 0x00), Op::W8(0x005, 0xFF)]);
    let mut io = Rec::default();
    ack_rx(&mut io, &decode(1 << 28)); // the periodic interrupt alone: acked in CSR_INT, left disarmed (a one-shot)
    assert_eq!(io.ops, [Op::CsrW(INT, 1 << 28), Op::W8(0x005, 0x00)]);
}

#[test]
fn the_fh_tx_arm_acks_the_tx_channel_bits() {
    let mut io = Rec::default();
    ack_fh_tx(&mut io);
    assert_eq!(io.ops, [Op::CsrW(FH_INT, 0x3)]); // rx.c:2109
}

#[test]
fn the_mask_helpers_write_the_masks_linux_writes() {
    let mut io = Rec::default();
    let mut s = TransState::default();
    let mut ctl = IrqCtl::default();
    ctl.enable_all(&mut io, &mut s);
    assert_eq!((s.inta_mask, ctl.int_enabled), (0xBA00_008F, true)); // internal.h:895-903
    enable_fw_load_int(&mut io, &mut s);
    assert_eq!(s.inta_mask, 1 << 27); // FH_TX only, internal.h:942-949
    enable_alive_int(&mut io, &mut s);
    assert_eq!(s.inta_mask, (1 << 0) | (1 << 31)); // ALIVE | FH_RX, internal.h:966-979
    ctl.disable_all(&mut io, &mut s);
    assert!(!ctl.int_enabled && s.inta_mask == 0);
    assert_eq!(
        &io.ops[io.ops.len() - 3..],
        [Op::CsrW(INT_MASK, 0), Op::CsrW(INT, 0xFFFF_FFFF), Op::CsrW(FH_INT, 0xFFFF_FFFF)] // internal.h:834-839
    );
}

#[test]
fn the_end_of_the_handler_puts_back_the_mask_the_current_state_wants() {
    let tx = decode(1 << 27);
    let rf = decode(1 << 7);
    let rx = decode(1 << 31);
    let alive = decode(1 << 0);
    // enabled: everything (rx.c:2136-2137)
    let (mut io, mut s, mut ctl) = (Rec::default(), TransState::default(), IrqCtl { int_enabled: true });
    reenable(&mut io, &mut s, &mut ctl, &tx);
    assert_eq!(s.inta_mask, 0xBA00_008F);
    // not enabled, FH_TX handled: the firmware-load mask (rx.c:2138-2140)
    let (mut io, mut s, mut ctl) = (Rec::default(), TransState::default(), IrqCtl::default());
    reenable(&mut io, &mut s, &mut ctl, &tx);
    assert_eq!(s.inta_mask, 1 << 27);
    // not enabled, RF_KILL handled: the rfkill mask (rx.c:2141-2143)
    let (mut io, mut s, mut ctl) = (Rec::default(), TransState::default(), IrqCtl::default());
    reenable(&mut io, &mut s, &mut ctl, &rf);
    assert_eq!(s.inta_mask, 1 << 7);
    // not enabled, ALIVE or FH_RX handled: the alive mask (rx.c:2144-2146)
    for c in [rx, alive] {
        let (mut io, mut s, mut ctl) = (Rec::default(), TransState::default(), IrqCtl::default());
        reenable(&mut io, &mut s, &mut ctl, &c);
        assert_eq!(s.inta_mask, (1 << 0) | (1 << 31));
    }
    // not enabled and nothing of the above handled: left alone
    let (mut io, mut s, mut ctl) = (Rec::default(), st(0x1234), IrqCtl::default());
    reenable(&mut io, &mut s, &mut ctl, &decode(1 << 1));
    assert_eq!((s.inta_mask, io.ops.len()), (0x1234, 0));
}
