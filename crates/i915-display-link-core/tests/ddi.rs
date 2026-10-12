// SPDX-License-Identifier: GPL-2.0-only
//! DDI sequencing vs intel_ddi.c (v7.0-rc6), with the Linux macros evaluated by hand in the expected values.
#![allow(unused_imports)]
use i915_common_core::Reg;
use i915_display_link_core::buf_trans::*;
use i915_display_link_core::ddi::*;
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::plan::{written_regs, Effect, PollPlan};
use i915_display_link_core::regs::*;
use i915_kbl_display_core::Ddi;

fn cfg(port: Ddi, lanes: u8, boost: u8, edp_low: bool, max_lanes: u8) -> DdiConfig {
    DdiConfig {
        port,
        lane_count: lanes,
        lane_reversal: false,
        ddi_a_4_lanes: max_lanes == 4,
        max_lanes,
        vbt_dp_boost: boost,
        trans: get_buf_trans(Sku::KblY, true, edp_low, port),
        enhanced_framing: false,
    }
}

// ---- registers -----------------------------------------------------------------------------------------------------
#[test]
fn ddi_register_offsets_and_fields_are_linuxs() {
    // intel_display_regs.h:2302-2342, :2379-2383, :45-51
    assert_eq!(dp_tp_ctl(Ddi::A).offset(), 0x64040);
    assert_eq!(dp_tp_ctl(Ddi::B).offset(), 0x64140);
    assert_eq!(dp_tp_ctl(Ddi::E).offset(), 0x64440);
    assert_eq!(dp_tp_status(Ddi::A).offset(), 0x64044);
    assert_eq!(dp_tp_status(Ddi::D).offset(), 0x64344);
    assert_eq!(ddi_buf_trans_lo(Ddi::A, 0).offset(), 0x64E00);
    assert_eq!(ddi_buf_trans_hi(Ddi::A, 0).offset(), 0x64E04);
    assert_eq!(ddi_buf_trans_lo(Ddi::A, 9).offset(), 0x64E48);
    assert_eq!(ddi_buf_trans_hi(Ddi::A, 9).offset(), 0x64E4C);
    assert_eq!(ddi_buf_trans_lo(Ddi::B, 0).offset(), 0x64E60);
    assert_eq!(ddi_buf_trans_lo(Ddi::E, 0).offset(), 0x64F80);
    assert_eq!(DISPIO_CR_TX_BMU_CR0.offset(), 0x6C00C);
    assert_eq!(DP_TP_CTL_ENABLE, 0x8000_0000);
    assert_eq!(DP_TP_CTL_MODE_SST, 0);
    assert_eq!(DP_TP_CTL_MODE_MST, 1 << 27);
    assert_eq!(DP_TP_CTL_ENHANCED_FRAME_ENABLE, 1 << 18);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_MASK, 0x700);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_PAT1, 0x000);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_PAT2, 0x100);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_IDLE, 0x200);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_NORMAL, 0x300);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_PAT3, 0x400);
    assert_eq!(DP_TP_CTL_LINK_TRAIN_PAT4, 0x500);
    assert_eq!(DP_TP_STATUS_IDLE_DONE, 1 << 25);
    assert_eq!(DDI_BUF_BALANCE_LEG_ENABLE, 1 << 31);
    assert_eq!(balance_leg_shift(Ddi::A), 8);
    assert_eq!(balance_leg_shift(Ddi::E), 20);
    assert_eq!(balance_leg_mask(Ddi::A), 0x700);
    assert_eq!(balance_leg_mask(Ddi::E), 0x70_0000);
    assert_eq!(balance_leg_disable(Ddi::A), 1 << 23);
    assert_eq!(balance_leg_disable(Ddi::E), 1 << 27);
    assert_eq!(ddi_buf_trans_select(5), 5 << 24);
    assert_eq!(ddi_port_width(1), 0);
    assert_eq!(ddi_port_width(2), 1 << 1);
    assert_eq!(ddi_port_width(3), 4 << 1); // DDI_PORT_WIDTH_ENCODE(3) == 4
    assert_eq!(ddi_port_width(4), 3 << 1);
    // the DDI_BUF_CTL register is the kbl-display-core one, not a second copy
    assert_eq!(ddi_buf_ctl_reg(Ddi::A).offset(), 0x64000);
    assert_eq!(ddi_buf_ctl_reg(Ddi::B).offset(), 0x64100);
}

