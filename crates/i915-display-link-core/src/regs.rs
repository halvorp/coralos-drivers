// SPDX-License-Identifier: GPL-2.0-only
//! Register offsets, fields and values for the eDP link: the AUX channel, the DDI DP transport and buffer translation,
//! the SPT panel power sequencer and the SPT/LPT PCH PWM backlight. Each line cites the Linux FILE:LINE it came from
//! (v7.0-rc6). Kaby Lake's display MMIO base is 0, so every offset is an absolute BAR0 offset.
//!
//! WHERE THE REGISTERS LIVE (WAVE2-BRIEF: "one home per register"):
//!  * `intel_dp_aux_regs.h`, `intel_pps_regs.h`, `intel_backlight_regs.h` are headers no existing crate owns, so they are
//!    homed HERE.
//!  * `DDI_BUF_CTL` and its fields are `intel_display_regs.h` and ALREADY live in `i915-kbl-display-core::regs`; they are
//!    imported ([`ddi_buf_ctl`] and the `DDI_*` fields), never re-typed.
//!  * `DP_TP_CTL`, `DP_TP_STATUS`, `DDI_BUF_TRANS_{LO,HI}` and `DISPIO_CR_TX_BMU_CR0` are also `intel_display_regs.h` but
//!    are NOT in kbl-display-core yet (it is a read-only readout and this task may not edit it). They are parked in the
//!    `ddi` section below and belong in `i915-kbl-display-core/src/regs.rs` when that crate is next extended; the offsets
//!    are cited to `intel_display_regs.h` so the move is mechanical. `SOUTH_CHICKEN1` is `i915_reg.h` (PCH, not display).
//! Copyright (c) Intel Corporation.
use i915_common_core::reg::{bit, field_prep, genmask};
use i915_common_core::Reg;
use i915_vbt_core::child::AuxCh;

// ---------------------------------------------------------------------------------------------------------------
// intel_dp_aux_regs.h — the AUX channel
// ---------------------------------------------------------------------------------------------------------------
/// `_DPA_AUX_CH_CTL` — intel_dp_aux_regs.h:24.
pub const DPA_AUX_CH_CTL: u32 = 0x64010; // intel_dp_aux_regs.h:24
/// `_DPB_AUX_CH_CTL` — intel_dp_aux_regs.h:25.
pub const DPB_AUX_CH_CTL: u32 = 0x64110; // intel_dp_aux_regs.h:25
/// `_DPA_AUX_CH_DATA1` — intel_dp_aux_regs.h:78.
pub const DPA_AUX_CH_DATA1: u32 = 0x64014; // intel_dp_aux_regs.h:78
/// `_DPB_AUX_CH_DATA1` — intel_dp_aux_regs.h:79.
pub const DPB_AUX_CH_DATA1: u32 = 0x64114; // intel_dp_aux_regs.h:79

/// `DP_AUX_CH_CTL(aux_ch)` = `_MMIO_PORT(aux_ch, _DPA_AUX_CH_CTL, _DPB_AUX_CH_CTL)` — intel_dp_aux_regs.h:26-27;
/// `_MMIO_PORT` is `_PICK_EVEN` (intel_display_reg_defs.h:19,26; i915_reg_defs.h:127).
pub const fn dp_aux_ch_ctl(ch: AuxCh) -> Reg {
    Reg::new(DPA_AUX_CH_CTL + (ch as u32) * (DPB_AUX_CH_CTL - DPA_AUX_CH_CTL))
}
/// `DP_AUX_CH_DATA(aux_ch, i)` (5 registers) — intel_dp_aux_regs.h:80-81.
pub const fn dp_aux_ch_data(ch: AuxCh, i: u32) -> Reg {
    Reg::new(DPA_AUX_CH_DATA1 + (ch as u32) * (DPB_AUX_CH_DATA1 - DPA_AUX_CH_DATA1) + i * 4)
}

