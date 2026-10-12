// SPDX-License-Identifier: GPL-2.0-only
#![allow(unused_imports)]
//! AUX message packing, the send/receive state machine and its bounds, vs the Linux source (intel_dp_aux.c, v7.0-rc6).
mod common;
use common::*;
use i915_display_link_core::aux::*;
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::regs::*;
use i915_vbt_core::child::AuxCh;

// ---- intel_dp_aux_pack / unpack, intel_dp_aux.c:35-55 --------------------------------------------------------------
#[test]
fn pack_puts_the_first_byte_in_the_most_significant_position() {
    assert_eq!(aux_pack(&[0x12, 0x34, 0x56, 0x78]), 0x1234_5678);
    assert_eq!(aux_pack(&[0xAB, 0xCD]), 0xABCD_0000);
    assert_eq!(aux_pack(&[0xEE]), 0xEE00_0000);
    assert_eq!(aux_pack(&[]), 0);
    // "if (src_bytes > 4) src_bytes = 4" — only the first four of a longer slice are used
    assert_eq!(aux_pack(&[1, 2, 3, 4, 5, 6]), 0x0102_0304);
}

#[test]
fn unpack_is_the_inverse_and_writes_only_the_asked_bytes() {
    let mut d = [0xFFu8; 4];
    aux_unpack(0x1234_5678, &mut d);
    assert_eq!(d, [0x12, 0x34, 0x56, 0x78]);
    let mut d = [0xFFu8; 4];
    aux_unpack(0x1234_5678, &mut d[..2]);
    assert_eq!(d, [0x12, 0x34, 0xFF, 0xFF]);
    let mut e = [0u8; 6];
    aux_unpack(0xA1B2_C3D4, &mut e); // dst_bytes > 4 is capped at 4 (:51)
    assert_eq!(e, [0xA1, 0xB2, 0xC3, 0xD4, 0, 0]);
}

// ---- intel_dp_aux_header, :460-468 -------------------------------------------------------------------------------
fn msg(request: u8, address: u32, size: usize) -> AuxMsg {
    AuxMsg { request, address, size, data: [0; 16] }
}

#[test]
fn header_matches_the_linux_layout() {
    // txbuf[0] = (request << 4) | ((address >> 16) & 0xf); [1] = address >> 8; [2] = address; [3] = size - 1
    assert_eq!(aux_header(&msg(DP_AUX_NATIVE_READ, 0x000, 15)), [0x90, 0x00, 0x00, 0x0e]);
    assert_eq!(aux_header(&msg(DP_AUX_NATIVE_READ, 0x2200, 15)), [0x90, 0x22, 0x00, 0x0e]);
    assert_eq!(aux_header(&msg(DP_AUX_NATIVE_WRITE, 0x102, 4)), [0x80, 0x01, 0x02, 0x03]);
    assert_eq!(aux_header(&msg(DP_AUX_NATIVE_WRITE, 0x68007, 5)), [0x86, 0x80, 0x07, 0x04]);
    // address bits above 19 never leak into byte 0
    assert_eq!(aux_header(&msg(DP_AUX_NATIVE_READ, 0xFF_FFFF, 1))[0], 0x9F);
    // an address-only message: size - 1 wraps (and is not sent)
    assert_eq!(aux_header(&msg(DP_AUX_NATIVE_WRITE, 0x10, 0))[3], 0xFF);
}

#[test]
fn aksv_write_sets_the_select_flag_and_nothing_else_does() {
    assert_eq!(aux_xfer_flags(&msg(DP_AUX_NATIVE_WRITE, 0x68007, 5)), 1 << 15);
    assert_eq!(aux_xfer_flags(&msg(DP_AUX_NATIVE_WRITE | DP_AUX_I2C_MOT, 0x68007, 5)), 1 << 15);
    assert_eq!(aux_xfer_flags(&msg(DP_AUX_NATIVE_READ, 0x68007, 5)), 0);
    assert_eq!(aux_xfer_flags(&msg(DP_AUX_NATIVE_WRITE, 0x68008, 5)), 0);
}

