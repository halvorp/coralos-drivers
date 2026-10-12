// SPDX-License-Identifier: GPL-2.0-only
//! Every refusal of this crate, by name (PORT_CONVENTIONS.md section 5), and the NAMED handshakes whose bounds are enforced.
//!
//! `i915_common_core::I915Error` is the family's device-facing error; these are the planner-level refusals that carry the
//! extra context (which well, which handshake) the common enum has no room for. [`ClockError::name`] is the stable token a
//! marker line prints.
use crate::regs;
use i915_common_core::Reg;
use i915_kbl_display_core::power::Well;

/// Every poll this crate plans is one of these. The name is what a timeout reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handshake {
    /// `hsw_wait_for_power_well_enable`: HSW_PWR_WELL_CTL_STATE set after the request (intel_display_power_well.c:293-320).
    PowerWellEnable(Well),
    /// `hsw_wait_for_power_well_disable` (intel_display_power_well.c:338-371): tolerated by Linux, so tolerated here.
    PowerWellDisable(Well),
    /// `gen9_wait_for_power_well_fuses` (intel_display_power_well.c:373-380): SKL_FUSE_PG_DIST_STATUS(pg).
    FuseDistribution(u32),
    /// `__snb_pcode_rw`'s wait for GEN6_PCODE_READY to clear (intel_pcode.c:79-85).
    PcodeMailboxReady,
    /// `skl_dpll0_enable` waits for LCPLL1_CTL lock (intel_cdclk.c:1112-1113).
    Dpll0Lock,
    /// `skl_dpll0_disable` waits for LCPLL1_CTL lock to drop (intel_cdclk.c:1126-1129).
    Dpll0Unlock,
    /// `skl_ddi_pll_enable` waits for DPLL_STATUS lock of DPLL 1..3 (intel_dpll_mgr.c:1415-1416).
    DpllLock(u8),
}

impl Handshake {
    /// The stable token. Written out literally per variant so a renamed well cannot silently rename a marker.
    pub const fn name(self) -> &'static str {
        match self {
            Handshake::PowerWellEnable(Well::Pw1) => "PW1-ENABLE-ACK",
            Handshake::PowerWellEnable(Well::Pw2) => "PW2-ENABLE-ACK",
            Handshake::PowerWellEnable(Well::MiscIo) => "MISC-IO-ENABLE-ACK",
            Handshake::PowerWellEnable(Well::DdiAE) => "DDI-A-E-ENABLE-ACK",
            Handshake::PowerWellEnable(Well::DdiB) => "DDI-B-ENABLE-ACK",
            Handshake::PowerWellEnable(Well::DdiC) => "DDI-C-ENABLE-ACK",
            Handshake::PowerWellEnable(Well::DdiD) => "DDI-D-ENABLE-ACK",
            Handshake::PowerWellDisable(Well::Pw1) => "PW1-DISABLE-ACK",
            Handshake::PowerWellDisable(Well::Pw2) => "PW2-DISABLE-ACK",
            Handshake::PowerWellDisable(Well::MiscIo) => "MISC-IO-DISABLE-ACK",
            Handshake::PowerWellDisable(Well::DdiAE) => "DDI-A-E-DISABLE-ACK",
            Handshake::PowerWellDisable(Well::DdiB) => "DDI-B-DISABLE-ACK",
            Handshake::PowerWellDisable(Well::DdiC) => "DDI-C-DISABLE-ACK",
            Handshake::PowerWellDisable(Well::DdiD) => "DDI-D-DISABLE-ACK",
            Handshake::FuseDistribution(0) => "FUSE-PG0-DISTRIBUTION",
            Handshake::FuseDistribution(1) => "FUSE-PG1-DISTRIBUTION",
            Handshake::FuseDistribution(2) => "FUSE-PG2-DISTRIBUTION",
            Handshake::FuseDistribution(_) => "FUSE-PGN-DISTRIBUTION",
            Handshake::PcodeMailboxReady => "PCODE-MAILBOX-READY",
            Handshake::Dpll0Lock => "DPLL0-LOCK",
            Handshake::Dpll0Unlock => "DPLL0-UNLOCK",
            Handshake::DpllLock(1) => "DPLL1-LOCK",
            Handshake::DpllLock(2) => "DPLL2-LOCK",
            Handshake::DpllLock(3) => "DPLL3-LOCK",
            Handshake::DpllLock(_) => "DPLLN-LOCK",
        }
    }
}

