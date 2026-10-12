// SPDX-License-Identifier: GPL-2.0-only
//! DdiConfig assembly from VBT facts and the firmware's DDI_BUF_CTL (intel_ddi.c:5413-5418).
use i915_display_link_core::buf_trans::*;
use i915_display_link_core::ddi::DdiConfig;
use i915_display_link_core::pps::PpsDelays;
use i915_display_link_core::vbt_link::*;
use i915_kbl_display_core::Ddi;
use i915_vbt_core::child::AuxCh;

fn inputs(lane_reversal: bool, boost: u8, low_vswing: bool) -> EdpVbtInputs {
    EdpVbtInputs {
        aux_ch: AuxCh::A,
        dp_boost: boost,
        lane_reversal,
        child_max_link_rate: 0,
        child_max_lanes: 0,
        edp_low_vswing: low_vswing,
        edp_max_link_rate: None,
        pps: PpsDelays::default(),
        backlight: None,
    }
}

#[test]
fn lane_reversal_is_the_vbt_flag_or_the_buffer_strap() {
    // dig_port->lane_reversal = intel_bios_encoder_lane_reversal() || ddi_buf_ctl & DDI_BUF_PORT_REVERSAL
    let rev = 1u32 << 16;
    assert!(!DdiConfig::edp_ddi_a(Sku::KblY, &inputs(false, 0, true), 0, 4, false).lane_reversal);
    assert!(DdiConfig::edp_ddi_a(Sku::KblY, &inputs(true, 0, true), 0, 4, false).lane_reversal);
    assert!(DdiConfig::edp_ddi_a(Sku::KblY, &inputs(false, 0, true), rev, 4, false).lane_reversal);
    assert!(DdiConfig::edp_ddi_a(Sku::KblY, &inputs(true, 0, true), rev, 4, false).lane_reversal);
}

#[test]
fn the_four_lane_strap_sets_max_lanes_and_the_flag_together() {
    let c4 = DdiConfig::edp_ddi_a(Sku::KblY, &inputs(false, 0, true), 1 << 4, 4, false);
    assert!(c4.ddi_a_4_lanes && c4.max_lanes == 4);
    let c2 = DdiConfig::edp_ddi_a(Sku::KblY, &inputs(false, 0, true), 0, 2, false);
    assert!(!c2.ddi_a_4_lanes && c2.max_lanes == 2);
    // other bits of DDI_BUF_CTL do not look like the strap
    let c = DdiConfig::edp_ddi_a(Sku::KblY, &inputs(false, 0, true), 0x8000_000E, 2, false);
    assert!(!c.ddi_a_4_lanes && c.max_lanes == 2 && !c.lane_reversal);
}

#[test]
fn boost_framing_port_and_table_follow_the_inputs() {
    let c = DdiConfig::edp_ddi_a(Sku::KblU, &inputs(false, 7, false), 0, 2, true);
    assert_eq!((c.port, c.vbt_dp_boost, c.lane_count, c.enhanced_framing), (Ddi::A, 7, 2, true));
    // eDP without low vswing: the SKU's plain DP table (kbl_u_trans_dp, 9 rows)
    assert_eq!(c.trans.entries.as_ptr(), KBL_U_TRANS_DP.as_ptr());
    assert_eq!(c.trans.n_entries, 9);
    let c = DdiConfig::edp_ddi_a(Sku::KblU, &inputs(false, 0, true), 0, 2, false);
    assert_eq!(c.trans.entries.as_ptr(), SKL_U_TRANS_EDP.as_ptr());
    assert_eq!(c.trans.n_entries, 10);
}
