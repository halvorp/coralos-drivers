// SPDX-License-Identifier: GPL-2.0-only
//! Link training vs intel_dp_link_training.c (v7.0-rc6): convergence on a scripted sink, Linux's exact loop limits, and the
//! NAMED failures. Every scripted run has an action budget, so a lost bound fails an assertion instead of looping.
#![allow(unused_imports)]
mod common;
use i915_display_link_core::buf_trans::*;
use i915_display_link_core::caps::*;
use i915_display_link_core::ddi::*;
use i915_display_link_core::dpcd::*;
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::linkstatus::*;
use i915_display_link_core::plan::Effect;
use i915_display_link_core::regs::*;
use i915_display_link_core::train::*;
use i915_kbl_display_core::regs::{DDI_BUF_CTL_ENABLE, DDI_BUF_EMP_MASK};
use i915_kbl_display_core::Ddi;

const EDP14_DPCD: [u8; 15] = [0x14, 0x14, 0x84, 0x01, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
const EDP14_EDP: [u8; 5] = [0x03, 0x05, 0x00, 0x00, 0x00];
const EDP14_RATES: [u8; 16] = [0xA4, 0x1F, 0x30, 0x2A, 0xBC, 0x34, 0x48, 0x3F, 0x60, 0x54, 0x78, 0x69, 0, 0, 0, 0];

/// Per-lane (cr_done, eq_done, symbol_locked) nibble, align byte, and per-lane (voltage, pre-emphasis) requests.
fn link_status(lanes: [u8; 4], align: u8, adj: [(u8, u8); 4]) -> [u8; 6] {
    [
        lanes[0] | (lanes[1] << 4),
        lanes[2] | (lanes[3] << 4),
        align,
        0,
        adj[0].0 | (adj[0].1 << 2) | (adj[1].0 << 4) | (adj[1].1 << 6),
        adj[2].0 | (adj[2].1 << 2) | (adj[3].0 << 4) | (adj[3].1 << 6),
    ]
}
const CR: u8 = DP_LANE_CR_DONE;
const FULL: u8 = DP_CHANNEL_EQ_BITS;
const NONE: u8 = 0;
const NOADJ: [(u8, u8); 4] = [(0, 0); 4];

/// What the scripted sink sees when asked for link status.
struct Ctx {
    /// 1-based count of status reads so far in the CURRENT phase.
    n: usize,
    /// True while the last pattern written to DP_TRAINING_PATTERN_SET is TPS1 (clock recovery), false for TPS2/3.
    cr_phase: bool,
}

struct Run {
    outcome: TrainOutcome,
    /// Every DPCD access, in order: (write?, addr, bytes) — for reads the bytes are empty.
    dpcd: Vec<(bool, u32, Vec<u8>)>,
    mmio: Vec<Effect>,
    delays: Vec<u32>,
    cr_reads: usize,
    eq_reads: usize,
    trainer: LinkTrainer,
}

fn run(mut t: LinkTrainer, mut status: impl FnMut(&Ctx) -> [u8; 6], mut fail_write: impl FnMut(u32, &[u8]) -> bool) -> Run {
    let mut dpcd = vec![];
    let mut mmio = vec![];
    let mut delays = vec![];
    let (mut cr_reads, mut eq_reads) = (0usize, 0usize);
    let mut cr_phase = true;
    let mut reply = TrainReply::Start;
    let mut budget = 0usize;
    let outcome = loop {
        budget += 1;
        assert!(budget < 5_000, "trainer exceeded the action budget: a loop bound is missing");
        match t.step(reply) {
            TrainAction::Finished(o) => break o,
            TrainAction::Mmio(p) => {
                mmio.extend(p);
                reply = TrainReply::Done;
            }
            TrainAction::DelayUs(us) => {
                delays.push(us);
                reply = TrainReply::Done;
            }
            TrainAction::Dpcd(DpcdRequest::Write { addr, len, data }) => {
                let bytes = data[..len].to_vec();
                dpcd.push((true, addr, bytes.clone()));
                if addr == DP_TRAINING_PATTERN_SET && !bytes.is_empty() {
                    cr_phase = bytes[0] & DP_TRAINING_PATTERN_MASK_TEST == DP_TRAINING_PATTERN_1;
                }
                reply = if fail_write(addr, &bytes) {
                    TrainReply::Dpcd(Err(DpcdFail { first: DpcdErr::Nack { reply: 1 }, attempts: 32 }))
                } else {
                    let mut d = [0u8; 20];
                    d[0] = len as u8;
                    TrainReply::Dpcd(Ok(DpcdOk { len, data: d }))
                };
            }
            TrainAction::Dpcd(DpcdRequest::Read { addr, len }) => {
                dpcd.push((false, addr, vec![len as u8]));
                let mut d = [0u8; 20];
                if addr == DP_LANE0_1_STATUS {
                    let n = if cr_phase { cr_reads += 1; cr_reads } else { eq_reads += 1; eq_reads };
                    let s = status(&Ctx { n, cr_phase });
                    d[..6].copy_from_slice(&s);
                }
                reply = TrainReply::Dpcd(Ok(DpcdOk { len, data: d }));
            }
        }
    };
    Run { outcome, dpcd, mmio, delays, cr_reads, eq_reads, trainer: t }
}

const DP_TRAINING_PATTERN_MASK_TEST: u8 = 0x0f;

fn caps_edp14() -> SinkCaps {
    sink_caps_from_blocks(EDP14_DPCD, Some(EDP14_EDP), &EDP14_RATES)
}

fn trainer_with(caps: SinkCaps, lanes: u8, port_clock: u32, low_vswing: bool, enhanced: bool) -> LinkTrainer {
    let trans = get_buf_trans(Sku::KblY, true, low_vswing, Ddi::A);
    let ddi = DdiLink::new(DdiConfig { port: Ddi::A, lane_count: lanes, lane_reversal: false, ddi_a_4_lanes: true, max_lanes: 4, vbt_dp_boost: 0, trans, enhanced_framing: enhanced });
    LinkTrainer::new(LinkParams { port_clock, lane_count: lanes, enhanced_framing: enhanced, msa_timing_par_ignore: false }, caps, ddi).unwrap()
}

fn trainer() -> LinkTrainer {
    trainer_with(caps_edp14(), 4, 540_000, true, false)
}

fn no_fail(_: u32, _: &[u8]) -> bool {
    false
}

fn writes_to(r: &Run, addr: u32) -> Vec<Vec<u8>> {
    r.dpcd.iter().filter(|(w, a, _)| *w && *a == addr).map(|(_, _, b)| b.clone()).collect()
}

// ---- convergence ---------------------------------------------------------------------------------------------------
#[test]
fn converges_on_a_scripted_sink_in_linux_order() {
    let r = run(
        trainer(),
        |c| match (c.cr_phase, c.n) {
            (true, 1) => link_status([NONE; 4], 0, [(1, 0); 4]),
            (true, 2) => link_status([NONE; 4], 0, [(2, 0); 4]),
            (true, _) => link_status([CR; 4], 0, [(2, 0); 4]),
            (false, 1) => link_status([CR; 4], 0, [(2, 1); 4]), // CR ok, EQ not done
            (false, _) => link_status([FULL; 4], DP_INTERLANE_ALIGN_DONE, [(2, 1); 4]),
        },
        no_fail,
    );
    let ok = r.outcome.expect("training converges");
    assert_eq!((ok.cr_reads, ok.eq_reads), (3, 2));
    assert_eq!(ok.train_set, [0x0a; 4]); // vswing 2 | pre-emphasis 1 << 3
    assert_eq!(ok.eq_pattern, DP_TRAINING_PATTERN_2); // the literal sink has no TPS3

    // DPCD traffic, exactly as intel_dp_prepare_link_train() + CR + EQ + disable (intel_dp_link_training.c:840-1101)
    let rate_idx = 5u8; // 540000 in the sink's table [162000, 216000, 270000, 324000, 432000, 540000]
    let expect: Vec<(bool, u32, Vec<u8>)> = vec![
        (false, 0x010, vec![16]),                            // WaEdpLinkRateDataReload read of 16 bytes (:855-868)
        (true, 0x107, vec![0x00, 0x01]),                     // DOWNSPREAD_CTRL: no MSA ignore, 8b/10b
        (true, 0x101, vec![0x04]),                           // DP_LANE_COUNT_SET (eDP 1.4 rate-select method)
        (true, 0x115, vec![rate_idx]),                       // DP_LINK_RATE_SET
        (true, 0x102, vec![0x21, 0, 0, 0, 0]),               // TPS1 | scrambling disabled, lanes zeroed
        (false, 0x202, vec![6]),
        (true, 0x103, vec![0x01; 4]),                        // first adjust: vswing 1
        (false, 0x202, vec![6]),
        (true, 0x103, vec![0x02; 4]),
        (false, 0x202, vec![6]),                             // CR done
        (true, 0x102, vec![0x22, 0x02, 0x02, 0x02, 0x02]),   // TPS2 | scrambling disabled (no sink TPS3)
        (false, 0x202, vec![6]),
        (true, 0x103, vec![0x0a; 4]),
        (false, 0x202, vec![6]),                             // EQ done
        (true, 0x102, vec![0x00]),                           // training pattern disabled
    ];
    assert_eq!(r.dpcd, expect);
    // delays: CR is 100 us for DPCD >= 1.4, EQ 400 us when TRAINING_AUX_RD_INTERVAL = 0 (drm_dp_helper.c:290-345)
    assert_eq!(r.delays, vec![100, 100, 100, 400, 400]);
}

#[test]
fn the_ddi_effects_follow_the_dpcd_in_the_linux_order() {
    let r = run(trainer(), |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([FULL; 4], DP_INTERLANE_ALIGN_DONE, NOADJ) }, no_fail);
    r.outcome.unwrap();
    let w: Vec<(u32, u32)> = r.mmio.iter().filter_map(|e| if let Effect::Write { reg, value } = e { Some((reg.offset(), *value)) } else { None }).collect();
    // prepare_link_retrain: DP_TP_CTL <- ENABLE | PAT1 | SST (0x80000000), then DDI_BUF_CTL with ENABLE (lanes=4 -> width field 3 << 1,
    // DDI_A_4_LANES bit 4, trans select 0)
    assert_eq!(w[0], (0x64040, 0x8000_0000));
    assert_eq!(w[1], (0x64000, 0x8000_0000 | (3 << 1) | (1 << 4)));
    // set_signal_levels (level 0) rewrites DDI_BUF_CTL with the same value, ENABLE kept
    assert_eq!(w[2], (0x64000, 0x8000_0000 | (3 << 1) | (1 << 4)));
    // the I_boost RMW of DISPIO_CR_TX_BMU_CR0 precedes it (port A with 4 lanes also programs port E)
    let rmws: Vec<&Effect> = r.mmio.iter().filter(|e| matches!(e, Effect::Rmw { reg, .. } if *reg == DISPIO_CR_TX_BMU_CR0)).collect();
    assert!(rmws.len() >= 2);
    // the training-pattern selects: PAT1 first, PAT2 for EQ, NORMAL is NOT the trainer's (intel_dp_stop_link_train), IDLE last
    let pats: Vec<u32> = r.mmio.iter().filter_map(|e| if let Effect::Rmw { reg, set, .. } = e { if reg.offset() == 0x64040 { Some(*set) } else { None } } else { None }).collect();
    assert_eq!(pats, vec![DP_TP_CTL_LINK_TRAIN_PAT1, DP_TP_CTL_LINK_TRAIN_PAT2, DP_TP_CTL_LINK_TRAIN_IDLE]);
    // port A before display ver 12: no idle-done poll (intel_ddi.c:3855-3862)
    assert!(!r.mmio.iter().any(|e| matches!(e, Effect::Poll(_))));
    // the DDI state the trainer leaves: buffer enabled
    assert!(r.trainer.ddi().dp & DDI_BUF_CTL_ENABLE != 0);
}

