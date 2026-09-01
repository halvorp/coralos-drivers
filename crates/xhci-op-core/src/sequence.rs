// SPDX-License-Identifier: GPL-2.0-only
//! Halt, reset, readiness, interrupt-enable, and start ordering, ported from Linux
//! `drivers/usb/host/xhci.c:101-:239, :595-:624, :5482-:5494`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.
//!
//! The state machine makes the halt-before-reset requirement structural: observing individual bits
//! out of order cannot accidentally grant permission to reset.

use crate::registers::{command, status};

pub const MAX_HALT_US: u64 = 32_000; // xhci-ext-caps.h:12; used by xhci.c:133-:135
pub const RESET_LONG_US: u64 = 10_000_000; // xhci.h:151
pub const RESET_SHORT_US: u64 = 250_000; // xhci.h:152
pub const INTEL_POST_RESET_WRITE_DELAY_US: u32 = 1_000; // xhci.c:212-:220

/// Ordered controller bring-up state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerState {
    Running,
    HaltRequested,
    HaltObserved,
    ResetRequested,
    ResetCleared,
    Ready,
    InterruptsEnabled,
    StartRequested,
    Started,
}

/// Observation supplied after a requested action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    Status(u32),
    Command(u32),
}

/// Names of every ordered state in [`ControllerState`].
pub const CONTROLLER_STATE_NAMES: [&str; 9] = [
    "Running",
    "HaltRequested",
    "HaltObserved",
    "ResetRequested",
    "ResetCleared",
    "Ready",
    "InterruptsEnabled",
    "StartRequested",
    "Started",
]; // xhci.c:101-239, :595-624

/// A sequence refusal that says what operation was refused and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceError {
    HaltRequestRefused {
        state: ControllerState,
        required: ControllerState,
    },
    ResetRequestRefused {
        state: ControllerState,
        required: ControllerState,
    },
    InterruptEnableRefused {
        state: ControllerState,
        required: ControllerState,
    },
    StartRequestRefused {
        state: ControllerState,
        required: ControllerState,
    },
    ObservationRefused {
        state: ControllerState,
        observation: Observation,
    },
    ControllerInaccessible {
        register: &'static str,
        value: u32,
    },
    HaltNotObserved {
        status: u32,
        required_mask: u32,
    },
    ResetStillAsserted {
        command: u32,
        asserted_mask: u32,
    },
    ControllerStillNotReady {
        status: u32,
        asserted_mask: u32,
    },
    StartNotObserved {
        status: u32,
        forbidden_mask: u32,
    },
}

/// Request halt from the running state. This must precede an observed halt and reset
/// (`xhci_halt`, xhci.c:127-:143; `xhci_gen_setup`, xhci.c:5482-:5494).
pub const fn request_halt(state: ControllerState) -> Result<ControllerState, SequenceError> {
    match state {
        ControllerState::Running => Ok(ControllerState::HaltRequested),
        _ => Err(SequenceError::HaltRequestRefused {
            state,
            required: ControllerState::Running,
        }),
    }
}

/// Observe USBSTS.HCHalted after halt was requested (`xhci_halt`, xhci.c:133-:143).
pub const fn observe_halted(
    state: ControllerState,
    status_word: u32,
) -> Result<ControllerState, SequenceError> {
    if !matches!(state, ControllerState::HaltRequested) {
        return Err(SequenceError::ObservationRefused {
            state,
            observation: Observation::Status(status_word),
        });
    }
    if status_word == u32::MAX {
        return Err(SequenceError::ControllerInaccessible {
            register: "USBSTS",
            value: status_word,
        });
    }
    if status_word & status::HALTED == 0 {
        return Err(SequenceError::HaltNotObserved {
            status: status_word,
            required_mask: status::HALTED,
        });
    }
    Ok(ControllerState::HaltObserved)
}