#[test]
fn register_waits_use_linuxs_timeouts() {
    // intel_ddi.c:213-215 (10 ms), :3866 (2 ms), :244-246 (518 us), and the poll spacing used here
    assert_eq!(DDI_BUF_IDLE_TIMEOUT_US, 10_000);
    assert_eq!(DP_IDLE_DONE_TIMEOUT_US, 2_000);
    assert_eq!(DDI_BUF_ACTIVE_WAIT_US, 518);
    assert_eq!(DDI_POLL_INTERVAL_US, 10);
    assert!(HAS_IBOOST && HAS_BUF_TRANS_SELECT);
}

#[test]
fn the_buf_ctl_value_is_width_trans_select_reversal_and_a_4_lanes() {
    // intel_ddi_init_dp_buf_reg(), intel_ddi.c:353-377 (no ENABLE yet)
    assert_eq!(DdiLink::new(cfg(Ddi::A, 4, 0, true, 4)).dp, (3 << 1) | (1 << 4));
    assert_eq!(DdiLink::new(cfg(Ddi::A, 2, 0, true, 2)).dp, 1 << 1);
    assert_eq!(DdiLink::new(cfg(Ddi::A, 1, 0, true, 2)).dp, 0);
    let mut c = cfg(Ddi::A, 2, 0, true, 2);
    c.lane_reversal = true;
    assert_eq!(DdiLink::new(c).dp, (1 << 1) | (1 << 16));
}

// ---- translation tables into hardware ------------------------------------------------------------------------------
#[test]
fn prepare_buffers_writes_every_usable_row_lo_then_hi() {
    // hsw_prepare_dp_ddi_buffers(), intel_ddi.c:134-157: ten rows for DDI A with the Y eDP table
    let l = DdiLink::new(cfg(Ddi::A, 4, 0, true, 4));
    let p = l.prepare_dp_buffers();
    assert_eq!(p.len(), 20);
    let t = &SKL_Y_TRANS_EDP;
    for i in 0..10 {
        assert_eq!(p[2 * i], Effect::Write { reg: Reg::new(0x64E00 + 8 * i as u32), value: t[i].trans1 });
        assert_eq!(p[2 * i + 1], Effect::Write { reg: Reg::new(0x64E04 + 8 * i as u32), value: t[i].trans2 });
    }
    // the Atlas eDP table has no balance-leg bit in trans1 unless the VBT asks for I_boost
    assert!(p.iter().step_by(2).all(|e| matches!(e, Effect::Write { value, .. } if value & 0x8000_0000 == 0)));
}

#[test]
fn a_vbt_dp_boost_sets_balance_leg_enable_in_every_trans1() {
    let l = DdiLink::new(cfg(Ddi::A, 4, 3, true, 4));
    let p = l.prepare_dp_buffers();
    for (i, e) in p.iter().step_by(2).enumerate() {
        assert_eq!(*e, Effect::Write { reg: Reg::new(0x64E00 + 8 * i as u32), value: SKL_Y_TRANS_EDP[i].trans1 | 0x8000_0000 });
    }
    // trans2 is untouched
    assert!(matches!(p[1], Effect::Write { value, .. } if value == SKL_Y_TRANS_EDP[0].trans2));
}

#[test]
fn a_nine_row_port_writes_nine_rows() {
    let l = DdiLink::new(cfg(Ddi::B, 2, 0, true, 2));
    assert_eq!(l.prepare_dp_buffers().len(), 18);
    assert_eq!(written_regs(&l.prepare_dp_buffers()).last().unwrap().offset(), 0x64E60 + 8 * 8 + 4);
}

