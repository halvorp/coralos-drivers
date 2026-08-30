// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the disable contract. Expected values are LINUX literals with file and line.

use i2c_designware_core::enable::{
    abort_needed, disable_took_effect, enable_settle_us, next_step, DisableStep, ABORT_TIMEOUT_US,
    ABORT_TOTAL_US, DISABLE_ATTEMPTS, DISABLE_POLL_MAX_US, DISABLE_POLL_MIN_US,
};
use i2c_designware_core::regs::bits;

/// i2c-designware-common.c:39 (ABORT_TIMEOUT_US), :672 (10x total), :643 (timeout = 100),
/// :675 (usleep_range(25, 250)).
#[test]
fn the_timeout_constants_match_linux() {
    assert_eq!(ABORT_TIMEOUT_US, 10);
    assert_eq!(ABORT_TOTAL_US, 100, "the abort poll runs for 10 * the interval");
    assert_eq!(DISABLE_ATTEMPTS, 100);
    assert_eq!((DISABLE_POLL_MIN_US, DISABLE_POLL_MAX_US), (25, 250));
}

/// i2c-designware-common.c:652 — EITHER condition means the master is holding the bus. Checking
/// only one leaves the other case to wedge SCL low for every device on the bus.
#[test]
fn either_hold_condition_alone_demands_an_abort() {
    assert!(abort_needed(bits::INTR_MST_ON_HOLD, 0), "the interrupt alone is enough");
    assert!(abort_needed(0, bits::STATUS_MASTER_HOLD_TX_FIFO_EMPTY), "the status alone is enough");
    assert!(abort_needed(bits::INTR_MST_ON_HOLD, bits::STATUS_MASTER_HOLD_TX_FIFO_EMPTY));
    assert!(!abort_needed(0, 0));
    // And unrelated bits in either register must not provoke one.
    assert!(!abort_needed(!bits::INTR_MST_ON_HOLD, !bits::STATUS_MASTER_HOLD_TX_FIFO_EMPTY));
}

/// i2c-designware-core.h:115, :134 — the two hold indicators.
#[test]
fn the_hold_bits_match_linux() {
    assert_eq!(bits::INTR_MST_ON_HOLD, 1 << 13);
    assert_eq!(bits::STATUS_MASTER_HOLD_TX_FIFO_EMPTY, 1 << 7);
}

/// i2c-designware-common.c:645 — `DIV_ROUND_CLOSEST_ULL(10 * MICRO, bus_freq_hz)`. Linux's own
/// comment states the answer for 400 kHz, so that is what this checks against.
#[test]
fn the_settle_wait_is_ten_signalling_periods() {
    assert_eq!(enable_settle_us(400_000), 25, "Linux's comment: 'for 400KHz this is 25us'");
    assert_eq!(enable_settle_us(100_000), 100, "standard mode: 10 * 1e6 / 1e5");
    assert_eq!(enable_settle_us(1_000_000), 10, "fast plus");
    // A zero frequency must not divide by zero.
    assert_eq!(enable_settle_us(0), 0);
    // DIV_ROUND_CLOSEST, not truncation — and the three cases above CANNOT tell the difference,
    // because 10e6 divides each of them exactly. A mutation to plain division passed all of them.
    // 600 kHz is chosen precisely because it does not divide: 10e6 / 6e5 = 16.67, which ROUNDS to
    // 17 and TRUNCATES to 16. The frequency need not be a standard bus speed for the arithmetic to
    // be wrong at it.
    assert_eq!(enable_settle_us(600_000), 17, "must round up from 16.67, not truncate to 16");
    assert_ne!(enable_settle_us(600_000), 16);
}

/// THE ORDERING IS THE CONTRACT. i2c-designware-common.c:653 — "Set ENABLE bit before setting
/// ABORT". A controller that is holding the bus while DISABLED must be ENABLED first: ABORT written
/// to a disabled controller does nothing, the poll times out, and the driver then disables a
/// controller still holding SCL low — wedging the bus while logging only a timeout.
#[test]
fn a_disabled_controller_that_is_holding_must_be_enabled_before_abort() {
    let holding = bits::INTR_MST_ON_HOLD;
    assert_eq!(
        next_step(holding, 0, 0, 400_000),
        DisableStep::EnableThenSettle { settle_us: 25 },
        "disabled + holding must enable FIRST, not abort"
    );
    assert_eq!(
        next_step(holding, 0, bits::ENABLE_ENABLE, 400_000),
        DisableStep::SetAbort,
        "already enabled + holding goes straight to abort"
    );
}

/// Nothing held means no abort dance at all — straight to ENABLE = 0.
#[test]
fn a_quiet_controller_is_simply_disabled() {
    assert_eq!(next_step(0, 0, bits::ENABLE_ENABLE, 400_000), DisableStep::DisableNow);
    assert_eq!(next_step(0, 0, 0, 400_000), DisableStep::DisableNow);
}

/// i2c-designware-common.c:661-:666. Linux's comment is load-bearing: "The enable status register
/// may be unimplemented, but in that case this test reads zero and exits the loop." A controller
/// that does not implement the register reports success immediately BY DESIGN — a port that
/// "fixed" this by treating an all-zero read as suspicious would hang on the hardware Linux
/// deliberately accommodates.
#[test]
fn an_unimplemented_status_register_reads_as_disabled_by_design() {
    assert!(disable_took_effect(0), "an unimplemented register reads zero and that means done");
    assert!(!disable_took_effect(1), "bit 0 set means still enabled");
    // ONLY bit 0 is consulted; the rest of the word is not this predicate's business.
    assert!(disable_took_effect(0xffff_fffe));
    assert!(!disable_took_effect(0xffff_ffff));
}
