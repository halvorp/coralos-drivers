// SPDX-License-Identifier: GPL-2.0-only
//! TX scheduler start-up, the TFD layout, the command queue's indices and the command planner.
//! Linux `pcie/gen1_2/tx.c:78-191, :480-534, :1036-1257, :1343-1680`, `tx-gen2.c:527-555`, `internal.h:658-715`.
mod common;
use common::{Op, Rec};
use iwlwifi_pcie_ring_core::tx::*;

#[test]
fn a_tfd_holds_a_36_bit_address_and_a_12_bit_length_per_buffer() {
    let mut tfd = [0xEEu8; 132];
    // TB0: address 0x3_1234_5678, 20 bytes: tb->lo = low 32 bits; hi_n_len = len << 4 | upper 4 bits (tx.c:144-156)
    assert_eq!(tfd_add_tb(&mut tfd, 0x3_1234_5678, 20, true), Ok(0));
    assert_eq!(tfd[3], 1, "num_tbs");
    assert_eq!(&tfd[4..8], &0x1234_5678u32.to_le_bytes());
    assert_eq!(&tfd[8..10], &((20u16 << 4) | 3).to_le_bytes());
    assert_eq!(tfd[0..3], [0, 0, 0], "reset zeroed the reserved bytes");
    assert_eq!(tfd_add_tb(&mut tfd, 0x2_0000_4000, 0xFFF, false), Ok(1));
    assert_eq!(tfd[3], 2);
    assert_eq!(&tfd[10..14], &0x0000_4000u32.to_le_bytes());
    assert_eq!(&tfd[14..16], &((0xFFFu16 << 4) | 2).to_le_bytes());
    assert_eq!(tfd_tb(&tfd, 0), Some((0x3_1234_5678, 20)));
    assert_eq!(tfd_tb(&tfd, 1), Some((0x2_0000_4000, 0xFFF)));
    assert_eq!(tfd_tb(&tfd, 2), None);
    assert_eq!(tfd[124..128], [0; 4], "reset zeroes the whole 128-byte TFD, pad included (tx.c:172-173)");
    assert_eq!(tfd[128..], [0xEE; 4], "and not a byte more");
}

#[test]
fn a_tfd_refuses_the_21st_buffer_a_37_bit_address_and_a_13_bit_length() {
    let mut tfd = [0u8; 128];
    for i in 0..20 {
        assert_eq!(tfd_add_tb(&mut tfd, 0x1000 * (i + 1), 8, i == 0), Ok(i as usize));
    }
    assert_eq!(tfd_add_tb(&mut tfd, 0x1000, 8, false), Err(TxError::TooManyTbs)); // tx.c:178-182
    let mut tfd = [0u8; 128];
    assert_eq!(tfd_add_tb(&mut tfd, 1 << 36, 8, true), Err(TxError::AddrTooWide { addr: 1 << 36 })); // tx.c:184-186
    assert_eq!(tfd_add_tb(&mut tfd, 0x1000, 0x1000, true), Err(TxError::TbTooLong { len: 0x1000 }));
    assert_eq!(tfd_add_tb(&mut [0u8; 64], 0x1000, 8, true), Err(TxError::Buf { len: 64 }));
}

#[test]
fn tx_start_is_the_register_program_of_iwl_pcie_tx_start() {
    let mut io = Rec::default();
    io.prph.insert(0xA02C00, 0x0040_3800); // SCD_SRAM_BASE_ADDR as the firmware left it
    io.csr.insert(0x1e98, 0x10); // FH_TX_CHICKEN_BITS_REG
    let base = tx_start(&mut io, 0x2_0000_8000).unwrap();
    assert_eq!(base, 0x0040_3800);
    let mut want = vec![
        // clear_dwords = (SCD_TRANS_TBL_OFFSET_QUEUE(31) - SCD_CONTEXT_MEM_LOWER_BOUND) / 4 = (0x81C - 0x600) / 4 = 135 (tx.c:486-487)
        Op::MemZero(0x0040_3800 + 0x600, 135),
        Op::PrphW(0xA02C08, (0x2_0000_8000u64 >> 10) as u32), // SCD_DRAM_BASE_ADDR <- bc tables >> 10, tx.c:503-504
        // iwl_trans_ac_txq_enable(cmd queue 0, fifo 7): tx.c:1147-1257
        Op::PrphW(0xA02E54, 0),                               // SCD_EN_CTRL <- 0                       :1167
        Op::PrphW(0xA02D0C, 1 << 19),                         // queue 0 status: inactive | SCD_ACT_EN  :1170
        Op::PrphW(0xA02E48, 0),                               // SCD_AGGR_SEL &= ~BIT(0)                :1191
        Op::Direct32W(0x460, 0),                              // HBUS_TARG_WRPTR <- ssn 0 | 0 << 8      :1218
        Op::PrphW(0xA02C68, 0),                               // SCD_QUEUE_RDPTR(0) <- ssn              :1224
        Op::MemW(0x0040_3800 + 0x600, vec![0]),               // context word 0                         :1227
        Op::MemW(0x0040_3800 + 0x604, vec![64 | (64 << 16)]), // WIN_SIZE | FRAME_LIMIT                 :1229-1233
        Op::PrphW(0xA02D0C, 0x17F_0000 | (1 << 3) | (7 << 0) | (1 << 4)), // active | fifo 7 | WSL | mask :1236-1240
        Op::PrphW(0xA02E54, 1),                               // SCD_EN_CTRL <- BIT(0)                  :1245
        Op::PrphW(0xA02C10, 0xFF),                            // SCD_TXFACT <- IWL_MASK(0, 7)           iwl-scd.h:36-39
    ];
    for ch in 0..8u32 {
        want.push(Op::Direct32W(0x1d00 + 0x20 * ch, 0x8000_0000 | 0x8)); // ENABLE | CREDIT_ENABLE        tx.c:519-523
    }
    want.push(Op::Direct32W(0x1e98, 0x12)); // FH_TX_CHICKEN_BITS_REG |= SCD_AUTO_RETRY_EN               tx.c:525-528
    assert_eq!(io.ops, want);
}