#[test]
fn signal_level_requests_select_the_matching_translation_row() {
    // request (vswing 1, pre-emphasis 1) -> train_set 0x09 -> index_to_dp_signal_levels row 5 (intel_ddi.c:100)
    let r = run(
        trainer(),
        |c| match (c.cr_phase, c.n) {
            (true, 1) => link_status([NONE; 4], 0, [(1, 1); 4]),
            (true, _) => link_status([CR; 4], 0, [(1, 1); 4]),
            (false, _) => link_status([FULL; 4], DP_INTERLANE_ALIGN_DONE, [(1, 1); 4]),
        },
        no_fail,
    );
    r.outcome.unwrap();
    let buf_writes: Vec<u32> = r.mmio.iter().filter_map(|e| if let Effect::Write { reg, value } = e { if reg.offset() == 0x64000 { Some(*value) } else { None } } else { None }).collect();
    let emp: Vec<u32> = buf_writes.iter().map(|v| (v & DDI_BUF_EMP_MASK) >> 24).collect();
    assert!(emp.contains(&5), "{emp:?}");
    assert_eq!(emp[0], 0); // the enable write
}

// ---- the limits, all named -----------------------------------------------------------------------------------------
#[test]
fn clock_recovery_gives_up_after_the_same_voltage_is_tried_five_times() {
    // constant request of vswing 1 at pre-emphasis 0: read 1 sees a change from the zeroed history, reads 2..5 are
    // repeats (voltage_tries 2,3,4,5), read 6 sees voltage_tries == 5 and stops (:975-986)
    let r = run(trainer(), |_| link_status([NONE; 4], 0, [(1, 0); 4]), no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoverySameVoltage5Times));
    assert_eq!(r.cr_reads, 6);
    assert_eq!(r.outcome.unwrap_err().name(), "LT-CR-SAME-VOLTAGE-5-TIMES");
    // with a sink that keeps asking for vswing 0 / pre-emphasis 0 the request never "changes", so it stops one read earlier
    let r = run(trainer(), |_| link_status([NONE; 4], 0, NOADJ), no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoverySameVoltage5Times));
    assert_eq!(r.cr_reads, 5);
    assert_eq!(CR_SAME_VOLTAGE_TRIES, 5);
}