pub const DP_AUX_CH_CTL_SEND_BUSY: u32 = bit(31); // intel_dp_aux_regs.h:46
pub const DP_AUX_CH_CTL_DONE: u32 = bit(30); // intel_dp_aux_regs.h:47
pub const DP_AUX_CH_CTL_INTERRUPT: u32 = bit(29); // intel_dp_aux_regs.h:48
pub const DP_AUX_CH_CTL_TIME_OUT_ERROR: u32 = bit(28); // intel_dp_aux_regs.h:49
pub const DP_AUX_CH_CTL_TIME_OUT_MASK: u32 = genmask(27, 26); // intel_dp_aux_regs.h:50
/// `DP_AUX_CH_CTL_TIME_OUT_400us` — intel_dp_aux_regs.h:51.
pub const DP_AUX_CH_CTL_TIME_OUT_400US: u32 = field_prep(DP_AUX_CH_CTL_TIME_OUT_MASK, 0); // intel_dp_aux_regs.h:51
/// `DP_AUX_CH_CTL_TIME_OUT_MAX` (1.6 ms on SKL-GLK) — intel_dp_aux_regs.h:54.
pub const DP_AUX_CH_CTL_TIME_OUT_MAX: u32 = field_prep(DP_AUX_CH_CTL_TIME_OUT_MASK, 3); // intel_dp_aux_regs.h:54
pub const DP_AUX_CH_CTL_RECEIVE_ERROR: u32 = bit(25); // intel_dp_aux_regs.h:55
pub const DP_AUX_CH_CTL_MESSAGE_SIZE_MASK: u32 = genmask(24, 20); // intel_dp_aux_regs.h:56
pub const DP_AUX_CH_CTL_AUX_AKSV_SELECT: u32 = bit(15); // intel_dp_aux_regs.h:62
pub const DP_AUX_CH_CTL_FW_SYNC_PULSE_SKL_MASK: u32 = genmask(9, 5); // intel_dp_aux_regs.h:73
pub const DP_AUX_CH_CTL_SYNC_PULSE_SKL_MASK: u32 = genmask(4, 0); // intel_dp_aux_regs.h:75

/// `DP_AUX_CH_CTL_MESSAGE_SIZE(x)` — intel_dp_aux_regs.h:57.
pub const fn dp_aux_ch_ctl_message_size(x: u32) -> u32 {
    field_prep(DP_AUX_CH_CTL_MESSAGE_SIZE_MASK, x)
}
/// `DP_AUX_CH_CTL_FW_SYNC_PULSE_SKL(c)` — intel_dp_aux_regs.h:74. `c` is the pulse count; the field holds `c - 1`.
pub const fn dp_aux_ch_ctl_fw_sync_pulse_skl(c: u32) -> u32 {
    field_prep(DP_AUX_CH_CTL_FW_SYNC_PULSE_SKL_MASK, c - 1)
}
/// `DP_AUX_CH_CTL_SYNC_PULSE_SKL(c)` — intel_dp_aux_regs.h:76.
pub const fn dp_aux_ch_ctl_sync_pulse_skl(c: u32) -> u32 {
    field_prep(DP_AUX_CH_CTL_SYNC_PULSE_SKL_MASK, c - 1)
}

// ---------------------------------------------------------------------------------------------------------------
// intel_display_regs.h — DDI DP transport, buffer translation, balance leg (parked here, see module doc)
// ---------------------------------------------------------------------------------------------------------------
pub const DP_TP_CTL_A: u32 = 0x64040; // intel_display_regs.h:2302
pub const DP_TP_CTL_B: u32 = 0x64140; // intel_display_regs.h:2303
pub const DP_TP_STATUS_A: u32 = 0x64044; // intel_display_regs.h:2329
pub const DP_TP_STATUS_B: u32 = 0x64144; // intel_display_regs.h:2330
pub const DP_TP_CTL_ENABLE: u32 = bit(31); // intel_display_regs.h:2307
pub const DP_TP_CTL_MODE_MASK: u32 = bit(27); // intel_display_regs.h:2309
pub const DP_TP_CTL_MODE_SST: u32 = field_prep(DP_TP_CTL_MODE_MASK, 0); // intel_display_regs.h:2310
pub const DP_TP_CTL_MODE_MST: u32 = field_prep(DP_TP_CTL_MODE_MASK, 1); // intel_display_regs.h:2311
pub const DP_TP_CTL_ENHANCED_FRAME_ENABLE: u32 = bit(18); // intel_display_regs.h:2317
pub const DP_TP_CTL_LINK_TRAIN_MASK: u32 = genmask(10, 8); // intel_display_regs.h:2319
pub const DP_TP_CTL_LINK_TRAIN_PAT1: u32 = field_prep(DP_TP_CTL_LINK_TRAIN_MASK, 0); // intel_display_regs.h:2320
pub const DP_TP_CTL_LINK_TRAIN_PAT2: u32 = field_prep(DP_TP_CTL_LINK_TRAIN_MASK, 1); // intel_display_regs.h:2321
pub const DP_TP_CTL_LINK_TRAIN_PAT3: u32 = field_prep(DP_TP_CTL_LINK_TRAIN_MASK, 4); // intel_display_regs.h:2322
pub const DP_TP_CTL_LINK_TRAIN_PAT4: u32 = field_prep(DP_TP_CTL_LINK_TRAIN_MASK, 5); // intel_display_regs.h:2323
pub const DP_TP_CTL_LINK_TRAIN_IDLE: u32 = field_prep(DP_TP_CTL_LINK_TRAIN_MASK, 2); // intel_display_regs.h:2324
pub const DP_TP_CTL_LINK_TRAIN_NORMAL: u32 = field_prep(DP_TP_CTL_LINK_TRAIN_MASK, 3); // intel_display_regs.h:2325
pub const DP_TP_STATUS_IDLE_DONE: u32 = bit(25); // intel_display_regs.h:2335