// ---- send_ctl, :202-236, written out bit by bit from intel_dp_aux_regs.h:46-76 -----------------------------------
#[test]
fn skl_send_ctl_is_the_linux_word() {
    // SEND_BUSY 31 | DONE 30 | INTERRUPT 29 | TIME_OUT_ERROR 28 | TIME_OUT_MAX (3 << 26) | RECEIVE_ERROR 25
    // | MESSAGE_SIZE(n) n << 20 | FW_SYNC_PULSE_SKL(18) = 17 << 5 | SYNC_PULSE_SKL(32) = 31
    assert_eq!(skl_aux_send_ctl(4), 0xFE40_023F);
    assert_eq!(skl_aux_send_ctl(3), 0xFE30_023F);
    assert_eq!(skl_aux_send_ctl(20), 0xFE00_0000 | (20 << 20) | 0x23F);
    assert_eq!(aux_sync_len(), 32); // precharge 16 + preamble 16, :141-147
    assert_eq!(aux_fw_sync_len(), 18); // precharge 10 + preamble 8, :149-166
    assert_eq!(skl_aux_clock_divider(0), 1); // :138
    assert_eq!(skl_aux_clock_divider(1), 0);
    assert_eq!(skl_aux_clock_divider(7), 0);
}

#[test]
fn aux_registers_sit_where_the_header_puts_them() {
    // intel_dp_aux_regs.h:24-25, :78-79: _MMIO_PORT(aux_ch, A, B) = A + ch * (B - A)
    assert_eq!(dp_aux_ch_ctl(AuxCh::A).offset(), 0x64010);
    assert_eq!(dp_aux_ch_ctl(AuxCh::B).offset(), 0x64110);
    assert_eq!(dp_aux_ch_ctl(AuxCh::C).offset(), 0x64210);
    assert_eq!(dp_aux_ch_ctl(AuxCh::D).offset(), 0x64310);
    assert_eq!(dp_aux_ch_ctl(AuxCh::E).offset(), 0x64410);
    for i in 0..5 {
        assert_eq!(dp_aux_ch_data(AuxCh::A, i).offset(), 0x64014 + 4 * i);
        assert_eq!(dp_aux_ch_data(AuxCh::B, i).offset(), 0x64114 + 4 * i);
    }
}

// ---- a full successful transfer ----------------------------------------------------------------------------------
#[test]
fn native_read_of_the_cap_block_drives_the_registers_in_linux_order() {
    let block = [0x14, 0x14, 0x84, 0x01, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let mut hw = AuxHw::new(AuxCh::A, move |_, _| ack_read(&block));
    let mut t = AuxTransfer::new(AuxCh::A, &msg(DP_AUX_NATIVE_READ, 0x000, 15)).unwrap();
    let mut feed = Feed::Start;
    let r = loop {
        match t.step(feed) {
            TransferStep::Op(op) => feed = hw.perform(op),
            TransferStep::Done(r) => break r.unwrap(),
        }
    };
    assert_eq!(r.reply, 0); // native ACK
    assert_eq!(r.ret, 15);
    assert_eq!(&r.data[..15], &block);
    // one header word loaded, then the control word, in that order; no other write except the status clear
    assert_eq!(hw.writes[0], (0x64014, 0x9000_000e));
    assert_eq!(hw.writes[1], (0x64010, 0xFE40_023F));
    assert_eq!(hw.writes.len(), 3);
    // "Clear done status and any errors": status | DONE | TIMEOUT | RECEIVE_ERROR
    assert_eq!(hw.writes[2].0, 0x64010);
    assert_eq!(hw.writes[2].1 & (DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_TIME_OUT_ERROR | DP_AUX_CH_CTL_RECEIVE_ERROR), DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_TIME_OUT_ERROR | DP_AUX_CH_CTL_RECEIVE_ERROR);
    assert!(hw.sleeps_ms.is_empty() && hw.sleeps_us.is_empty());
}

#[test]
fn write_message_loads_header_then_payload_words_and_a_short_write_count_is_honoured() {
    let mut m = msg(DP_AUX_NATIVE_WRITE, 0x102, 5);
    m.data[..5].copy_from_slice(&[0x21, 0x00, 0x00, 0x00, 0x00]);
    // the sink ACKs but reports only 3 bytes written (rxbuf[1])
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE, vec![0x00, 0x03]));
    let mut t = AuxTransfer::new(AuxCh::A, &m).unwrap();
    let mut feed = Feed::Start;
    let r = loop {
        match t.step(feed) {
            TransferStep::Op(op) => feed = hw.perform(op),
            TransferStep::Done(r) => break r.unwrap(),
        }
    };
    assert_eq!(r.ret, 3); // clamp_t(int, rxbuf[1], 0, msg->size)
    // txsize = HEADER_SIZE + 5 = 9 bytes: words [80 01 02 04] [21 00 00 00] [00]
    assert_eq!(hw.writes[0], (0x64014, 0x8001_0204));
    assert_eq!(hw.writes[1], (0x64018, 0x2100_0000));
    assert_eq!(hw.writes[2], (0x6401c, 0x0000_0000));
    assert_eq!(hw.writes[3].1 & DP_AUX_CH_CTL_MESSAGE_SIZE_MASK, 9 << 20);
    // a short-write count larger than the message is clamped to the message size
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE, vec![0x00, 0x7f]));
    let mut t = AuxTransfer::new(AuxCh::A, &m).unwrap();
    let mut feed = Feed::Start;
    let r = loop {
        match t.step(feed) {
            TransferStep::Op(op) => feed = hw.perform(op),
            TransferStep::Done(r) => break r.unwrap(),
        }
    };
    assert_eq!(r.ret, 5);
}