/// Which of the two bounds on a poll tripped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// The sample budget (`PollPlan::max_polls`) ran out: protects a host whose clock does not advance.
    Polls,
    /// The wall-clock budget (`PollPlan::timeout_us`) ran out.
    Time,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    /// A planned poll never saw `(value & mask) == want` inside its bound.
    HandshakeTimeout { what: Handshake, bound: Bound, reg: Reg, mask: u32, want: u32, got: u32, polls: u32, elapsed_us: u32 },
    /// The PCU still held GEN6_PCODE_READY when a request was about to be sent (`-EAGAIN`, intel_pcode.c:71-72).
    PcodeBusy { mailbox: u32 },
    /// The PCU answered the request with a non-zero status and never acknowledged (`status ? status : ret`, intel_pcode.c:205).
    PcodeRejected { status: u32, errno_class: PcodeErrno },
    /// The attempt budget ran out with the PCU silent or not yet acknowledging.
    PcodeNoAck { attempts: u32, elapsed_us: u32 },
    /// A well was asked to turn on before the well it sits behind (PW2 and the DDI wells behind PW1).
    WellOrder { well: Well, needs: Well },
    /// A well was asked to turn off while a well that sits behind it is still on.
    WellStillNeeded { well: Well, held_by: Well },
    /// `put` of a domain or well with no reference (`drm_WARN(!count)`, intel_display_power.c:626, intel_display_power_well.c:149).
    UnbalancedPut { what: &'static str },
    /// A plan outgrew its fixed capacity. Never silently truncated.
    PlanFull { cap: usize },
    /// CDCLK VCO is not DPLL0's 8100000 / 8640000 kHz (`skl_dpll0_link_rate` `drm_WARN_ON`, intel_cdclk.c:1085).
    CdclkVcoInvalid { vco_khz: u32 },
    /// Skylake with the 8640 MHz VCO: Display WA #1183 forbids it (intel_cdclk.c:1175-1176, a WARN in Linux; a refusal here).
    CdclkSkylakeVco8640 { vco_khz: u32 },
    /// The CDCLK is not one of the eight SKL steps (`skl_cdclk_freq_sel` default arm, intel_cdclk.c:1138-1143).
    CdclkNotAStep { cdclk_khz: u32, vco_khz: u32 },
    /// `min_cdclk > max_cdclk_freq` (intel_cdclk.c:3048-3053, `-EINVAL`).
    CdclkAboveMax { min_khz: u32, max_khz: u32 },
    /// `skl_ddi_calculate_wrpll` found no divider inside the DCO deviation window (`-EINVAL`, intel_dpll_mgr.c:1739-1740).
    WrpllNoDivider { clock_khz: u32 },
    /// The best divider has a P0 or P2 the CFGCR2 fields cannot encode (divider 5 gives P0 = 5: no PDIV value). Linux `WARN(1,
    /// "Incorrect PDiv")`s, leaves the field 0 (P0 = 1) and programs the PLL anyway (intel_dpll_mgr.c:1647-1648); this refuses.
    WrpllDividerUnencodable { clock_khz: u32, divider: u32 },
    /// A DP port clock that is not a DPLL_CTRL1 link rate (Linux's `switch` has no default, intel_dpll_mgr.c:1874-1900).
    DpRateUnsupported { port_clock_khz: u32 },
    /// An eDP link rate whose VCO family differs from the VCO DPLL0 runs at (8100 vs 8640, intel_cdclk.c:3167-3203).
    Dpll0VcoMismatch { needs_khz: u32, running_khz: u32 },
    /// DPLL id outside 0..=3.
    NoSuchDpll { id: u8 },
    /// `intel_find_dpll` found neither a shareable nor an unused PLL in the allowed set (intel_dpll_mgr.c:376-427).
    NoFreeDpll { allowed_mask: u8 },
}

