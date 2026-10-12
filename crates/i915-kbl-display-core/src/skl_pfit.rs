// SPDX-License-Identifier: GPL-2.0-only
//! The pipe scaler / panel fitter decision for a native-mode eDP: is one NEEDED, and if so, the named refusal.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/skl_scaler.c at v7.0-rc6 (`skl_pfit_enable`, `skl_pfit_disable`,
//! `skl_update_scaler_crtc`) and intel_panel.c (`intel_panel_fitting`, which sets `pch_pfit.enabled` only when the pipe source
//! differs from the adjusted mode). Copyright (c) Intel Corporation.
//!
//! With the pipe source equal to the panel's native mode (`pipe_src == adjusted_mode` size) `pch_pfit.enabled` is false,
//! `skl_pfit_enable()` returns before writing anything, and `TRANS_DDI_FUNC_CTL` selects `TRANS_DDI_EDP_INPUT_A_ON`. That is the
//! only configuration this port programs. A source smaller than the native mode needs `PS_CTRL` / `PS_WIN_POS` / `PS_WIN_SZ` and the
//! scale-factor registers (`skl_scaler_setup_filter`, `skl_scaler_calc_phase`); those are NOT ported and are refused by name.

/// The decision for one pipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelFit {
    /// `pch_pfit.enabled == false`: no scaler register is written (`skl_pfit_enable`: "if (!crtc_state->pch_pfit.enabled) return").
    NotNeeded,
}

/// Named refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PfitError {
    /// The pipe source differs from the panel's native mode: a panel fit would be required.
    NotPorted { src_w: u32, src_h: u32, native_w: u32, native_h: u32 },
}

impl PfitError {
    pub const fn name(&self) -> &'static str {
        match self {
            PfitError::NotPorted { .. } => "PFIT-SCALER-NOT-PORTED",
        }
    }
}

/// `intel_panel_fitting()`'s enable test for display ver 9: any difference between the pipe source and the mode.
pub const fn check_panel_fit(src_w: u32, src_h: u32, native_w: u32, native_h: u32) -> Result<PanelFit, PfitError> {
    if src_w == native_w && src_h == native_h {
        Ok(PanelFit::NotNeeded)
    } else {
        Err(PfitError::NotPorted { src_w, src_h, native_w, native_h })
    }
}
