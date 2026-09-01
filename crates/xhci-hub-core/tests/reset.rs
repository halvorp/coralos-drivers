// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for cold and warm resets.

use xhci_hub_core::portsc::*;
use xhci_hub_core::reset::*;

/// xhci-hub.c:1501-:1518: RESET sets PR; BH_PORT_RESET sets WPR. Both are followed by a read.
#[test]
fn cold_and_warm_reset_use_distinct_linux_bits_and_flush() {
    assert_eq!(plan_reset(PORT_CONNECT | PORT_POWER, ResetKind::Cold), ResetPlan {
        write: 0x0000_0211, flush_posted_write: true,
    });
    assert_eq!(plan_reset(PORT_CONNECT | PORT_POWER, ResetKind::Warm), ResetPlan {
        write: 0x8000_0201, flush_posted_write: true,
    });
}

/// xhci-hub.c:1848-:1868: only disconnected, CAS-clear Polling/Compliance ports get the workaround.
#[test]
fn missing_cas_recovery_only_warm_resets_stuck_ports() {
    assert_eq!(plan_missing_cas_recovery(LinkState::Polling.bits()),
               MissingCasAction::WarmReset(ResetPlan { write: 0x8000_00e0, flush_posted_write: true }));
    assert_eq!(plan_missing_cas_recovery(LinkState::Compliance.bits() | PORT_CSC | PORT_WKOC_E),
               MissingCasAction::WarmReset(ResetPlan { write: 0x8000_0140, flush_posted_write: true }));
    assert_eq!(plan_missing_cas_recovery(PORT_CONNECT | LinkState::Polling.bits()), MissingCasAction::NoReset);
    assert_eq!(plan_missing_cas_recovery(PORT_CAS | LinkState::Polling.bits()), MissingCasAction::NoReset);
    assert_eq!(plan_missing_cas_recovery(LinkState::U3.bits()), MissingCasAction::NoReset);
}

/// xhci-hub.c:21-:22 and :1862-:1865: PED, six change bits, CEC, and all wake enables are cleared
/// before WPR. This mixed vector proves no stale action survives.
#[test]
fn missing_cas_warm_reset_clears_wake_change_and_ped_bits() {
    let input = LinkState::Polling.bits() | PORT_PE | PORT_CSC | PORT_PEC | PORT_WRC | PORT_OCC
        | PORT_RC | PORT_PLC | PORT_CEC | PORT_WKCONN_E | PORT_WKDISC_E | PORT_WKOC_E;
    assert_eq!(plan_missing_cas_recovery(input), MissingCasAction::WarmReset(ResetPlan {
        write: 0x8000_00e0, flush_posted_write: true,
    }));
}