#[test]
fn a_nack_reply_comes_back_as_reply_code_one_with_no_payload() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE, vec![0x10]));
    let mut t = AuxTransfer::new(AuxCh::A, &msg(DP_AUX_NATIVE_READ, 0x700, 5)).unwrap();
    let mut feed = Feed::Start;
    let r = loop {
        match t.step(feed) {
            TransferStep::Op(op) => feed = hw.perform(op),
            TransferStep::Done(r) => break r.unwrap(),
        }
    };
    assert_eq!(r.reply, DP_AUX_NATIVE_REPLY_NACK);
    assert_eq!(r.ret, 0);
}

// ---- the bounds --------------------------------------------------------------------------------------------------
fn xfer(send: &[u8], recv: usize) -> AuxXfer {
    AuxXfer::new(AuxCh::A, send, recv, 0)
}

#[test]
fn a_channel_that_is_never_idle_is_given_up_after_exactly_three_reads_and_three_sleeps() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| panic!("nothing may be sent while the channel is busy"));
    hw.busy_left = usize::MAX;
    let e = run_xfer(&mut hw, &mut xfer(&[0x90, 0, 0, 0], 16)).unwrap_err();
    assert_eq!((e.errno, e.why), (AuxErrno::Busy, "not-started"));
    assert_eq!(hw.sleeps_ms, vec![1, 1, 1]); // msleep(1) after each of the three busy reads (:314-319)
    assert_eq!(hw.sends, 0);
    assert!(hw.writes.is_empty());
    assert_eq!(PRE_BUSY_TRIES, 3);
}

#[test]
fn a_channel_idle_on_the_third_read_proceeds() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| ack_read(&[0x14]));
    hw.busy_left = 2;
    let r = run_xfer(&mut hw, &mut xfer(&[0x90, 0, 0, 0], 2)).unwrap();
    assert_eq!(r.len, 2); // reply byte + one payload byte
    assert_eq!(hw.sleeps_ms, vec![1, 1]);
}

#[test]
fn send_is_attempted_exactly_five_times_on_hardware_timeouts() {
    // status TIMEOUT only (no DONE): five tries, then "not done" — never a sixth
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_TIME_OUT_ERROR, vec![]));
    let e = run_xfer(&mut hw, &mut xfer(&[0x90, 0, 0, 0], 16)).unwrap_err();
    assert_eq!((e.errno, e.why), (AuxErrno::Busy, "not-done"));
    assert_eq!(hw.sends, 5);
    assert_eq!(SEND_TRIES, 5);
    // a timeout needs no extra sleep (the hardware timeout already meets the 400 us rule)
    assert!(hw.sleeps_us.is_empty());
}

