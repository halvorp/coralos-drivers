// SPDX-License-Identifier: GPL-2.0-only
//! USR busy-detect vectors from Linux `8250_dw.c:145-:291` and `:421-:484`.
//!
//! Copyright 2011 Picochip, Jamie Iles; Copyright 2013 Intel Corporation.

use serial_8250_lpss_core::busy::{
    can_skip_write, interrupt_kind, lcr_write_accepted, BusyDetector, BusyError, BusyStep,
    InterruptKind, BUSY_CLEAR_ATTEMPTS,
};

#[test]
fn interrupt_decoder_names_busy_and_timeout() {
    assert_eq!(interrupt_kind(0x07), InterruptKind::BusyDetect); // serial_reg.h:42, 8250_dw.c:444
    assert_eq!(interrupt_kind(0x0c), InterruptKind::RxTimeout); // serial_reg.h:44, 8250_dw.c:426
    assert_eq!(interrupt_kind(0xc1), InterruptKind::Other { iid: 1 }); // low GENMASK(3,0), 8250_dw.c:432
}

#[test]
fn lcr_check_ignores_only_stick_parity() {
    assert!(lcr_write_accepted(0x03, 0x23)); // UART_LCR_SPAR=0x20, serial_reg.h:112
    assert!(!lcr_write_accepted(0x03, 0x02));
    assert!(!lcr_write_accepted(0x03, 0x43));
}

#[test]
fn only_an_identical_non_compatible_lcr_write_is_skipped() {
    assert!(can_skip_write(3, 0x83, 0x83, false)); // UART_LCR=3, serial_reg.h:105
    assert!(!can_skip_write(3, 0x83, 0x03, false));
    assert!(!can_skip_write(2, 0x83, 0x83, false));
    assert!(!can_skip_write(3, 0x83, 0x83, true));
}

#[test]
fn busy_sequence_retries_four_times_then_requires_sanity_read() {
    assert_eq!(BUSY_CLEAR_ATTEMPTS, 4); // 8250_dw.c:190
    let mut state = BusyDetector::new();
    for _ in 0..3 {
        assert_eq!(
            state.observe_clear_probe(0x01),
            Ok(BusyStep::RetryAfterFrame)
        ); // USR_BUSY BIT(0), :193
    }
    assert_eq!(state.observe_clear_probe(0x01), Ok(BusyStep::CheckUsrAgain));
    assert_eq!(
        state.sanity_check(0x01),
        Err(BusyError::BusyAfterRetries {
            usr: 0x01,
            attempts: 4
        })
    ); // 8250_dw.c:206-:209 returns -EBUSY
}

#[test]
fn even_an_idle_probe_is_followed_by_the_final_sanity_read() {
    let mut state = BusyDetector::new();
    assert_eq!(state.observe_clear_probe(0x00), Ok(BusyStep::CheckUsrAgain));
    assert_eq!(state.sanity_check(0x00), Ok(BusyStep::Ready));
}

#[test]
fn wrong_state_refuses_by_name() {
    let mut state = BusyDetector::new();
    assert_eq!(
        state.sanity_check(0),
        Err(BusyError::WrongPhase {
            operation: "USR sanity check refused: clear probes have not completed"
        })
    );
    assert_eq!(state.observe_clear_probe(0), Ok(BusyStep::CheckUsrAgain));
    assert_eq!(state.sanity_check(0), Ok(BusyStep::Ready));
    assert_eq!(
        state.observe_clear_probe(0),
        Err(BusyError::WrongPhase {
            operation: "USR clear probe refused: detector is not in probe phase"
        })
    );
}
