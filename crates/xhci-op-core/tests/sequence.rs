// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors proving halt-before-reset and interrupt-before-start ordering.
//!
//! Ported from Linux `drivers/usb/host/xhci.c`, `drivers/usb/host/xhci.h`, and
//! `drivers/usb/host/xhci-ext-caps.h`.
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

use xhci_op_core::sequence::*;

#[test]
fn all_nine_sequence_states_are_pinned_by_name() {
    // xhci.c:101-239,595-624. Frozen independently from enum iteration (there is none).
    assert_eq!(CONTROLLER_STATE_NAMES.len(), 9);
    assert_eq!(
        CONTROLLER_STATE_NAMES,
        [
            "Running",
            "HaltRequested",
            "HaltObserved",
            "ResetRequested",
            "ResetCleared",
            "Ready",
            "InterruptsEnabled",
            "StartRequested",
            "Started",
        ]
    );
}

#[test]
fn the_complete_linux_order_reaches_started() {
    // xhci.c:5482-5494 proves halt then reset; :222-235 proves HCRST then CNR; :603-621 proves
    // interrupts then Run/Stop; :167 observes HCHalted clear.
    let s = request_halt(ControllerState::Running).unwrap();
    assert_eq!(s, ControllerState::HaltRequested);
    let s = observe_halted(s, 0x1).unwrap();
    assert_eq!(s, ControllerState::HaltObserved);
    let s = request_reset(s).unwrap();
    assert_eq!(s, ControllerState::ResetRequested);
    let s = observe_reset_cleared(s, 0x0).unwrap();
    assert_eq!(s, ControllerState::ResetCleared);
    let s = observe_ready(s, 0x0).unwrap();
    assert_eq!(s, ControllerState::Ready);
    let s = enable_interrupts(s).unwrap();
    assert_eq!(s, ControllerState::InterruptsEnabled);
    let s = request_start(s).unwrap();
    assert_eq!(s, ControllerState::StartRequested);
    assert_eq!(observe_started(s, 0x0), Ok(ControllerState::Started));
}

#[test]
fn reset_is_refused_until_halt_was_requested_and_observed_in_order() {
    // THE SILENT BUG: xhci.c:202-205 refuses reset unless halted, while xhci.c:5482-5494 orders
    // xhci_halt before xhci_reset. A raw HALTED bit passed directly to reset is not enough here.
    assert_eq!(
        request_reset(ControllerState::Running),
        Err(SequenceError::ResetRequestRefused {
            state: ControllerState::Running,
            required: ControllerState::HaltObserved,
        })
    );
    let requested = request_halt(ControllerState::Running).unwrap();
    assert_eq!(
        request_reset(requested),
        Err(SequenceError::ResetRequestRefused {
            state: ControllerState::HaltRequested,
            required: ControllerState::HaltObserved,
        })
    );
    let observed = observe_halted(requested, 0x1).unwrap();
    assert_eq!(request_reset(observed), Ok(ControllerState::ResetRequested));
}

#[test]
fn each_observation_names_the_value_and_required_bit_on_refusal() {
    // xhci.c:133-135,222,235,167.
    assert_eq!(
        observe_halted(ControllerState::HaltRequested, 0x100),
        Err(SequenceError::HaltNotObserved {
            status: 0x100,
            required_mask: 0x1
        })
    );
    assert_eq!(
        observe_reset_cleared(ControllerState::ResetRequested, 0x102),
        Err(SequenceError::ResetStillAsserted {
            command: 0x102,
            asserted_mask: 0x2
        })
    );
    assert_eq!(
        observe_ready(ControllerState::ResetCleared, 0x900),
        Err(SequenceError::ControllerStillNotReady {
            status: 0x900,
            asserted_mask: 0x800
        })
    );
    assert_eq!(
        observe_started(ControllerState::StartRequested, 0x101),
        Err(SequenceError::StartNotObserved {
            status: 0x101,
            forbidden_mask: 0x1
        })
    );
}

#[test]
fn inaccessible_all_ones_is_named_and_wrong_observations_are_refused() {
    // xhci.c:195-199 and xhci_handshake :89-91.
    assert_eq!(
        observe_halted(ControllerState::HaltRequested, 0xffff_ffff),
        Err(SequenceError::ControllerInaccessible {
            register: "USBSTS",
            value: 0xffff_ffff
        })
    );
    assert_eq!(
        observe_reset_cleared(ControllerState::ResetRequested, 0xffff_ffff),
        Err(SequenceError::ControllerInaccessible {
            register: "USBCMD",
            value: 0xffff_ffff
        })
    );
    assert_eq!(
        observe_halted(ControllerState::Running, 1),
        Err(SequenceError::ObservationRefused {
            state: ControllerState::Running,
            observation: Observation::Status(1),
        })
    );
}

#[test]
fn interrupts_are_required_before_start() {
    // xhci.c:603-621 explicitly enables both interrupt levels before xhci_start.
    assert_eq!(
        request_start(ControllerState::Ready),
        Err(SequenceError::StartRequestRefused {
            state: ControllerState::Ready,
            required: ControllerState::InterruptsEnabled,
        })
    );
    assert_eq!(
        enable_interrupts(ControllerState::ResetCleared),
        Err(SequenceError::InterruptEnableRefused {
            state: ControllerState::ResetCleared,
            required: ControllerState::Ready,
        })
    );
    let enabled = enable_interrupts(ControllerState::Ready).unwrap();
    assert_eq!(request_start(enabled), Ok(ControllerState::StartRequested));
}

#[test]
fn halt_request_itself_has_a_named_order_refusal() {
    assert_eq!(
        request_halt(ControllerState::Ready),
        Err(SequenceError::HaltRequestRefused {
            state: ControllerState::Ready,
            required: ControllerState::Running,
        })
    );
}

#[test]
fn linux_wait_and_delay_literals_are_pinned() {
    // xhci-ext-caps.h:12; xhci.h:151-152; xhci.c:212-220.
    assert_eq!(MAX_HALT_US, 32_000);
    assert_eq!(RESET_LONG_US, 10_000_000);
    assert_eq!(RESET_SHORT_US, 250_000);
    assert_eq!(INTEL_POST_RESET_WRITE_DELAY_US, 1_000);
}
