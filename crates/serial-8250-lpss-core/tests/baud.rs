// SPDX-License-Identifier: GPL-2.0-only
//! DesignWare divisor vectors from Linux `8250_dwlib.c:71-:94`.
//!
//! Copyright 2011 Picochip, Jamie Iles; Copyright 2013 Intel Corporation.

use serial_8250_lpss_core::baud::{divisor, Divisor, DivisorError};

#[test]
fn exact_divisor_has_no_fraction() {
    assert_eq!(
        divisor(1_843_200, 115_200, 4),
        Ok(Divisor {
            integer: 1,
            fractional: 0
        })
    ); // 8250_dwlib.c:86-:93: 1843200 / (115200 * 16) = 1, remainder 0
}

#[test]
fn fractional_divisor_rounds_to_nearest() {
    assert_eq!(
        divisor(25_000_000, 1_000_000, 4),
        Ok(Divisor {
            integer: 1,
            fractional: 9
        })
    ); // 8250_dwlib.c:86-:93: rem=9,000,000; (9,000,000<<4)/16,000,000=9 exactly
    assert_eq!(
        divisor(26_000_000, 1_000_000, 4),
        Ok(Divisor {
            integer: 1,
            fractional: 10
        })
    ); // :92 uses DIV_ROUND_CLOSEST: 10,000,000*16/16,000,000=10 exactly
    assert_eq!(
        divisor(25_500_000, 1_000_000, 4),
        Ok(Divisor {
            integer: 1,
            fractional: 10
        })
    ); // :92: 9,500,000*16/16,000,000=9.5, which rounds to 10; truncation gives 9
}

#[test]
fn divisor_refusals_name_the_value_and_bound() {
    assert_eq!(
        divisor(100_000_000, 0, 4),
        Err(DivisorError::BaudIsZero { baud: 0 })
    );
    assert_eq!(
        divisor(100_000_000, 115_200, 32),
        Err(DivisorError::FractionWidthOutOfRange {
            dlf_size: 32,
            maximum: 31
        })
    );
    assert_eq!(
        divisor(1_000_000, 115_200, 4),
        Err(DivisorError::UartClockBelowBaudBase {
            uartclk_hz: 1_000_000,
            baud_base_hz: 1_843_200
        })
    );
    assert_eq!(
        divisor(100_000_000, 268_435_456, 4),
        Err(DivisorError::BaudBaseOverflow {
            baud: 268_435_456,
            maximum_baud: 268_435_455
        })
    );
}
