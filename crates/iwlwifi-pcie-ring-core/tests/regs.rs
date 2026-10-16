// SPDX-License-Identifier: GPL-2.0-only
//! Every RFH / scheduler / queue constant pinned by VALUE against the Linux literal. `iwl-fh.h`, `iwl-prph.h`, `iwl-scd.h`,
//! `iwl-trans.h`, `iwl-config.h`, `fw/api/txq.h`, `cfg/9000.c` at Linux v7.0-rc6.
use iwlwifi_pcie_ring_core::rxregs::*;
use iwlwifi_pcie_ring_core::txregs::*;

#[test]
fn rfh_registers() {
    assert_eq!(RFH_Q0_FRBDCB_BA_LSB, 0xA08000); // iwl-fh.h:331
    assert_eq!(RFH_Q0_FRBDCB_WIDX, 0xA08080); // iwl-fh.h:334
    assert_eq!(RFH_Q0_FRBDCB_WIDX_TRG, 0x1C80); // iwl-fh.h:337
    assert_eq!(RFH_Q0_FRBDCB_RIDX, 0xA080C0); // iwl-fh.h:340
    assert_eq!(RFH_Q0_URBDCB_BA_LSB, 0xA08100); // iwl-fh.h:343
    assert_eq!(RFH_Q0_URBDCB_WIDX, 0xA08180); // iwl-fh.h:346
    assert_eq!(RFH_Q0_URBD_STTS_WPTR_LSB, 0xA08200); // iwl-fh.h:351
    assert_eq!(RFH_RXF_DMA_CFG, 0xA09820); // iwl-fh.h:387
    assert_eq!(RFH_RXF_RXQ_ACTIVE, 0xA0980C); // iwl-fh.h:422
    assert_eq!(RFH_GEN_CFG, 0xA09800); // iwl-fh.h:424
}

#[test]
fn per_queue_register_strides() {
    // iwl-fh.h:332 (+ q*8), :335 (+ q*4), :338, :341, :344 (+ q*8), :347, :352 (+ q*8)
    assert_eq!(rfh_q_frbdcb_ba_lsb(1) - rfh_q_frbdcb_ba_lsb(0), 8);
    assert_eq!(rfh_q_frbdcb_widx(1) - rfh_q_frbdcb_widx(0), 4);
    assert_eq!(rfh_q_frbdcb_widx_trg(1) - rfh_q_frbdcb_widx_trg(0), 4);
    assert_eq!(rfh_q_frbdcb_ridx(1) - rfh_q_frbdcb_ridx(0), 4);
    assert_eq!(rfh_q_urbdcb_ba_lsb(1) - rfh_q_urbdcb_ba_lsb(0), 8);
    assert_eq!(rfh_q_urbdcb_widx(1) - rfh_q_urbdcb_widx(0), 4);
    assert_eq!(rfh_q_urbd_stts_wptr_lsb(1) - rfh_q_urbd_stts_wptr_lsb(0), 8);
    assert_eq!((rfh_q_frbdcb_ba_lsb(0), rfh_q_frbdcb_widx_trg(0), rfh_q_urbd_stts_wptr_lsb(0)), (0xA08000, 0x1C80, 0xA08200));
}

#[test]
fn rfh_dma_config_fields() {
    assert_eq!(RFH_RXF_DMA_RB_SIZE_POS, 16); // iwl-fh.h:391
    assert_eq!(RFH_RXF_DMA_RB_SIZE_2K, 0x2_0000); // :393
    assert_eq!(RFH_RXF_DMA_RB_SIZE_4K, 0x4_0000); // :394
    assert_eq!(RFH_RXF_DMA_RB_SIZE_8K, 0x8_0000); // :395
    assert_eq!(RFH_RXF_DMA_RB_SIZE_12K, 0x9_0000); // :396
    assert_eq!(RFH_RXF_DMA_RBDCB_SIZE_POS, 20); // :404
    assert_eq!(RFH_RXF_DMA_RBDCB_SIZE_512, 0x90_0000); // :411
    assert_eq!(RFH_RXF_DMA_MIN_RB_SIZE_POS, 24); // :415
    assert_eq!(RFH_RXF_DMA_MIN_RB_4_8, 0x300_0000); // :416
    assert_eq!(RFH_RXF_DMA_DROP_TOO_LARGE_MASK, 0x0400_0000); // :417
    assert_eq!(RFH_DMA_EN_ENABLE_VAL, 0x8000_0000); // :420
    assert_eq!((RFH_GEN_CFG_SERVICE_DMA_SNOOP, RFH_GEN_CFG_RFH_DMA_SNOOP, RFH_GEN_CFG_RB_CHUNK_SIZE), (1, 2, 0x10)); // :425-427
    assert_eq!((RFH_GEN_CFG_RB_CHUNK_SIZE_128, RFH_GEN_CFG_RB_CHUNK_SIZE_64), (1, 0)); // :428-429
    assert_eq!(RFH_GEN_CFG_DEFAULT_RXQ_NUM, 0xF00); // :431
}

