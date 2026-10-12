// SPDX-License-Identifier: GPL-2.0-only
//! The VBT facts the eDP link needs, gathered ONCE from `i915-vbt-core` (this crate never re-parses a VBT): which AUX channel
//! DDI-A's eDP child uses, its I_boost / lane-reversal / link-rate / lane-count limits, the eDP block's low-vswing select and
//! power sequence, and the panel's PWM backlight description.
//!
//! Linux reads the same fields through `intel_bios_dp_aux_ch()` (intel_bios.c:3718-3724), `intel_bios_dp_boost_level()`
//! (:3747-3753), `intel_bios_encoder_lane_reversal()`, `intel_bios_dp_max_link_rate()` (:2498-2507),
//! `intel_bios_dp_max_lane_count()` (:2509-2515) and `connector->panel.vbt.{edp,backlight}` (`parse_edp()` :1416-1548,
//! `parse_lfp_backlight()` :1014-1099), each already ported by `i915-vbt-core`. The DDI-A child is found as
//! `intel_bios_encoder_data` is: the child whose `dvo_port` maps to port A (`dvo_port_to_port`, intel_bios.c:2344-2352) and which
//! `supports_edp()`. When a VBT has no eDP block the Linux defaults apply (`low_vswing` false, an all-zero power sequence, which
//! `PpsTiming::compute` turns into the eDP spec limits).
use crate::ddi::DdiConfig;
use crate::buf_trans::{get_buf_trans, Sku};
use crate::caps::ddi_a_max_lanes;
use crate::pps::PpsDelays;
use alloc::vec::Vec;
use i915_kbl_display_core::regs::{DDI_A_4_LANES, DDI_BUF_PORT_REVERSAL};
use i915_kbl_display_core::Ddi;
use i915_vbt_core::backlight::{parse_lfp_backlight, Backlight};
use i915_vbt_core::child::{AuxCh, Port};
use i915_vbt_core::edp::parse_edp;
use i915_vbt_core::general::parse_general_definitions;
use i915_vbt_core::{Vbt, VbtError};

/// Why the VBT could not describe the link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VbtLinkError {
    /// The VBT parser refused (named inside).
    Vbt(VbtError),
    /// No child device on DDI A that is eDP: this VBT does not describe an internal panel on port A.
    NoEdpChildOnDdiA,
}

impl VbtLinkError {
    pub const fn name(&self) -> &'static str {
        match self {
            VbtLinkError::Vbt(_) => "VBT-LINK-VBT-REFUSED",
            VbtLinkError::NoEdpChildOnDdiA => "VBT-LINK-NO-EDP-CHILD-ON-DDI-A",
        }
    }
}

impl From<VbtError> for VbtLinkError {
    fn from(e: VbtError) -> VbtLinkError {
        VbtLinkError::Vbt(e)
    }
}

/// Everything the link planners take from the VBT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdpVbtInputs {
    /// `intel_dp_aux_ch()`: the VBT's channel, else the port's default (DDI A -> AUX A, intel_dp_aux.c:`default_aux_ch`).
    pub aux_ch: AuxCh,
    /// `intel_bios_dp_boost_level()`: 0, or the translated I_boost (1 / 3 / 7).
    pub dp_boost: u8,
    /// `intel_bios_encoder_lane_reversal()`.
    pub lane_reversal: bool,
    /// `intel_bios_dp_max_link_rate()`; 0 = platform default.
    pub child_max_link_rate: u32,
    /// `intel_bios_dp_max_lane_count()`; 0 = unspecified.
    pub child_max_lanes: u8,
    /// `connector->panel.vbt.edp.low_vswing`.
    pub edp_low_vswing: bool,
    /// `connector->panel.vbt.edp.max_link_rate` (BDB 244+).
    pub edp_max_link_rate: Option<u32>,
    /// `connector->panel.vbt.edp.pps`.
    pub pps: PpsDelays,
    /// `connector->panel.vbt.backlight` (`None` if the VBT has no backlight block).
    pub backlight: Option<Backlight>,
}

/// Collect the eDP-on-DDI-A inputs for `panel_type` (the caller selects it with `i915_vbt_core::panel::select_panel_type`).
pub fn edp_inputs(vbt: &Vbt, panel_type: u8) -> Result<EdpVbtInputs, VbtLinkError> {
    let gd = parse_general_definitions(vbt)?;
    let child = gd
        .children
        .iter()
        .find(|c| c.port() == Some(Port::A) && c.supports_edp())
        .ok_or(VbtLinkError::NoEdpChildOnDdiA)?;
    let edp = parse_edp(vbt, panel_type)?;
    let backlight = parse_lfp_backlight(vbt, panel_type)?;
    Ok(EdpVbtInputs {
        aux_ch: child.aux_ch().unwrap_or(AuxCh::A),
        dp_boost: child.dp_boost_level(),
        lane_reversal: child.lane_reversal(),
        child_max_link_rate: child.dp_max_link_rate(),
        child_max_lanes: child.dp_max_lane_count(),
        edp_low_vswing: edp.as_ref().and_then(|e| e.low_vswing).unwrap_or(false),
        edp_max_link_rate: edp.as_ref().and_then(|e| e.max_link_rate),
        pps: edp.as_ref().map(PpsDelays::from_vbt).unwrap_or_default(),
        backlight,
    })
}

impl DdiConfig {
    /// The DDI-A eDP configuration `intel_ddi_init()` + `intel_ddi_init_dp_buf_reg()` + `get_buf_trans` arrive at:
    /// `ddi_buf_ctl_a` is the `DDI_BUF_CTL(A)` the firmware left (`DDI_A_4_LANES` and `DDI_BUF_PORT_REVERSAL` are read from it,
    /// intel_ddi.c:5413-5418). `lane_count` / `enhanced_framing` are the link's.
    pub fn edp_ddi_a(sku: Sku, v: &EdpVbtInputs, ddi_buf_ctl_a: u32, lane_count: u8, enhanced_framing: bool) -> DdiConfig {
        DdiConfig {
            port: Ddi::A,
            lane_count,
            lane_reversal: v.lane_reversal || ddi_buf_ctl_a & DDI_BUF_PORT_REVERSAL != 0,
            ddi_a_4_lanes: ddi_buf_ctl_a & DDI_A_4_LANES != 0,
            max_lanes: ddi_a_max_lanes(ddi_buf_ctl_a),
            vbt_dp_boost: v.dp_boost,
            trans: get_buf_trans(sku, true, v.edp_low_vswing, Ddi::A),
            enhanced_framing,
        }
    }
}

/// All children that describe an eDP port, for diagnostics.
pub fn edp_child_ports(vbt: &Vbt) -> Result<Vec<Port>, VbtLinkError> {
    let gd = parse_general_definitions(vbt)?;
    Ok(gd.children.iter().filter(|c| c.supports_edp()).filter_map(|c| c.port()).collect())
}