/// `DP_TP_CTL(port)` = `_MMIO_PORT(port, _DP_TP_CTL_A, _DP_TP_CTL_B)` — intel_display_regs.h:2305.
pub const fn dp_tp_ctl(port: i915_kbl_display_core::Ddi) -> Reg {
    Reg::new(DP_TP_CTL_A + (port as u32) * (DP_TP_CTL_B - DP_TP_CTL_A))
}
/// `DP_TP_STATUS(port)` — intel_display_regs.h:2332.
pub const fn dp_tp_status(port: i915_kbl_display_core::Ddi) -> Reg {
    Reg::new(DP_TP_STATUS_A + (port as u32) * (DP_TP_STATUS_B - DP_TP_STATUS_A))
}

pub const DDI_BUF_TRANS_A: u32 = 0x64E00; // intel_display_regs.h:2379
pub const DDI_BUF_TRANS_B: u32 = 0x64E60; // intel_display_regs.h:2380
/// `DDI_BUF_BALANCE_LEG_ENABLE` (bit 31 of trans1) — intel_display_regs.h:2382.
pub const DDI_BUF_BALANCE_LEG_ENABLE: u32 = bit(31); // intel_display_regs.h:2382
/// `DDI_BUF_TRANS_LO(port, i)` — intel_display_regs.h:2381.
pub const fn ddi_buf_trans_lo(port: i915_kbl_display_core::Ddi, i: u32) -> Reg {
    Reg::new(DDI_BUF_TRANS_A + (port as u32) * (DDI_BUF_TRANS_B - DDI_BUF_TRANS_A) + i * 8)
}
/// `DDI_BUF_TRANS_HI(port, i)` — intel_display_regs.h:2383.
pub const fn ddi_buf_trans_hi(port: i915_kbl_display_core::Ddi, i: u32) -> Reg {
    Reg::new(DDI_BUF_TRANS_A + (port as u32) * (DDI_BUF_TRANS_B - DDI_BUF_TRANS_A) + i * 8 + 4)
}

/// `DISPIO_CR_TX_BMU_CR0` — intel_display_regs.h:45.
pub const DISPIO_CR_TX_BMU_CR0: Reg = Reg::new(0x6C00C); // intel_display_regs.h:45
/// `BALANCE_LEG_SHIFT(port)` — intel_display_regs.h:47.
pub const fn balance_leg_shift(port: i915_kbl_display_core::Ddi) -> u32 {
    8 + 3 * (port as u32)
}
/// `BALANCE_LEG_MASK(port)` — intel_display_regs.h:48.
pub const fn balance_leg_mask(port: i915_kbl_display_core::Ddi) -> u32 {
    7 << (8 + 3 * (port as u32))
}
/// `BALANCE_LEG_DISABLE(port)` — intel_display_regs.h:51.
pub const fn balance_leg_disable(port: i915_kbl_display_core::Ddi) -> u32 {
    1 << (23 + (port as u32))
}
/// `DDI_BUF_TRANS_SELECT(n)` = `REG_FIELD_PREP(DDI_BUF_EMP_MASK, n)` — intel_display_regs.h:2353.
pub const fn ddi_buf_trans_select(n: u32) -> u32 {
    field_prep(i915_kbl_display_core::regs::DDI_BUF_EMP_MASK, n)
}
/// `DDI_PORT_WIDTH(width)` = `REG_FIELD_PREP(DDI_PORT_WIDTH_MASK, DDI_PORT_WIDTH_ENCODE(width))`; the encode maps 3 lanes to
/// 4 and otherwise `width - 1` — intel_display_regs.h:2368-2371.
pub const fn ddi_port_width(width: u32) -> u32 {
    let enc = if width == 3 { 4 } else { width - 1 };
    field_prep(i915_kbl_display_core::regs::DDI_PORT_WIDTH_MASK, enc)
}

