// SPDX-License-Identifier: GPL-2.0-only
//! Enumeration policy vectors from Linux hub.c. Original copyright: Linus Torvalds,
//! Johannes Erdfelt, Gregory P. Smith, Brad Hards, and the Linux USB core authors.

use usb_hub_enum_core::policy::*;
use usb_hub_enum_core::port::UsbSpeed;

/// hub.c:2893-:2899, :4880, :5032, :5074-:5107, :5504-:5506.
#[test]
fn every_enumeration_count_and_wait_is_a_linux_literal() {
    assert_eq!(SET_ADDRESS_TRIES, 2);
    assert_eq!(GET_DESCRIPTOR_TRIES, 2);
    assert_eq!(GET_MAXPACKET0_TRIES, 3);
    assert_eq!(PORT_INIT_TRIES, 4);
    assert_eq!(DETECT_DISCONNECT_TRIES, 5);
    assert_eq!(GET_DESCRIPTOR_BUFSIZE, 64);
    assert_eq!(DESCRIPTOR_RETRY_DELAY_MS, 100);
    assert_eq!(SET_ADDRESS_RETRY_DELAY_MS, 200);
    assert_eq!(SET_ADDRESS_SETTLE_MS, 10);
    assert_eq!(QUIRK_DELAY_INIT_MS, 2_000);
}

/// hub.c:2907-:2930: `(PORT_INIT_TRIES + 1) / 2` is integer division, so attempt 2 switches.
#[test]
fn scheme_selection_pins_all_four_outer_attempts() {
    let got = [0, 1, 2, 3].map(|retry| {
        use_new_scheme(UsbSpeed::Full, retry, false, false, true)
    });
    assert_eq!(got.len(), 4);
    assert_eq!(got, [true, true, false, false]);
    let old_first = [0, 1, 2, 3].map(|retry| {
        use_new_scheme(UsbSpeed::Full, retry, true, false, true)
    });
    assert_eq!(old_first, [false, false, true, true]);
    assert!(!use_new_scheme(UsbSpeed::Super, 0, false, false, true));
    assert!(!use_new_scheme(UsbSpeed::SuperPlus, 3, true, false, true));
    assert!(!use_new_scheme(UsbSpeed::Full, 0, false, true, false));
}

/// hub.c:3209-:3229 permits two unsuccessful attempts and names the refusal here.
#[test]
fn early_stop_refusal_is_named() {
    assert_eq!(early_stop_refusal(false, true, 99), None);
    assert_eq!(early_stop_refusal(true, false, 0), None);
    assert_eq!(early_stop_refusal(true, false, 1), None);
    assert_eq!(
        early_stop_refusal(true, false, 2),
        Some("port early-stop permits only two unsuccessful attempts")
    );
    assert_eq!(
        early_stop_refusal(true, true, 0),
        Some("port early-stop is already ignoring events")
    );
}

/// hub.c:5458-:5611: four attempts, no retry for ENOTCONN/ENOTSUPP, power-cycle after attempt 1.
#[test]
fn failed_attempt_policy_pins_each_attempt_and_terminal_error() {
    assert_eq!(after_failed_attempt(0, EnumerationFailure::Other), RetryAction::Retry);
    assert_eq!(
        after_failed_attempt(1, EnumerationFailure::Other),
        RetryAction::PowerCycleThenRetry { off_wait_multiplier: 2, on_wait_multiplier: 1 }
    );
    assert_eq!(after_failed_attempt(2, EnumerationFailure::Other), RetryAction::Retry);
    assert_eq!(after_failed_attempt(3, EnumerationFailure::Other), RetryAction::Exhausted);
    assert_eq!(
        after_failed_attempt(0, EnumerationFailure::DeviceNotConnected),
        RetryAction::Stop { reason: EnumerationFailure::DeviceNotConnected }
    );
    assert_eq!(
        after_failed_attempt(0, EnumerationFailure::OperationNotSupported),
        RetryAction::Stop { reason: EnumerationFailure::OperationNotSupported }
    );
    assert_eq!(after_failed_attempt(0, EnumerationFailure::HubOrDeviceGone), RetryAction::Retry);
}

/// hub.c:4947-:4949 permits unknown discovery and SS-to-SSP only.
#[test]
fn speed_change_policy_matches_reset_rule() {
    assert!(reset_speed_is_acceptable(UsbSpeed::Unknown, UsbSpeed::High));
    assert!(reset_speed_is_acceptable(UsbSpeed::Full, UsbSpeed::Full));
    assert!(reset_speed_is_acceptable(UsbSpeed::Super, UsbSpeed::SuperPlus));
    assert!(!reset_speed_is_acceptable(UsbSpeed::Full, UsbSpeed::High));
    assert!(!reset_speed_is_acceptable(UsbSpeed::SuperPlus, UsbSpeed::Super));
}

/// hub.c:4958-:4976 has five handled speed cases and refuses unknown/default.
#[test]
fn initial_ep0_guesses_are_literal_vectors() {
    let got = [UsbSpeed::Low, UsbSpeed::Full, UsbSpeed::High, UsbSpeed::Super,
               UsbSpeed::SuperPlus, UsbSpeed::Unknown, UsbSpeed::Wireless]
        .map(initial_ep0_max_packet);
    assert_eq!(got.len(), 7);
    assert_eq!(got, [Some(8), Some(64), Some(64), Some(512), Some(512), None, None]);
}