/// The `-E*` class `gen7_check_mailbox_status` maps a PCU status to (intel_pcode.c:33-54).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcodeErrno {
    /// `-ENXIO`: ILLEGAL_CMD and ILLEGAL_SUBCOMMAND.
    Enxio,
    /// `-ETIMEDOUT`: GEN7_PCODE_TIMEOUT.
    Etimedout,
    /// `-EINVAL`: ILLEGAL_DATA.
    Einval,
    /// `-EBUSY`: LOCKED.
    Ebusy,
    /// `-EACCES`: REJECTED.
    Eacces,
    /// `-EOVERFLOW`: MIN_FREQ_TABLE_GT_RATIO_OUT_OF_RANGE.
    Eoverflow,
    /// `MISSING_CASE`: a status Linux does not know (it logs and returns 0, which the retry loop then treats as success of
    /// the status check - this crate treats it as a refusal instead).
    Unknown,
}

impl ClockError {
    pub const fn name(&self) -> &'static str {
        match self {
            ClockError::HandshakeTimeout { .. } => "HANDSHAKE-TIMEOUT",
            ClockError::PcodeBusy { .. } => "PCODE-BUSY",
            ClockError::PcodeRejected { .. } => "PCODE-REJECTED",
            ClockError::PcodeNoAck { .. } => "PCODE-NO-ACK",
            ClockError::WellOrder { .. } => "POWER-WELL-ORDER",
            ClockError::WellStillNeeded { .. } => "POWER-WELL-STILL-NEEDED",
            ClockError::UnbalancedPut { .. } => "UNBALANCED-PUT",
            ClockError::PlanFull { .. } => "PLAN-FULL",
            ClockError::CdclkVcoInvalid { .. } => "CDCLK-VCO-INVALID",
            ClockError::CdclkSkylakeVco8640 { .. } => "CDCLK-SKL-VCO-8640-WA1183",
            ClockError::CdclkNotAStep { .. } => "CDCLK-NOT-A-STEP",
            ClockError::CdclkAboveMax { .. } => "CDCLK-ABOVE-MAX",
            ClockError::WrpllNoDivider { .. } => "WRPLL-NO-DIVIDER",
            ClockError::WrpllDividerUnencodable { .. } => "WRPLL-DIVIDER-UNENCODABLE",
            ClockError::DpRateUnsupported { .. } => "DP-RATE-UNSUPPORTED",
            ClockError::Dpll0VcoMismatch { .. } => "DPLL0-VCO-MISMATCH",
            ClockError::NoSuchDpll { .. } => "NO-SUCH-DPLL",
            ClockError::NoFreeDpll { .. } => "NO-FREE-DPLL",
        }
    }
}

/// `gen7_check_mailbox_status` (intel_pcode.c:33-54): `None` is success, `Some` the class of the failure.
pub const fn pcode_status(mailbox: u32) -> Option<PcodeErrno> {
    match mailbox & regs::GEN6_PCODE_ERROR_MASK {
        regs::GEN6_PCODE_SUCCESS => None,
        regs::GEN6_PCODE_ILLEGAL_CMD | regs::GEN11_PCODE_ILLEGAL_SUBCOMMAND => Some(PcodeErrno::Enxio),
        regs::GEN7_PCODE_TIMEOUT => Some(PcodeErrno::Etimedout),
        regs::GEN7_PCODE_ILLEGAL_DATA => Some(PcodeErrno::Einval),
        regs::GEN11_PCODE_LOCKED => Some(PcodeErrno::Ebusy),
        regs::GEN11_PCODE_REJECTED => Some(PcodeErrno::Eacces),
        regs::GEN7_PCODE_MIN_FREQ_TABLE_GT_RATIO_OUT_OF_RANGE => Some(PcodeErrno::Eoverflow),
        _ => Some(PcodeErrno::Unknown),
    }
}
