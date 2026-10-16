// SPDX-License-Identifier: GPL-2.0-only
//! The receive-flow-handler (RFH) registers and the receive-buffer constants, ported from Linux
//! `iwl-fh.h`, `iwl-trans.h`, `iwl-config.h`, `pcie/gen1_2/internal.h`. Values are the C literals
//! evaluated; every citation names the `#define`'s file and line.
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.

pub const RFH_Q0_FRBDCB_BA_LSB: u32 = 0xA0_8000; // iwl-fh.h:331
pub const RFH_Q0_FRBDCB_WIDX: u32 = 0xA0_8080; // iwl-fh.h:334
pub const RFH_Q0_FRBDCB_WIDX_TRG: u32 = 0x1C80; // iwl-fh.h:337
pub const RFH_Q0_FRBDCB_RIDX: u32 = 0xA0_80C0; // iwl-fh.h:340
pub const RFH_Q0_URBDCB_BA_LSB: u32 = 0xA0_8100; // iwl-fh.h:343
pub const RFH_Q0_URBDCB_WIDX: u32 = 0xA0_8180; // iwl-fh.h:346
pub const RFH_Q0_URBD_STTS_WPTR_LSB: u32 = 0xA0_8200; // iwl-fh.h:351
pub const RFH_RXF_DMA_CFG: u32 = 0xA0_9820; // iwl-fh.h:387
pub const RFH_RXF_RXQ_ACTIVE: u32 = 0xA0_980C; // iwl-fh.h:422
pub const RFH_GEN_CFG: u32 = 0xA0_9800; // iwl-fh.h:424

pub const RFH_RXF_DMA_RB_SIZE_POS: u32 = 16; // iwl-fh.h:391
pub const RFH_RXF_DMA_RB_SIZE_2K: u32 = 0x2 << RFH_RXF_DMA_RB_SIZE_POS; // iwl-fh.h:393
pub const RFH_RXF_DMA_RB_SIZE_4K: u32 = 0x4 << RFH_RXF_DMA_RB_SIZE_POS; // iwl-fh.h:394
pub const RFH_RXF_DMA_RB_SIZE_8K: u32 = 0x8 << RFH_RXF_DMA_RB_SIZE_POS; // iwl-fh.h:395
pub const RFH_RXF_DMA_RB_SIZE_12K: u32 = 0x9 << RFH_RXF_DMA_RB_SIZE_POS; // iwl-fh.h:396
pub const RFH_RXF_DMA_RBDCB_SIZE_POS: u32 = 20; // iwl-fh.h:404
pub const RFH_RXF_DMA_RBDCB_SIZE_512: u32 = 0x9 << RFH_RXF_DMA_RBDCB_SIZE_POS; // iwl-fh.h:411
pub const RFH_RXF_DMA_MIN_RB_SIZE_POS: u32 = 24; // iwl-fh.h:415
pub const RFH_RXF_DMA_MIN_RB_4_8: u32 = 3 << RFH_RXF_DMA_MIN_RB_SIZE_POS; // iwl-fh.h:416
pub const RFH_RXF_DMA_DROP_TOO_LARGE_MASK: u32 = 0x0400_0000; // iwl-fh.h:417
pub const RFH_DMA_EN_ENABLE_VAL: u32 = 1 << 31; // iwl-fh.h:420
pub const RFH_GEN_CFG_SERVICE_DMA_SNOOP: u32 = 1 << 0; // iwl-fh.h:425
pub const RFH_GEN_CFG_RFH_DMA_SNOOP: u32 = 1 << 1; // iwl-fh.h:426
/// `RFH_GEN_CFG_RB_CHUNK_SIZE` is the single bit 4; `RFH_GEN_CFG_VAL(RB_CHUNK_SIZE, x)` is
/// `FIELD_PREP(BIT(4), x)`. // iwl-fh.h:427
pub const RFH_GEN_CFG_RB_CHUNK_SIZE: u32 = 1 << 4; // iwl-fh.h:427
pub const RFH_GEN_CFG_RB_CHUNK_SIZE_128: u32 = 1; // iwl-fh.h:428
pub const RFH_GEN_CFG_RB_CHUNK_SIZE_64: u32 = 0; // iwl-fh.h:429
pub const RFH_GEN_CFG_DEFAULT_RXQ_NUM: u32 = 0xF00; // iwl-fh.h:431

/// `RFH_Q_FRBDCB_BA_LSB(q)` and the other per-queue register strides. // iwl-fh.h:332-352
pub const fn rfh_q_frbdcb_ba_lsb(q: u32) -> u32 {
    RFH_Q0_FRBDCB_BA_LSB + q * 8
}
pub const fn rfh_q_frbdcb_widx(q: u32) -> u32 {
    RFH_Q0_FRBDCB_WIDX + q * 4
}
pub const fn rfh_q_frbdcb_widx_trg(q: u32) -> u32 {
    RFH_Q0_FRBDCB_WIDX_TRG + q * 4
}
pub const fn rfh_q_frbdcb_ridx(q: u32) -> u32 {
    RFH_Q0_FRBDCB_RIDX + q * 4
}
pub const fn rfh_q_urbdcb_ba_lsb(q: u32) -> u32 {
    RFH_Q0_URBDCB_BA_LSB + q * 8
}
pub const fn rfh_q_urbdcb_widx(q: u32) -> u32 {
    RFH_Q0_URBDCB_WIDX + q * 4
}
pub const fn rfh_q_urbd_stts_wptr_lsb(q: u32) -> u32 {
    RFH_Q0_URBD_STTS_WPTR_LSB + q * 8
}

/// `FH_RSCSR_FRAME_ALIGN`: each packet in a receive buffer starts on this boundary. // iwl-trans.h:65
pub const FH_RSCSR_FRAME_ALIGN: usize = 0x40; // iwl-trans.h:65
/// A buffer the firmware has not written starts with this word. // iwl-trans.h:64
pub const FH_RSCSR_FRAME_INVALID: u32 = 0x5555_0000; // iwl-trans.h:64
pub const FH_RSCSR_FRAME_SIZE_MSK: u32 = 0x0000_3FFF; // iwl-trans.h:63
pub const FH_RSCSR_RXQ_POS: u32 = 16; // iwl-trans.h:68
pub const FH_RSCSR_RXQ_MASK: u32 = 0x003F_0000; // iwl-trans.h:69

/// `IWL_NUM_RBDS_NON_HE`: the JF radio's RBD count, hence the RX queue size. // iwl-config.h:381
pub const IWL_NUM_RBDS_NON_HE: usize = 64 * 8; // iwl-config.h:381
/// sizeof(struct iwl_rb_status): four le16 and a le32, packed. // iwl-fh.h:581-587
pub const RB_STATUS_LEN: usize = 12;
/// Free RBD: one le64 (`page_dma | vid`). Used RBD: one le32 (`vid`). // rx.c:664-678
pub const FREE_BD_LEN: usize = 8;
pub const USED_BD_LEN: usize = 4;
/// The virtual RB id is 12 bits. // rx.c:1473
pub const RX_VID_MASK: u32 = 0x0FFF;
/// The device's write index advances in multiples of 8. // rx.c:193
pub const RX_WRITE_PTR_ALIGN: u16 = 8;
/// `trans->conf.rx_buf_size` = 4K (`IWL_AMSDU_4K`): the RB is one page. // mvm/ops.c:1397
pub const RB_BYTES: usize = 4096;
