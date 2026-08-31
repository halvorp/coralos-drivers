// SPDX-License-Identifier: GPL-2.0-only
//! Narrow action sequence for changing CMOS interrupt controls without performing MMIO.
//!
//! Ported from Linux `drivers/rtc/rtc-cmos.c:343-399,477-509`; original copyright Paul
//! Gortmaker, David Brownell, and the Linux RTC authors.

use rtc_mc146818_core::registers::{AIE, INTR_FLAGS};

/// One caller-executed action in Linux's register-B/register-C ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlAction {
    ReadInterruptFlags { register: u8 },
    WriteControl { value: u8 },
    HpetSet { mask: u8 },
    HpetMask { mask: u8 },
    AcpiWakeOn,
    AcpiWakeOff,
}

/// Bounded action list; Linux's paths need at most five actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlActions {
    actions: [Option<ControlAction>; 5],
    len: usize,
}

impl ControlActions {
    /// Number of actions the caller must execute.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether no action was requested.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Return one action by index.
    pub const fn get(&self, index: usize) -> Option<ControlAction> {
        if index < self.len { self.actions[index] } else { None }
    }
}

const fn empty_actions() -> ControlActions {
    ControlActions { actions: [None; 5], len: 0 }
}

const fn push(mut list: ControlActions, action: ControlAction) -> ControlActions {
    list.actions[list.len] = Some(action);
    list.len += 1;
    list
}

/// Actions for `cmos_irq_enable`: flush pending C first, write B, update HPET/ACPI, then check C.
pub const fn irq_enable_actions(
    control_b: u8,
    mask: u8,
    use_hpet_alarm: bool,
    use_acpi_alarm: bool,
    has_wake_hook: bool,
) -> ControlActions {
    let mut out = push(empty_actions(), ControlAction::ReadInterruptFlags { register: INTR_FLAGS });
    out = push(out, ControlAction::WriteControl { value: control_b | mask });
    if use_hpet_alarm {
        out = push(out, ControlAction::HpetSet { mask });
    }
    if mask & AIE != 0 && use_acpi_alarm && has_wake_hook {
        out = push(out, ControlAction::AcpiWakeOn);
    }
    push(out, ControlAction::ReadInterruptFlags { register: INTR_FLAGS })
} // drivers/rtc/rtc-cmos.c:360-381

/// Actions for `cmos_irq_disable`: write B, update HPET/ACPI, then acknowledge/check C.
pub const fn irq_disable_actions(
    control_b: u8,
    mask: u8,
    use_hpet_alarm: bool,
    use_acpi_alarm: bool,
    has_wake_hook: bool,
) -> ControlActions {
    let mut out = push(empty_actions(), ControlAction::WriteControl { value: control_b & !mask });
    if use_hpet_alarm {
        out = push(out, ControlAction::HpetMask { mask });
    }
    if mask & AIE != 0 && use_acpi_alarm && has_wake_hook {
        out = push(out, ControlAction::AcpiWakeOff);
    }
    push(out, ControlAction::ReadInterruptFlags { register: INTR_FLAGS })
} // drivers/rtc/rtc-cmos.c:383-399