/// Request reset only after halt was actually observed. This is stricter than checking a status
/// bit at call time: it proves the ordering that prevents a one-device-then-wedge controller
/// (`xhci_reset`, xhci.c:182-:210; `xhci_gen_setup`, xhci.c:5482-:5494).
pub const fn request_reset(state: ControllerState) -> Result<ControllerState, SequenceError> {
    match state {
        ControllerState::HaltObserved => Ok(ControllerState::ResetRequested),
        _ => Err(SequenceError::ResetRequestRefused {
            state,
            required: ControllerState::HaltObserved,
        }),
    }
}

/// Observe HCRST self-clear (`xhci_reset`, xhci.c:222-:224).
pub const fn observe_reset_cleared(
    state: ControllerState,
    command_word: u32,
) -> Result<ControllerState, SequenceError> {
    if !matches!(state, ControllerState::ResetRequested) {
        return Err(SequenceError::ObservationRefused {
            state,
            observation: Observation::Command(command_word),
        });
    }
    if command_word == u32::MAX {
        return Err(SequenceError::ControllerInaccessible {
            register: "USBCMD",
            value: command_word,
        });
    }
    if command_word & command::RESET != 0 {
        return Err(SequenceError::ResetStillAsserted {
            command: command_word,
            asserted_mask: command::RESET,
        });
    }
    Ok(ControllerState::ResetCleared)
}

/// Observe USBSTS.CNR clear before allowing any further operational-register programming or
/// doorbells (`xhci_reset`, xhci.c:226-:235).
pub const fn observe_ready(
    state: ControllerState,
    status_word: u32,
) -> Result<ControllerState, SequenceError> {
    if !matches!(state, ControllerState::ResetCleared) {
        return Err(SequenceError::ObservationRefused {
            state,
            observation: Observation::Status(status_word),
        });
    }
    if status_word == u32::MAX {
        return Err(SequenceError::ControllerInaccessible {
            register: "USBSTS",
            value: status_word,
        });
    }
    if status_word & status::CONTROLLER_NOT_READY != 0 {
        return Err(SequenceError::ControllerStillNotReady {
            status: status_word,
            asserted_mask: status::CONTROLLER_NOT_READY,
        });
    }
    Ok(ControllerState::Ready)
}

/// Record that USBCMD.INTE and the primary interrupter were enabled before Run/Stop. Linux cites
/// xHCI sections 4.2 and 5.5.2 for this order (`xhci_run_finished`, xhci.c:603-:615).
pub const fn enable_interrupts(state: ControllerState) -> Result<ControllerState, SequenceError> {
    match state {
        ControllerState::Ready => Ok(ControllerState::InterruptsEnabled),
        _ => Err(SequenceError::InterruptEnableRefused {
            state,
            required: ControllerState::Ready,
        }),
    }
}

/// Request Run/Stop only after interrupt enable (`xhci_run_finished`, xhci.c:603-:621).
pub const fn request_start(state: ControllerState) -> Result<ControllerState, SequenceError> {
    match state {
        ControllerState::InterruptsEnabled => Ok(ControllerState::StartRequested),
        _ => Err(SequenceError::StartRequestRefused {
            state,
            required: ControllerState::InterruptsEnabled,
        }),
    }
}

/// Observe HCHalted clear to prove the controller started (`xhci_start`, xhci.c:149-:176).
pub const fn observe_started(
    state: ControllerState,
    status_word: u32,
) -> Result<ControllerState, SequenceError> {
    if !matches!(state, ControllerState::StartRequested) {
        return Err(SequenceError::ObservationRefused {
            state,
            observation: Observation::Status(status_word),
        });
    }
    if status_word == u32::MAX {
        return Err(SequenceError::ControllerInaccessible {
            register: "USBSTS",
            value: status_word,
        });
    }
    if status_word & status::HALTED != 0 {
        return Err(SequenceError::StartNotObserved {
            status: status_word,
            forbidden_mask: status::HALTED,
        });
    }
    Ok(ControllerState::Started)
}