#[test]
fn done_with_timeout_after_five_tries_is_etimedout() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_TIME_OUT_ERROR, vec![]));
    let e = run_xfer(&mut hw, &mut xfer(&[0x90, 0, 0, 0], 16)).unwrap_err();
    assert_eq!(e.errno, AuxErrno::TimedOut);
    assert_eq!(hw.sends, 5);
}

#[test]
fn receive_errors_sleep_400_to_500_us_each_try_then_fail_after_five() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_RECEIVE_ERROR, vec![]));
    let e = run_xfer(&mut hw, &mut xfer(&[0x90, 0, 0, 0], 16)).unwrap_err();
    assert_eq!(e.errno, AuxErrno::Io);
    assert_eq!(hw.sends, 5);
    assert_eq!(hw.sleeps_us, vec![(400, 500); 5]);
}

#[test]
fn a_success_after_failures_stops_retrying() {
    let mut hw = AuxHw::new(AuxCh::A, |n, _| if n < 2 { (DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_RECEIVE_ERROR, vec![]) } else { ack_read(&[0x42]) });
    let r = run_xfer(&mut hw, &mut xfer(&[0x90, 0, 0, 0], 2)).unwrap();
    assert_eq!(hw.sends, 3);
    assert_eq!(r.data[0], 0x00);
}

#[test]
fn forbidden_receive_sizes_are_ebusy() {
    // a reply that claims 0 bytes (and the sink reports DONE)
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE, vec![]));
    let e = run_xfer(&mut hw, &mut xfer(&[0x90, 0, 0, 0], 16)).unwrap_err();
    assert_eq!((e.errno, e.why), (AuxErrno::Busy, "forbidden-recv-bytes"));
    // and one that claims 21 (more than the 5 registers can hold): the harness derives the size from the rx bytes, so
    // drive the machine by hand with a status word whose size field is 21
    let mut x = xfer(&[0x90, 0, 0, 0], 16);
    let mut feed = Feed::Start;
    let mut ops = 0;
    let err = loop {
        match x.step(feed) {
            AuxStep::Op(AuxOp::Read(_)) => feed = Feed::Value(0),
            AuxStep::Op(AuxOp::Write(..)) => feed = Feed::Ack,
            AuxStep::Op(AuxOp::WaitNotBusy(_)) => feed = Feed::Value(DP_AUX_CH_CTL_DONE | (21 << 20)),
            AuxStep::Op(_) => feed = Feed::Ack,
            AuxStep::Done(r) => break r.unwrap_err(),
        }
        ops += 1;
        assert!(ops < 1000);
    };
    assert_eq!((err.errno, err.why), (AuxErrno::Busy, "forbidden-recv-bytes"));
}

#[test]
fn oversized_messages_are_e2big_before_anything_is_sent() {
    let mut hw = AuxHw::new(AuxCh::A, |_, _| panic!("nothing may be sent"));
    let e = run_xfer(&mut hw, &mut xfer(&[0u8; 21], 2)).unwrap_err();
    assert_eq!(e.errno, AuxErrno::TooBig);
    let mut hw = AuxHw::new(AuxCh::A, |_, _| panic!("nothing may be sent"));
    let e = run_xfer(&mut hw, &mut xfer(&[0u8; 4], 21)).unwrap_err();
    assert_eq!(e.errno, AuxErrno::TooBig);
    assert!(hw.writes.is_empty());
    // transfer level: a 17-byte native write needs 21 bytes on the wire (:503); a 20-byte read needs 21 (:531)
    let mut big = msg(DP_AUX_NATIVE_WRITE, 0x100, 17);
    big.data = [0; 16];
    // size 17 exceeds the 16-byte payload buffer too: refused as E2BIG, not a panic
    assert_eq!(AuxTransfer::new(AuxCh::A, &big).unwrap_err().errno, AuxErrno::TooBig);
    assert_eq!(AuxTransfer::new(AuxCh::A, &msg(DP_AUX_NATIVE_READ, 0, 20)).unwrap_err().errno, AuxErrno::TooBig);
    assert!(AuxTransfer::new(AuxCh::A, &msg(DP_AUX_NATIVE_READ, 0, 19)).is_ok());
    assert!(AuxTransfer::new(AuxCh::A, &msg(DP_AUX_NATIVE_WRITE, 0, 16)).is_ok());
    assert_eq!(AuxTransfer::new(AuxCh::A, &msg(0x3, 0, 1)).unwrap_err().errno, AuxErrno::Invalid);
}

