// SPDX-License-Identifier: GPL-2.0-only
//! Literal register vectors from Linux `8250_dw.c`, `8250_lpss.c`, `8250_dwlib.c`, and
//! `include/uapi/linux/serial_reg.h`.
//!
//! Copyright 2011 Picochip, Jamie Iles; Copyright 2013, 2016 Intel Corporation; copyright
//! 1992, 1994 Theodore Ts'o.

use serial_8250_lpss_core::regs::{bits, dw, index, lpss};

#[test]
fn designware_and_lpss_offsets_match_linux_literals() {
    assert_eq!(index::USR, 0x1f); // 8250_dw.c:38
    assert_eq!(dw::DLF, 0xc0); // 8250_dwlib.c:23
    assert_eq!(lpss::PRV_CLK, 0x800); // 8250_lpss.c:39
    assert_eq!(lpss::TX_OVF_INT, 0x820); // 8250_lpss.c:45
}

#[test]
fn standard_indices_match_linux_literals() {
    assert_eq!(
        [
            index::RX,
            index::TX,
            index::IER,
            index::IIR,
            index::FCR,
            index::LCR,
            index::MCR,
            index::LSR,
            index::MSR
        ],
        [0, 0, 1, 2, 2, 3, 4, 5, 6]
    ); // serial_reg.h:19-:20, :22, :31, :50, :105, :128, :141, :152
}

#[test]
fn busy_and_clock_fields_match_linux_literals() {
    assert_eq!(bits::IIR_IID_MASK, 0x0f); // 8250_dw.c:47
    assert_eq!(bits::USR_BUSY, 0x01); // 8250_dw.c:51
    assert_eq!(bits::LCR_SPAR, 0x20); // serial_reg.h:112
    assert_eq!(bits::LCR_DLAB, 0x80); // serial_reg.h:108
    assert_eq!(bits::PRV_CLK_EN, 0x0000_0001); // 8250_lpss.c:40
    assert_eq!(bits::PRV_CLK_M_VAL_SHIFT, 1); // 8250_lpss.c:41
    assert_eq!(bits::PRV_CLK_N_VAL_SHIFT, 16); // 8250_lpss.c:42
    assert_eq!(bits::PRV_CLK_UPDATE, 0x8000_0000); // 8250_lpss.c:43
    assert_eq!(bits::TX_OVF_INT_MASK, 0x0000_0002); // 8250_lpss.c:46
}

#[test]
fn fifo_and_flow_fields_match_linux_literals() {
    assert_eq!(bits::FCR_ENABLE_FIFO, 0x01); // serial_reg.h:51
    assert_eq!(bits::FCR_CLEAR_RCVR, 0x02); // serial_reg.h:52
    assert_eq!(bits::FCR_CLEAR_XMIT, 0x04); // serial_reg.h:53
    assert_eq!(bits::FCR_DMA_SELECT, 0x08); // serial_reg.h:54
    assert_eq!(bits::FCR_TRIGGER_MASK, 0xc0); // serial_reg.h:83
    assert_eq!(bits::MCR_AFE, 0x20); // serial_reg.h:132
}