#[test]
fn clock_recovery_gives_up_after_ten_reads_on_a_dpcd_14_sink() {
    // the request alternates (vswing 1, vswing 0) so voltage_tries never reaches 5 and max swing is never reached
    let r = run(trainer(), |c| link_status([NONE; 4], 0, [(if c.n % 2 == 1 { 1 } else { 0 }, 0); 4]), no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoveryExhausted { max_tries: 10 }));
    assert_eq!(r.cr_reads, 10);
    assert_eq!(writes_to(&r, DP_TRAINING_LANE0_SET).len(), 10); // 10 updates, the last one included (:996-1013)
    assert_eq!(CR_MAX_TRIES_DP14, 10);
    assert_eq!(r.outcome.unwrap_err().name(), "LT-CR-EXHAUSTED");
}

#[test]
fn clock_recovery_gives_up_after_eighty_reads_on_a_pre_14_sink() {
    let mut dpcd = EDP14_DPCD;
    dpcd[0] = 0x12; // DPCD 1.2
    let caps = sink_caps_from_blocks(dpcd, None, &[0; 16]);
    assert!(!caps.use_rate_select);
    let t = trainer_with(caps, 4, 540_000, true, false);
    assert_eq!(t.max_cr_tries(), 80);
    let r = run(t, |c| link_status([NONE; 4], 0, [(if c.n % 2 == 1 { 1 } else { 0 }, 0); 4]), no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoveryExhausted { max_tries: 80 }));
    assert_eq!(r.cr_reads, 80);
    assert_eq!(CR_MAX_TRIES_PRE14, 80);
    // pre-1.4 sink: the CR delay comes from TRAINING_AUX_RD_INTERVAL (0 -> 100 us)
    assert!(r.delays.iter().take(80).all(|&d| d == 100));
}

