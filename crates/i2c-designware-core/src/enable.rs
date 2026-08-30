// SPDX-License-Identifier: GPL-2.0-only
//! Taking the controller down — the ordering and the waits that make a disable actually disable.
//!
//! Ported from Linux `drivers/i2c/busses/`:
//!   * `__i2c_dw_disable` (i2c-designware-common.c:637-:678)
//!   * `__i2c_dw_disable_nowait` (i2c-designware-core.h:370-:374)
//!   * `DW_IC_ABORT_TIMEOUT_US` (i2c-designware-common.c:39)
//!
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.
//!
//! A disable is not one register write. If the controller is holding the bus mid-transaction,
//! writing ENABLE=0 leaves SCL held low and the bus wedged for every other device on it. Linux
//! aborts first, and the abort has an ordering requirement that reads like a formality and is not.

/// `DW_IC_ABORT_TIMEOUT_US` (i2c-designware-common.c:39) — the poll interval while waiting for the
/// ABORT bit to self-clear. The total budget is TEN times this (:672).
pub const ABORT_TIMEOUT_US: u32 = 10;
/// The abort poll runs for `10 * DW_IC_ABORT_TIMEOUT_US` (i2c-designware-common.c:672).
pub const ABORT_TOTAL_US: u32 = 10 * ABORT_TIMEOUT_US;
/// `int timeout = 100` (i2c-designware-common.c:643) — how many times the disable loop retries.
pub const DISABLE_ATTEMPTS: u32 = 100;
/// `usleep_range(25, 250)` between disable attempts (i2c-designware-common.c:675).
pub const DISABLE_POLL_MIN_US: u32 = 25;
pub const DISABLE_POLL_MAX_US: u32 = 250;

/// Whether the controller is mid-transaction and must be ABORTED before it can be disabled.
///
/// i2c-designware-common.c:652 — `(raw_intr_stats & DW_IC_INTR_MST_ON_HOLD) || (ic_stats &
/// DW_IC_STATUS_MASTER_HOLD_TX_FIFO_EMPTY)`. EITHER condition means the master is holding the bus.
/// Checking only one leaves the other case to wedge SCL low for every device on the bus.
pub fn abort_needed(raw_intr_stat: u32, status: u32) -> bool {
    raw_intr_stat & crate::regs::bits::INTR_MST_ON_HOLD != 0
        || status & crate::regs::bits::STATUS_MASTER_HOLD_TX_FIFO_EMPTY != 0
}

/// The wait Linux performs after enabling, before setting ABORT: "10 times the signaling period of
/// the highest I2C transfer supported by the driver" (i2c-designware-common.c:645), i.e.
/// `DIV_ROUND_CLOSEST_ULL(10 * MICRO, bus_freq_hz)` microseconds.
///
/// Linux's own comment states the expected answer for 400 kHz — 25 us — which is what the vector
/// checks against, rather than against this function's output.
pub fn enable_settle_us(bus_freq_hz: u32) -> u32 {
    if bus_freq_hz == 0 {
        return 0;
    }
    let ten_micro = 10 * crate::timing::MICRO;
    ((ten_micro + bus_freq_hz as u64 / 2) / bus_freq_hz as u64) as u32
}

/// What a disable must do next, given what the controller currently reports.
///
/// Modelled as a decision rather than a loop so the ORDERING can be asserted without a clock: the
/// sequence is the contract, and the sequence is what a port gets wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisableStep {
    /// The controller is holding the bus but is DISABLED. It must be ENABLED first, then settled,
    /// before ABORT can be set.
    ///
    /// "Set ENABLE bit before setting ABORT" (i2c-designware-common.c:653). This reads like a
    /// formality and is not: ABORT written to a disabled controller does nothing, the abort poll
    /// then times out, and the driver proceeds to disable a controller that is still holding SCL
    /// low — wedging the bus for every other device while reporting only a timeout in the log.
    EnableThenSettle { settle_us: u32 },
    /// The controller is enabled and holding: set ABORT and poll for it to self-clear.
    SetAbort,
    /// Nothing is being held: write ENABLE = 0 and poll ENABLE_STATUS.
    DisableNow,
}

/// The step to take, from a single sample of the three registers Linux reads first
/// (i2c-designware-common.c:648-:650: RAW_INTR_STAT, STATUS, ENABLE).
pub fn next_step(raw_intr_stat: u32, status: u32, enable: u32, bus_freq_hz: u32) -> DisableStep {
    if !abort_needed(raw_intr_stat, status) {
        return DisableStep::DisableNow;
    }
    if enable & crate::regs::bits::ENABLE_ENABLE == 0 {
        DisableStep::EnableThenSettle { settle_us: enable_settle_us(bus_freq_hz) }
    } else {
        DisableStep::SetAbort
    }
}

/// Whether the disable has taken effect, from an ENABLE_STATUS read.
///
/// i2c-designware-common.c:665 — `if (!(status & 1)) return;`. Linux's comment is load-bearing:
/// "The enable status register may be unimplemented, but in that case this test reads zero and
/// exits the loop." So a controller that does not implement the register reports success
/// immediately, BY DESIGN. That is a deliberate choice, not an oversight, and a port that "fixes"
/// it by treating an all-zero read as suspicious would hang on exactly the hardware Linux
/// accommodates.
pub fn disable_took_effect(enable_status: u32) -> bool {
    enable_status & 1 == 0
}
