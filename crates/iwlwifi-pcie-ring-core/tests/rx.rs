// SPDX-License-Identifier: GPL-2.0-only
//! The receive queue: the RFH program, descriptor encoding, index arithmetic, the packets inside a buffer.
//! Linux `pcie/gen1_2/rx.c:145-241, :900-986, :1294-1441, :1498-1625`.
mod common;
use common::{Op, Rec};
use iwlwifi_pcie_ring_core::rx::*;
use iwlwifi_pcie_ring_core::rxregs::*;

#[test]
fn the_rfh_program_is_rx_mq_hw_init_in_order() {
    let mut io = Rec::default();
    let dma = RxqDma { bd: 0x2_0000_1000, used_bd: 0x2_0000_2000, rb_stts: 0x2_0000_3000 };
    rx_mq_hw_init(&mut io, &dma).unwrap();
    let dma_cfg = 0x8000_0000 | (4 << 16) | (3 << 24) | 0x0400_0000 | (9 << 20);
    assert_eq!(
        io.ops,
        [
            Op::Grab,                                    // rx.c:924
            Op::PrphW(0xA09820, 0),                      // stop Rx DMA                     :928
            Op::PrphW(0xA0980C, 0),                      // disable free and used queues     :930
            Op::Prph64W(0xA08000, 0x2_0000_1000),        // free table                       :934-936
            Op::Prph64W(0xA08100, 0x2_0000_2000),        // used table                       :938-940
            Op::Prph64W(0xA08200, 0x2_0000_3000),        // status block                     :942-944
            Op::PrphW(0xA08080, 0),                      // FRBDCB_WIDX                      :946
            Op::PrphW(0xA080C0, 0),                      // FRBDCB_RIDX                      :947
            Op::PrphW(0xA08180, 0),                      // URBDCB_WIDX                      :948
            Op::PrphW(0xA09820, dma_cfg),                // enable: 4K, min 4/8, drop, 512   :960-964
            Op::PrphW(0xA09800, 0x13),                   // snoop x2 | 128-byte chunks       :971-978
            Op::PrphW(0xA0980C, 0x0001_0001),            // queue 0 active: BIT(0) | BIT(16) :980
            Op::Release,                                 //                                  :982
            Op::W8(0x004, 0x40),                         // CSR_INT_COALESCING               :985
        ]
    );
}

#[test]
fn a_failed_grab_is_the_refusal_and_nothing_is_programmed() {
    let mut io = Rec::default();
    io.fail_grab = true;
    assert!(rx_mq_hw_init(&mut io, &RxqDma { bd: 0, used_bd: 0, rb_stts: 0 }).is_err());
    assert!(io.ops.is_empty());
}

#[test]
fn field_prep_shifts_to_the_masks_lowest_bit_and_clips() {
    assert_eq!(field_prep(0x10, 1), 0x10);
    assert_eq!(field_prep(0xF00, 0), 0);
    assert_eq!(field_prep(0xF00, 3), 0x300);
    assert_eq!(field_prep(0xF00, 0x13), 0x300, "bits above the field are clipped");
    assert_eq!(field_prep(0x0F00_0000, 5), 0x0500_0000);
}

#[test]
fn a_free_descriptor_is_the_page_address_or_the_virtual_id() {
    assert_eq!(free_bd(0x2_0000_1000, 7), Ok(0x2_0000_1007)); // rx.c:232-236
    assert_eq!(free_bd(0x2_0000_1000, 0xFFF), Ok(0x2_0000_1FFF));
    assert_eq!(free_bd(0x1000, 0), Err(RxError::Vid { vid: 0 }), "VID 0 means no buffer");
    assert_eq!(free_bd(0x1000, 0x1000), Err(RxError::Vid { vid: 0x1000 }), "12 bits");
    assert_eq!(free_bd(0x1800, 1), Err(RxError::AddrOverlapsVid { page_dma: 0x1800 }), "the page leaves the VID bits clear");
}

#[test]
fn a_used_descriptor_carries_the_vid_in_12_bits() {
    assert_eq!(used_bd_vid(0x0000_0123), 0x123);
    assert_eq!(used_bd_vid(0xFFFF_F123), 0x123, "rx.c:1473: & 0x0FFF");
}

#[test]
fn the_status_block_is_12_bytes_with_closed_rb_num_first_in_12_bits() {
    let mut s = [0u8; 12];
    s[0..2].copy_from_slice(&0xF234u16.to_le_bytes());
    s[2..4].copy_from_slice(&0x0003u16.to_le_bytes());
    s[4..6].copy_from_slice(&0x0004u16.to_le_bytes());
    s[6..8].copy_from_slice(&0x0005u16.to_le_bytes());
    assert_eq!(closed_rb(&s), Ok(0x234)); // internal.h:205: & 0xFFF
    assert_eq!(rb_status_fields(&s), Ok([0xF234, 3, 4, 5]));
    assert_eq!(closed_rb(&s[..11]), Err(RxError::Buf { len: 11 }));
}

