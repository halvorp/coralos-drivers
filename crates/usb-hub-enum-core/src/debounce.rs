// SPDX-License-Identifier: GPL-2.0-only
//! Connection debounce timing and state machine.
//!
//! Ported from Linux `drivers/usb/core/hub.c`: timing definitions (hub.c:138-:140) and
//! `hub_port_debounce` (hub.c:4681-:4737).
//!
//! Copyright 1999 Linus Torvalds, Johannes Erdfelt, and Gregory P. Smith.
//! Copyright 2001 Brad Hards and the Linux USB core authors.

use crate::port::{change, status};

/// Maximum debounce time in milliseconds (hub.c:138).
pub const DEBOUNCE_TIMEOUT_MS: u16 = 2_000;
/// Status sampling interval in milliseconds (hub.c:139).
pub const DEBOUNCE_STEP_MS: u16 = 25;
/// Required unchanged interval in milliseconds (hub.c:140).
pub const DEBOUNCE_STABLE_MS: u16 = 100;

/// Named debounce refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceError {
    ConnectionDidNotStabilize { elapsed_ms: u16, required_stable_ms: u16 },
}

/// What the caller does after feeding one caller-supplied status sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceAction {
    /// Clear `USB_PORT_FEAT_C_CONNECTION`, then wait before sampling again (hub.c:4721-:4728).
    ClearConnectionChangeThenWait { wait_ms: u16 },
    /// No change bit needs clearing; wait before sampling again.
    Wait { wait_ms: u16 },
    /// The connection has remained acceptable and unchanged for 100 ms.
    Stable { portstatus: u16, elapsed_ms: u16 },
}

/// Pure state corresponding to Linux's `connection`, `total_time`, and `stable_time` locals
/// (hub.c:4698-:4702).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Debouncer {
    connection: Option<bool>,
    elapsed_ms: u16,
    stable_ms: u16,
    must_be_connected: bool,
}

impl Debouncer {
    /// Start a debounce run. `must_be_connected` has Linux's hub.c:4711-:4713 meaning.
    pub const fn new(must_be_connected: bool) -> Self {
        Self { connection: None, elapsed_ms: 0, stable_ms: 0, must_be_connected }
    }

    pub const fn elapsed_ms(self) -> u16 {
        self.elapsed_ms
    }

    pub const fn stable_ms(self) -> u16 {
        self.stable_ms
    }

    /// Consume one status/change sample. The crate never clears a feature or sleeps itself.
    pub fn sample(
        &mut self,
        portstatus: u16,
        portchange: u16,
    ) -> Result<DebounceAction, DebounceError> {
        let connected = portstatus & status::CONNECTION != 0;
        let connection_changed = portchange & change::CONNECTION != 0;

        if !connection_changed && self.connection == Some(connected) {
            if !self.must_be_connected || connected {
                self.stable_ms += DEBOUNCE_STEP_MS;
            }
            if self.stable_ms >= DEBOUNCE_STABLE_MS {
                return Ok(DebounceAction::Stable {
                    portstatus,
                    elapsed_ms: self.elapsed_ms,
                });
            }
        } else {
            self.stable_ms = 0;
            self.connection = Some(connected);
        }

        if self.elapsed_ms >= DEBOUNCE_TIMEOUT_MS {
            return Err(DebounceError::ConnectionDidNotStabilize {
                elapsed_ms: self.elapsed_ms,
                required_stable_ms: DEBOUNCE_STABLE_MS,
            });
        }
        self.elapsed_ms += DEBOUNCE_STEP_MS;
        if connection_changed {
            Ok(DebounceAction::ClearConnectionChangeThenWait { wait_ms: DEBOUNCE_STEP_MS })
        } else {
            Ok(DebounceAction::Wait { wait_ms: DEBOUNCE_STEP_MS })
        }
    }
}