#[test]
fn clock_recovery_stops_at_max_swing_with_max_pre_emphasis() {
    // 10-row eDP table: voltage_max = swing 3. A request of (3, 0) is "3+0": flagged max after the first update, and the
    // NEXT read (not the first) is the one that gives up (:988-993).
    let r = run(trainer(), |_| link_status([NONE; 4], 0, [(3, 0); 4]), no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoveryMaxSwingReached));
    assert_eq!(r.cr_reads, 2);
    assert_eq!(writes_to(&r, DP_TRAINING_LANE0_SET), vec![vec![0x07; 4]]); // 3 | MAX_SWING_REACHED (1 << 2)
    // 9-row DP table: voltage_max = swing 2, so the end of the range is "2+1"
    let r = run(trainer_with(caps_edp14(), 4, 540_000, false, false), |_| link_status([NONE; 4], 0, [(2, 1); 4]), no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoveryMaxSwingReached));
    assert_eq!(r.cr_reads, 2);
    assert_eq!(writes_to(&r, DP_TRAINING_LANE0_SET), vec![vec![0x0e; 4]]);
    // ...while (2, 0) is not the end of the range there: the loop continues until another limit
    let r = run(trainer_with(caps_edp14(), 4, 540_000, false, false), |_| link_status([NONE; 4], 0, [(2, 0); 4]), no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoverySameVoltage5Times));
}