// ---------------------------------------------------------------------------------------------------------------
// intel_pps_regs.h — the panel power sequencer, at the PCH base (HAS_PCH_SPLIT, intel_pps.c:1775)
// ---------------------------------------------------------------------------------------------------------------
/// `PPS_BASE` — intel_pps_regs.h:12.
pub const PPS_BASE: u32 = 0x61200; // intel_pps_regs.h:12
/// `PCH_PPS_BASE` — intel_pps_regs.h:14. `intel_pps_setup()` selects it for every `HAS_PCH_SPLIT` platform (intel_pps.c:1775).
pub const PCH_PPS_BASE: u32 = 0xC7200; // intel_pps_regs.h:14
pub const PP_STATUS_OFF: u32 = 0x61200; // intel_pps_regs.h:19
pub const PP_CONTROL_OFF: u32 = 0x61204; // intel_pps_regs.h:46
pub const PP_ON_DELAYS_OFF: u32 = 0x61208; // intel_pps_regs.h:56
pub const PP_OFF_DELAYS_OFF: u32 = 0x6120C; // intel_pps_regs.h:67
pub const PP_DIVISOR_OFF: u32 = 0x61210; // intel_pps_regs.h:72

/// `_MMIO_PPS(display, pps_idx, reg)` = `mmio_base - PPS_BASE + reg + pps_idx * 0x100` — intel_pps_regs.h:16-17, with
/// `mmio_base` = [`PCH_PPS_BASE`].
const fn pps_reg(pps_idx: u32, reg: u32) -> Reg {
    Reg::new(PCH_PPS_BASE - PPS_BASE + reg + pps_idx * 0x100)
}
/// `PP_STATUS(display, pps_idx)` — intel_pps_regs.h:20.
pub const fn pp_status(pps_idx: u32) -> Reg {
    pps_reg(pps_idx, PP_STATUS_OFF)
}
/// `PP_CONTROL(display, pps_idx)` — intel_pps_regs.h:47.
pub const fn pp_control(pps_idx: u32) -> Reg {
    pps_reg(pps_idx, PP_CONTROL_OFF)
}
/// `PP_ON_DELAYS(display, pps_idx)` — intel_pps_regs.h:57.
pub const fn pp_on_delays(pps_idx: u32) -> Reg {
    pps_reg(pps_idx, PP_ON_DELAYS_OFF)
}
/// `PP_OFF_DELAYS(display, pps_idx)` — intel_pps_regs.h:68.
pub const fn pp_off_delays(pps_idx: u32) -> Reg {
    pps_reg(pps_idx, PP_OFF_DELAYS_OFF)
}
/// `PP_DIVISOR(display, pps_idx)` — intel_pps_regs.h:73.
pub const fn pp_divisor(pps_idx: u32) -> Reg {
    pps_reg(pps_idx, PP_DIVISOR_OFF)
}

pub const PP_ON: u32 = bit(31); // intel_pps_regs.h:21
pub const PP_READY: u32 = bit(30); // intel_pps_regs.h:29
pub const PP_SEQUENCE_MASK: u32 = genmask(29, 28); // intel_pps_regs.h:30
pub const PP_SEQUENCE_NONE: u32 = field_prep(PP_SEQUENCE_MASK, 0); // intel_pps_regs.h:31
pub const PP_SEQUENCE_POWER_UP: u32 = field_prep(PP_SEQUENCE_MASK, 1); // intel_pps_regs.h:32
pub const PP_SEQUENCE_POWER_DOWN: u32 = field_prep(PP_SEQUENCE_MASK, 2); // intel_pps_regs.h:33
pub const PP_CYCLE_DELAY_ACTIVE: u32 = bit(27); // intel_pps_regs.h:34
pub const PP_SEQUENCE_STATE_MASK: u32 = genmask(3, 0); // intel_pps_regs.h:35
pub const PP_SEQUENCE_STATE_OFF_IDLE: u32 = field_prep(PP_SEQUENCE_STATE_MASK, 0x0); // intel_pps_regs.h:36
pub const PP_SEQUENCE_STATE_ON_IDLE: u32 = field_prep(PP_SEQUENCE_STATE_MASK, 0x8); // intel_pps_regs.h:40

