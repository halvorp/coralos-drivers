// SPDX-License-Identifier: GPL-2.0-only
//! drm_dp_helper.c link-status decode (v7.0-rc6): lane nibbles, adjust requests per lane, the read intervals.
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::linkstatus::*;

#[test]
fn lane_status_is_two_lanes_per_byte_low_nibble_first() {
    // 0x202: lane0 | lane1 << 4, 0x203: lane2 | lane3 << 4 (drm_dp_helper.c:82-90)
    let ls: LinkStatus = [0x21, 0x43, 0, 0, 0, 0];
    assert_eq!(lane_status(&ls, 0), 0x1);
    assert_eq!(lane_status(&ls, 1), 0x2);
    assert_eq!(lane_status(&ls, 2), 0x3);
    assert_eq!(lane_status(&ls, 3), 0x4);
}

#[test]
fn clock_recovery_looks_at_exactly_the_active_lanes() {
    let ls: LinkStatus = [0x11, 0x01, 0, 0, 0, 0]; // lanes 0,1,2 CR done, lane 3 not
    assert!(clock_recovery_ok(&ls, 1));
    assert!(clock_recovery_ok(&ls, 2));
    assert!(clock_recovery_ok(&ls, 3));
    assert!(!clock_recovery_ok(&ls, 4));
    let ls: LinkStatus = [0x10, 0x11, 0, 0, 0, 0]; // lane 0 missing
    assert!(!clock_recovery_ok(&ls, 1));
    assert!(!clock_recovery_ok(&ls, 4));
    assert!(clock_recovery_ok(&ls, 0)); // no lanes, vacuously
}

#[test]
fn channel_eq_needs_align_and_all_three_bits_on_every_active_lane() {
    let full = 0x7u8;
    let ls: LinkStatus = [full | (full << 4), full | (full << 4), 0x01, 0, 0, 0];
    assert!(channel_eq_ok(&ls, 4));
    let mut no_align = ls;
    no_align[2] = 0;
    assert!(!channel_eq_ok(&no_align, 4));
    for (lane, byte, shift) in [(0u32, 0usize, 0u32), (1, 0, 4), (2, 1, 0), (3, 1, 4)] {
        for missing in [0x1u8, 0x2, 0x4] {
            let mut t = ls;
            t[byte] &= !(missing << shift);
            assert!(!channel_eq_ok(&t, lane + 1), "lane {lane} missing bit {missing:#x}");
            if lane > 0 {
                assert!(channel_eq_ok(&t, lane), "lanes below {lane} are fine");
            }
        }
    }
    // the align bit is bit 0 of 0x204; other bits there do not count
    let mut other = ls;
    other[2] = 0x02;
    assert!(!channel_eq_ok(&other, 4));
    other[2] = 0xFF;
    assert!(channel_eq_ok(&other, 4));
}

#[test]
fn adjust_requests_decode_per_lane_with_the_shifts_of_drm_dp_h() {
    // 0x206: lane0 v[1:0] p[3:2], lane1 v[5:4] p[7:6]; 0x207 likewise for lanes 2 and 3.
    // lane0 (v1,p2) = 0b1001 -> low nibble 0x9; lane1 (v3,p1) = 0b0111 -> 0x7; lane2 (v2,p3) = 0b1110 -> 0xE; lane3 (v0,p1) = 0b0100 -> 0x4
    let ls: LinkStatus = [0, 0, 0, 0, 0x79, 0x4E];
    let want = [(1u8, 2u8), (3, 1), (2, 3), (0, 1)];
    for (lane, (v, p)) in want.iter().enumerate() {
        assert_eq!(adjust_request_voltage(&ls, lane as u32), v << DP_TRAIN_VOLTAGE_SWING_SHIFT, "lane {lane} voltage");
        assert_eq!(adjust_request_pre_emphasis(&ls, lane as u32), p << 3, "lane {lane} pre-emphasis");
    }
    // literal values, independent of the shifts above
    assert_eq!(adjust_request_voltage(&ls, 0), 1);
    assert_eq!(adjust_request_pre_emphasis(&ls, 0), 0x10);
    assert_eq!(adjust_request_voltage(&ls, 1), 3);
    assert_eq!(adjust_request_pre_emphasis(&ls, 1), 0x08);
    assert_eq!(adjust_request_voltage(&ls, 2), 2);
    assert_eq!(adjust_request_pre_emphasis(&ls, 2), 0x18);
    assert_eq!(adjust_request_voltage(&ls, 3), 0);
    assert_eq!(adjust_request_pre_emphasis(&ls, 3), 0x08);
}

#[test]
fn read_intervals_convert_to_delays() {
    // __8b10b_clock_recovery_delay_us / __8b10b_channel_eq_delay_us (drm_dp_helper.c:238-260)
    assert_eq!(cr_delay_us_from_interval(0), 100);
    assert_eq!(eq_delay_us_from_interval(0), 400);
    for (i, us) in [(1u8, 4_000u32), (2, 8_000), (3, 12_000), (4, 16_000)] {
        assert_eq!(cr_delay_us_from_interval(i), us);
        assert_eq!(eq_delay_us_from_interval(i), us);
    }
}

#[test]
fn the_dpcd_14_clock_recovery_delay_is_fixed_and_older_sinks_use_the_interval() {
    let mut d = [0u8; 15];
    d[0] = 0x14;
    d[0x0e] = 0x03;
    assert_eq!(clock_recovery_delay_us(&d), 100); // DPCD >= 1.4 ignores the interval for clock recovery
    d[0] = 0x13;
    assert_eq!(clock_recovery_delay_us(&d), 12_000);
    d[0] = 0x12;
    d[0x0e] = 0x00;
    assert_eq!(clock_recovery_delay_us(&d), 100);
    // the interval field is 7 bits (bit 7 is "extended receiver cap present")
    d[0x0e] = 0x80 | 0x02;
    assert_eq!(clock_recovery_delay_us(&d), 8_000);
    // channel equalisation always honours it
    d[0] = 0x14;
    assert_eq!(channel_eq_delay_us(&d), 8_000);
    d[0x0e] = 0;
    assert_eq!(channel_eq_delay_us(&d), 400);
}
