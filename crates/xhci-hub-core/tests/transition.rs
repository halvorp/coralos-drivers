// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for link-state transitions.

use xhci_hub_core::portsc::*;
use xhci_hub_core::transition::*;

/// xhci-hub.c:1363-:1418 special-cases Disabled, RxDetect, and Compliance before PED checks.
#[test]
fn special_link_requests_follow_linux_ordering() {
    assert_eq!(plan_link_transition(0, LinkState::RxDetect, false),
               Ok(LinkAction::Write { value: 0x0001_00a0 }));
    assert_eq!(plan_link_transition(PORT_CONNECT, LinkState::Compliance, true),
               Err(TransitionError::ComplianceModeRefusedConnectedPort));
    assert_eq!(plan_link_transition(0, LinkState::Compliance, false), Ok(LinkAction::NoAction));
    assert_eq!(plan_link_transition(0, LinkState::Compliance, true),
               Ok(LinkAction::Write { value: 0x0001_0140 }));
    assert_eq!(plan_link_transition(0, LinkState::Disabled, false),
               Ok(LinkAction::Write { value: 0x00fe_0002 }));
}

/// xhci-hub.c:1420-:1490. U0 differs for U1/U2, U3, and Resume/Recovery; U3 has 16 polls of
/// usleep_range(4000, 8000), while U0 completion uses 500ms.
#[test]
fn ordinary_u_state_transitions_are_exact() {
    assert_eq!(plan_link_transition(PORT_PE, LinkState::U0, false), Ok(LinkAction::NoAction));
    assert_eq!(plan_link_transition(PORT_PE | 0x20, LinkState::U0, false),
               Ok(LinkAction::Write { value: 0x0001_0000 }));
    assert_eq!(plan_link_transition(PORT_PE | 0x60, LinkState::U0, false),
               Ok(LinkAction::WriteU0AndWait { value: 0x0001_0000, timeout_ms: 500 }));
    assert_eq!(plan_link_transition(PORT_PE | 0x100, LinkState::U0, false),
               Ok(LinkAction::WaitForU0 { timeout_ms: 500 }));
    assert_eq!(plan_link_transition(PORT_PE | 0x1e0, LinkState::U0, false),
               Ok(LinkAction::WaitForU0 { timeout_ms: 500 }));
    assert_eq!(plan_link_transition(PORT_PE, LinkState::U3, false),
               Ok(LinkAction::StopEndpointsThenWriteU3 {
                   value: 0x0001_0060, retries: 16, poll_min_us: 4000, poll_max_us: 8000,
               }));
}

/// xhci-hub.c:1420-:1429 and :1454-:1457 name the refusal conditions.
#[test]
fn invalid_transitions_are_named_refusals() {
    assert_eq!(plan_link_transition(0, LinkState::U1, false), Err(TransitionError::PortDisabled));
    assert_eq!(plan_link_transition(PORT_PE, LinkState::Test, false),
               Err(TransitionError::RequestedStateAboveU3 { requested: LinkState::Test, maximum: LinkState::U3 }));
    assert_eq!(plan_link_transition(PORT_PE | 0x80, LinkState::U0, false),
               Err(TransitionError::CurrentStateCannotTransitionToU0 { current: LinkState::Disabled }));
}

/// xhci-hub.c:1568-:1594: reset refuses resume; U3 writes Resume, waits USB_RESUME_TIMEOUT (20ms),
/// then writes U0; other states only ring the device.
#[test]
fn usb2_resume_sequence_is_pinned() {
    assert_eq!(plan_usb2_resume(PORT_RESET | PORT_PE | 0x60), Err(TransitionError::PortResetActive));
    assert_eq!(plan_usb2_resume(0x60), Err(TransitionError::PortDisabled));
    assert_eq!(plan_usb2_resume(PORT_PE | 0x60), Ok(Usb2ResumeAction::WriteResumeWaitWriteU0 {
        resume: 0x0001_01e0, wait_ms: 20, u0: 0x0001_0000,
    }));
    assert_eq!(plan_usb2_resume(PORT_PE), Ok(Usb2ResumeAction::RingDevice));
}

/// xhci-hub.c:861-:906: CAS fakes Compliance+Connection, Resume reports U3, and the compliance
/// quirk fakes connection only for Compliance.
#[test]
fn usb3_reported_link_state_hides_internal_resume_and_exposes_cas() {
    assert_eq!(report_usb3_link_state(LinkState::Resume.bits(), false), 0x0060);
    assert_eq!(report_usb3_link_state(PORT_CAS | LinkState::U1.bits(), false), 0x0141);
    assert_eq!(report_usb3_link_state(PORT_CAS | LinkState::Inactive.bits(), false), 0x00c1);
    assert_eq!(report_usb3_link_state(LinkState::Compliance.bits(), true), 0x0141);
    assert_eq!(report_usb3_link_state(LinkState::Compliance.bits(), false), 0x0140);
}
