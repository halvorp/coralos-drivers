// SPDX-License-Identifier: GPL-2.0-only
//! FIFO and MCR automatic-flow-control encoding from Linux `include/uapi/linux/serial_reg.h`,
//! selected for LPSS by `8250_lpss.c:343`.
//!
//! Copyright 2016 Intel Corporation; copyright 1992, 1994 Theodore Ts'o.

use crate::regs::bits;

/// The four 16550 receive trigger encodings (`serial_reg.h:84-:87`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxTrigger {
    One,
    Four,
    Eight,
    Fourteen,
}

/// Frozen names for the four trigger states Linux defines.
pub const RX_TRIGGER_STATES: [(&str, RxTrigger); 4] = [
    ("TRIGGER_1", RxTrigger::One),
    ("TRIGGER_4", RxTrigger::Four),
    ("TRIGGER_8", RxTrigger::Eight),
    ("TRIGGER_14", RxTrigger::Fourteen),
]; // serial_reg.h:84-:87

/// Inputs to the 8250 FIFO Control Register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FifoControl {
    pub enabled: bool,
    pub clear_receiver: bool,
    pub clear_transmitter: bool,
    pub dma_mode: bool,
    pub rx_trigger: RxTrigger,
}

/// Encode an FCR byte from Linux's literal fields (`serial_reg.h:50-:87`).
pub fn encode_fcr(control: FifoControl) -> u8 {
    let mut value = match control.rx_trigger {
        RxTrigger::One => bits::FCR_TRIGGER_1,
        RxTrigger::Four => bits::FCR_TRIGGER_4,
        RxTrigger::Eight => bits::FCR_TRIGGER_8,
        RxTrigger::Fourteen => bits::FCR_TRIGGER_14,
    };
    if control.enabled {
        value |= bits::FCR_ENABLE_FIFO;
    }
    if control.clear_receiver {
        value |= bits::FCR_CLEAR_RCVR;
    }
    if control.clear_transmitter {
        value |= bits::FCR_CLEAR_XMIT;
    }
    if control.dma_mode {
        value |= bits::FCR_DMA_SELECT;
    }
    value
}

/// Apply MCR-based RTS/CTS automatic flow control.
///
/// LPSS advertises `UART_CAP_AFE` (`8250_lpss.c:343`); its register bit is `UART_MCR_AFE`
/// (`serial_reg.h:132`). The bit is replaced, not merely ORed, so disabling flow clears stale AFE.
pub fn encode_mcr_auto_flow(mcr: u8, enabled: bool) -> u8 {
    if enabled {
        mcr | bits::MCR_AFE
    } else {
        mcr & !bits::MCR_AFE
    }
}