// ---- signal levels -------------------------------------------------------------------------------------------------
#[test]
fn the_ten_signal_level_pairs_map_to_their_rows() {
    // index_to_dp_signal_levels, intel_ddi.c:94-105: (vswing, pre-emphasis) per row
    let want: [(u8, u8); 10] = [(0, 0), (0, 1), (0, 2), (0, 3), (1, 0), (1, 1), (1, 2), (2, 0), (2, 1), (3, 0)];
    for (i, (v, p)) in want.iter().enumerate() {
        let sl = v | (p << 3);
        assert_eq!(INDEX_TO_DP_SIGNAL_LEVELS[i], sl);
        assert_eq!(translate_signal_level(sl), Some(i));
    }
    // pairs not in the table
    for sl in [0x0bu8, 0x0f, 0x13, 0x1b, 0x1a, 0x17] {
        assert_eq!(translate_signal_level(sl), None, "{sl:#x}");
    }
}

#[test]
fn voltage_and_preemph_maxima() {
    // intel_ddi_dp_voltage_max(): the swing of the last usable row; intel_ddi_dp_preemph_max(): level 3
    assert_eq!(ddi_dp_voltage_max(10), 3);
    assert_eq!(ddi_dp_voltage_max(9), 2);
    assert_eq!(ddi_dp_voltage_max(8), 2);
    assert_eq!(ddi_dp_voltage_max(7), 1); // row 6 is (vswing 1, pre-emphasis 2)
    assert_eq!(ddi_dp_voltage_max(6), 1);
    assert_eq!(ddi_dp_voltage_max(1), 0);
    assert_eq!(ddi_dp_voltage_max(0), 0); // clamped to 1 row
    assert_eq!(ddi_dp_voltage_max(99), 3); // clamped to 10 rows
    assert_eq!(ddi_dp_preemph_max(), 0x18);
}

#[test]
fn set_signal_levels_writes_the_selected_row_into_ddi_buf_ctl() {
    let mut l = DdiLink::new(cfg(Ddi::A, 4, 0, true, 4));
    let _ = l.prepare_link_retrain();
    let base = l.dp;
    // train_set (vswing 1, pre-emphasis 2) = 0x11 -> row 6
    let p = l.set_signal_levels(&[0x11, 0, 0, 0]).unwrap();
    let expect = (base & !DDI_BUF_EMP_MASK_FOR_TEST) | (6 << 24);
    assert!(p.contains(&Effect::Write { reg: Reg::new(0x64000), value: expect }));
    assert_eq!(p.last(), Some(&Effect::PostingRead { reg: Reg::new(0x64000) }));
    assert_eq!(l.dp, expect);
    // flag bits in the request are ignored for the row lookup (MAX_SWING_REACHED = 1 << 2, MAX_PRE_EMPHASIS_REACHED = 1 << 5)
    let p2 = l.set_signal_levels(&[0x11 | 0x04 | 0x20, 0, 0, 0]).unwrap();
    assert!(p2.contains(&Effect::Write { reg: Reg::new(0x64000), value: expect }));
    // only lane 0's word selects the row
    let p3 = l.set_signal_levels(&[0x00, 0x11, 0x11, 0x11]).unwrap();
    assert!(p3.contains(&Effect::Write { reg: Reg::new(0x64000), value: (expect & !DDI_BUF_EMP_MASK_FOR_TEST) }));
}

const DDI_BUF_EMP_MASK_FOR_TEST: u32 = 0x0f00_0000;

#[test]
fn a_row_beyond_the_table_is_clamped_to_the_last_usable_row() {
    // 9-row DDI B table with a 10th-row request (v3 p0 = row 9): intel_ddi.c:1524-1527 clamps to n_entries - 1 = 8
    let mut l = DdiLink::new(cfg(Ddi::B, 2, 0, true, 2));
    let p = l.set_signal_levels(&[0x03, 0, 0, 0]).unwrap();
    assert!(p.iter().any(|e| matches!(e, Effect::Write { reg, value } if reg.offset() == 0x64100 && (value & DDI_BUF_EMP_MASK_FOR_TEST) >> 24 == 8)));
}