#[test]
fn channel_equalisation_gives_up_after_five_reads() {
    let r = run(
        trainer(),
        |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([CR; 4], 0, [(1, 1); 4]) }, // CR ok, EQ never done
        no_fail,
    );
    assert_eq!(r.outcome, Err(TrainFailure::ChannelEqExhausted));
    assert_eq!(r.eq_reads, 5);
    assert_eq!(EQ_MAX_TRIES, 5);
    // five updates: the 5th read's update is made too (:1074-1090)
    assert_eq!(writes_to(&r, DP_TRAINING_LANE0_SET).len(), 5);
    assert_eq!(r.outcome.unwrap_err().name(), "LT-EQ-EXHAUSTED");
}

#[test]
fn channel_equalisation_stops_when_clock_recovery_is_lost() {
    let r = run(trainer(), |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else if c.n == 1 { link_status([NONE; 4], 0, NOADJ) } else { unreachable!() }, no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ChannelEqClockRecoveryLost));
    assert_eq!(r.eq_reads, 1);
}

#[test]
fn equalisation_needs_interlane_alignment_as_well_as_every_lane() {
    // all lanes good but ALIGN not done: not equalised
    let r = run(trainer(), |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([FULL; 4], 0, NOADJ) }, no_fail);
    assert_eq!(r.outcome, Err(TrainFailure::ChannelEqExhausted));
    // one bad lane out of four
    let r = run(
        trainer(),
        |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([FULL, FULL, FULL, CR], DP_INTERLANE_ALIGN_DONE, NOADJ) },
        no_fail,
    );
    assert_eq!(r.outcome, Err(TrainFailure::ChannelEqExhausted));
    // ...but with only 2 lanes active, lanes 2 and 3 are not looked at
    let t2 = trainer_with(caps_edp14(), 2, 540_000, true, false);
    let r = run(t2, |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([FULL, FULL, NONE, NONE], DP_INTERLANE_ALIGN_DONE, NOADJ) }, no_fail);
    assert!(r.outcome.is_ok());
}

// ---- DPCD write failures -------------------------------------------------------------------------------------------
#[test]
fn a_failed_start_write_is_named() {
    let r = run(trainer(), |_| unreachable!(), |a, _| a == DP_TRAINING_PATTERN_SET);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoveryStartFailed));
    // cleanup still happens: pattern disabled on the DPCD and the DDI idled (:1368-1396)
    assert_eq!(writes_to(&r, DP_TRAINING_PATTERN_SET).last().unwrap(), &vec![0x00]);
    assert!(r.mmio.iter().any(|e| matches!(e, Effect::Rmw { reg, set, .. } if reg.offset() == 0x64040 && *set == DP_TP_CTL_LINK_TRAIN_IDLE)));
}

#[test]
fn a_failed_lane_update_is_named_in_each_phase() {
    let r = run(trainer(), |_| link_status([NONE; 4], 0, [(1, 0); 4]), |a, _| a == DP_TRAINING_LANE0_SET);
    assert_eq!(r.outcome, Err(TrainFailure::ClockRecoveryUpdateFailed));
    let r = run(
        trainer(),
        |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([CR; 4], 0, [(1, 1); 4]) },
        |a, _| a == DP_TRAINING_LANE0_SET,
    );
    assert_eq!(r.outcome, Err(TrainFailure::ChannelEqUpdateFailed));
}

