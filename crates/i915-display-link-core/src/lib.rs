// SPDX-License-Identifier: GPL-2.0-only
//! i915-display-link-core: the eDP link bring-up of a Kaby Lake (display version 9, SPT-LP PCH) panel on DDI-A, as
//! planners. NOTHING here touches a register or a wire: every operation is a state machine or a plan that names the
//! register accesses and AUX transactions it wants, and takes what the hardware answered as its next input. Only
//! `devices/i915` executes effects.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/{intel_dp_aux.c, intel_dp_link_training.c, intel_ddi.c,
//! intel_ddi_buf_trans.c, intel_pps.c, intel_backlight.c, intel_dp.c} and drivers/gpu/drm/display/drm_dp_helper.c at
//! v7.0-rc6; every register, constant, table row and bound carries FILE:LINE. Copyright (c) Intel Corporation and the
//! Linux DRM authors.
//!
//! MODULES (one concern each):
//!  * [`regs`], [`dpdefs`]  — register offsets / DPCD constants
//!  * [`plan`]              — [`plan::Effect`] / [`plan::PollPlan`], the bounded-wait vocabulary
//!  * [`aux`]               — the SKL AUX channel: message packing, `DP_AUX_CH_CTL` send/receive state machine
//!  * [`dpcd`]              — `drm_dp_dpcd_access()`: 32 retries over AUX, the probe-before-read rule
//!  * [`caps`]              — DPCD capability parse, eDP rate table, source/sink/common rates, lane counts
//!  * [`train`]             — clock-recovery + channel-equalisation as a state machine with Linux's exact loop limits
//!  * [`buf_trans`]         — the SKL/KBL DP and eDP buffer translation tables (U / Y / H-S) and their selection
//!  * [`ddi`]               — DDI_BUF_CTL / DP_TP_CTL / DDI_BUF_TRANS sequencing for DDI-A as plans
//!  * [`pps`]               — the SPT-LP panel power sequencer: delays (VBT/hw/spec), VDD, power on/off, backlight gate
//!  * [`sinkpower`]         — `intel_dp_set_power()`: sink D0 / D3 over DPCD
//!  * [`linkstatus`]        — drm_dp_helper.c lane-status / adjust-request / read-interval helpers
//!  * [`vbt_link`]          — the VBT facts the above need, gathered from `i915-vbt-core`
//!  * [`backlight`]         — the SPT PWM (BLC_PWM_PCH_CTL1/2): frequency from the VBT, enable / set / disable
//!
//! THE ORDER A CALLER FOLLOWS (Linux `hsw_ddi_pre_enable_dp`, intel_ddi.c:2909-2954, KBL arm; the clock / power / transcoder steps
//! are other crates'): `vbt_link::edp_inputs` and a `caps::CapsProbe` (over `dpcd::DpcdXfer`) give the link description ->
//! `pps::PpsTiming::registers_plan` once -> `pps::Kind::PanelOn` (`intel_pps_on`) -> [DDI clock select, DDI IO power] ->
//! `ddi::DdiLink::prepare_dp_buffers` -> `set_signal_levels` -> `sinkpower::SetPower` D0 -> `train::LinkTrainer` (which does
//! `prepare_link_retrain`, clock recovery, channel equalisation, pattern disable, idle) -> `DdiLink::stop_link_train` ->
//! [transcoder clock / function] -> `backlight::BacklightState::enable_plan` then `pps::Kind::BacklightOn`. Teardown is
//! `Kind::BacklightOff`, `backlight::disable_plan`, `SetPower` D3, `DdiLink::buf_disable`, `Kind::VddOn` + `Kind::PanelOff`.
//!
//! WHAT IS DELIBERATELY NOT PORTED (first display milestone = KBL SPT-LP eDP, single pipe, DDI-A): LTTPRs (an eDP panel has
//! none: `lttpr_count` is 0), 128b/132b (UHBR) training, MST, FEC/DSC, HDMI/DVI buffers, I2C-over-AUX and EDID reads,
//! panel replay / PSR sink setup, hotplug, DP quirks tables (`intel_has_dpcd_quirk`), and the `intel_dp_schedule_fallback`
//! re-modeset (only its pure rate/lane reduction is ported). Each is named at its point of omission.
//!
//! DEVIATION FROM LINUX, in one place: Linux waits with the kernel's clock and sleeps (`msleep`, `usleep_range`,
//! `wait_event_timeout`); here every wait is an [`plan::Effect::DelayUs`] / [`plan::PollPlan`] the executor performs, and every
//! poll carries a hard read cap so a host with no clock still terminates.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

pub mod aux;
pub mod backlight;
pub mod buf_trans;
pub mod caps;
pub mod ddi;
pub mod linkstatus;
pub mod sinkpower;
pub mod dpcd;
pub mod dpdefs;
pub mod plan;
pub mod pps;
pub mod regs;
pub mod train;
pub mod vbt_link;
