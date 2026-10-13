// SPDX-License-Identifier: GPL-2.0-only
//! Controller bring-up: the link reset and its named time-outs, interrupt control, the init/stop order, the processing-pipe enables.
mod common;
use avs_fw_core::effect::{Cause, Op, Space, Width};
use avs_fw_core::runner::PROGRAM_CAP;
use common::*;
use hda_ext_core::caps::Link;
use hda_ext_core::controller::*;
use hda_ext_core::error::HdaError;
use hda_ext_core::stream::Streams;

const GCTL: u32 = 0x08;
const STREAMS: Streams = Streams { capture: 7, playback: 7 };

fn link(index: u32, base: u32) -> Link {
    Link { index, base, lcaps: 0, slcount: 1, id: None }
}

#[test]
fn full_reset_cycles_gctl_with_linux_widths_and_ends_by_reading_the_codec_mask() {
    let mut f = Fake::new();
    f.bar0[GCTL as usize] = 1; // out of reset
    f.bar0[0x0e] = 0x03; // two codecs reported
    f.run(link_reset(true)).unwrap();
    assert_eq!(
        f.labels(),
        vec!["statests-clear", "gctl-enter-reset", "gctl-reset-entered", "gctl-exit-reset", "gctl-reset-exited", "codec-mask"]
    );
    let w: Vec<_> = f.log.iter().map(|a| (a.label, a.off, a.width, a.write)).collect();
    // STATESTS is cleared with a 16-bit write of 0xff (hda_register.h:160); the enter is a 32-bit update, the exit a byte update
    assert_eq!(w[0], ("statests-clear", 0x0e, Width::W16, Some(0xff)));
    assert_eq!((w[1].0, w[1].1, w[1].2), ("gctl-enter-reset", GCTL, Width::D32));
    assert_eq!((w[3].0, w[3].1, w[3].2), ("gctl-exit-reset", GCTL, Width::B8));
    assert_eq!((w[5].0, w[5].1, w[5].2, w[5].3), ("codec-mask", 0x0e, Width::W16, None));
    assert_eq!(f.hda(GCTL, Width::B8) & 1, 1, "left out of reset");
    // the settles are the lower bounds of usleep_range(500, 1000) and (1000, 1200)
    assert_eq!(f.delays_us, 500 + 1000);
}

#[test]
fn the_reset_waits_are_polls_of_100ms_in_500us_steps_with_the_planners_own_read_bound() {
    let p = link_reset(true);
    let polls: Vec<_> = p
        .as_slice()
        .iter()
        .filter_map(|s| if let Op::Poll(ps) = s.op { Some((s.label, ps.reg.off, ps.reg.width, ps.mask, ps.want, ps.interval_us, ps.timeout_us, ps.max_polls)) } else { None })
        .collect();
    // controller.c:497-500 and :516-518: 100 ms, 500 us; reads <= timeout/interval + 2 = 202
    assert_eq!(polls, vec![("gctl-reset-entered", GCTL, Width::B8, 1, 0, 500, 100_000, 202), ("gctl-reset-exited", GCTL, Width::B8, 1, 1, 500, 100_000, 202)]);
}

#[test]
fn a_controller_that_will_not_enter_reset_fails_by_name_after_exactly_the_read_bound() {
    let mut f = Fake::new();
    f.bar0[GCTL as usize] = 1;
    f.gctl_stuck = true;
    let e = f.run(link_reset(true)).unwrap_err();
    assert_eq!(e.step, "gctl-reset-entered");
    assert_eq!(e.cause, Cause::PollBound { reads: 202, last: 1 });
    assert!(!f.labels().contains(&"gctl-exit-reset"), "nothing past the failed wait ran");
}

#[test]
fn a_controller_that_will_not_leave_reset_fails_by_name_and_never_reads_the_codec_mask() {
    let mut f = Fake::new(); // in reset, and the reset bit never sets: entering is already satisfied, leaving never completes
    f.gctl_stuck = true;
    let e = f.run(link_reset(true)).unwrap_err();
    assert_eq!(e.step, "gctl-reset-exited");
    assert_eq!(e.cause, Cause::PollBound { reads: 202, last: 0 });
    assert!(!f.labels().contains(&"codec-mask"));
}

#[test]
fn a_slow_controller_inside_the_bound_succeeds() {
    let mut f = Fake::new();
    f.bar0[GCTL as usize] = 1;
    f.gctl_lag = 150; // 150 reads of the old value: inside 202
    f.run(link_reset(true)).unwrap();
    f.gctl_lag = 250; // outside
    f.bar0[GCTL as usize] = 1;
    f.run(link_reset(true)).unwrap_err();
}