pub const PANEL_UNLOCK_MASK: u32 = genmask(31, 16); // intel_pps_regs.h:48
pub const PANEL_UNLOCK_REGS: u32 = field_prep(PANEL_UNLOCK_MASK, 0xabcd); // intel_pps_regs.h:49
pub const BXT_POWER_CYCLE_DELAY_MASK: u32 = genmask(8, 4); // intel_pps_regs.h:50
pub const EDP_FORCE_VDD: u32 = bit(3); // intel_pps_regs.h:51
pub const EDP_BLC_ENABLE: u32 = bit(2); // intel_pps_regs.h:52
pub const PANEL_POWER_RESET: u32 = bit(1); // intel_pps_regs.h:53
pub const PANEL_POWER_ON: u32 = bit(0); // intel_pps_regs.h:54

pub const PANEL_PORT_SELECT_MASK: u32 = genmask(31, 30); // intel_pps_regs.h:58
pub const PANEL_PORT_SELECT_DPA: u32 = field_prep(PANEL_PORT_SELECT_MASK, 1); // intel_pps_regs.h:60
pub const PANEL_POWER_UP_DELAY_MASK: u32 = genmask(28, 16); // intel_pps_regs.h:64
pub const PANEL_LIGHT_ON_DELAY_MASK: u32 = genmask(12, 0); // intel_pps_regs.h:65
pub const PANEL_POWER_DOWN_DELAY_MASK: u32 = genmask(28, 16); // intel_pps_regs.h:69
pub const PANEL_LIGHT_OFF_DELAY_MASK: u32 = genmask(12, 0); // intel_pps_regs.h:70
pub const PP_REFERENCE_DIVIDER_MASK: u32 = genmask(31, 8); // intel_pps_regs.h:74
pub const PANEL_POWER_CYCLE_DELAY_MASK: u32 = genmask(4, 0); // intel_pps_regs.h:75

// ---------------------------------------------------------------------------------------------------------------
// intel_backlight_regs.h / i915_reg.h — the PCH PWM
// ---------------------------------------------------------------------------------------------------------------
pub const BLC_PWM_CPU_CTL2: Reg = Reg::new(0x48250); // intel_backlight_regs.h:74
pub const BLM_PWM_ENABLE: u32 = bit(31); // intel_backlight_regs.h:25
pub const BLC_PWM_PCH_CTL1: Reg = Reg::new(0xc8250); // intel_backlight_regs.h:81
pub const BLM_PCH_PWM_ENABLE: u32 = bit(31); // intel_backlight_regs.h:82
pub const BLM_PCH_OVERRIDE_ENABLE: u32 = bit(30); // intel_backlight_regs.h:83
pub const BLM_PCH_POLARITY: u32 = bit(29); // intel_backlight_regs.h:84
pub const BLC_PWM_PCH_CTL2: Reg = Reg::new(0xc8254); // intel_backlight_regs.h:85
/// `BACKLIGHT_DUTY_CYCLE_MASK` (0xffff) — intel_backlight_regs.h:65.
pub const BACKLIGHT_DUTY_CYCLE_MASK: u32 = 0xffff; // intel_backlight_regs.h:65
/// `SOUTH_CHICKEN1` — i915_reg.h:1035.
pub const SOUTH_CHICKEN1: Reg = Reg::new(0xc2000); // i915_reg.h:1035
/// `SPT_PWM_GRANULARITY` (SOUTH_CHICKEN1 bit 0) — i915_reg.h:1055.
pub const SPT_PWM_GRANULARITY: u32 = bit(0); // i915_reg.h:1055

// ---------------------------------------------------------------------------------------------------------------
// intel_display_regs.h — PCH raw clock (the PPS divisor and the SPT PWM both derive from it)
// ---------------------------------------------------------------------------------------------------------------
/// `PCH_RAWCLK_FREQ` — intel_display_regs.h:1807. Parked here like the other `intel_display_regs.h` registers (module doc).
pub const PCH_RAWCLK_FREQ: Reg = Reg::new(0xc6204); // intel_display_regs.h:1807
/// `RAWCLK_FREQ_MASK` — intel_display_regs.h:1812.
pub const RAWCLK_FREQ_MASK: u32 = 0x3ff; // intel_display_regs.h:1812