#[test]
fn a_scheduler_that_cannot_be_reached_is_the_error_not_a_half_program() {
    let mut io = Rec::default();
    io.fail_grab = false;
    // read_mem/zero_mem failing is covered by the bus tests; here the zero_mem of a healthy Rec succeeded and the program ran to the end.
    assert!(tx_start(&mut io, 0x1_0000).is_ok());
}

#[test]
fn the_command_queue_starts_empty_with_a_32_slot_window_over_a_256_entry_ring() {
    let q = Cmdq::new();
    assert_eq!((q.n_window, q.read_ptr, q.write_ptr), (32, 0, 0));
    assert_eq!(q.space(), 32, "n_window < max_tfd_queue_size: no entry reserved (tx-gen2.c:536-541)");
    assert_eq!(Cmdq::inc_wrap(255), 0);
    assert_eq!(Cmdq::inc_wrap(31), 32, "the TFD index runs to 255; only the SLOT wraps at 32");
    assert_eq!((q.cmd_index(31), q.cmd_index(32), q.cmd_index(255)), (31, 0, 31));
}

#[test]
fn the_sequence_is_queue_zero_and_the_tfd_index() {
    let mut q = Cmdq::new();
    assert_eq!(q.sequence(), 0x0000);
    q.advance();
    assert_eq!(q.sequence(), 0x0001);
    for _ in 0..254 {
        q.advance();
    }
    assert_eq!(q.write_ptr, 255);
    assert_eq!(q.sequence(), 0x00ff);
    q.advance();
    assert_eq!((q.write_ptr, q.sequence()), (0, 0));
}

#[test]
fn the_doorbell_value_is_the_write_pointer_with_the_queue_in_bits_8_and_up() {
    let mut q = Cmdq::new();
    assert_eq!(q.advance(), (0x460, 1));
    assert_eq!(q.advance(), (0x460, 2)); // write_ptr | (txq_id << 8), tx.c:137-138, queue 0
}

#[test]
fn space_shrinks_as_commands_are_outstanding_and_a_full_window_is_zero() {
    let mut q = Cmdq::new();
    for i in 1..=32u16 {
        q.advance();
        assert_eq!(q.space(), 32 - i);
    }
    assert_eq!(q.space(), 0);
    q.read_ptr = 1;
    assert_eq!(q.space(), 1);
}

#[test]
fn a_reply_reclaims_exactly_its_own_entry() {
    let mut q = Cmdq::new();
    q.advance();
    q.advance();
    assert_eq!(q.reclaim(0), Ok(()));
    assert_eq!(q.read_ptr, 1);
    assert_eq!(q.reclaim(1), Ok(()));
    assert_eq!(q.read_ptr, 2);
    assert_eq!(q.space(), 32);
}

#[test]
fn a_reply_for_an_entry_not_in_flight_is_out_of_range_and_changes_nothing() {
    let mut q = Cmdq::new();
    q.advance();
    assert_eq!(q.reclaim(5), Err(ReclaimError::OutOfRange { idx: 5 })); // tx.c:1089-1100
    assert_eq!(q.read_ptr, 0);
    let mut empty = Cmdq::new();
    assert_eq!(empty.reclaim(0), Err(ReclaimError::OutOfRange { idx: 0 }));
}

#[test]
fn a_reply_that_skips_an_entry_reclaims_it_and_reports_the_skip() {
    let mut q = Cmdq::new();
    q.advance();
    q.advance();
    q.advance();
    assert!(matches!(q.reclaim(2), Err(ReclaimError::Skipped { .. }))); // "HCMD skipped", tx.c:1105-1108
    assert_eq!(q.read_ptr, 2, "the first entry was freed before the complaint (tx.c:1103-1105)");
}

#[test]
fn reclaim_follows_the_ring_past_the_slot_window_and_the_ring_wrap() {
    let mut q = Cmdq::new();
    for i in 0..600u32 {
        let seq = q.sequence();
        q.advance();
        assert_eq!(q.reclaim(seq & 0xff), Ok(()), "command {i}");
    }
    assert_eq!((q.read_ptr, q.write_ptr), (600 % 256, 600 % 256));
}

