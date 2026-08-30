// SPDX-License-Identifier: GPL-2.0-only
//! Reset and config-write unlocking, ported from Linux's r8169.
//!
//! Both sequences are short, and both fail in ways the hardware does not report.

use crate::regs::{self, cfg9346, chip_cmd};

/// Register access and the one wait primitive these sequences need. Declared here rather than taken
/// from a shared crate, mirroring sdhci-core's own `Bus`: each ported driver owns the narrow seam it
/// needs, so a crate stays buildable and testable on its own.
pub trait Bus {
    fn r8(&mut self, reg: u32) -> u8;
    fn w8(&mut self, reg: u32, val: u8);
    /// 16-bit access. IntrMask and IntrStatus are both 16-bit and must be accessed as such — a
    /// byte-wise read of IntrStatus would see half an event word.
    fn r16(&mut self, reg: u32) -> u16;
    fn w16(&mut self, reg: u32, val: u16);
    /// 32-bit access. PHYAR (0x60) and GPHY_OCP (0xb8) are both 32-bit and carry a busy flag in bit
    /// 31 alongside the data, so a narrower access cannot see the completion it must poll for.
    fn r32(&mut self, reg: u32) -> u32;
    fn w32(&mut self, reg: u32, val: u32);
    /// Wait between status polls. A host test satisfies this with a counter; a real driver yields,
    /// because a non-yielding spin in task context starves whatever shares the core.
    fn delay_us(&mut self, us: u32);
}

/// Why a reset gave up. A timeout is NOT an absent device, and reporting one as the other sends the
/// reader looking in the wrong place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResetError {
    /// `CmdReset` never went low within the poll budget: the chip took the write and did not finish.
    Timeout,
}

/// Hardware reset — write `CmdReset`, then wait for the chip to clear it.
///
/// Ported from `rtl_hw_reset` (r8169_main.c:2671-2676), whose entire body is
/// `RTL_W8(tp, ChipCmd, CmdReset)` followed by `rtl_loop_wait_low(tp, &rtl_chipcmd_cond, 100, 100)`,
/// with `rtl_chipcmd_cond` (r8169_main.c:2668) reading `RTL_R8(tp, ChipCmd) & CmdReset`.
///
/// THE BIT IS SELF-CLEARING, which is why this polls instead of sleeping a fixed time. A fixed delay
/// is either too short on a slow part — leaving the driver programming a chip mid-reset, which
/// presents later as unexplained register corruption — or wasted on every boot. Returns the number
/// of polls it took, so a caller can tell "reset immediately" from "reset on the 99th poll"; the
/// second is a working device worth noticing.
pub fn hw_reset<B: Bus>(bus: &mut B) -> Result<u32, ResetError> {
    bus.w8(regs::CHIP_CMD, chip_cmd::RESET);
    for polls in 1..=regs::RESET_POLL_MAX {
        if bus.r8(regs::CHIP_CMD) & chip_cmd::RESET == 0 {
            return Ok(polls);
        }
        bus.delay_us(regs::RESET_POLL_INTERVAL_US);
    }
    Err(ResetError::Timeout)
}

/// Run `f` with the Config0..5 write lock open, closing it again afterwards.
///
/// From the paired `RTL_W8(tp, Cfg9346, Cfg9346_Unlock)` / `Cfg9346_Lock` around config writes
/// (r8169_main.c:817, :822). A scope rather than two loose calls, because forgetting the unlock is
/// SILENT — the config write is discarded and nothing reports it — and forgetting the lock leaves
/// the registers writable. A scope cannot forget either half.
pub fn with_config_unlocked<B: Bus, R>(bus: &mut B, f: impl FnOnce(&mut B) -> R) -> R {
    bus.w8(regs::CFG_9346, cfg9346::UNLOCK);
    let r = f(bus);
    bus.w8(regs::CFG_9346, cfg9346::LOCK);
    r
}