#[test]
fn the_eq_start_write_failing_is_named() {
    let mut seen_tps1 = false;
    let r = run(
        trainer(),
        |_| link_status([CR; 4], 0, NOADJ),
        move |a, b| {
            if a == DP_TRAINING_PATTERN_SET && b.first() == Some(&0x21) {
                seen_tps1 = true;
                return false;
            }
            a == DP_TRAINING_PATTERN_SET && seen_tps1 && b.first() == Some(&0x22)
        },
    );
    assert_eq!(r.outcome, Err(TrainFailure::ChannelEqStartFailed));
}

#[test]
fn a_failed_status_read_is_named() {
    // a status read that returns the wrong number of bytes
    let mut t = trainer();
    let mut reply = TrainReply::Start;
    let mut seen = 0;
    let out = loop {
        match t.step(reply) {
            TrainAction::Finished(o) => break o,
            TrainAction::Dpcd(DpcdRequest::Read { addr, .. }) if addr == DP_LANE0_1_STATUS => {
                seen += 1;
                reply = TrainReply::Dpcd(Err(DpcdFail { first: DpcdErr::Nack { reply: 1 }, attempts: 32 }));
            }
            TrainAction::Dpcd(DpcdRequest::Write { len, .. }) => reply = TrainReply::Dpcd(Ok(DpcdOk { len, data: [0; 20] })),
            TrainAction::Dpcd(DpcdRequest::Read { len, .. }) => reply = TrainReply::Dpcd(Ok(DpcdOk { len, data: [0; 20] })),
            _ => reply = TrainReply::Done,
        }
        assert!(seen < 5);
    };
    assert_eq!(out, Err(TrainFailure::ClockRecoveryStatusReadFailed));
    assert_eq!(out.unwrap_err().name(), "LT-CR-STATUS-READ-FAILED");
}

// ---- parameters ------------------------------------------------------------------------------------------------------
#[test]
fn a_rate_the_sink_does_not_list_is_refused_before_anything_is_sent() {
    let ddi = DdiLink::new(DdiConfig {
        port: Ddi::A,
        lane_count: 4,
        lane_reversal: false,
        ddi_a_4_lanes: true,
        max_lanes: 4,
        vbt_dp_boost: 0,
        trans: get_buf_trans(Sku::KblY, true, true, Ddi::A),
        enhanced_framing: false,
    });
    let e = LinkTrainer::new(LinkParams { port_clock: 810_000, lane_count: 4, enhanced_framing: false, msa_timing_par_ignore: false }, caps_edp14(), ddi).err().unwrap();
    assert_eq!(e, TrainFailure::RateNotInSinkTable { port_clock: 810_000 });
    assert_eq!(e.name(), "LT-RATE-NOT-IN-SINK-TABLE");
}

#[test]
fn a_pre_edp14_sink_is_programmed_with_link_bw_set() {
    let mut dpcd = EDP14_DPCD;
    dpcd[0] = 0x12;
    let caps = sink_caps_from_blocks(dpcd, None, &[0; 16]);
    let r = run(trainer_with(caps, 2, 270_000, true, false), |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([FULL; 4], DP_INTERLANE_ALIGN_DONE, NOADJ) }, no_fail);
    r.outcome.unwrap();
    assert_eq!(writes_to(&r, DP_LINK_BW_SET), vec![vec![0x0a, 0x02]]); // bw code 2.7 Gbps, 2 lanes
    assert!(writes_to(&r, DP_LANE_COUNT_SET).is_empty() && writes_to(&r, DP_LINK_RATE_SET).is_empty());
    assert!(!r.dpcd.iter().any(|(w, a, _)| !*w && *a == DP_SUPPORTED_LINK_RATES)); // no rate reload without rate-select
    // only 2 lane settings are written
    assert!(writes_to(&r, DP_TRAINING_PATTERN_SET).iter().all(|b| b.len() == 3 || b.len() == 1));
}