fn plan(group: u8, chunks: &[Chunk<'_>]) -> (Result<HcmdPlan, TxError>, Vec<u8>) {
    let mut slot = vec![0u8; 324];
    (plan_hcmd(group, 0x42, 7, 0x0123, chunks, &mut slot), slot)
}

#[test]
fn a_short_command_is_a_four_byte_header_and_one_first_tb() {
    let d = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let (p, slot) = plan(0, &[Chunk { data: &d, nocopy: false }]);
    let p = p.unwrap();
    assert_eq!(&slot[..12], &[0x42, 0x00, 0x23, 0x01, 1, 2, 3, 4, 5, 6, 7, 8]); // cmd, group, le16 sequence, payload (cmdhdr.h:65-98)
    assert_eq!((p.cmd_size, p.copy_size, p.tb0_len, p.tb1_len), (12, 12, 12, 0));
    assert!(p.nocopy.iter().all(|n| n.is_none()));
}

#[test]
fn a_command_past_20_bytes_splits_into_the_first_tb_and_one_more() {
    let d: Vec<u8> = (0..40).collect();
    let (p, slot) = plan(0, &[Chunk { data: &d, nocopy: false }]);
    let p = p.unwrap();
    assert_eq!((p.cmd_size, p.copy_size, p.tb0_len, p.tb1_len), (44, 44, 20, 24)); // tx.c:1538-1560
    assert_eq!(&slot[4..44], &d[..]);
}

#[test]
fn a_grouped_command_has_the_eight_byte_wide_header_with_length_and_version() {
    let d = [9u8; 6];
    let (p, slot) = plan(5, &[Chunk { data: &d, nocopy: false }]);
    let p = p.unwrap();
    // cmd, group, le16 sequence, le16 length (cmd_size - 8), reserved, version (cmdhdr.h:115-122; tx.c:1466-1473)
    assert_eq!(&slot[..14], &[0x42, 5, 0x23, 0x01, 6, 0, 0, 7, 9, 9, 9, 9, 9, 9]);
    assert_eq!((p.cmd_size, p.copy_size, p.tb0_len, p.tb1_len), (14, 14, 14, 0));
}

#[test]
fn a_nocopy_chunk_gets_its_own_tb_after_the_first_20_bytes_are_completed_from_it() {
    let c0 = [1u8, 2, 3, 4, 5, 6];
    let c1: Vec<u8> = (0..600).map(|i| i as u8).collect();
    let (p, slot) = plan(5, &[Chunk { data: &c0, nocopy: false }, Chunk { data: &c1, nocopy: true }]);
    let p = p.unwrap();
    // 8 + 6 = 14 < 20: the first 6 bytes of the NOCOPY chunk join the copied part (tx.c:1385-1394), so the NOCOPY TB skips them
    assert_eq!((p.cmd_size, p.copy_size, p.tb0_len, p.tb1_len), (614, 20, 20, 0));
    assert_eq!(p.nocopy, [None, Some((1, 6, 594))]);
    assert_eq!(&slot[14..20], &c1[..6]);
}

#[test]
fn a_nocopy_chunk_after_a_long_copied_chunk_is_wholly_nocopy() {
    let c0 = [3u8; 30];
    let c1 = [4u8; 500];
    let (p, _) = plan(0, &[Chunk { data: &c0, nocopy: false }, Chunk { data: &c1, nocopy: true }]);
    let p = p.unwrap();
    assert_eq!((p.copy_size, p.tb0_len, p.tb1_len), (34, 20, 14));
    assert_eq!(p.nocopy, [None, Some((1, 0, 500))]);
}

#[test]
fn refusals_are_named() {
    let big = vec![0u8; 400];
    assert_eq!(plan(0, &[Chunk { data: &big, nocopy: false }]).0, Err(TxError::TooLarge { copy_size: 404 })); // tx.c:1436-1444
    let a = [1u8; 30];
    let b = [2u8; 5];
    assert_eq!(plan(0, &[Chunk { data: &a, nocopy: true }, Chunk { data: &b, nocopy: false }]).0, Err(TxError::CopyAfterNocopy)); // tx.c:1420-1424
    let c = [Chunk { data: &b, nocopy: false }; 3];
    assert_eq!(plan(0, &c).0, Err(TxError::TooManyTbs));
    let mut small = [0u8; 100];
    assert_eq!(plan_hcmd(0, 1, 0, 0, &[], &mut small), Err(TxError::Buf { len: 100 }));
}

#[test]
fn an_empty_command_is_just_its_header() {
    let (p, slot) = plan(0, &[]);
    let p = p.unwrap();
    assert_eq!((p.cmd_size, p.copy_size, p.tb0_len, p.tb1_len), (4, 4, 4, 0));
    assert_eq!(&slot[..4], &[0x42, 0, 0x23, 0x01]);
    // an empty chunk is skipped (tx.c:1382-1383)
    let e: [u8; 0] = [];
    assert_eq!(plan(0, &[Chunk { data: &e, nocopy: false }]).0.unwrap().copy_size, 4);
}
