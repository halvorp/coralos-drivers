// SPDX-License-Identifier: GPL-2.0-only
//! A host playback stream: identity, format words, the prepare/start/stop/cleanup programs and SPIB, against the fake controller.
mod common;
use avs_fw_core::effect::{Cause, Width};
use common::*;
use hda_ext_core::bdl::plan_bdl;
use hda_ext_core::error::HdaError;
use hda_ext_core::stream::*;

const S: Streams = Streams { capture: 7, playback: 7 };

#[test]
fn stream_counts_come_from_gcap_bits_11_8_and_15_12() {
    // avs/core.c:96-97
    assert_eq!(Streams::from_gcap(0x7701), Streams { capture: 7, playback: 7 });
    assert_eq!(Streams::from_gcap(0x3a02), Streams { capture: 0xa, playback: 3 });
    assert_eq!(Streams::from_gcap(0xf0ff), Streams { capture: 0, playback: 0xf });
    assert_eq!(Streams::from_gcap(0x0f00), Streams { capture: 0xf, playback: 0 });
}

#[test]
fn playback_streams_follow_the_capture_streams_and_tags_count_from_one_per_direction() {
    let p0 = S.playback_stream(0).unwrap();
    assert_eq!((p0.index, p0.tag, p0.dma_id()), (7, 1, 0));
    let p6 = S.playback_stream(6).unwrap();
    assert_eq!((p6.index, p6.tag, p6.dma_id()), (13, 7, 6));
    let c0 = S.capture_stream(0).unwrap();
    assert_eq!((c0.index, c0.tag), (0, 1), "capture tags restart at 1");
    assert_eq!(S.playback_stream(7), Err(HdaError::StreamIndexOutOfRange { index: 14, streams: 14 }));
    assert_eq!(S.capture_stream(7), Err(HdaError::StreamIndexOutOfRange { index: 7, streams: 14 }));
}

#[test]
fn descriptor_addresses_follow_stream_c_98() {
    // "SDI0=0x80, SDI1=0xa0, ..." and every register offset of the descriptor
    for (idx, base) in [(0u32, 0x80u32), (1, 0xa0), (2, 0xc0), (7, 0x160), (13, 0x220)] {
        assert_eq!(sd_reg(idx, 0x00, Width::B8).off, base);
        assert_eq!(sd_reg(idx, 0x08, Width::D32).off, base + 0x08);
        assert_eq!(sd_reg(idx, 0x18, Width::D32).off, base + 0x18);
    }
    assert_eq!((ppctl_procen(0), ppctl_procen(7), ppctl_procen(13)), (1, 0x80, 0x2000));
    // ext/stream.c:70-75: pphc = pp + 0x10 + 0x10 idx; pplc = pp + 0x10 + 0x10 num_streams + 0x10 idx
    assert_eq!(pphc_base(0x800, 3), 0x800 + 0x10 + 0x30);
    assert_eq!(pplc_base(0x800, 14, 3), 0x800 + 0x10 + 0xe0 + 0x30);
}

#[test]
fn stream_format_words_equal_the_values_gcc_computes_from_the_linux_rate_table() {
    // device.c:706-716 evaluated with gcc (HDA_RATE and AC_FMT_*): rate -> base word; channels-1 in bits 3:0; bits-per-sample at bit 4
    let table = [(8000u32, 0x500u32), (11025, 0x4300), (16000, 0x200), (22050, 0x4100), (32000, 0xa00), (44100, 0x4000), (48000, 0), (88200, 0x4800), (96000, 0x800), (176400, 0x5800), (192000, 0x1800)];
    for (hz, base) in table {
        assert_eq!(stream_format(2, 16, hz), Ok(base | 1 | 0x10), "{hz}");
    }
    // bits: 8 -> 0x00, 16 -> 0x10, 20 -> 0x20, 24 -> 0x30, 32 -> 0x40
    for (bits, w) in [(8u32, 0x00u32), (16, 0x10), (20, 0x20), (24, 0x30), (32, 0x40)] {
        assert_eq!(stream_format(1, bits, 48000), Ok(w), "{bits} bits");
    }
    assert_eq!(stream_format(2, 24, 48000), Ok(0x31));
    assert_eq!(stream_format(16, 16, 48000), Ok(0x1f));
}

#[test]
fn an_invalid_format_is_a_named_refusal_where_linux_returns_zero() {
    for (ch, bits, rate) in [(2u32, 16u32, 47999u32), (2, 16, 9600), (0, 16, 48000), (17, 16, 48000), (2, 12, 48000), (2, 0, 48000)] {
        assert_eq!(stream_format(ch, bits, rate), Err(HdaError::InvalidStreamFormat { rate, channels: ch, bits }));
    }
}