#[test]
fn receive_is_clamped_to_the_requested_size() {
    // sink returns 6 bytes though only 3 were asked for
    let mut hw = AuxHw::new(AuxCh::A, |_, _| ack_read(&[1, 2, 3, 4, 5]));
    let r = run_xfer(&mut hw, &mut xfer(&[0x90, 0, 0, 2], 3)).unwrap();
    assert_eq!(r.len, 3);
    assert_eq!(&r.data[..3], &[0x00, 1, 2]);
}

#[test]
fn a_wrong_kind_of_feed_is_a_named_error_not_a_panic() {
    let mut x = xfer(&[0x90, 0, 0, 0], 2);
    let _ = x.step(Feed::Start);
    match x.step(Feed::Ack) {
        AuxStep::Done(Err(e)) => assert_eq!(e.errno, AuxErrno::Invalid),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_completion_wait_is_a_ten_ms_poll_on_send_busy_capped_at_its_read_count() {
    // intel_dp_aux_wait_done(): wait_event_timeout(.., 10 ms) on SEND_BUSY clear (:57-77); polled here every 50 us
    let mut x = xfer(&[0x90, 0, 0, 0], 2);
    let mut feed = Feed::Start;
    let plan = loop {
        match x.step(feed) {
            AuxStep::Op(AuxOp::Read(_)) => feed = Feed::Value(0),
            AuxStep::Op(AuxOp::Write(..)) => feed = Feed::Ack,
            AuxStep::Op(AuxOp::WaitNotBusy(p)) => break p,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(plan.reg.offset(), 0x64010);
    assert_eq!((plan.mask, plan.want), (DP_AUX_CH_CTL_SEND_BUSY, 0));
    assert_eq!((WAIT_DONE_TIMEOUT_US, WAIT_DONE_INTERVAL_US), (10_000, 50));
    assert_eq!((plan.interval_us, plan.max_polls), (50, 201));
    // a channel stuck busy ends the wait at exactly that many reads and hands back the last status
    let mut reads = 0;
    let r = plan.run(|| { reads += 1; DP_AUX_CH_CTL_SEND_BUSY }, |_| {});
    assert_eq!(reads, 201);
    assert_eq!(r.unwrap_err().got, DP_AUX_CH_CTL_SEND_BUSY);
}

#[test]
fn a_status_that_stays_busy_after_the_wait_is_retried_and_ends_not_done() {
    // the wait times out with BUSY still set and no DONE: five sends, then "not done" (:387-392)
    let mut x = xfer(&[0x90, 0, 0, 0], 2);
    let mut feed = Feed::Start;
    let mut sends = 0;
    let err = loop {
        match x.step(feed) {
            AuxStep::Op(AuxOp::Read(_)) => feed = Feed::Value(0),
            AuxStep::Op(AuxOp::Write(r, v)) => {
                if r.offset() == 0x64010 && v == skl_aux_send_ctl(4) {
                    sends += 1;
                }
                feed = Feed::Ack;
            }
            AuxStep::Op(AuxOp::WaitNotBusy(_)) => feed = Feed::Value(DP_AUX_CH_CTL_SEND_BUSY),
            AuxStep::Op(_) => feed = Feed::Ack,
            AuxStep::Done(r) => break r.unwrap_err(),
        }
        assert!(sends <= 5);
    };
    assert_eq!(sends, 5);
    assert_eq!((err.errno, err.why), (AuxErrno::Busy, "not-done"));
    assert_eq!(err.name(), "AUX-EBUSY");
    assert_eq!(AuxFail { errno: AuxErrno::TooBig, why: "", status: 0 }.name(), "AUX-E2BIG");
    assert_eq!(AuxFail { errno: AuxErrno::Io, why: "", status: 0 }.name(), "AUX-EIO");
    assert_eq!(AuxFail { errno: AuxErrno::TimedOut, why: "", status: 0 }.name(), "AUX-ETIMEDOUT");
    assert_eq!(AuxFail { errno: AuxErrno::Invalid, why: "", status: 0 }.name(), "AUX-EINVAL");
}

// ---- mutation-sweep additions: exact boundaries -----------------------------------------------------------------------
#[test]
fn unpack_never_writes_past_four_bytes_even_into_a_longer_slice() {
    let mut e = [0xEEu8; 5];
    aux_unpack(0xA1B2_C3D4, &mut e);
    assert_eq!(e, [0xA1, 0xB2, 0xC3, 0xD4, 0xEE]);
    assert_eq!(aux_pack(&[1, 2, 3, 4, 5]), 0x0102_0304);
}

/// Run one transfer to completion and return the ops it asked for.
fn ops_of(x: &mut AuxXfer, status_after_send: u32, rx_words: &[u32]) -> (Vec<AuxOp>, Result<AuxRx, AuxFail>) {
    let mut ops = vec![];
    let mut feed = Feed::Start;
    let mut data = rx_words.iter();
    loop {
        match x.step(feed) {
            AuxStep::Op(op) => {
                ops.push(op);
                feed = match op {
                    AuxOp::Read(r) if r.offset() == 0x64010 => Feed::Value(0),
                    AuxOp::Read(_) => Feed::Value(*data.next().expect("more data registers read than provided")),
                    AuxOp::WaitNotBusy(_) => Feed::Value(status_after_send),
                    _ => Feed::Ack,
                };
            }
            AuxStep::Done(r) => return (ops, r),
        }
        assert!(ops.len() < 200);
    }
}

#[test]
fn exactly_the_needed_receive_registers_are_read() {
    for (n, regs) in [(1usize, 1usize), (3, 1), (4, 1), (5, 2), (8, 2), (9, 3), (16, 4), (17, 5), (20, 5)] {
        let status = DP_AUX_CH_CTL_DONE | dp_aux_ch_ctl_message_size(n as u32);
        let mut x = xfer(&[0x90, 0, 0, 0], 20);
        let (ops, r) = ops_of(&mut x, status, &[0x0102_0304; 5]);
        let r = r.unwrap();
        assert_eq!(r.len, n, "n={n}");
        let reads = ops.iter().filter(|o| matches!(o, AuxOp::Read(r) if r.offset() >= 0x64014)).count();
        assert_eq!(reads, regs, "n={n}");
        // the bytes land in order, MSB first, and the last partial word only fills what is left
        let want: Vec<u8> = (0..n).map(|i| [1u8, 2, 3, 4][i % 4]).collect();
        assert_eq!(&r.data[..n], &want[..], "n={n}");
        assert!(r.data[n..].iter().all(|&b| b == 0), "bytes beyond the reply stay untouched (n={n})");
    }
}

#[test]
fn twenty_bytes_in_and_twenty_bytes_out_are_allowed() {
    // sizes of exactly 20 are the limit, not over it (:338, :425)
    let mut x = xfer(&[0xAA; 20], 20);
    let status = DP_AUX_CH_CTL_DONE | dp_aux_ch_ctl_message_size(20);
    let (ops, r) = ops_of(&mut x, status, &[0x1111_1111; 5]);
    assert_eq!(r.unwrap().len, 20);
    let loads = ops.iter().filter(|o| matches!(o, AuxOp::Write(r, _) if r.offset() >= 0x64014 && r.offset() <= 0x64024)).count();
    assert_eq!(loads, 5);
    // through the message layer: a 19-byte native read is 20 bytes on the wire
    let mut hw = AuxHw::new(AuxCh::A, |_, _| ack_read(&[7; 19]));
    let mut t = AuxTransfer::new(AuxCh::A, &msg(DP_AUX_NATIVE_READ, 0x10, 19)).unwrap();
    let mut feed = Feed::Start;
    let r = loop {
        match t.step(feed) {
            TransferStep::Op(op) => feed = hw.perform(op),
            TransferStep::Done(r) => break r.unwrap(),
        }
    };
    assert_eq!(r.ret, 19);
    assert_eq!(&r.data[..19], &[7u8; 19]);
    // and a 16-byte native write is 20 bytes on the wire
    let mut m = msg(DP_AUX_NATIVE_WRITE, 0x100, 16);
    m.data = [9; 16];
    let mut hw = AuxHw::new(AuxCh::A, |_, _| ack_write());
    let mut t = AuxTransfer::new(AuxCh::A, &m).unwrap();
    let mut feed = Feed::Start;
    let r = loop {
        match t.step(feed) {
            TransferStep::Op(op) => feed = hw.perform(op),
            TransferStep::Done(r) => break r.unwrap(),
        }
    };
    assert_eq!(r.ret, 16);
    assert_eq!(hw.sent[0].len(), 20);
}

#[test]
fn every_retry_reloads_the_send_data_registers() {
    let mut x = xfer(&[0x80, 0x01, 0x02, 0x03, 0x21, 0x00, 0x00, 0x00, 0x00], 2);
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_RECEIVE_ERROR, vec![]));
    let _ = run_xfer(&mut hw, &mut x).unwrap_err();
    for reg in [0x64014u32, 0x64018, 0x6401c] {
        let n = hw.writes.iter().filter(|(r, _)| *r == reg).count();
        assert_eq!(n, 5, "data register {reg:#x} must be rewritten on each of the 5 tries");
    }
}

#[test]
fn refusals_report_status_zero_because_nothing_was_read() {
    let mut big = msg(DP_AUX_NATIVE_WRITE, 0x100, 17);
    big.data = [0; 16];
    assert_eq!(AuxTransfer::new(AuxCh::A, &big).unwrap_err().status, 0);
    assert_eq!(AuxTransfer::new(AuxCh::A, &msg(DP_AUX_NATIVE_READ, 0, 20)).unwrap_err().status, 0);
    assert_eq!(AuxTransfer::new(AuxCh::A, &msg(3, 0, 1)).unwrap_err().status, 0);
    let mut hw = AuxHw::new(AuxCh::A, |_, _| (DP_AUX_CH_CTL_DONE, vec![0, 0]));
    let mut t = AuxTransfer::new(AuxCh::A, &msg(DP_AUX_NATIVE_READ, 0, 1)).unwrap();
    let mut feed = Feed::Start;
    loop {
        match t.step(feed) {
            TransferStep::Op(op) => feed = hw.perform(op),
            TransferStep::Done(r) => {
                r.unwrap();
                break;
            }
        }
    }
    // stepping a finished transfer is a named error with status 0
    match t.step(Feed::Ack) {
        TransferStep::Done(Err(e)) => assert_eq!((e.errno, e.why, e.status), (AuxErrno::Invalid, "stepped-after-done", 0)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_write_asks_for_at_most_two_reply_bytes() {
    // rxsize = 2 for writes (:501): a sink that answers with 3 is clamped to 2 bytes, which still carries reply + short count
    let mut m = msg(DP_AUX_NATIVE_WRITE, 0x102, 1);
    m.data[0] = 1;
    let status = DP_AUX_CH_CTL_DONE | dp_aux_ch_ctl_message_size(3);
    let mut x = AuxXfer::new(AuxCh::A, &[0x80, 0x01, 0x02, 0x00, 1], 2, 0);
    let (ops, r) = ops_of(&mut x, status, &[0x0003_0000]);
    assert_eq!(r.unwrap().len, 2);
    assert_eq!(ops.iter().filter(|o| matches!(o, AuxOp::Read(r) if r.offset() == 0x64014)).count(), 1);
    let _ = m;
}