#[test]
fn enhanced_framing_sets_the_bit_in_the_sink_and_in_dp_tp_ctl() {
    let r = run(trainer_with(caps_edp14(), 4, 540_000, true, true), |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([FULL; 4], DP_INTERLANE_ALIGN_DONE, NOADJ) }, no_fail);
    r.outcome.unwrap();
    assert_eq!(writes_to(&r, DP_LANE_COUNT_SET), vec![vec![0x84]]); // 4 | DP_LANE_COUNT_ENHANCED_FRAME_EN
    let tp_write = r.mmio.iter().find_map(|e| if let Effect::Write { reg, value } = e { if reg.offset() == 0x64040 { Some(*value) } else { None } } else { None }).unwrap();
    assert_eq!(tp_write, 0x8000_0000 | (1 << 18));
}

#[test]
fn tps3_is_used_when_the_sink_supports_it() {
    let mut dpcd = EDP14_DPCD;
    dpcd[2] = 0xC4; // TPS3 + enhanced + 4 lanes
    let caps = sink_caps_from_blocks(dpcd, Some(EDP14_EDP), &EDP14_RATES);
    assert_eq!(training_pattern(&caps), DP_TRAINING_PATTERN_3);
    assert_eq!(training_pattern(&caps_edp14()), DP_TRAINING_PATTERN_2);
    let r = run(trainer_with(caps, 4, 540_000, true, false), |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([FULL; 4], DP_INTERLANE_ALIGN_DONE, NOADJ) }, no_fail);
    let ok = r.outcome.unwrap();
    assert_eq!(ok.eq_pattern, DP_TRAINING_PATTERN_3);
    assert!(writes_to(&r, DP_TRAINING_PATTERN_SET).contains(&vec![0x23, 0, 0, 0, 0])); // TPS3 | scrambling disabled
    let pats: Vec<u32> = r.mmio.iter().filter_map(|e| if let Effect::Rmw { reg, set, .. } = e { if reg.offset() == 0x64040 { Some(*set) } else { None } } else { None }).collect();
    assert_eq!(pats[1], DP_TP_CTL_LINK_TRAIN_PAT3);
}

#[test]
fn msa_timing_ignore_is_written_to_downspread_ctrl() {
    let ddi = DdiLink::new(DdiConfig { port: Ddi::A, lane_count: 4, lane_reversal: false, ddi_a_4_lanes: true, max_lanes: 4, vbt_dp_boost: 0, trans: get_buf_trans(Sku::KblY, true, true, Ddi::A), enhanced_framing: false });
    let t = LinkTrainer::new(LinkParams { port_clock: 540_000, lane_count: 4, enhanced_framing: false, msa_timing_par_ignore: true }, caps_edp14(), ddi).unwrap();
    let r = run(t, |c| if c.cr_phase { link_status([CR; 4], 0, NOADJ) } else { link_status([FULL; 4], DP_INTERLANE_ALIGN_DONE, NOADJ) }, no_fail);
    assert_eq!(writes_to(&r, DP_DOWNSPREAD_CTRL), vec![vec![0x80, 0x01]]);
}

// ---- the adjust-request arithmetic -----------------------------------------------------------------------------------
#[test]
fn signal_levels_are_the_maximum_across_the_active_lanes_on_display_ver_9() {
    // has_per_lane_signal_levels() is false for ver 9 with no LTTPR (:380-388): lane 0 asks vswing 1, lane 1 asks vswing 2,
    // lane 2 pre-emphasis 1 — all four train_set slots get the same word.
    let ls = link_status([NONE; 4], 0, [(1, 0), (2, 0), (0, 1), (0, 0)]);
    let mut ts = [0u8; 4];
    assert!(get_adjust_train(&mut ts, &ls, 4, DP_TRAIN_VOLTAGE_SWING_LEVEL_3, DP_TRAIN_PRE_EMPH_LEVEL_3));
    // v = 2, p = 1<<3: v <= dp_voltage_max(p = level 1) = 2 -> 2 | 8
    assert_eq!(ts, [0x0a; 4]);
    // the same request again changes nothing
    assert!(!get_adjust_train(&mut ts, &ls, 4, DP_TRAIN_VOLTAGE_SWING_LEVEL_3, DP_TRAIN_PRE_EMPH_LEVEL_3));
    // only active lanes are consulted
    let mut ts2 = [0u8; 4];
    get_adjust_train(&mut ts2, &ls, 1, DP_TRAIN_VOLTAGE_SWING_LEVEL_3, DP_TRAIN_PRE_EMPH_LEVEL_3);
    assert_eq!(ts2, [0x01; 4]);
}

