// SPDX-License-Identifier: GPL-2.0-only
//! The effect vocabulary of the GT core. Nothing in this crate touches a register: every planner returns [`Effect`]s and only
//! `devices/i915` executes them (plan section 1, "Shape rules"). All writes and reads are 32-bit.
use i915_common_core::Reg;

/// One 32-bit register write (`intel_uncore_write_fw(reg, value)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WritePlan {
    pub reg: Reg,
    pub value: u32,
}

/// A read-modify-write: read `reg`, compute `val = (old & !clear) | set`, write `val`. Two Linux flavours differ in ONE point,
/// carried in `skip_if_unchanged`: `wa_list_apply`'s open-coded form (intel_workarounds.c:1781-1790) and `intel_uncore_rmw_fw`
/// (intel_uncore.h:441-450) write only when `val != old`; `intel_uncore_rmw` (intel_uncore.h:430-439, what the clock-gating code
/// calls) writes unconditionally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RmwPlan {
    pub reg: Reg,
    pub clear: u32,
    pub set: u32,
    pub skip_if_unchanged: bool,
}

impl RmwPlan {
    /// The value to write for a register that currently reads `old`, or `None` when the write is skipped.
    pub const fn result(&self, old: u32) -> Option<u32> {
        let val = (old & !self.clear) | self.set;
        if val != old || !self.skip_if_unchanged {
            Some(val)
        } else {
            None
        }
    }
}

/// A bounded wait: read `reg` until `(value & mask) == want`. Linux's `__intel_wait_for_register_fw(uncore, reg, mask, want,
/// fast_timeout_us, 0, NULL)`. The executor must stop after `max_polls` reads OR `timeout_us`, whichever comes first, and report
/// the last value it read; the planners in this crate that consume poll results enforce `max_polls` themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollPlan {
    pub reg: Reg,
    pub mask: u32,
    pub want: u32,
    pub timeout_us: u32,
    pub max_polls: u32,
    /// The Linux step this wait belongs to, for markers (`"RESET_CTL ready-to-reset"`).
    pub what: &'static str,
}

impl PollPlan {
    pub const fn satisfied_by(&self, value: u32) -> bool {
        value & self.mask == self.want
    }
}

/// One step of a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Write(WritePlan),
    Rmw(RmwPlan),
    Poll(PollPlan),
    /// A posting read (`intel_uncore_posting_read`): read `reg` and discard the value, to flush the preceding write.
    PostingRead(Reg),
    /// Wait this many microseconds (`udelay`); the executor owns the clock.
    DelayUs(u32),
}

impl Effect {
    pub const FILL: Effect = Effect::DelayUs(0);
}
