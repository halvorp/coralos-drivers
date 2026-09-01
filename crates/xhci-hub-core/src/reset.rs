// SPDX-License-Identifier: GPL-2.0-only
//! Cold-reset and warm-reset write sequences as pure state-machine plans.
//!
//! Ported from Linux `drivers/usb/host/xhci-hub.c` and
//! `drivers/usb/host/xhci-port.h`, by Sarah Sharp and the Linux xHCI authors.
//! Original copyright: Copyright (C) 2008 Intel Corp.

use crate::portsc::{self, LinkState, PORT_CAS, PORT_CONNECT, PORT_RESET, PORT_WR};

/// Linux's two root-port reset features (xhci-hub.c:1501-:1518).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetKind {
    Cold,
    Warm,
}

/// A caller-executed reset plan. The crate does not touch hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetPlan {
    pub write: u32,
    pub flush_posted_write: bool,
}

/// Plan SetPortFeature(RESET) or SetPortFeature(BH_PORT_RESET) (xhci-hub.c:1501-:1518).
pub const fn plan_reset(portsc: u32, kind: ResetKind) -> ResetPlan {
    let base = portsc::neutral(portsc);
    let write = match kind {
        ResetKind::Cold => base | PORT_RESET,
        ResetKind::Warm => base | PORT_WR,
    };
    ResetPlan { write, flush_posted_write: true }
}

/// The missing-CAS warm-reset quirk action (xhci-hub.c:1843-:1868).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingCasAction {
    NoReset,
    WarmReset(ResetPlan),
}

/// If a disconnected, CAS-clear port is stuck in Polling or Compliance, clear wake/change bits and
/// issue a warm reset (xhci-hub.c:1852-:1868).
pub const fn plan_missing_cas_recovery(portsc: u32) -> MissingCasAction {
    if portsc & (PORT_CONNECT | PORT_CAS) != 0 {
        return MissingCasAction::NoReset;
    }
    let pls = portsc & portsc::PORT_PLS_MASK;
    if pls != LinkState::Polling.bits() && pls != LinkState::Compliance.bits() {
        return MissingCasAction::NoReset;
    }

    // PORT_RWC_BITS at xhci-hub.c:21-:22 plus CEC and PORT_WAKE_BITS at :20.
    let clear = portsc::PORT_CSC | portsc::PORT_PEC | portsc::PORT_WRC | portsc::PORT_OCC
        | portsc::PORT_RC | portsc::PORT_PLC | portsc::PORT_PE | portsc::PORT_CEC
        | portsc::PORT_WKOC_E | portsc::PORT_WKDISC_E | portsc::PORT_WKCONN_E;
    MissingCasAction::WarmReset(ResetPlan {
        write: (portsc & !clear) | PORT_WR,
        flush_posted_write: true,
    })
}
