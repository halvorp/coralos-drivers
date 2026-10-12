// SPDX-License-Identifier: GPL-2.0-only
//! Which transcoder feeds a pipe, and what TRANS_DDI_FUNC_CTL / TRANSCONF say about it.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_display.c (`hsw_enabled_transcoders`,
//! `assert_enabled_transcoders`, `hsw_get_transcoder_state`) and intel_ddi.c (`intel_ddi_get_encoder_pipes`,
//! `intel_ddi_read_func_ctl`). Copyright (c) Intel Corporation.

use crate::error::ReadoutError;
use crate::regs::*;
use crate::types::{Ddi, Pipe, Transcoder};

/// The output protocol in TRANS_DDI_FUNC_CTL bits 26:24 (`intel_ddi_read_func_ctl`, intel_ddi.c:4141-4160).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdiMode {
    Hdmi,   // TRANS_DDI_MODE_SELECT_HDMI, intel_ddi.c:4141
    Dvi,    // TRANS_DDI_MODE_SELECT_DVI, intel_ddi.c:4143
    DpSst,  // TRANS_DDI_MODE_SELECT_DP_SST, intel_ddi.c:4147
    DpMst,  // TRANS_DDI_MODE_SELECT_DP_MST, intel_ddi.c:4149
    /// TRANS_DDI_MODE_SELECT_FDI_OR_128B132B without DP 2.0 (`!HAS_DP20`, intel_ddi.c:4145): FDI.
    Fdi,
}

impl DdiMode {
    pub fn name(self) -> &'static str {
        match self {
            DdiMode::Hdmi => "HDMI",
            DdiMode::Dvi => "DVI",
            DdiMode::DpSst => "DP-SST",
            DdiMode::DpMst => "DP-MST",
            DdiMode::Fdi => "FDI",
        }
    }
    pub fn is_dp(self) -> bool {
        matches!(self, DdiMode::DpSst | DdiMode::DpMst)
    }
}

/// A decoded TRANS_DDI_FUNC_CTL of an ENABLED transcoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuncCtl {
    pub raw: u32,
    pub mode: DdiMode,
    /// The DDI the transcoder drives: eDP's is always A (the port bits are ignored, intel_display_regs.h:2238);
    /// `None` when a pipe transcoder's port select is 0 (no port).
    pub ddi: Option<Ddi>,
    /// Bits per pixel from the BPC field (6/8/10/12 bpc -> 18/24/30/36, intel_ddi.c:4122-4135).
    pub bpp: u32,
    pub hsync_positive: bool, // TRANS_DDI_PHSYNC, intel_ddi.c:4111
    pub vsync_positive: bool, // TRANS_DDI_PVSYNC, intel_ddi.c:4115
    /// DP lane count (`DDI_PORT_WIDTH_DECODE`, intel_display_regs.h:2369); `None` for HDMI/DVI/FDI.
    pub lanes: Option<u32>,
}

/// `DDI_PORT_WIDTH_DECODE(regval)` (intel_display_regs.h:2369): a field value of 4 means THREE lanes.
/// The reader in intel_ddi.c:4049 computes `+ 1` and so would call it five; the macro is the inverse of the writer's
/// `DDI_PORT_WIDTH_ENCODE` (intel_display_regs.h:2368), which is what the hardware field means.
pub fn decode_port_width(field: u32) -> Option<u32> {
    match field {
        0..=3 => Some(field + 1),
        4 => Some(3),
        _ => None, // 5..=7 are undefined
    }
}

/// Decode TRANS_DDI_FUNC_CTL of transcoder `t`. Returns `Ok(None)` when TRANS_DDI_FUNC_ENABLE is clear (the other
/// fields are not meaningful then and are not validated).
pub fn decode_func_ctl(t: Transcoder, raw: u32) -> Result<Option<FuncCtl>, ReadoutError> {
    if raw & TRANS_DDI_FUNC_ENABLE == 0 {
        return Ok(None);
    }
    let name = t.name();
    let mode = match raw & TRANS_DDI_MODE_SELECT_MASK {
        TRANS_DDI_MODE_SELECT_HDMI => DdiMode::Hdmi,
        TRANS_DDI_MODE_SELECT_DVI => DdiMode::Dvi,
        TRANS_DDI_MODE_SELECT_DP_SST => DdiMode::DpSst,
        TRANS_DDI_MODE_SELECT_DP_MST => DdiMode::DpMst,
        TRANS_DDI_MODE_SELECT_FDI_OR_128B132B => DdiMode::Fdi,
        other => return Err(ReadoutError::DdiModeReserved { trans: name, field: other >> 24 }),
    };
    let bpp = match raw & TRANS_DDI_BPC_MASK {
        TRANS_DDI_BPC_6 => 18,
        TRANS_DDI_BPC_8 => 24,
        TRANS_DDI_BPC_10 => 30,
        TRANS_DDI_BPC_12 => 36,
        other => return Err(ReadoutError::BpcReserved { trans: name, field: other >> 20 }),
    };
    let ddi = if t == Transcoder::Edp {
        Some(Ddi::A) // "ignored by pipe EDP since it can only connect to DDI A", intel_display_regs.h:2238
    } else {
        let field = (raw & TRANS_DDI_PORT_MASK) >> TRANS_DDI_PORT_SHIFT;
        match field {
            0 => None, // TRANS_DDI_SELECT_PORT(PORT_A) is 0: no port selected
            1..=4 => Ddi::from_index(field), // TRANS_DDI_SELECT_PORT(x) = x << 28, intel_display_regs.h:2243
            _ => return Err(ReadoutError::DdiPortAbsent { trans: name, field }),
        }
    };
    let lanes = if mode.is_dp() {
        let field = (raw & DDI_PORT_WIDTH_MASK) >> DDI_PORT_WIDTH_SHIFT;
        match decode_port_width(field) {
            Some(n) => Some(n),
            None => return Err(ReadoutError::PortWidthReserved { trans: name, field }),
        }
    } else {
        None
    };
    Ok(Some(FuncCtl {
        raw,
        mode,
        ddi,
        bpp,
        hsync_positive: raw & TRANS_DDI_PHSYNC != 0,
        vsync_positive: raw & TRANS_DDI_PVSYNC != 0,
        lanes,
    }))
}

