// SPDX-License-Identifier: GPL-2.0-only
//! DDI_BUF_CTL: is a port's buffer on, how many lanes and which way round.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_ddi.c (`intel_ddi_get_encoder_pipes`, the `DDI_A_4_LANES`
//! handling in `intel_ddi_init`) and intel_display_regs.h (DDI_BUF_CTL). Copyright (c) Intel Corporation.

use crate::regs::*;
use crate::transcoder::decode_port_width;

/// A decoded DDI_BUF_CTL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdiBuf {
    pub raw: u32,
    /// `DDI_BUF_CTL_ENABLE`: the port's buffer is on (the test `intel_ddi_get_encoder_pipes` starts with, intel_ddi.c:822).
    pub enabled: bool,
    /// `DDI_BUF_IS_IDLE`: the buffer has finished going idle.
    pub idle: bool,
    /// `DDI_BUF_PORT_REVERSAL`: lane order reversed (Type-C flip, or a board that wires the lanes the other way).
    pub reversed: bool,
    /// `DDI_A_4_LANES`: DDI A has all four lanes (DDI E's lanes are folded into A). Meaningful on port A only.
    pub a_4_lanes: bool,
    /// The buffer's own port width field decoded (`DDI_PORT_WIDTH_GET`, intel_display_regs.h:2372); `None` if undefined.
    pub width_lanes: Option<u32>,
    /// `DDI_BUF_TRANS_SELECT`: the voltage-swing / pre-emphasis table entry (DDI_BUF_EMP_MASK).
    pub trans_select: u32,
}

pub fn decode_ddi_buf(raw: u32) -> DdiBuf {
    DdiBuf {
        raw,
        enabled: raw & DDI_BUF_CTL_ENABLE != 0,
        idle: raw & DDI_BUF_IS_IDLE != 0,
        reversed: raw & DDI_BUF_PORT_REVERSAL != 0,
        a_4_lanes: raw & DDI_A_4_LANES != 0,
        width_lanes: decode_port_width((raw & DDI_PORT_WIDTH_MASK) >> DDI_PORT_WIDTH_SHIFT),
        trans_select: (raw & DDI_BUF_EMP_MASK) >> DDI_BUF_EMP_MASK.trailing_zeros(),
    }
}
