// SPDX-License-Identifier: GPL-2.0-only
//! DesignWare USR busy-detect decisions from Linux `8250_dw.c:145-:291` and `:421-:484`.
//!
//! Copyright 2011 Picochip, Jamie Iles; Copyright 2013 Intel Corporation.

use crate::regs::{bits, index};

/// Interrupt identities singled out by the DesignWare handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptKind {
    BusyDetect,
    RxTimeout,
    Other { iid: u8 },
}

/// Decode the low interrupt identity bits exactly as `dw8250_handle_irq` does.
pub fn interrupt_kind(iir: u32) -> InterruptKind {
    let iid = (iir & bits::IIR_IID_MASK) as u8; // 8250_dw.c:432
    if iid as u32 == bits::IIR_BUSY {
        InterruptKind::BusyDetect // 8250_dw.c:444
    } else if iir & bits::IIR_RX_TIMEOUT_MASK == bits::IIR_RX_TIMEOUT {
        InterruptKind::RxTimeout // 8250_dw.c:426
    } else {
        InterruptKind::Other { iid }
    }
}

/// Whether an LCR write took effect. Stick parity is deliberately ignored (`8250_dw.c:253`).
pub fn lcr_write_accepted(requested: u32, observed: u32) -> bool {
    requested & !bits::LCR_SPAR == observed & !bits::LCR_SPAR
}

/// Port of `dw8250_can_skip_reg_write` (`8250_dw.c:279-:291`).
pub fn can_skip_write(
    offset: u32,
    current: u32,
    requested: u32,
    uart_16550_compatible: bool,
) -> bool {
    offset == index::LCR && !uart_16550_compatible && current == requested
}

/// Linux starts with four clear/read attempts (`8250_dw.c:190`).
pub const BUSY_CLEAR_ATTEMPTS: u8 = 4; // 8250_dw.c:190

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Probe,
    Sanity,
    Done,
}

/// Pure state for the USR polling portion of `dw8250_idle_enter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusyDetector {
    phase: Phase,
    attempts: u8,
}

/// What the caller must do after one FIFO-clear/USR sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyStep {
    /// Clear FIFOs again after one frame time (`8250_dw.c:192-:197`).
    RetryAfterFrame,
    /// Perform Linux's final independent USR sanity read (`8250_dw.c:205-:209`).
    CheckUsrAgain,
    /// BUSY is deasserted and divisor/LCR writes may proceed.
    Ready,
}

/// Named busy-state refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyError {
    WrongPhase { operation: &'static str },
    BusyAfterRetries { usr: u32, attempts: u8 },
}

impl BusyDetector {
    /// Start a fresh four-attempt busy-clear sequence.
    pub const fn new() -> Self {
        Self {
            phase: Phase::Probe,
            attempts: 0,
        }
    }

    /// Supply USR read after the caller clears both FIFOs.
    pub fn observe_clear_probe(&mut self, usr: u32) -> Result<BusyStep, BusyError> {
        if self.phase != Phase::Probe {
            return Err(BusyError::WrongPhase {
                operation: "USR clear probe refused: detector is not in probe phase",
            });
        }
        self.attempts += 1;
        if usr & bits::USR_BUSY == 0 || self.attempts == BUSY_CLEAR_ATTEMPTS {
            self.phase = Phase::Sanity;
            Ok(BusyStep::CheckUsrAgain)
        } else {
            Ok(BusyStep::RetryAfterFrame)
        }
    }

    /// Supply Linux's final sanity read, which occurs even if an earlier probe observed idle.
    pub fn sanity_check(&mut self, usr: u32) -> Result<BusyStep, BusyError> {
        if self.phase != Phase::Sanity {
            return Err(BusyError::WrongPhase {
                operation: "USR sanity check refused: clear probes have not completed",
            });
        }
        self.phase = Phase::Done;
        if usr & bits::USR_BUSY != 0 {
            Err(BusyError::BusyAfterRetries {
                usr,
                attempts: self.attempts,
            })
        } else {
            Ok(BusyStep::Ready)
        }
    }
}

impl Default for BusyDetector {
    fn default() -> Self {
        Self::new()
    }
}
