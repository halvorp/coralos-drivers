// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for the update-in-progress stable-read protocol.
//!
//! Ported from `drivers/rtc/rtc-mc146818-lib.c:11-98`; copyright the Linux RTC authors.

use rtc_mc146818_core::uip::{
    ReadOutcome, ReadRequest, StableRead, UipRefusal, RECHECKS_PER_MS, RECHECK_DELAY_US,
    RTC_WORK_TIMEOUT_MS, SLOW_READ_WARNING, SLOW_READ_WARNING_MS,
};

/// rtc-mc146818-lib.c:11-13,80-82,94-97.
#[test]
fn protocol_constants_match_linux_literals() {
    assert_eq!(RECHECK_DELAY_US, 100);
    assert_eq!(RECHECKS_PER_MS, 10);
    assert_eq!(SLOW_READ_WARNING_MS, 100);
    assert_eq!(
        SLOW_READ_WARNING,
        "Reading current time from RTC took around %li ms\n"
    );
    assert_eq!(RTC_WORK_TIMEOUT_MS, 1000);
}

/// rtc-mc146818-lib.c:37-43,49-50,62,74 — seconds surrounds BOTH UIP checks.
#[test]
fn successful_read_requests_linux_order_and_returns_first_seconds() {
    let mut read = StableRead::new(10);
    assert_eq!(read.request(), Some(ReadRequest::Seconds));
    assert_eq!(
        read.supply(42),
        ReadOutcome::Read(ReadRequest::FrequencySelect)
    );
    assert_eq!(read.request(), Some(ReadRequest::FrequencySelect));
    assert_eq!(read.supply(0), ReadOutcome::Read(ReadRequest::Seconds));
    assert_eq!(read.supply(42), ReadOutcome::Capture { seconds: 42 });
    assert_eq!(
        read.request(),
        None,
        "the caller captures the remaining RTC registers here"
    );
    assert_eq!(
        read.capture_complete(),
        ReadOutcome::Read(ReadRequest::FrequencySelect)
    );
    assert_eq!(read.supply(0), ReadOutcome::Read(ReadRequest::Seconds));
    assert_eq!(
        read.supply(42),
        ReadOutcome::Stable {
            seconds: 42,
            elapsed_ms: 0,
            warn_slow: false
        }
    );
}

/// rtc-mc146818-lib.c:43-47 and :62-65 — either UIP observation restarts from seconds.
#[test]
fn either_uip_check_restarts_the_protocol() {
    let mut first = StableRead::new(10);
    first.supply(1);
    assert_eq!(first.supply(0x80), ReadOutcome::Read(ReadRequest::Seconds));
    assert_eq!(first.elapsed_ms(), 0, "one retry is i/10 == 0ms");

    let mut second = StableRead::new(10);
    second.supply(1);
    second.supply(0);
    assert_eq!(second.supply(1), ReadOutcome::Capture { seconds: 1 });
    second.capture_complete();
    assert_eq!(second.supply(0x80), ReadOutcome::Read(ReadRequest::Seconds));

    // Mutating only the second UIP branch must also fail, rather than being hidden by the first.
    let mut second_clear = StableRead::new(10);
    second_clear.supply(1);
    second_clear.supply(0);
    second_clear.supply(1);
    second_clear.capture_complete();
    assert_eq!(
        second_clear.supply(0),
        ReadOutcome::Read(ReadRequest::Seconds)
    );
    assert_eq!(
        second_clear.supply(1),
        ReadOutcome::Stable {
            seconds: 1,
            elapsed_ms: 0,
            warn_slow: false
        }
    );
}

/// rtc-mc146818-lib.c:49-53 and :68-77 — either changed-seconds validation restarts.
#[test]
fn either_seconds_change_restarts_the_protocol() {
    let mut early = StableRead::new(10);
    early.supply(1);
    early.supply(0);
    assert_eq!(early.supply(2), ReadOutcome::Read(ReadRequest::Seconds));

    let mut late = StableRead::new(10);
    late.supply(1);
    late.supply(0);
    assert_eq!(late.supply(1), ReadOutcome::Capture { seconds: 1 });
    late.capture_complete();
    late.supply(0);
    assert_eq!(late.supply(2), ReadOutcome::Read(ReadRequest::Seconds));

    // Mutating only the final comparison must fail independently of the early comparison.
    let mut stable = StableRead::new(10);
    stable.supply(1);
    stable.supply(0);
    stable.supply(1);
    stable.capture_complete();
    stable.supply(0);
    assert_eq!(
        stable.supply(1),
        ReadOutcome::Stable {
            seconds: 1,
            elapsed_ms: 0,
            warn_slow: false
        }
    );
}

/// rtc-mc146818-lib.c:29,85-86 — `i / 10 < timeout`; timeout names budget and attempts.
#[test]
fn timeout_refusal_names_what_failed_and_its_budget() {
    let mut read = StableRead::new(1);
    for _ in 0..10 {
        assert!(matches!(
            read.supply(1),
            ReadOutcome::Read(ReadRequest::FrequencySelect)
        ));
        assert_eq!(read.supply(0x80), ReadOutcome::Read(ReadRequest::Seconds));
    }
    assert_eq!(read.elapsed_ms(), 1);
    assert_eq!(
        read.supply(1),
        ReadOutcome::Refused(UipRefusal::StableReadTimedOut {
            timeout_ms: 1,
            attempts: 10
        })
    );
}

/// rtc-mc146818-lib.c:80-82 warns once integer elapsed time reaches 100ms.
#[test]
fn slow_success_carries_linux_warning_decision() {
    let mut read = StableRead::new(101);
    for _ in 0..1000 {
        read.supply(9);
        read.supply(0x80);
    }
    assert_eq!(read.elapsed_ms(), 100);
    read.supply(9);
    read.supply(0);
    assert_eq!(read.supply(9), ReadOutcome::Capture { seconds: 9 });
    read.capture_complete();
    read.supply(0);
    assert_eq!(
        read.supply(9),
        ReadOutcome::Stable {
            seconds: 9,
            elapsed_ms: 100,
            warn_slow: true
        }
    );
}
