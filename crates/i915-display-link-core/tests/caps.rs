// SPDX-License-Identifier: GPL-2.0-only
//! DPCD capability parse, rates and lanes vs intel_dp.c / drm_dp_helper.c (v7.0-rc6).
//!
//! PROVENANCE of the cap block: the Pixelbook Go's own DPCD has not been captured yet (atlas-linux-recon.sh reads
//! `i915_dpcd` on the recon boot). `EDP14_DPCD` is a representative eDP 1.4 sink, built field by field from the DPCD map
//! (drm_dp.h) the way Linux logs it in "DPCD: 14 14 84 01 ..." lines: DPCD 1.4, HBR2, 4 lanes + enhanced framing, 0.5 %
//! downspread, ANSI 8b/10b, no extended block. Replace with the recon dump when it lands.
#![allow(unused_imports)]
use i915_display_link_core::caps::*;
use i915_display_link_core::dpcd::*;
use i915_display_link_core::dpdefs::*;
use i915_kbl_display_core::regs::DDI_A_4_LANES;
use std::collections::HashMap;

const EDP14_DPCD: [u8; 15] = [0x14, 0x14, 0x84, 0x01, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
// DPCD 0x700..0x704: eDP 1.4 (0x03), TCON/backlight caps, GP caps — only [0] drives link behaviour.
const EDP14_EDP: [u8; 5] = [0x03, 0x05, 0x00, 0x00, 0x00];
// DP_SUPPORTED_LINK_RATES (0x010): eDP 1.4 intermediate rates, little-endian, units of 200 kHz:
// 8100 = 1.62 Gbps, 10800 = 2.16, 13500 = 2.7, 16200 = 3.24, 21600 = 4.32, 27000 = 5.4, then zeros.
const EDP14_RATES: [u8; 16] = [0xA4, 0x1F, 0x30, 0x2A, 0xBC, 0x34, 0x48, 0x3F, 0x60, 0x54, 0x78, 0x69, 0, 0, 0, 0];

struct Mem {
    mem: HashMap<u32, Vec<u8>>,
    fail: Vec<u32>,
    log: Vec<(u32, usize)>,
}

fn run_probe(m: &mut Mem) -> Result<SinkCaps, CapsError> {
    let mut p = CapsProbe::new();
    let mut reply = None;
    for _ in 0..16 {
        match p.step(reply.take()) {
            ProbeStep::Done(r) => return r,
            ProbeStep::Request(DpcdRequest::Read { addr, len }) => {
                m.log.push((addr, len));
                if m.fail.contains(&addr) {
                    reply = Some(Err(DpcdFail { first: DpcdErr::Nack { reply: 1 }, attempts: 32 }));
                } else {
                    let mut data = [0u8; 20];
                    let src = m.mem.get(&addr).cloned().unwrap_or_default();
                    for i in 0..len {
                        data[i] = src.get(i).copied().unwrap_or(0);
                    }
                    reply = Some(Ok(DpcdOk { len, data }));
                }
            }
            ProbeStep::Request(other) => panic!("a probe only reads: {other:?}"),
        }
    }
    panic!("probe did not finish in 16 requests");
}

fn edp14() -> Mem {
    let mut mem = HashMap::new();
    mem.insert(DP_DPCD_REV, EDP14_DPCD.to_vec());
    mem.insert(DP_EDP_DPCD_REV, EDP14_EDP.to_vec());
    mem.insert(DP_SUPPORTED_LINK_RATES, EDP14_RATES.to_vec());
    Mem { mem, fail: vec![], log: vec![] }
}

#[test]
fn the_rate_literal_is_the_edp_14_intermediate_set() {
    // sanity of the literal itself: u16 LE values 8100, 10800, 13500, 16200, 21600, 27000
    let v: Vec<u16> = EDP14_RATES.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    assert_eq!(v, vec![8100, 10800, 13500, 16200, 21600, 27000, 0, 0]);
}

#[test]
fn an_edp_14_panel_is_probed_in_linux_order_and_parsed() {
    let mut m = edp14();
    let caps = run_probe(&mut m).unwrap();
    // intel_edp_init_dpcd(): base block, [no extended block], eDP block, ALPM byte, the 16-byte rate table
    assert_eq!(m.log, vec![(0x000, 15), (0x700, 5), (0x02e, 1), (0x010, 16)]);
    assert_eq!(caps.dpcd, EDP14_DPCD);
    assert_eq!(caps.edp_dpcd, EDP14_EDP);
    assert!(!caps.use_max_params); // edp_dpcd[0] = 3 >= DP_EDP_14
    assert!(caps.use_rate_select);
    // rate = le16 * 200 / 10
    assert_eq!(caps.sink_rates.as_slice(), &[162_000, 216_000, 270_000, 324_000, 432_000, 540_000]);
    assert_eq!(caps.max_sink_lane_count, 4);
    assert!(!caps.rates_defaulted && !caps.lane_count_defaulted);
}

#[test]
fn accessors_decode_the_literal_block() {
    let d = &EDP14_DPCD;
    assert_eq!(dp_max_lane_count(d), 4); // 0x84 & 0x1f
    assert!(dp_enhanced_frame_cap(d)); // rev >= 1.1 && bit 7
    assert!(!dp_tps3_supported(d)); // bit 6 clear
    assert!(!dp_tps4_supported(d)); // max_downspread bit 7 clear
    assert!(!dp_is_branch(d));
    let mut t = *d;
    t[2] = 0xC4; // TPS3 + enhanced + 4 lanes
    assert!(dp_tps3_supported(&t));
    t[0] = 0x11;
    assert!(!dp_tps3_supported(&t)); // rev < 1.2
    assert!(dp_enhanced_frame_cap(&t)); // rev 1.1 still ok
    t[0] = 0x10;
    assert!(!dp_enhanced_frame_cap(&t));
    let mut u = *d;
    u[3] = 0x81;
    assert!(dp_tps4_supported(&u));
    u[0] = 0x13;
    assert!(!dp_tps4_supported(&u));
    u[5] = 0x01;
    assert!(dp_is_branch(&u));
}

#[test]
fn a_pre_14_edp_falls_back_to_max_link_rate_and_bw_set() {
    let mut m = edp14();
    m.mem.insert(DP_EDP_DPCD_REV, vec![0x02, 0, 0, 0, 0]); // eDP 1.3
    let caps = run_probe(&mut m).unwrap();
    assert!(caps.use_max_params);
    assert!(!caps.use_rate_select);
    // MAX_LINK_RATE 0x14 = 5.4 Gbps: 162000, 270000, 540000 (dp_rates[] up to max, intel_dp.c:216-243)
    assert_eq!(caps.sink_rates.as_slice(), &[162_000, 270_000, 540_000]);
    assert!(!m.log.contains(&(DP_SUPPORTED_LINK_RATES, 16)));
    m.mem.get_mut(&DP_DPCD_REV).unwrap()[1] = 0x0a; // 2.7 Gbps
    let caps = run_probe(&mut m).unwrap();
    assert_eq!(caps.sink_rates.as_slice(), &[162_000, 270_000]);
}

#[test]
fn an_edp_14_panel_with_an_empty_rate_table_uses_the_dpcd_rates() {
    let mut m = edp14();
    m.mem.insert(DP_SUPPORTED_LINK_RATES, vec![0; 16]);
    let caps = run_probe(&mut m).unwrap();
    assert!(!caps.use_rate_select);
    assert_eq!(caps.sink_rates.as_slice(), &[162_000, 270_000, 540_000]);
    // a failed table read is treated as an empty table (named deviation in the module doc)
    let mut m = edp14();
    m.fail.push(DP_SUPPORTED_LINK_RATES);
    let caps = run_probe(&mut m).unwrap();
    assert!(!caps.use_rate_select);
}

#[test]
fn no_rate_below_max_means_the_default_162000() {
    let mut m = edp14();
    m.mem.insert(DP_EDP_DPCD_REV, vec![0x02, 0, 0, 0, 0]);
    m.mem.get_mut(&DP_DPCD_REV).unwrap()[1] = 0x03; // 0x03*27000 = 81000 < 162000: nothing qualifies (0x01/0x02/0x04 are the UHBR codes)
    let caps = run_probe(&mut m).unwrap();
    assert!(caps.rates_defaulted);
    assert_eq!(caps.sink_rates.as_slice(), &[162_000]);
}

#[test]
fn an_unusable_lane_count_defaults_to_one() {
    for (raw, want, defaulted) in [(0x81u8, 1u8, false), (0x82, 2, false), (0x84, 4, false), (0x83, 1, true), (0x80, 1, true), (0x08, 1, true)] {
        let mut m = edp14();
        m.mem.get_mut(&DP_DPCD_REV).unwrap()[2] = raw;
        let caps = run_probe(&mut m).unwrap();
        assert_eq!((caps.max_sink_lane_count, caps.lane_count_defaulted), (want, defaulted), "raw {raw:#x}");
    }
}

#[test]
fn the_probe_names_its_failures() {
    let mut m = edp14();
    m.fail.push(0x000);
    assert!(matches!(run_probe(&mut m), Err(CapsError::BaseReadFailed(_))));
    let mut m = edp14();
    m.mem.get_mut(&DP_DPCD_REV).unwrap()[0] = 0;
    let e = run_probe(&mut m).unwrap_err();
    assert_eq!(e, CapsError::DpcdRevZero);
    assert_eq!(e.name(), "CAPS-DPCD-REV-ZERO");
    let mut m = edp14();
    m.fail.push(DP_RECEIVER_ALPM_CAP);
    let e = run_probe(&mut m).unwrap_err();
    assert!(matches!(e, CapsError::AlpmReadFailed(_)));
    assert_eq!(e.name(), "CAPS-ALPM-READ-FAILED");
    // a failed eDP block read is NOT fatal; edp_dpcd stays zero and use_max_params stays false
    let mut m = edp14();
    m.fail.push(DP_EDP_DPCD_REV);
    let caps = run_probe(&mut m).unwrap();
    assert_eq!(caps.edp_dpcd, [0; 5]);
    assert!(!caps.use_max_params);
    assert!(!caps.use_rate_select);
}

#[test]
fn extended_caps_replace_the_base_block_when_newer() {
    // base says DPCD 1.2 but flags an extended block; the block at 0x2200 says 1.4 with TPS3
    let mut m = edp14();
    {
        let b = m.mem.get_mut(&DP_DPCD_REV).unwrap();
        b[0] = 0x12;
        b[0x0e] = 0x80; // DP_EXTENDED_RECEIVER_CAP_FIELD_PRESENT
    }
    let mut ext = EDP14_DPCD;
    ext[2] = 0xC4;
    m.mem.insert(DP_DP13_DPCD_REV, ext.to_vec());
    let caps = run_probe(&mut m).unwrap();
    assert_eq!(caps.dpcd, ext);
    assert_eq!(m.log[1], (0x2200, 15));
    // "Extended DPCD rev less than base DPCD rev": the base block is kept
    let mut m = edp14();
    {
        let b = m.mem.get_mut(&DP_DPCD_REV).unwrap();
        b[0] = 0x14;
        b[0x0e] = 0x80;
    }
    let mut old = EDP14_DPCD;
    old[0] = 0x11;
    m.mem.insert(DP_DP13_DPCD_REV, old.to_vec());
    let caps = run_probe(&mut m).unwrap();
    assert_eq!(caps.dpcd[0], 0x14);
    // a failing extended read fails the whole probe
    let mut m = edp14();
    m.mem.get_mut(&DP_DPCD_REV).unwrap()[0x0e] = 0x80;
    m.fail.push(DP_DP13_DPCD_REV);
    assert!(matches!(run_probe(&mut m), Err(CapsError::ExtendedReadFailed(_))));
}

// ---- rates -------------------------------------------------------------------------------------------------------
#[test]
fn bw_code_conversions_follow_drm_dp_helper() {
    // drm_dp_bw_code_to_link_rate / link_rate_to_bw_code (drm_dp_helper.c:530-560)
    for (code, rate) in [(0x06u8, 162_000u32), (0x0a, 270_000), (0x14, 540_000), (0x1e, 810_000)] {
        assert_eq!(bw_code_to_link_rate(code), rate);
        assert_eq!(link_rate_to_bw_code(rate), code);
    }
    assert_eq!(bw_code_to_link_rate(0x01), 1_000_000);
    assert_eq!(bw_code_to_link_rate(0x04), 1_350_000);
    assert_eq!(bw_code_to_link_rate(0x02), 2_000_000);
    assert_eq!(link_rate_to_bw_code(1_000_000), 0x01);
    assert_eq!(link_rate_to_bw_code(1_350_000), 0x04);
    assert_eq!(link_rate_to_bw_code(2_000_000), 0x02);
    assert!(is_uhbr_rate(1_000_000) && is_uhbr_rate(2_000_000) && !is_uhbr_rate(810_000));
}

#[test]
fn the_skl_source_table_is_linuxs() {
    // intel_dp.c:598-600
    assert_eq!(SKL_SOURCE_RATES, [162_000, 216_000, 270_000, 324_000, 432_000, 540_000]);
    assert_eq!(source_rates(0).as_slice(), &SKL_SOURCE_RATES[..]);
}

#[test]
fn a_vbt_maximum_cuts_the_source_table() {
    assert_eq!(source_rates(270_000).as_slice(), &[162_000, 216_000, 270_000]);
    assert_eq!(source_rates(300_000).as_slice(), &[162_000, 216_000, 270_000]); // largest <= max
    assert_eq!(source_rates(540_000).len(), 6);
    assert_eq!(source_rates(810_000).len(), 6);
    assert_eq!(source_rates(100_000).len(), 0); // nothing qualifies
    // intel_dp_rate_limit_len()
    assert_eq!(rate_limit_len(&[1, 2, 3], 2), 2);
    assert_eq!(rate_limit_len(&[1, 2, 3], 0), 0);
    assert_eq!(rate_limit_len(&[1, 2, 3], 9), 3);
}

#[test]
fn vbt_max_link_rate_takes_the_smaller_when_both_are_set() {
    // vbt_max_link_rate(), intel_dp.c:559-576
    assert_eq!(vbt_max_link_rate(0, None), 0);
    assert_eq!(vbt_max_link_rate(540_000, None), 540_000);
    assert_eq!(vbt_max_link_rate(0, Some(270_000)), 270_000);
    assert_eq!(vbt_max_link_rate(540_000, Some(270_000)), 270_000);
    assert_eq!(vbt_max_link_rate(270_000, Some(540_000)), 270_000);
}

#[test]
fn common_rates_intersect_in_order_and_fall_back_to_162000() {
    let caps = {
        let mut m = edp14();
        run_probe(&mut m).unwrap()
    };
    let c = common_rates(&source_rates(0), &caps.sink_rates);
    assert_eq!(c.as_slice(), &[162_000, 216_000, 270_000, 324_000, 432_000, 540_000]);
    let sink = Rates::from_slice(&[162_000, 270_000, 540_000, 810_000]);
    assert_eq!(common_rates(&source_rates(0), &sink).as_slice(), &[162_000, 270_000, 540_000]);
    // nothing in common: the "Paranoia" single entry
    let odd = Rates::from_slice(&[100_000, 200_000]);
    assert_eq!(common_rates(&source_rates(0), &odd).as_slice(), &[162_000]);
    assert_eq!(common_rates(&Rates::new(), &odd).as_slice(), &[162_000]);
}

#[test]
fn rate_select_is_the_index_in_the_sink_table() {
    let caps = run_probe(&mut edp14()).unwrap();
    // eDP 1.4: DP_LINK_RATE_SET gets the SINK table index, DP_LINK_BW_SET stays 0 (intel_dp.c:1594-1610)
    assert_eq!(compute_rate(&caps, 162_000), Ok(RateSel { link_bw: 0, rate_select: 0 }));
    assert_eq!(compute_rate(&caps, 270_000), Ok(RateSel { link_bw: 0, rate_select: 2 }));
    assert_eq!(compute_rate(&caps, 540_000), Ok(RateSel { link_bw: 0, rate_select: 5 }));
    let e = compute_rate(&caps, 810_000).unwrap_err();
    assert_eq!(e, RateSelectError::RateNotInSinkTable { port_clock: 810_000 });
    assert_eq!(e.name(), "RATE-NOT-IN-SINK-TABLE");
    // pre-1.4: DP_LINK_BW_SET gets the bw code, rate_select 0
    let mut m = edp14();
    m.mem.insert(DP_EDP_DPCD_REV, vec![0x02, 0, 0, 0, 0]);
    let old = run_probe(&mut m).unwrap();
    assert_eq!(compute_rate(&old, 270_000), Ok(RateSel { link_bw: 0x0a, rate_select: 0 }));
    assert_eq!(compute_rate(&old, 540_000), Ok(RateSel { link_bw: 0x14, rate_select: 0 }));
}

// ---- lanes -------------------------------------------------------------------------------------------------------
#[test]
fn ddi_a_has_four_lanes_only_with_the_buf_ctl_strap() {
    // intel_ddi_max_lanes(), intel_ddi.c:4951-4980
    assert_eq!(ddi_a_max_lanes(DDI_A_4_LANES), 4);
    assert_eq!(ddi_a_max_lanes(0), 2);
    assert_eq!(ddi_a_max_lanes(0x8000_0000), 2); // other bits do not matter
}

#[test]
fn common_lane_count_is_the_minimum_of_source_vbt_and_sink() {
    assert_eq!(max_common_lane_count(4, 0, 4), 4);
    assert_eq!(max_common_lane_count(2, 0, 4), 2);
    assert_eq!(max_common_lane_count(4, 2, 4), 2); // VBT caps the source
    assert_eq!(max_common_lane_count(4, 4, 1), 1);
    assert_eq!(max_common_lane_count(2, 4, 4), 2); // VBT never raises it
}

// ---- fallback reduction ------------------------------------------------------------------------------------------
#[test]
fn link_params_reduce_rate_first_then_lanes_at_the_top_rate() {
    let c = Rates::from_slice(&[162_000, 270_000, 540_000]);
    // intel_dp_link_training.c:1273-1294
    assert_eq!(reduce_link_params(&c, 540_000, 4), Some((270_000, 4)));
    assert_eq!(reduce_link_params(&c, 270_000, 4), Some((162_000, 4)));
    assert_eq!(reduce_link_params(&c, 162_000, 4), Some((540_000, 2))); // lowest rate: halve lanes, back to the MAX rate
    assert_eq!(reduce_link_params(&c, 162_000, 2), Some((540_000, 1)));
    assert_eq!(reduce_link_params(&c, 162_000, 1), None); // nothing left
    assert_eq!(reduce_lane_count(4), Some(2));
    assert_eq!(reduce_lane_count(1), None);
    assert_eq!(reduce_link_rate(&c, 162_000), None);
    assert_eq!(reduce_link_rate(&c, 300_000), None); // not a common rate: index < 0
    // never crosses between UHBR and non-UHBR
    let u = Rates::from_slice(&[540_000, 1_000_000]);
    assert_eq!(reduce_link_rate(&u, 1_000_000), None);
}

// ---- mutation-sweep additions ---------------------------------------------------------------------------------------
#[test]
fn enhanced_framing_needs_the_capability_bit_not_just_a_new_enough_sink() {
    let mut d = EDP14_DPCD;
    d[2] = 0x04; // 4 lanes, bit 7 clear
    assert!(!dp_enhanced_frame_cap(&d));
    d[2] = 0x84;
    assert!(dp_enhanced_frame_cap(&d));
    d[0] = 0x11;
    assert!(dp_enhanced_frame_cap(&d)); // exactly 1.1 counts
    d[0] = 0x10;
    assert!(!dp_enhanced_frame_cap(&d));
}

#[test]
fn tps3_and_tps4_thresholds_are_inclusive() {
    let mut d = EDP14_DPCD;
    d[2] = 0x44; // TPS3 bit, 4 lanes
    d[0] = 0x12;
    assert!(dp_tps3_supported(&d)); // exactly 1.2
    d[0] = 0x11;
    assert!(!dp_tps3_supported(&d));
    d[2] = 0x04;
    d[0] = 0x14;
    assert!(!dp_tps3_supported(&d)); // new enough but the bit is clear
    d[3] = 0x80;
    assert!(dp_tps4_supported(&d)); // exactly 1.4
    d[3] = 0x00;
    assert!(!dp_tps4_supported(&d));
}

#[test]
fn an_8_1_gbps_capable_dpcd_lists_810000_as_a_sink_rate() {
    let mut m = edp14();
    m.mem.insert(DP_EDP_DPCD_REV, vec![0x02, 0, 0, 0, 0]);
    m.mem.get_mut(&DP_DPCD_REV).unwrap()[1] = 0x1e;
    let caps = run_probe(&mut m).unwrap();
    assert_eq!(caps.sink_rates.as_slice(), &[162_000, 270_000, 540_000, 810_000]);
    // ... and the source table (SKL tops out at 5.4 Gbps) intersects it away
    assert_eq!(common_rates(&source_rates(0), &caps.sink_rates).as_slice(), &[162_000, 270_000, 540_000]);
}

#[test]
fn the_edp_revision_comes_from_byte_zero_of_the_edp_block() {
    // eDP 1.4 in byte 0 with garbage below 3 in byte 1: still 1.4 (rate table read, use_max_params false)
    let mut m = edp14();
    m.mem.insert(DP_EDP_DPCD_REV, vec![0x03, 0x00, 0, 0, 0]);
    let caps = run_probe(&mut m).unwrap();
    assert!(caps.use_rate_select && !caps.use_max_params);
    assert!(m.log.contains(&(DP_SUPPORTED_LINK_RATES, 16)));
    // eDP 1.3 in byte 0 with a "1.4" in byte 1: still 1.3
    let mut m = edp14();
    m.mem.insert(DP_EDP_DPCD_REV, vec![0x02, 0x7f, 0x7f, 0x7f, 0x7f]);
    let caps = run_probe(&mut m).unwrap();
    assert!(!caps.use_rate_select && caps.use_max_params);
    assert!(!m.log.contains(&(DP_SUPPORTED_LINK_RATES, 16)));
    // the same through the pure function
    let c = sink_caps_from_blocks(EDP14_DPCD, Some([0x03, 0, 0, 0, 0]), &EDP14_RATES);
    assert!(c.use_rate_select && !c.use_max_params);
    let c = sink_caps_from_blocks(EDP14_DPCD, Some([0x02, 0x7f, 0, 0, 0]), &EDP14_RATES);
    assert!(!c.use_rate_select && c.use_max_params);
    let c = sink_caps_from_blocks(EDP14_DPCD, None, &EDP14_RATES);
    assert!(!c.use_rate_select && !c.use_max_params, "no eDP block: rates table ignored, use_max_params stays false");
}

#[test]
fn an_extended_block_of_the_same_revision_but_different_content_wins() {
    // "if (!memcmp(dpcd, dpcd_ext)) return 0; ... memcpy(dpcd, dpcd_ext)": equal revisions, different bytes -> the extended block
    let mut m = edp14();
    m.mem.get_mut(&DP_DPCD_REV).unwrap()[0x0e] = 0x80;
    let mut ext = EDP14_DPCD;
    ext[0x0e] = 0x80;
    ext[2] = 0x82; // 2 lanes + enhanced
    m.mem.insert(DP_DP13_DPCD_REV, ext.to_vec());
    let caps = run_probe(&mut m).unwrap();
    assert_eq!(caps.dpcd[2], 0x82);
    assert_eq!(caps.max_sink_lane_count, 2);
}

#[test]
fn rate_intersection_skips_unmatched_sink_entries_one_at_a_time() {
    // k[0] below every source rate, k[1] equal to the first: advancing the sink index by 2 would miss it
    let sink = Rates::from_slice(&[100_000, 162_000, 270_000]);
    assert_eq!(common_rates(&source_rates(0), &sink).as_slice(), &[162_000, 270_000]);
    // and the source index likewise
    let sink = Rates::from_slice(&[216_000, 540_000]);
    assert_eq!(common_rates(&source_rates(0), &sink).as_slice(), &[216_000, 540_000]);
    // more matches than the array holds: stops at the capacity, no panic
    let many = Rates::from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(many.len(), MAX_RATES);
}

#[test]
fn rates_container_basics() {
    let r = Rates::from_slice(&[10, 20, 30]);
    assert_eq!((r.len(), r.is_empty(), r.index_of(20), r.index_of(25)), (3, false, Some(1), None));
    assert!(Rates::new().is_empty());
    assert_eq!(Rates::default(), Rates::new());
    assert_eq!(MAX_RATES, 8);
}