#[test]
fn indices_wrap_at_the_queue_size_and_the_device_index_moves_in_eights() {
    let mut q = Rxq::new(512);
    assert!(!q.needs_wr_ptr());
    for want in 0..7u16 {
        assert_eq!(q.restock_slot(), want);
    }
    assert!(!q.needs_wr_ptr(), "7 posted: round_down(7, 8) == 0 == write_actual");
    assert_eq!(q.restock_slot(), 7);
    assert!(q.needs_wr_ptr(), "8 posted");
    assert_eq!(q.wr_ptr_update(0), (0x1C80, 8)); // WIDX_TRG, rx.c:193-201
    assert!(!q.needs_wr_ptr());
    for _ in 0..5 {
        q.restock_slot();
    }
    assert_eq!((q.write, q.write_actual), (13, 8));
    assert_eq!(q.wr_ptr_update(0), (0x1C80, 8), "13 rounds down to 8");
    for _ in 0..(512 - 13) {
        q.restock_slot();
    }
    assert_eq!(q.write, 0, "wrapped at 512");
}

#[test]
fn pending_counts_the_buffers_the_card_closed_across_the_wrap() {
    let mut q = Rxq::new(512);
    assert_eq!(q.pending(0), 0);
    assert_eq!(q.pending(5), 5);
    for _ in 0..510 {
        q.take_read();
    }
    assert_eq!(q.pending(3), 5, "closed 3, read 510: 512 - 510 + 3");
    assert_eq!(q.pending(0x1000 | 3), 5, "the closed index is taken modulo the queue size (the 9000 A0 wrap bug, rx.c:1517)");
    assert_eq!(q.take_read(), 510);
    assert_eq!(q.take_read(), 511);
    assert_eq!(q.take_read(), 0);
}

fn pkt(group: u8, cmd: u8, seq: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = ((4 + payload.len()) as u32).to_le_bytes().to_vec();
    v.extend_from_slice(&[cmd, group]);
    v.extend_from_slice(&seq.to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn rb(pkts: &[Vec<u8>], marker: bool) -> Vec<u8> {
    let mut b = vec![0u8; 4096];
    let mut o = 0;
    for p in pkts {
        b[o..o + p.len()].copy_from_slice(p);
        o += p.len().div_ceil(64) * 64;
    }
    if marker {
        b[o..o + 4].copy_from_slice(&0x5555_0000u32.to_le_bytes());
    }
    b
}

#[test]
fn packets_in_a_buffer_are_64_byte_aligned_and_end_at_the_invalid_marker() {
    let b = rb(&[pkt(0, 0x01, 0x8000, &[1; 60]), pkt(3, 0x08, 0x8001, &[2; 10]), pkt(0, 0xc1, 0x8002, &[])], true);
    let got: Vec<(usize, u8, u8, u16, usize)> = rb_packets(&b).map(|p| (p.offset, p.group_id, p.cmd, p.sequence, p.data.len())).collect();
    // first: 4 (len word) + 4 (hdr) + 60 = 68 bytes -> aligned to 128; second at 128: 4+4+10 -> 64; third at 192
    assert_eq!(got, [(0, 0, 0x01, 0x8000, 60), (128, 3, 0x08, 0x8001, 10), (192, 0, 0xc1, 0x8002, 0)]);
    assert!(rb_packets(&b).all(|p| p.is_notification() && p.rx_queue() == 0));
}

#[test]
fn a_buffer_with_no_marker_or_a_bad_length_ends_the_walk_without_a_phantom_packet() {
    // no marker: the zero word after the packet is a zero-length frame (< sizeof(*pkt)), rx.c:1353
    let b = rb(&[pkt(0, 0x01, 0x8000, &[1; 4])], false);
    assert_eq!(rb_packets(&b).count(), 1);
    // a frame claiming more than the buffer holds
    let mut b = rb(&[], true);
    b[0..4].copy_from_slice(&0x3FFFu32.to_le_bytes());
    b[4] = 0x42;
    assert_eq!(rb_packets(&b).count(), 0);
    // an all-0x5555 buffer (never written)
    assert_eq!(rb_packets(&vec![0x55u8; 4096]).count(), 0);
    // the invalid marker is the EXACT word (rx.c:1325 compares equal): 0x5555_0001 is a frame of 1 byte, too short for a header...
    let mut b = vec![0u8; 4096];
    b[0..4].copy_from_slice(&0x5555_0001u32.to_le_bytes());
    assert_eq!(rb_packets(&b).count(), 0);
    // ...and 0x5555_0010 is NOT the marker: it is a 16-byte frame whose flags happen to have those high bits
    b[0..4].copy_from_slice(&0x5555_0010u32.to_le_bytes());
    b[4] = 0x42;
    assert_eq!(rb_packets(&b).map(|p| (p.cmd, p.data.len())).collect::<Vec<_>>(), [(0x42, 12)]);
}

#[test]
fn the_queue_field_of_a_frame_is_reported() {
    let mut p = pkt(0, 0x01, 0x8000, &[]);
    p[2] = 0x05; // len_n_flags bits 16-21: rxq 5
    let b = rb(&[p], true);
    assert_eq!(rb_packets(&b).next().unwrap().rx_queue(), 5);
}

#[test]
fn only_replies_are_reclaimed_and_tx_cmd_never_is() {
    let p = |group, cmd, seq| RxPkt { offset: 0, len_n_flags: 8, cmd, group_id: group, sequence: seq, data: &[] };
    assert!(reclaim(&p(0, 0x88, 0x0003), &[0x1c]), "a reply to a command");
    assert!(!reclaim(&p(0, 0x88, 0x8003), &[0x1c]), "SEQ_RX_FRAME: a notification");
    assert!(!reclaim(&p(0, 0x1c, 0x0003), &[0x1c]), "TX_CMD is in no_reclaim_cmds (mvm/ops.c:1203-1205)");
    assert!(reclaim(&p(1, 0x1c, 0x0003), &[0x1c]), "the list applies to group 0 only (rx.c:1366)");
}