#[test]
fn without_full_reset_only_the_readiness_check_and_the_codec_mask_run() {
    let mut f = Fake::new();
    f.bar0[GCTL as usize] = 1;
    f.run(link_reset(false)).unwrap();
    assert_eq!(f.labels(), vec!["gctl-reset-exited", "codec-mask"]);
    let mut g = Fake::new(); // in reset: not ready -> Linux -EBUSY (controller.c:548-551), here the named wait
    assert_eq!(g.run(link_reset(false)).unwrap_err().step, "gctl-reset-exited");
}

#[test]
fn interrupt_clear_touches_every_stream_then_the_controller_registers_with_the_right_widths() {
    let progs = int_clear(&STREAMS).unwrap();
    let steps: Vec<_> = progs.iter().flat_map(|p| p.as_slice().iter().copied()).collect();
    assert_eq!(steps.len(), 14 + 3);
    for (i, s) in steps[..14].iter().enumerate() {
        match s.op {
            Op::Write { reg, value } => assert_eq!((reg.off, reg.width, value), (0x80 + 0x20 * i as u32 + 0x03, Width::B8, 0x1c), "SD_STS of stream {i}"),
            o => panic!("{o:?}"),
        }
    }
    let tail: Vec<_> = steps[14..].iter().map(|s| if let Op::Write { reg, value } = s.op { (reg.off, reg.width, value) } else { panic!() }).collect();
    assert_eq!(tail, vec![(0x0e, Width::W16, 0xff), (0x5d, Width::B8, 0x05), (0x24, Width::D32, 0x4000_0000 | 0x3fff_ffff)]);
    for p in &progs {
        assert!(p.len() <= PROGRAM_CAP);
    }
}

#[test]
fn a_large_controller_splits_into_programs_that_each_fit_and_lose_no_step() {
    let big = Streams { capture: 15, playback: 15 };
    let progs = int_clear(&big).unwrap();
    assert!(progs.len() >= 1 && progs.iter().all(|p| p.len() <= PROGRAM_CAP));
    assert_eq!(progs.iter().map(|p| p.len()).sum::<usize>(), 30 + 3);
    let progs = int_disable(&big).unwrap();
    assert_eq!(progs.iter().map(|p| p.len()).sum::<usize>(), 30 + 1);
}

#[test]
fn interrupt_enable_sets_exactly_the_controller_and_global_bits() {
    let mut f = Fake::new();
    f.bar0[0x20..0x24].copy_from_slice(&0x0000_00ffu32.to_le_bytes());
    f.run(int_enable()).unwrap();
    assert_eq!(f.hda(0x20, Width::D32), 0xc000_00ff);
}

#[test]
fn interrupt_disable_clears_each_streams_enables_then_zeroes_intctl() {
    let mut f = Fake::new();
    f.bar0[0x20..0x24].copy_from_slice(&0xc000_ffffu32.to_le_bytes());
    for i in 0..14u32 {
        f.bar0[(0x80 + 0x20 * i) as usize] = 0xff;
    }
    f.run_all(int_disable(&STREAMS).unwrap()).unwrap();
    assert_eq!(f.hda(0x20, Width::D32), 0);
    for i in 0..14u32 {
        assert_eq!(f.bar0[(0x80 + 0x20 * i) as usize], 0xff & !0x1c, "stream {i}: only the three interrupt enables cleared");
    }
}

#[test]
fn the_position_buffer_is_two_dwords_and_must_be_aligned() {
    let mut f = Fake::new();
    f.run(posbuf(Some(0x0000_0001_2345_6780)).unwrap()).unwrap();
    assert_eq!((f.hda(0x70, Width::D32), f.hda(0x74, Width::D32)), (0x2345_6780, 1));
    assert_eq!(posbuf(Some(0x1000_0040)).unwrap_err(), HdaError::UnalignedDmaAddress { addr: 0x1000_0040, align: 128 });
    f.run(posbuf(None).unwrap()).unwrap();
    assert_eq!((f.hda(0x70, Width::D32), f.hda(0x74, Width::D32)), (0, 0));
}