#[test]
fn voltage_is_limited_by_the_pre_emphasis_it_travels_with() {
    // dp_voltage_max(), intel_dp_link_training.c:285-298
    assert_eq!(dp_voltage_max(DP_TRAIN_PRE_EMPH_LEVEL_0), 3);
    assert_eq!(dp_voltage_max(DP_TRAIN_PRE_EMPH_LEVEL_1), 2);
    assert_eq!(dp_voltage_max(DP_TRAIN_PRE_EMPH_LEVEL_2), 1);
    assert_eq!(dp_voltage_max(DP_TRAIN_PRE_EMPH_LEVEL_3), 0);
    assert_eq!(dp_voltage_max(DP_TRAIN_PRE_EMPH_LEVEL_1 | DP_TRAIN_MAX_PRE_EMPHASIS_REACHED), 2); // flag bits ignored
    // request (v3, p2): v clamps to 1
    let ls = link_status([NONE; 4], 0, [(3, 2); 4]);
    assert_eq!(lane_adjust_vswing_preemph(&ls, 4, 3, DP_TRAIN_PRE_EMPH_LEVEL_3), 0x01 | 0x10);
    // request p3 -> max pre-emphasis flagged; v clamps to 0 and (voltage_max 3) is not reached
    let ls = link_status([NONE; 4], 0, [(1, 3); 4]);
    assert_eq!(lane_adjust_vswing_preemph(&ls, 4, 3, DP_TRAIN_PRE_EMPH_LEVEL_3), 0x00 | 0x18 | 0x20);
}

#[test]
fn max_swing_detection_matches_the_two_plus_one_or_three_plus_zero_rule() {
    // intel_dp_lane_max_vswing_reached(), :677-691
    assert!(lane_max_vswing_reached(0x07)); // v3 p0 + MAX_SWING
    assert!(lane_max_vswing_reached(0x0e)); // v2 p1 + MAX_SWING
    assert!(!lane_max_vswing_reached(0x03)); // v3 p0 without the flag
    assert!(!lane_max_vswing_reached(0x06)); // flag but v2 p0 (sum 2)
    assert!(!lane_max_vswing_reached(0x0f)); // v3 p1: sum 4
    assert!(link_max_vswing_reached(&[0x07, 0x07, 0, 0], 2));
    assert!(!link_max_vswing_reached(&[0x07, 0x07, 0, 0], 3));
    assert!(!link_max_vswing_reached(&[0x07, 0x03, 0, 0], 2));
}

#[test]
fn a_changed_request_on_any_active_lane_counts_as_changed() {
    let a = link_status([NONE; 4], 0, [(1, 0); 4]);
    let mut b = a;
    assert!(!adjust_request_changed(&a, &b, 4));
    b[5] = 0x01; // lane 2 voltage request changes
    assert!(adjust_request_changed(&a, &b, 4));
    assert!(!adjust_request_changed(&a, &b, 2)); // lane 2 is not active
}

#[test]
fn the_sequential_failure_budget_is_two() {
    // MAX_SEQ_TRAIN_FAILURES (intel_dp_link_training.c:61): the caller's, pinned here because the reduction uses it
    assert_eq!(MAX_SEQ_TRAIN_FAILURES, 2);
}

#[test]
fn every_dpcd_write_is_sized_for_the_active_lanes() {
    // the lane settings write is exactly lane_count bytes, the pattern write lane_count + 1
    let r = run(trainer_with(caps_edp14(), 2, 270_000, true, false), |c| if c.cr_phase { link_status([NONE; 4], 0, [(1, 0); 4]) } else { unreachable!() }, no_fail);
    assert!(r.outcome.is_err());
    assert!(writes_to(&r, DP_TRAINING_LANE0_SET).iter().all(|b| b.len() == 2));
    assert_eq!(writes_to(&r, DP_TRAINING_PATTERN_SET)[0].len(), 3);
}
