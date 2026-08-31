// SPDX-License-Identifier: GPL-2.0-only
//! Pure state machine for Linux's update-in-progress stable-read protocol.
//!
//! Ported from Linux `drivers/rtc/rtc-mc146818-lib.c:11-98`; copyright Torsten Duwe, Motorola,
//! and the Linux RTC authors. Register-A's UIP bit comes from `include/linux/mc146818rtc.h:76-80`.
//! No register is accessed here: the caller performs each requested read and feeds the value back.

use crate::registers::UIP;

pub const RECHECK_DELAY_US: u32 = 100; // drivers/rtc/rtc-mc146818-lib.c:11
pub const RECHECKS_PER_MS: u32 = 10; // drivers/rtc/rtc-mc146818-lib.c:12
pub const SLOW_READ_WARNING_MS: u32 = 100; // drivers/rtc/rtc-mc146818-lib.c:80-82
pub const SLOW_READ_WARNING: &str = "Reading current time from RTC took around %li ms\n"; // drivers/rtc/rtc-mc146818-lib.c:81
pub const RTC_WORK_TIMEOUT_MS: u32 = 1000; // drivers/rtc/rtc-mc146818-lib.c:94-97

/// Read requested next by the stable-read state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadRequest {
    Seconds,
    FrequencySelect,
}

/// Result after feeding one requested register value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOutcome {
    /// Perform this read next.
    Read(ReadRequest),
    /// Run the caller's snapshot callback, which may be requested more than once.
    Capture { seconds: u8 },
    /// The initial seconds value is stable around both UIP checks.
    Stable {
        seconds: u8,
        elapsed_ms: u32,
        warn_slow: bool,
    },
    /// No stable window appeared before the caller's timeout.
    Refused(UipRefusal),
}

/// A timeout that names what refused and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UipRefusal {
    StableReadTimedOut { timeout_ms: u32, attempts: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    FirstSeconds,
    FirstUip,
    RevalidateSeconds,
    Capture,
    SecondUip,
    FinalSeconds,
}

/// Stateful, hardware-free form of `mc146818_avoid_UIP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StableRead {
    timeout_ms: u32,
    attempts: u32,
    seconds: u8,
    phase: Phase,
}

impl StableRead {
    /// Begin with Linux's required ordering: seconds BEFORE the first UIP test.
    pub const fn new(timeout_ms: u32) -> Self {
        Self {
            timeout_ms,
            attempts: 0,
            seconds: 0,
            phase: Phase::FirstSeconds,
        }
    } // drivers/rtc/rtc-mc146818-lib.c:29,37-43

    /// The register read currently required.
    pub const fn request(&self) -> Option<ReadRequest> {
        match self.phase {
            Phase::FirstSeconds | Phase::RevalidateSeconds | Phase::FinalSeconds => {
                Some(ReadRequest::Seconds)
            }
            Phase::FirstUip | Phase::SecondUip => Some(ReadRequest::FrequencySelect),
            Phase::Capture => None,
        }
    }

    /// Mark the caller's snapshot callback complete and request Linux's second UIP check.
    pub fn capture_complete(&mut self) -> ReadOutcome {
        debug_assert_eq!(self.phase, Phase::Capture);
        self.phase = Phase::SecondUip;
        ReadOutcome::Read(ReadRequest::FrequencySelect)
    } // drivers/rtc/rtc-mc146818-lib.c:55-62

    /// Feed the value obtained from `request`; wrong-register input is impossible by construction.
    pub fn supply(&mut self, value: u8) -> ReadOutcome {
        match self.phase {
            Phase::FirstSeconds => {
                if self.elapsed_ms() >= self.timeout_ms {
                    return ReadOutcome::Refused(UipRefusal::StableReadTimedOut {
                        timeout_ms: self.timeout_ms,
                        attempts: self.attempts,
                    });
                } // drivers/rtc/rtc-mc146818-lib.c:29,85-86
                self.seconds = value;
                self.phase = Phase::FirstUip;
                ReadOutcome::Read(ReadRequest::FrequencySelect)
            }
            Phase::FirstUip => {
                if value & UIP != 0 {
                    self.retry()
                } else {
                    self.phase = Phase::RevalidateSeconds;
                    ReadOutcome::Read(ReadRequest::Seconds)
                }
            } // drivers/rtc/rtc-mc146818-lib.c:43-47
            Phase::RevalidateSeconds => {
                if value != self.seconds {
                    self.retry()
                } else {
                    self.phase = Phase::Capture;
                    ReadOutcome::Capture {
                        seconds: self.seconds,
                    }
                }
            } // drivers/rtc/rtc-mc146818-lib.c:49-56
            Phase::Capture => ReadOutcome::Capture {
                seconds: self.seconds,
            },
            Phase::SecondUip => {
                if value & UIP != 0 {
                    self.retry()
                } else {
                    self.phase = Phase::FinalSeconds;
                    ReadOutcome::Read(ReadRequest::Seconds)
                }
            } // drivers/rtc/rtc-mc146818-lib.c:58-66
            Phase::FinalSeconds => {
                if value != self.seconds {
                    self.retry()
                } else {
                    let elapsed_ms = self.elapsed_ms();
                    ReadOutcome::Stable {
                        seconds: self.seconds,
                        elapsed_ms,
                        warn_slow: elapsed_ms >= SLOW_READ_WARNING_MS,
                    }
                }
            } // drivers/rtc/rtc-mc146818-lib.c:68-84
        }
    }

    /// Linux's integer elapsed-time expression `i / 10`.
    pub const fn elapsed_ms(&self) -> u32 {
        self.attempts / RECHECKS_PER_MS
    } // drivers/rtc/rtc-mc146818-lib.c:12-13

    fn retry(&mut self) -> ReadOutcome {
        self.attempts = self.attempts.saturating_add(1);
        self.phase = Phase::FirstSeconds;
        ReadOutcome::Read(ReadRequest::Seconds)
    }
}