fn bdl4() -> hda_ext_core::bdl::Bdl {
    let ex = [avs_fw_core::bdl::Extent { addr: 0x2000_0000, len: 4 * 4096 }];
    plan_bdl(&ex, 4 * 4096, 4096, true).unwrap()
}

#[test]
fn setup_programs_the_descriptor_in_linux_order_and_values() {
    let s = S.playback_stream(0).unwrap(); // index 7, tag 1
    let fmt = stream_format(2, 32, 48000).unwrap();
    let params = SetupParams::new(fmt, &bdl4(), 0x0000_0001_3000_0000, true);
    assert_eq!((params.buffer_size, params.lvi), (16384, 3));
    let mut f = Fake::new();
    f.bar0[0x160 + 0] = 0xff; // stale run/interrupt bits in SD_CTL byte 0 (stream index 7 -> 0x80 + 0x20 * 7 = 0x160)
    f.run(setup(&S, &s, &params).unwrap()).unwrap();
    let w: Vec<_> = f.writes().iter().map(|a| (a.label, a.off, a.width)).collect();
    assert_eq!(
        w,
        vec![
            ("sd-dma-stop", 0x160, Width::B8),
            ("sd-sts-clear", 0x163, Width::B8),
            ("sd-stream-tag", 0x160, Width::D32),
            ("sd-cbl", 0x168, Width::D32),
            ("sd-format", 0x172, Width::W16),
            ("sd-lvi", 0x16c, Width::W16),
            ("sd-bdlpl", 0x178, Width::D32),
            ("sd-bdlpu", 0x17c, Width::D32),
            ("dplbase-enable", 0x70, Width::D32),
            ("sd-int-enable", 0x160, Width::D32),
        ]
    );
    assert_eq!(f.hda(0x168, Width::D32), 16384);
    assert_eq!(f.hda(0x172, Width::W16), fmt);
    assert_eq!(f.hda(0x16c, Width::W16), 3);
    assert_eq!((f.hda(0x178, Width::D32), f.hda(0x17c, Width::D32)), (0x3000_0000, 1));
    // SD_CTL: tag 1 in bits 23:20, traffic priority (bit 18) because bus->snoop is never set, interrupt enables 0x1c, run bit clear
    let ctl = f.hda(0x160, Width::D32);
    assert_eq!(ctl & 0x00f0_0000, 1 << 20);
    assert_eq!(ctl & (1 << 18), 1 << 18);
    assert_eq!(ctl & 0x1c, 0x1c);
    assert_eq!(ctl & 0x02, 0, "DMA not running");
    assert_eq!(f.hda(0x70, Width::D32) & 1, 1, "position buffer enabled");
    assert_eq!(f.labels().last(), Some(&"sd-fifosize"));
}

#[test]
fn setup_refuses_a_bad_tag_a_bad_index_and_an_unaligned_bdl_by_name() {
    let params = SetupParams::new(0x11, &bdl4(), 0x3000_0000, false);
    let good = S.playback_stream(0).unwrap();
    assert_eq!(setup(&S, &HostStream { tag: 0, ..good }, &params).unwrap_err(), HdaError::StreamTagOutOfRange { tag: 0 });
    assert_eq!(setup(&S, &HostStream { tag: 16, ..good }, &params).unwrap_err(), HdaError::StreamTagOutOfRange { tag: 16 });
    assert_eq!(setup(&S, &HostStream { index: 14, ..good }, &params).unwrap_err(), HdaError::StreamIndexOutOfRange { index: 14, streams: 14 });
    let un = SetupParams { bdl_addr: 0x3000_0040, ..params };
    assert_eq!(setup(&S, &good, &un).unwrap_err(), HdaError::UnalignedDmaAddress { addr: 0x3000_0040, align: 128 });
    // without a position buffer the DPLBASE step is not planned
    let mut f = Fake::new();
    f.run(setup(&S, &good, &params).unwrap()).unwrap();
    assert!(!f.labels().contains(&"dplbase-enable"));
}

#[test]
fn the_stream_tag_field_is_replaced_not_ored() {
    let s = S.playback_stream(2).unwrap(); // tag 3, index 9 -> 0x80 + 0x120 = 0x1a0
    let mut f = Fake::new();
    f.bar0[0x1a0 + 2] = 0x70; // old tag 7 in bits 23:20
    let params = SetupParams::new(0x11, &bdl4(), 0x3000_0000, false);
    f.run(setup(&S, &s, &params).unwrap()).unwrap();
    assert_eq!(f.hda(0x1a0, Width::D32) & 0x00f0_0000, 3 << 20);
}

