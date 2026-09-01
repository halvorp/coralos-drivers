// SPDX-License-Identifier: GPL-2.0-only
//! Port link-state transition decisions without waits, commands, or MMIO.
//!
//! Ported from Linux `drivers/usb/host/xhci-hub.c` and
//! `drivers/usb/host/xhci-port.h`, by Sarah Sharp and the Linux xHCI authors.
//! Original copyright: Copyright (C) 2008 Intel Corp.

use core::fmt;

use crate::portsc::{self, LinkState, PORT_CAS, PORT_CONNECT, PORT_PE, PORT_RESET};

/// Linux's named actions while setting a SuperSpeed port link state (xhci-hub.c:1360-:1490).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkAction {
    /// No write is required.
    NoAction,
    /// Write a link-state request and do not wait for PLC.
    Write { value: u32 },
    /// Write U0 and wait for the U3-exit completion (xhci-hub.c:1446-:1465).
    WriteU0AndWait { value: u32, timeout_ms: u16 },
    /// Resume/Recovery already initiated U0; only wait (xhci-hub.c:1433-:1449).
    WaitForU0 { timeout_ms: u16 },
    /// Stop endpoints before writing U3 (xhci-hub.c:1469-:1489).
    StopEndpointsThenWriteU3 { value: u32, retries: u8, poll_min_us: u16, poll_max_us: u16 },
}

/// Named transition refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    PortDisabled,
    PortResetActive,
    RequestedStateAboveU3 { requested: LinkState, maximum: LinkState },
    CurrentStateCannotTransitionToU0 { current: LinkState },
    ComplianceModeRefusedConnectedPort,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PortDisabled => f.write_str("link-state transition refused because PORTSC.PED is clear"),
            Self::PortResetActive => f.write_str("link-state transition refused because PORTSC.PR is set"),
            Self::RequestedStateAboveU3 { requested, maximum } => write!(f, "link-state transition to {requested:?} exceeds maximum ordinary state {maximum:?}"),
            Self::CurrentStateCannotTransitionToU0 { current } => write!(f, "U0 transition refused from non-resumable state {current:?}"),
            Self::ComplianceModeRefusedConnectedPort => f.write_str("compliance-mode transition refused because PORTSC.CCS is set"),
        }
    }
}

/// Plan Linux's SetPortFeature(LINK_STATE) path (xhci-hub.c:1360-:1490).
///
/// `cte_supported` is HCCPARAMS2.CTC. Special Disabled, RxDetect and Compliance requests precede
/// the ordinary `PED` and `requested <= U3` checks exactly as they do in Linux.
pub fn plan_link_transition(portsc: u32, requested: LinkState, cte_supported: bool) -> Result<LinkAction, TransitionError> {
    if requested == LinkState::Disabled {
        // xhci-hub.c:1363-:1374: clear every change bit before disabling via PED RW1C.
        let changes = portsc::PORT_CSC | portsc::PORT_PEC | portsc::PORT_WRC | portsc::PORT_OCC
            | portsc::PORT_RC | portsc::PORT_PLC | portsc::PORT_CEC;
        return Ok(LinkAction::Write { value: portsc::neutral(portsc) | changes | PORT_PE });
    }
    if requested == LinkState::RxDetect {
        return Ok(LinkAction::Write { value: portsc::set_link_state(portsc, requested) });
    }
    if requested == LinkState::Compliance {
        if !cte_supported {
            return Ok(LinkAction::NoAction);
        }
        if portsc & PORT_CONNECT != 0 {
            return Err(TransitionError::ComplianceModeRefusedConnectedPort);
        }
        return Ok(LinkAction::Write { value: portsc::set_link_state(portsc, requested) });
    }
    if portsc & PORT_PE == 0 {
        return Err(TransitionError::PortDisabled);
    }
    if requested.bits() > LinkState::U3.bits() {
        return Err(TransitionError::RequestedStateAboveU3 { requested, maximum: LinkState::U3 });
    }

    if requested == LinkState::U0 {
        let current = LinkState::decode(portsc).map_err(|_| TransitionError::CurrentStateCannotTransitionToU0 { current: LinkState::Test })?;
        return match current {
            LinkState::U0 => Ok(LinkAction::NoAction),
            LinkState::U1 | LinkState::U2 => Ok(LinkAction::Write { value: portsc::set_link_state(portsc, LinkState::U0) }),
            LinkState::U3 => Ok(LinkAction::WriteU0AndWait { value: portsc::set_link_state(portsc, LinkState::U0), timeout_ms: 500 }),
            LinkState::Resume | LinkState::Recovery => Ok(LinkAction::WaitForU0 { timeout_ms: 500 }),
            current => Err(TransitionError::CurrentStateCannotTransitionToU0 { current }),
        };
    }

    if requested == LinkState::U3 {
        return Ok(LinkAction::StopEndpointsThenWriteU3 {
            value: portsc::set_link_state(portsc, LinkState::U3),
            retries: 16,
            poll_min_us: 4000,
            poll_max_us: 8000,
        });
    }

    Ok(LinkAction::Write { value: portsc::set_link_state(portsc, requested) })
}

/// USB2 clear-suspend sequencing (xhci-hub.c:1568-:1594).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usb2ResumeAction {
    RingDevice,
    WriteResumeWaitWriteU0 { resume: u32, wait_ms: u16, u0: u32 },
}

/// Plan ClearPortFeature(SUSPEND) for USB2 (xhci-hub.c:1568-:1594).
pub fn plan_usb2_resume(portsc: u32) -> Result<Usb2ResumeAction, TransitionError> {
    if portsc & PORT_RESET != 0 {
        return Err(TransitionError::PortResetActive);
    }
    if LinkState::decode(portsc) == Ok(LinkState::U3) {
        if portsc & PORT_PE == 0 {
            return Err(TransitionError::PortDisabled);
        }
        return Ok(Usb2ResumeAction::WriteResumeWaitWriteU0 {
            resume: portsc::set_link_state(portsc, LinkState::Resume),
            wait_ms: 20, // USB_RESUME_TIMEOUT used at xhci-hub.c:1582
            u0: portsc::set_link_state(portsc, LinkState::U0),
        });
    }
    Ok(Usb2ResumeAction::RingDevice)
}

/// Translate the internal xHCI link state for the USB3 hub status word (xhci-hub.c:855-:907).
pub fn report_usb3_link_state(portsc: u32, compliance_mode_quirk: bool) -> u16 {
    let mut pls = (portsc & portsc::PORT_PLS_MASK) as u16;
    if portsc & PORT_CAS != 0 {
        if pls != LinkState::Compliance.bits() as u16 && pls != LinkState::Inactive.bits() as u16 {
            pls = LinkState::Compliance.bits() as u16;
        }
        pls | 0x0001 // USB_PORT_STAT_CONNECTION, ch11.h:123
    } else if pls == LinkState::Resume.bits() as u16 {
        0x0060 // USB_SS_PORT_LS_U3, ch11.h:159
    } else {
        if compliance_mode_quirk && pls == LinkState::Compliance.bits() as u16 {
            pls |= 0x0001; // USB_PORT_STAT_CONNECTION, ch11.h:123
        }
        pls
    }
}
