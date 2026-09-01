// SPDX-License-Identifier: GPL-2.0-only
//! Enumeration scheme choice and failed-attempt retry policy.
//!
//! Ported from Linux `drivers/usb/core/hub.c`: retry constants and `use_new_scheme`
//! (hub.c:2884-:2930), early stop (hub.c:3192-:3229), descriptor/address waits
//! (hub.c:5032, :5074-:5107), and outer retry/power-cycle policy (hub.c:5458-:5619).
//!
//! Copyright 1999 Linus Torvalds, Johannes Erdfelt, and Gregory P. Smith.
//! Copyright 2001 Brad Hards and the Linux USB core authors.

use crate::port::UsbSpeed;

/// Normal-build SET_ADDRESS attempts (hub.c:2893).
pub const SET_ADDRESS_TRIES: u8 = 2;
/// Normal-build short descriptor attempts (hub.c:2894).
pub const GET_DESCRIPTOR_TRIES: u8 = 2;
/// Normal-build bMaxPacketSize0 attempts (hub.c:2895).
pub const GET_MAXPACKET0_TRIES: u8 = 3;
/// Complete port initialization attempts (hub.c:2896).
pub const PORT_INIT_TRIES: u8 = 4;
/// Warm-reset disconnect-detection polls (hub.c:2899).
pub const DETECT_DISCONNECT_TRIES: u8 = 5;
/// Buffer for the new-scheme initial request (hub.c:4880).
pub const GET_DESCRIPTOR_BUFSIZE: u8 = 64;
/// Delay between descriptor-loop retries (hub.c:5032).
pub const DESCRIPTOR_RETRY_DELAY_MS: u16 = 100;
/// Delay between failed SET_ADDRESS attempts (hub.c:5074-:5078).
pub const SET_ADDRESS_RETRY_DELAY_MS: u16 = 200;
/// Delay after successful SET_ADDRESS (hub.c:5104-:5107).
pub const SET_ADDRESS_SETTLE_MS: u16 = 10;
/// `USB_QUIRK_DELAY_INIT` delay (hub.c:5504-:5506).
pub const QUIRK_DELAY_INIT_MS: u16 = 2_000;

/// Errors whose Linux errno values terminate retries (hub.c:5601-:5602).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumerationFailure {
    DeviceNotConnected,
    OperationNotSupported,
    HubOrDeviceGone,
    Other,
}

/// Decision after one failed outer attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryAction {
    Stop { reason: EnumerationFailure },
    Retry,
    PowerCycleThenRetry { off_wait_multiplier: u8, on_wait_multiplier: u8 },
    Exhausted,
}

/// Linux's Windows-like new/old initialization scheme selection (hub.c:2907-:2930).
pub fn use_new_scheme(
    speed: UsbSpeed,
    retry: u8,
    old_scheme_first: bool,
    port_forces_old_scheme: bool,
    use_both_schemes: bool,
) -> bool {
    if speed >= UsbSpeed::Super {
        return false;
    }
    let old_first = port_forces_old_scheme || old_scheme_first;
    if use_both_schemes && retry >= (PORT_INIT_TRIES + 1) / 2 {
        old_first
    } else {
        !old_first
    }
}

/// Whether the early-stop port attribute refuses this attempt (hub.c:3209-:3229).
pub fn early_stop_refusal(early_stop: bool, ignore_event: bool, retries: u8) -> Option<&'static str> {
    if !early_stop {
        return None;
    }
    if ignore_event {
        return Some("port early-stop is already ignoring events");
    }
    if retries >= 2 {
        return Some("port early-stop permits only two unsuccessful attempts");
    }
    None
}

/// Apply Linux's outer retry policy after attempt `attempt` failed (hub.c:5458, :5601-:5611).
pub fn after_failed_attempt(attempt: u8, failure: EnumerationFailure) -> RetryAction {
    if failure == EnumerationFailure::DeviceNotConnected
        || failure == EnumerationFailure::OperationNotSupported
    {
        return RetryAction::Stop { reason: failure };
    }
    if attempt + 1 >= PORT_INIT_TRIES {
        return RetryAction::Exhausted;
    }
    if attempt == (PORT_INIT_TRIES - 1) / 2 {
        // Linux waits 2 * power-good while off and 1 * power-good after restoring power.
        RetryAction::PowerCycleThenRetry { off_wait_multiplier: 2, on_wait_multiplier: 1 }
    } else {
        RetryAction::Retry
    }
}

/// Whether a reset speed change is accepted (hub.c:4947-:4949): only unknown-to-known and
/// SuperSpeed-to-a-faster SuperSpeedPlus result are legal.
pub fn reset_speed_is_acceptable(old: UsbSpeed, new: UsbSpeed) -> bool {
    old == UsbSpeed::Unknown || old == new || (old == UsbSpeed::Super && new > old)
}

/// Initial endpoint-zero max packet guess selected by speed (hub.c:4958-:4976).
pub fn initial_ep0_max_packet(speed: UsbSpeed) -> Option<u16> {
    match speed {
        UsbSpeed::SuperPlus | UsbSpeed::Super => Some(512),
        UsbSpeed::High | UsbSpeed::Full => Some(64),
        UsbSpeed::Low => Some(8),
        UsbSpeed::Unknown | UsbSpeed::Wireless => None,
    }
}