#[test]
fn reset_waits_for_the_bit_to_set_then_clear_with_a_300us_bound_and_names_the_failure() {
    let mut f = Fake::new();
    f.run(reset(&S, 7).unwrap()).unwrap();
    assert_eq!(f.labels(), vec!["sd-dma-stop", "sd-sts-clear", "sd-reset-enter", "sd-reset-entered", "sd-reset-exit", "sd-reset-exited"]);
    assert_eq!(f.bar0[0x160] & 1, 0);
    // 300 us / 3 us + 2 = 102 reads
    let mut g = Fake::new();
    g.sd_stuck = true;
    let e = g.run(reset(&S, 7).unwrap()).unwrap_err();
    assert_eq!((e.step, e.cause), ("sd-reset-entered", Cause::PollBound { reads: 102, last: 0 }));
    let mut h = Fake::new();
    h.sd_lag = 50;
    h.run(reset(&S, 7).unwrap()).unwrap();
    h.sd_lag = 500;
    assert!(h.run(reset(&S, 7).unwrap()).is_err());
}

#[test]
fn start_enables_the_stream_interrupt_then_the_dma_and_stop_undoes_it_in_the_other_order() {
    let mut f = Fake::new();
    f.run(start(&S, 7).unwrap()).unwrap();
    assert_eq!(f.hda(0x20, Width::D32), 1 << 7);
    assert_eq!(f.bar0[0x160], 0x02 | 0x1c);
    assert_eq!(f.labels(), vec!["intctl-sie-on", "sd-dma-start"]);
    f.run(stop(&S, 7).unwrap()).unwrap();
    assert_eq!(f.hda(0x20, Width::D32), 0);
    assert_eq!(f.bar0[0x160], 0);
    assert_eq!(f.labels()[2..], ["sd-dma-stop", "sd-sts-clear", "intctl-sie-off"]);
}

#[test]
fn start_only_touches_its_own_streams_bits() {
    let mut f = Fake::new();
    f.bar0[0x20..0x24].copy_from_slice(&0xc000_0001u32.to_le_bytes());
    f.run(start(&S, 9).unwrap()).unwrap();
    assert_eq!(f.hda(0x20, Width::D32), 0xc000_0001 | (1 << 9));
}

#[test]
fn cleanup_zeroes_the_bdl_address_and_the_control_register() {
    let mut f = Fake::new();
    f.bar0[0x178..0x180].copy_from_slice(&0x1_3000_0000u64.to_le_bytes());
    f.bar0[0x160..0x164].copy_from_slice(&0x0010_001cu32.to_le_bytes());
    f.run(cleanup(&S, 7).unwrap()).unwrap();
    assert_eq!((f.hda(0x178, Width::D32), f.hda(0x17c, Width::D32), f.hda(0x160, Width::D32)), (0, 0, 0));
}

#[test]
fn decouple_sets_and_clears_the_streams_processing_pipe_bit() {
    let mut f = Fake::new();
    f.run(decouple(&S, 0x800, 7, true).unwrap()).unwrap();
    assert_eq!(f.hda(0x804, Width::D32), 1 << 7);
    f.run(decouple(&S, 0x800, 7, false).unwrap()).unwrap();
    assert_eq!(f.hda(0x804, Width::D32), 0);
    assert!(decouple(&S, 0x800, 14, true).is_err());
}

#[test]
fn spib_enables_the_stream_in_the_spb_capability_and_writes_the_position() {
    let mut f = Fake::new();
    f.run(spib(&S, 0x700, 7, 4096, 16384).unwrap()).unwrap();
    assert_eq!(f.hda(0x704, Width::D32), 1 << 7, "SPBFCCTL");
    assert_eq!(f.hda(0x700 + 0x08 + 0x08 * 7, Width::D32), 4096, "SPIB of stream 7");
    f.run(spib(&S, 0x700, 8, 16384, 16384).unwrap()).unwrap();
    assert_eq!(f.hda(0x704, Width::D32), (1 << 7) | (1 << 8));
    assert_eq!(spib(&S, 0x700, 7, 16385, 16384).unwrap_err(), HdaError::SpibBeyondBuffer { spib: 16385, buffer: 16384 });
}

#[test]
fn prepare_is_decouple_reset_setup_and_then_spib_in_that_order() {
    let s = S.playback_stream(0).unwrap();
    let params = SetupParams::new(0x11, &bdl4(), 0x3000_0000, true);
    let mut f = Fake::new();
    f.run_all(prepare(&S, 0x800, &s, &params, Some((0x700, 4096))).unwrap()).unwrap();
    let l = f.labels();
    let pos = |n: &str| l.iter().position(|x| *x == n).unwrap();
    assert!(pos("ppctl-decouple") < pos("sd-reset-enter"));
    assert!(pos("sd-reset-exited") < pos("sd-stream-tag"));
    assert!(pos("sd-fifosize") < pos("spbfcctl-enable"));
    let mut g = Fake::new();
    g.run_all(prepare(&S, 0x800, &s, &params, None).unwrap()).unwrap();
    assert!(!g.labels().contains(&"spib-write"));
}