#[test]
fn receive_buffer_constants() {
    assert_eq!(FH_RSCSR_FRAME_ALIGN, 0x40); // iwl-trans.h:65
    assert_eq!(FH_RSCSR_FRAME_INVALID, 0x5555_0000); // iwl-trans.h:64
    assert_eq!(FH_RSCSR_FRAME_SIZE_MSK, 0x3FFF); // iwl-trans.h:63
    assert_eq!((FH_RSCSR_RXQ_POS, FH_RSCSR_RXQ_MASK), (16, 0x3F_0000)); // iwl-trans.h:68-69
    assert_eq!(IWL_NUM_RBDS_NON_HE, 512); // iwl-config.h:381 (64 * 8)
    assert_eq!(RB_STATUS_LEN, 12); // iwl-fh.h:581-587: 4 x le16 + le32
    assert_eq!((FREE_BD_LEN, USED_BD_LEN), (8, 4)); // rx.c:664-678
    assert_eq!(RX_VID_MASK, 0xFFF); // rx.c:1473
    assert_eq!(RX_WRITE_PTR_ALIGN, 8); // rx.c:193
    assert_eq!(RB_BYTES, 4096); // IWL_AMSDU_4K
}

#[test]
fn scheduler_registers_and_fields() {
    assert_eq!(SCD_BASE, 0xA02C00); // iwl-prph.h:223
    assert_eq!(SCD_SRAM_BASE_ADDR, 0xA02C00); // :225
    assert_eq!(SCD_DRAM_BASE_ADDR, 0xA02C08); // :226
    assert_eq!(SCD_TXFACT, 0xA02C10); // :228
    assert_eq!(SCD_AGGR_SEL, 0xA02E48); // :232
    assert_eq!(SCD_EN_CTRL, 0xA02E54); // :235
    assert_eq!(SCD_CONTEXT_MEM_LOWER_BOUND, 0x600); // :203
    assert_eq!(SCD_TRANS_TBL_MEM_LOWER_BOUND, 0x7E0); // :211
    assert_eq!((SCD_QUEUE_STTS_REG_POS_TXF, SCD_QUEUE_STTS_REG_POS_ACTIVE, SCD_QUEUE_STTS_REG_POS_WSL, SCD_QUEUE_STTS_REG_POS_SCD_ACT_EN), (0, 3, 4, 19)); // :185-188
    assert_eq!(SCD_QUEUE_STTS_REG_MSK, 0x017F_0000); // :189
    assert_eq!((SCD_QUEUE_CTX_REG2_WIN_SIZE, SCD_QUEUE_CTX_REG2_FRAME_LIMIT), (0x7F, 0x7F_0000)); // :195-196
    // SCD_CONTEXT_QUEUE_OFFSET(x) = 0x600 + x*8 (:214-215); SCD_TRANS_TBL_OFFSET_QUEUE(x) = (0x7E0 + x*2) & 0xfffc (:220-221)
    assert_eq!((scd_context_queue_offset(0), scd_context_queue_offset(3)), (0x600, 0x618));
    assert_eq!((scd_trans_tbl_offset_queue(0), scd_trans_tbl_offset_queue(31)), (0x7E0, 0x81C));
    // iwl-scd.h:60-74: SCD_QUEUE_RDPTR(q) = SCD_BASE + 0x68 + q*4; SCD_QUEUE_STATUS_BITS(q) = SCD_BASE + 0x10c + q*4 (q < 20)
    assert_eq!((scd_queue_rdptr(0), scd_queue_rdptr(2)), (0xA02C68, 0xA02C70));
    assert_eq!((scd_queue_status_bits(0), scd_queue_status_bits(2)), (0xA02D0C, 0xA02D14));
}

#[test]
fn queue_and_command_constants() {
    assert_eq!(IWL9000_NUM_OF_QUEUES, 31); // cfg/9000.c:31
    assert_eq!(MAX_TFD_QUEUE_SIZE, 256); // cfg/9000.c:32
    assert_eq!((IWL_MVM_DQA_CMD_QUEUE, IWL_MVM_TX_FIFO_CMD), (0, 7)); // fw/api/txq.h:35, :54
    assert_eq!(IWL_CMD_QUEUE_SIZE, 32); // fw/api/txq.h:93
    assert_eq!(IWL_FRAME_LIMIT, 64); // iwl-trans.h:273
    assert_eq!(IWL_DEF_WD_TIMEOUT, 2500); // iwl-config.h:86
    assert_eq!(TFD_SIZE, 128); // sizeof(struct iwl_tfd): 3 reserved + num_tbs + 20 x 6 + 4 pad = 128 (iwl-fh.h:692-697)
    assert_eq!(4 + 20 * 6 + 4, TFD_SIZE);
    assert_eq!(IWL_NUM_OF_TBS, 20); // iwl-fh.h:598
    assert_eq!(IWL_TX_DMA_BITS, 36); // iwl-fh.h:597 (DMA_BIT_MASK(36))
    assert_eq!((IWL_FIRST_TB_SIZE, IWL_FIRST_TB_SIZE_ALIGN), (20, 64)); // iwl-trans.h:712-713
    assert_eq!(DEF_CMD_PAYLOAD_SIZE, 320); // iwl-trans.h:121
    assert_eq!(TFD_MAX_PAYLOAD_SIZE, 324); // sizeof(struct iwl_device_cmd): 4 + 320 (iwl-trans.h:135-148)
    assert_eq!(8 + (320 - 8 + 4), TFD_MAX_PAYLOAD_SIZE, "the wide arm of the union has the same size");
    assert_eq!(IWL_MAX_CMD_TBS_PER_TFD, 2); // iwl-trans.h:168
    assert_eq!(IWL_ALWAYS_LONG_GROUP, 1); // fw/api/cmdhdr.h:57
    assert_eq!((CMD_HDR_LEN, CMD_HDR_WIDE_LEN), (4, 8)); // fw/api/cmdhdr.h:65-122
    assert_eq!(HOST_COMPLETE_TIMEOUT_MS, 2000); // pcie/gen1_2/tx.c:2548
}