#[test]
fn init_chip_runs_in_the_avs_order_and_ends_with_the_dum_bit() {
    let mut f = Fake::new();
    f.bar0[GCTL as usize] = 1;
    f.pci[0x48] = 0x40; // clock gating enabled
    let links = [link(0, 0x840), link(1, 0x880)];
    f.run_all(init_chip(&STREAMS, &links, Some(0x1_0000_0000)).unwrap()).unwrap();
    let l = f.labels();
    let pos = |name: &str| l.iter().position(|x| *x == name).unwrap_or_else(|| panic!("{name} missing in {l:?}"));
    // avs/core.c:106-128: gating off, [reset, clear, enable, posbuf], link map reset, gating on, DUM
    assert!(pos("clock-gating-off") < pos("gctl-enter-reset"));
    assert!(pos("codec-mask") < pos("sd-sts-clear"));
    assert!(pos("intsts-clear") < pos("intctl-enable"));
    assert!(pos("intctl-enable") < pos("dplbase"));
    assert!(pos("dpubase") < pos("link-losidv-reset"));
    assert!(pos("link-losidv-reset") < pos("clock-gating-on"));
    assert_eq!(*l.last().unwrap(), "vs-em2-dum");
    // the clock gating bit went off during the reset and is back on; the DUM bit is set; INTCTL has CIE and GIE
    assert_eq!(f.pci[0x48] & 0x40, 0x40);
    assert_eq!(f.hda(0x1030, Width::D32) & (1 << 23), 1 << 23);
    assert_eq!(f.hda(0x20, Width::D32) & 0xc000_0000, 0xc000_0000);
    // gating was OFF at the moment of the reset write (avs/core.c:112)
    let off_at = f.log.iter().position(|a| a.label == "clock-gating-off").unwrap();
    assert_eq!((f.log[off_at].space, f.log[off_at].off, f.log[off_at].write), (Space::PciConfig, 0x48, Some(0)));
    let losidv: Vec<_> = f.log.iter().filter(|a| a.label == "link-losidv-reset").map(|a| (a.off, a.width, a.write)).collect();
    assert_eq!(losidv, vec![(0x840 + 0x08, Width::D32, Some(0)), (0x880 + 0x08, Width::D32, Some(0))]);
}

#[test]
fn init_chip_without_a_position_buffer_or_links_skips_them_and_a_reset_timeout_stops_everything() {
    let mut f = Fake::new();
    f.bar0[GCTL as usize] = 1;
    f.run_all(init_chip(&STREAMS, &[], None).unwrap()).unwrap();
    assert!(!f.labels().contains(&"dplbase") && !f.labels().contains(&"link-losidv-reset"));
    let mut g = Fake::new();
    g.bar0[GCTL as usize] = 1;
    g.gctl_stuck = true;
    let e = g.run_all(init_chip(&STREAMS, &[], None).unwrap()).unwrap_err();
    assert_eq!(e.step, "gctl-reset-entered");
    assert!(!g.labels().contains(&"intctl-enable"), "interrupts are not enabled on a controller that did not reset");
}

#[test]
fn stop_chip_disables_and_clears_interrupts_and_drops_the_position_buffer() {
    let mut f = Fake::new();
    f.bar0[0x20..0x24].copy_from_slice(&0xc000_00ffu32.to_le_bytes());
    f.bar0[0x70..0x74].copy_from_slice(&0x1234_5681u32.to_le_bytes());
    f.run_all(stop_chip(&STREAMS, true).unwrap()).unwrap();
    assert_eq!(f.hda(0x20, Width::D32), 0);
    assert_eq!(f.hda(0x70, Width::D32), 0);
    let l = f.labels();
    assert!(l.iter().position(|x| *x == "intctl-disable").unwrap() < l.iter().position(|x| *x == "intsts-clear").unwrap());
    let mut g = Fake::new();
    g.run_all(stop_chip(&STREAMS, false).unwrap()).unwrap();
    assert!(!g.labels().contains(&"dplbase-clear"));
}

#[test]
fn the_processing_pipe_enables_are_single_bit_updates_at_the_capability_offset() {
    let mut f = Fake::new();
    f.run(ppcap_enable(0x800, true)).unwrap();
    assert_eq!(f.hda(0x804, Width::D32), 1 << 30, "GPROCEN");
    f.run(ppcap_int_enable(0x800, true)).unwrap();
    assert_eq!(f.hda(0x804, Width::D32), (1 << 30) | (1 << 31), "GPROCEN | PIE");
    f.run(ppcap_enable(0x800, false)).unwrap();
    assert_eq!(f.hda(0x804, Width::D32), 1 << 31);
    f.run(ppcap_int_enable(0x800, false)).unwrap();
    assert_eq!(f.hda(0x804, Width::D32), 0);
}
