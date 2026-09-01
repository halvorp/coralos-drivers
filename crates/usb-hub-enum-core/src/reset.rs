// SPDX-License-Identifier: GPL-2.0-only
//! Port reset timing and caller-driven wait state machine.
//!
//! Ported from Linux `drivers/usb/core/hub.c`: reset constants (hub.c:2884-:2905),
//! `hub_port_wait_reset` (hub.c:2953-:3046), reset recovery (hub.c:3153-:3165), and initial delay
//! selection (hub.c:4909-:4935).
//!
//! Copyright 1999 Linus Torvalds, Johannes Erdfelt, and Gregory P. Smith.
//! Copyright 2001 Brad Hards and the Linux USB core authors.

use crate::port::{change, status, warm_reset_required};

/// Normal-build reset attempts (hub.c:2892).
pub const PORT_RESET_TRIES: u8 = 5;
/// Root-port initial reset delay, milliseconds (hub.c:2901).
pub const ROOT_RESET_MS: u16 = 60;
/// Normal short reset delay, milliseconds (hub.c:2902).
pub const SHORT_RESET_MS: u16 = 10;
/// Warm/BH reset delay, milliseconds (hub.c:2903).
pub const BH_RESET_MS: u16 = 50;
/// Long retry reset delay, milliseconds (hub.c:2904).
pub const LONG_RESET_MS: u16 = 200;
/// Reset-completion timeout, milliseconds (hub.c:2905).
pub const RESET_TIMEOUT_MS: u16 = 800;
/// Minimum TRSTRCY after success (hub.c:3158-:3160).
pub const RESET_RECOVERY_MS: u16 = 50;
/// Extra recovery for `USB_QUIRK_HUB_SLOW_RESET` (hub.c:3161-:3163).
pub const SLOW_HUB_EXTRA_RECOVERY_MS: u16 = 100;
/// Fast-enumeration recovery range (hub.c:3155-:3156).
pub const FAST_ENUM_RECOVERY_MIN_US: u16 = 10_000;
pub const FAST_ENUM_RECOVERY_MAX_US: u16 = 12_000;

/// Why reset completion was refused (hub.c:2999-:3021).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetError {
    ResetTimedOut { elapsed_ms: u16, timeout_ms: u16 },
    WarmResetStillRequired,
    DeviceDisconnected,
    ConnectionBounced,
    PortNotEnabled,
}

/// Caller action after a status sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetAction {
    Wait { delay_ms: u16 },
    Complete,
}

/// Pure counterpart of Linux's `delay_time` and mutable `delay` loop variables (hub.c:2956-:2996).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetWait {
    elapsed_ms: u16,
    delay_ms: u16,
    superspeed_hub: bool,
    warm_marked: bool,
}

impl ResetWait {
    /// Begin waiting after reset has been issued. The first action is Linux's initial sleep
    /// (hub.c:2961-:2965).
    pub const fn new(initial_delay_ms: u16, superspeed_hub: bool, warm_marked: bool) -> Self {
        Self { elapsed_ms: 0, delay_ms: initial_delay_ms, superspeed_hub, warm_marked }
    }

    pub const fn next_delay_ms(self) -> u16 {
        self.delay_ms
    }

    pub const fn elapsed_ms(self) -> u16 {
        self.elapsed_ms
    }

    /// Record that the caller waited the requested interval and then read these words.
    pub fn sample(
        &mut self,
        portstatus: u16,
        portchange: u16,
    ) -> Result<ResetAction, ResetError> {
        let complete_and_connected =
            portstatus & status::RESET == 0 && portstatus & status::CONNECTION != 0;

        if !complete_and_connected {
            // This comparison uses C's for-loop counter BEFORE its increment (hub.c:2961-:2963,
            // :2990-:2992). Consequently Linux performs three 10 ms waits before switching, then
            // increments the loop budget by the newly selected 200 ms delay.
            if self.elapsed_ms >= 2 * SHORT_RESET_MS {
                self.delay_ms = LONG_RESET_MS;
            }
            self.elapsed_ms = self.elapsed_ms.saturating_add(self.delay_ms);
            if self.elapsed_ms < RESET_TIMEOUT_MS {
                return Ok(ResetAction::Wait { delay_ms: self.delay_ms });
            }
        }

        if portstatus & status::RESET != 0 {
            return Err(ResetError::ResetTimedOut {
                elapsed_ms: self.elapsed_ms,
                timeout_ms: RESET_TIMEOUT_MS,
            });
        }
        if warm_reset_required(self.superspeed_hub, self.warm_marked, portstatus) {
            return Err(ResetError::WarmResetStillRequired);
        }
        if portstatus & status::CONNECTION == 0 {
            return Err(ResetError::DeviceDisconnected);
        }
        if !self.superspeed_hub && portchange & change::CONNECTION != 0 {
            return Err(ResetError::ConnectionBounced);
        }
        if portstatus & status::ENABLE == 0 {
            return Err(ResetError::PortNotEnabled);
        }
        Ok(ResetAction::Complete)
    }
}

/// Linux's initial reset-delay policy (hub.c:4909, :4923-:4935). Low speed overrides root-port
/// timing, exactly as the later check does in C.
pub fn initial_reset_delay_ms(root_port: bool, low_speed: bool) -> u16 {
    if low_speed {
        LONG_RESET_MS
    } else if root_port {
        ROOT_RESET_MS
    } else {
        SHORT_RESET_MS
    }
}

/// Post-reset recovery delay (hub.c:3153-:3165). Fast enumeration has its own microsecond range.
pub fn reset_recovery_ms(slow_hub: bool) -> u16 {
    RESET_RECOVERY_MS + if slow_hub { SLOW_HUB_EXTRA_RECOVERY_MS } else { 0 }
}