#[test]
fn an_unsupported_request_is_refused_by_name() {
    let mut l = DdiLink::new(cfg(Ddi::A, 4, 0, true, 4));
    let e = l.set_signal_levels(&[0x0b, 0, 0, 0]).unwrap_err(); // v3 p1
    assert_eq!(e, DdiError::UnsupportedSignalLevel { signal_levels: 0x0b });
    assert_eq!(e.name(), "DDI-UNSUPPORTED-SIGNAL-LEVEL");
}

#[test]
fn i_boost_from_the_table_goes_to_the_balance_leg_field_of_port_a_and_e() {
    // KBL-Y DP table, row 3 has i_boost 3 (:240); the VBT says nothing (0), so the table's value is used (:1105-1116)
    let mut l = DdiLink::new({
        let mut c = cfg(Ddi::A, 4, 0, false, 4);
        c.trans = get_buf_trans(Sku::KblY, true, false, Ddi::A);
        c
    });
    let p = l.set_signal_levels(&[0x18, 0, 0, 0]).unwrap(); // v0 p3 -> row 3
    assert_eq!(p[0], Effect::Rmw { reg: DISPIO_CR_TX_BMU_CR0, clear: 0x700 | (1 << 23), set: 3 << 8 });
    // "if (encoder->port == PORT_A && dig_port->max_lanes == 4) _skl_ddi_set_iboost(display, PORT_E, iboost)"
    assert_eq!(p[1], Effect::Rmw { reg: DISPIO_CR_TX_BMU_CR0, clear: 0x70_0000 | (1 << 27), set: 3 << 20 });
    // with two lanes DDI E belongs to someone else: only A
    let mut l2 = DdiLink::new({
        let mut c = cfg(Ddi::A, 2, 0, false, 2);
        c.trans = get_buf_trans(Sku::KblY, true, false, Ddi::A);
        c
    });
    let p2 = l2.set_signal_levels(&[0x18, 0, 0, 0]).unwrap();
    assert_eq!(p2.iter().filter(|e| matches!(e, Effect::Rmw { .. })).count(), 1);
}

#[test]
fn zero_i_boost_sets_the_disable_bit_instead() {
    // _skl_ddi_set_iboost(): iboost == 0 -> BALANCE_LEG_DISABLE(port), intel_ddi.c:1086-1089
    let mut l = DdiLink::new(cfg(Ddi::A, 4, 0, true, 4)); // eDP Y table: all i_boost 0
    let p = l.set_signal_levels(&[0x00, 0, 0, 0]).unwrap();
    assert_eq!(p[0], Effect::Rmw { reg: DISPIO_CR_TX_BMU_CR0, clear: 0x700 | (1 << 23), set: 1 << 23 });
}

#[test]
fn the_vbt_boost_overrides_the_table_and_an_invalid_one_writes_nothing() {
    let mut l = DdiLink::new(cfg(Ddi::A, 4, 7, true, 4));
    let p = l.set_signal_levels(&[0x00, 0, 0, 0]).unwrap();
    assert_eq!(p[0], Effect::Rmw { reg: DISPIO_CR_TX_BMU_CR0, clear: 0x700 | (1 << 23), set: 7 << 8 });
    // "Invalid I_boost value" (:1117-1121): the iboost register is left alone, DDI_BUF_CTL is still written
    let mut l = DdiLink::new(cfg(Ddi::A, 4, 2, true, 4));
    let p = l.set_signal_levels(&[0x00, 0, 0, 0]).unwrap();
    assert!(!p.iter().any(|e| matches!(e, Effect::Rmw { .. })));
    assert!(p.iter().any(|e| matches!(e, Effect::Write { reg, .. } if reg.offset() == 0x64000)));
}