/// The pipe the eDP transcoder's input select names (`hsw_enabled_transcoders` switch, intel_display.c:3779-3797).
/// `A_ON` and `A_ONOFF` are both pipe A (the latter additionally means `pch_pfit.force_thru`, intel_display.c:3878-3883).
/// Linux warns on 1..=3 and treats them as pipe A; here they are a named error.
pub fn edp_input_pipe(func_ctl: u32) -> Result<Pipe, ReadoutError> {
    match func_ctl & TRANS_DDI_EDP_INPUT_MASK {
        TRANS_DDI_EDP_INPUT_A_ON | TRANS_DDI_EDP_INPUT_A_ONOFF => Ok(Pipe::A), // intel_display.c:3785-3787
        TRANS_DDI_EDP_INPUT_B_ONOFF => Ok(Pipe::B),                          // intel_display.c:3789-3790
        TRANS_DDI_EDP_INPUT_C_ONOFF => Ok(Pipe::C),                          // intel_display.c:3792-3793
        v @ TRANS_DDI_EDP_INPUT_D_ONOFF => Err(ReadoutError::EdpInputPipeAbsent { field: v >> 12 }), // :3795-3796
        v => Err(ReadoutError::EdpInputReserved { field: v >> 12 }), // default: drm_WARN, intel_display.c:3780-3784
    }
}

/// `TRANS_DDI_EDP_INPUT_A_ONOFF` set: the eDP transcoder takes pipe A with the panel fitter in the path
/// (`pipe_config->pch_pfit.force_thru`, intel_display.c:3878-3883).
pub const fn edp_force_thru(func_ctl: u32) -> bool {
    func_ctl & TRANS_DDI_EDP_INPUT_MASK == TRANS_DDI_EDP_INPUT_A_ONOFF
}

/// Which transcoders are enabled for `pipe` (`hsw_enabled_transcoders`, intel_display.c:3753-3818, without DSI and
/// joiner): bit `Transcoder::mask_bit`. `edp_func_ctl` is TRANS_DDI_FUNC_CTL(EDP) (0 if its power domain is off),
/// `pipe_func_ctl` is TRANS_DDI_FUNC_CTL of the transcoder with the pipe's own number (0 likewise).
pub fn enabled_transcoders(pipe: Pipe, edp_func_ctl: u32, pipe_func_ctl: u32) -> Result<u8, ReadoutError> {
    let mut mask = 0u8;
    if edp_func_ctl & TRANS_DDI_FUNC_ENABLE != 0 && edp_input_pipe(edp_func_ctl)? == pipe {
        mask |= Transcoder::Edp.mask_bit(); // intel_display.c:3801
    }
    if pipe_func_ctl & TRANS_DDI_FUNC_ENABLE != 0 {
        mask |= Transcoder::of_pipe(pipe).mask_bit(); // intel_display.c:3806-3807
    }
    Ok(mask)
}

/// `ffs(enabled_transcoders) - 1` after `assert_enabled_transcoders` (intel_display.c:3861-3872): `None` when no
/// transcoder is enabled, the transcoder when exactly one is, a named error when two are.
pub fn pick_transcoder(pipe: Pipe, mask: u8) -> Result<Option<Transcoder>, ReadoutError> {
    let own = Transcoder::of_pipe(pipe);
    if mask == 0 {
        Ok(None)
    } else if mask == Transcoder::Edp.mask_bit() {
        Ok(Some(Transcoder::Edp))
    } else if mask == own.mask_bit() {
        Ok(Some(own))
    } else {
        // two bits (intel_display.c:3848-3850), or another pipe's transcoder, which `enabled_transcoders` cannot build
        Err(ReadoutError::MultipleTranscoders { mask })
    }
}

/// TRANSCONF of the picked transcoder: `TRANSCONF_ENABLE` decides whether the pipe is active
/// (`hsw_get_transcoder_state`, intel_display.c:3886-3889) and the HSW interlace field its scan mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transconf {
    pub raw: u32,
    pub enabled: bool,
    pub interlaced: bool,
}

pub const fn decode_transconf(raw: u32) -> Transconf {
    Transconf {
        raw,
        enabled: raw & TRANSCONF_ENABLE != 0,
        interlaced: crate::timing::transconf_interlaced(raw),
    }
}
