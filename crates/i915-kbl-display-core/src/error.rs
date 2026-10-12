// SPDX-License-Identifier: GPL-2.0-only
//! The named decode errors. A readout that hits a value Linux treats as `MISSING_CASE`, `drm_WARN` or "unknown pipe
//! linked to transcoder" says WHICH register held WHICH reserved value; it never clamps to a default.
//!
//! Linux often logs a warning and carries on with a fallback (e.g. an unknown eDP input becomes pipe A,
//! intel_display.c:3779-3786). This crate reports the surprise instead, because a readout that adopts a guess
//! would feed it to a modeset. Each variant cites the Linux site that warns.

use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadoutError {
    /// TRANS_DDI_FUNC_CTL(EDP) input select is 1, 2 or 3 (undefined): Linux "unknown pipe linked to transcoder"
    /// (intel_display.c:3779-3783, intel_ddi.c:831-833).
    EdpInputReserved { field: u32 },
    /// TRANS_DDI_FUNC_CTL(EDP) names pipe D, which Kaby Lake does not have (intel_display.c:3795-3796).
    EdpInputPipeAbsent { field: u32 },
    /// More than one transcoder is enabled for one pipe (`assert_enabled_transcoders`, intel_display.c:3841-3851).
    MultipleTranscoders { mask: u8 },
    /// TRANS_DDI_FUNC_CTL port select names a DDI Kaby Lake lacks (values 5..=7).
    DdiPortAbsent { trans: &'static str, field: u32 },
    /// TRANS_DDI_FUNC_CTL mode select is 5..=7 (undefined); Linux silently ignores it (intel_ddi.c:4141-4163).
    DdiModeReserved { trans: &'static str, field: u32 },
    /// TRANS_DDI_FUNC_CTL bits-per-colour is 4..=7 (undefined); Linux leaves pipe_bpp unset (intel_ddi.c:4116-4130).
    BpcReserved { trans: &'static str, field: u32 },
    /// The DDI port-width field is 5..=7 (undefined; DDI_PORT_WIDTH_DECODE, intel_display_regs.h:2369).
    PortWidthReserved { trans: &'static str, field: u32 },
    /// DPLL_CTRL1 link-rate field is 6 or 7 (`Unsupported link rate`, intel_dpll_mgr.c:1930).
    LinkRateReserved { pll: u8, field: u32 },
    /// A WRPLL PDIV field value that is neither 1, 2, 3, 7 nor the tolerated 7-invalid (`MISSING_CASE(p0)`,
    /// intel_dpll_mgr.c:1792).
    WrpllPdivReserved { pll: u8, field: u32 },
    /// KDIV field is not one of the four defined values (cannot happen: the field has exactly four values) or a divider is zero
    /// (`drm_WARN_ON(p0 == 0 || p1 == 0 || p2 == 0)`, intel_dpll_mgr.c:1820).
    WrpllDividerZero { pll: u8 },
    /// The DDI's clock is routed to a PLL whose enable bit is clear (`skl_ddi_pll_get_hw_state` returns false,
    /// intel_dpll_mgr.c:1462-1463).
    PllNotEnabled { ddi: char, pll: u8 },
    /// (`plane` in the two variants below is the plane's NUMBER, PLANE_1 = 1, as Linux names them in its logs.)
    /// PLANE_CTL tiling field is not linear / X / Y / Yf (`MISSING_CASE(tiling)`, skl_universal_plane.c:3146).
    TilingReserved { plane: u8, field: u32 },
    /// PLANE_CTL pixel-format field is not one Linux's SKL decode defines (skl_universal_plane_regs.h:50-60).
    FormatReserved { plane: u8, field: u32 },
}

impl ReadoutError {
    /// A stable, greppable name for the log line.
    pub fn name(&self) -> &'static str {
        match self {
            ReadoutError::EdpInputReserved { .. } => "edp-input-reserved",
            ReadoutError::EdpInputPipeAbsent { .. } => "edp-input-pipe-absent",
            ReadoutError::MultipleTranscoders { .. } => "multiple-transcoders",
            ReadoutError::DdiPortAbsent { .. } => "ddi-port-absent",
            ReadoutError::DdiModeReserved { .. } => "ddi-mode-reserved",
            ReadoutError::BpcReserved { .. } => "bpc-reserved",
            ReadoutError::PortWidthReserved { .. } => "port-width-reserved",
            ReadoutError::LinkRateReserved { .. } => "link-rate-reserved",
            ReadoutError::WrpllPdivReserved { .. } => "wrpll-pdiv-reserved",
            ReadoutError::WrpllDividerZero { .. } => "wrpll-divider-zero",
            ReadoutError::PllNotEnabled { .. } => "pll-not-enabled",
            ReadoutError::TilingReserved { .. } => "tiling-reserved",
            ReadoutError::FormatReserved { .. } => "format-reserved",
        }
    }
}

impl fmt::Display for ReadoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.name();
        match *self {
            ReadoutError::EdpInputReserved { field } | ReadoutError::EdpInputPipeAbsent { field } => {
                write!(f, "{} field={:#x}", n, field)
            }
            ReadoutError::MultipleTranscoders { mask } => write!(f, "{} mask={:#x}", n, mask),
            ReadoutError::DdiPortAbsent { trans, field }
            | ReadoutError::DdiModeReserved { trans, field }
            | ReadoutError::BpcReserved { trans, field }
            | ReadoutError::PortWidthReserved { trans, field } => write!(f, "{} trans={} field={:#x}", n, trans, field),
            ReadoutError::LinkRateReserved { pll, field } | ReadoutError::WrpllPdivReserved { pll, field } => {
                write!(f, "{} dpll={} field={:#x}", n, pll, field)
            }
            ReadoutError::WrpllDividerZero { pll } => write!(f, "{} dpll={}", n, pll),
            ReadoutError::PllNotEnabled { ddi, pll } => write!(f, "{} ddi={} dpll={}", n, ddi, pll),
            ReadoutError::TilingReserved { plane, field } | ReadoutError::FormatReserved { plane, field } => {
                write!(f, "{} plane={} field={:#x}", n, plane, field)
            }
        }
    }
}