// ---- enable / pattern / idle / disable -----------------------------------------------------------------------------
#[test]
fn prepare_link_retrain_enables_dp_tp_then_the_buffer_then_waits_518us() {
    let mut l = DdiLink::new(cfg(Ddi::A, 4, 0, true, 4));
    let before = l.dp;
    let p = l.prepare_link_retrain();
    assert_eq!(
        p,
        vec![
            Effect::Write { reg: Reg::new(0x64040), value: 0x8000_0000 },
            Effect::PostingRead { reg: Reg::new(0x64040) },
            Effect::Write { reg: Reg::new(0x64000), value: before | 0x8000_0000 },
            Effect::PostingRead { reg: Reg::new(0x64000) },
            Effect::DelayUs(518),
        ]
    );
    assert_eq!(l.dp, before | 0x8000_0000);
    let mut c = cfg(Ddi::A, 4, 0, true, 4);
    c.enhanced_framing = true;
    let mut l = DdiLink::new(c);
    assert_eq!(l.prepare_link_retrain()[0], Effect::Write { reg: Reg::new(0x64040), value: 0x8000_0000 | (1 << 18) });
}

#[test]
fn set_link_train_is_a_masked_read_modify_write_of_the_pattern_field() {
    let l = DdiLink::new(cfg(Ddi::A, 4, 0, true, 4));
    for (pat, want) in [(0u8, 0x300u32), (1, 0x000), (2, 0x100), (3, 0x400), (7, 0x500)] {
        // the DPCD scrambling bit never reaches DP_TP_CTL
        for flags in [0u8, DP_LINK_SCRAMBLING_DISABLE] {
            assert_eq!(l.set_link_train(pat | flags).unwrap(), vec![Effect::Rmw { reg: Reg::new(0x64040), clear: 0x700, set: want }], "pattern {pat}");
        }
    }
    let e = l.set_link_train(5).unwrap_err();
    assert_eq!(e, DdiError::UnknownTrainingPattern { pattern: 5 });
    assert_eq!(e.name(), "DDI-UNKNOWN-TRAINING-PATTERN");
    assert_eq!(l.stop_link_train(), vec![Effect::Rmw { reg: Reg::new(0x64040), clear: 0x700, set: 0x300 }]);
}

#[test]
fn idle_on_port_a_does_not_wait_but_other_ports_wait_two_ms_bounded() {
    let a = DdiLink::new(cfg(Ddi::A, 4, 0, true, 4)).set_idle_link_train();
    assert_eq!(a, vec![Effect::Rmw { reg: Reg::new(0x64040), clear: 0x700, set: 0x200 }]);
    let b = DdiLink::new(cfg(Ddi::B, 2, 0, true, 2)).set_idle_link_train();
    assert_eq!(b.len(), 2);
    match b[1] {
        Effect::Poll(p) => {
            assert_eq!((p.reg.offset(), p.mask, p.want), (0x64144, 1 << 25, 1 << 25));
            assert_eq!(p.max_polls, 2_000 / 10 + 1);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn buffer_disable_clears_enable_in_both_registers_then_waits_for_idle() {
    let mut l = DdiLink::new(cfg(Ddi::A, 4, 0, true, 4));
    let _ = l.prepare_link_retrain();
    let p = l.buf_disable();
    assert_eq!(p[0], Effect::Rmw { reg: Reg::new(0x64000), clear: 0x8000_0000, set: 0 });
    assert_eq!(p[1], Effect::Rmw { reg: Reg::new(0x64040), clear: 0x8000_0000, set: 0 });
    match p[2] {
        Effect::Poll(w) => {
            assert_eq!((w.reg.offset(), w.mask, w.want), (0x64000, 1 << 7, 1 << 7));
            assert_eq!(w.max_polls, 10_000 / 10 + 1); // 10 ms
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(l.dp & 0x8000_0000, 0);
}

#[test]
fn nothing_in_a_ddi_plan_writes_outside_the_ddi_registers() {
    let mut l = DdiLink::new(cfg(Ddi::A, 4, 3, true, 4));
    let mut all = l.prepare_dp_buffers();
    all.extend(l.prepare_link_retrain());
    all.extend(l.set_signal_levels(&[0x11, 0, 0, 0]).unwrap());
    all.extend(l.set_link_train(1).unwrap());
    all.extend(l.set_idle_link_train());
    all.extend(l.buf_disable());
    for r in written_regs(&all) {
        let o = r.offset();
        let ok = (0x64E00..0x64E60 + 0x60).contains(&o) || [0x64000, 0x64040, 0x6C00C].contains(&o);
        assert!(ok, "unexpected write {o:#x}");
    }
}
