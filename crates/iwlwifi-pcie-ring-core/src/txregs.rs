// SPDX-License-Identifier: GPL-2.0-only
//! The TX scheduler (SCD) and queue constants, ported from Linux `iwl-prph.h`, `iwl-scd.h`,
//! `iwl-fh.h`, `iwl-trans.h`, `iwl-config.h`, `fw/api/txq.h`, `fw/api/cmdhdr.h`, `cfg/9000.c`.
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.

// ── scheduler (periphery) ── iwl-prph.h
pub const SCD_BASE: u32 = 0xA0_2C00; // iwl-prph.h:223
pub const SCD_SRAM_BASE_ADDR: u32 = SCD_BASE; // iwl-prph.h:225
pub const SCD_DRAM_BASE_ADDR: u32 = SCD_BASE + 0x8; // iwl-prph.h:226
pub const SCD_TXFACT: u32 = SCD_BASE + 0x10; // iwl-prph.h:228
pub const SCD_AGGR_SEL: u32 = SCD_BASE + 0x248; // iwl-prph.h:232
pub const SCD_EN_CTRL: u32 = SCD_BASE + 0x254; // iwl-prph.h:235
pub const SCD_MEM_LOWER_BOUND: u32 = 0x0000; // iwl-prph.h:170
pub const SCD_CONTEXT_MEM_LOWER_BOUND: u32 = SCD_MEM_LOWER_BOUND + 0x600; // iwl-prph.h:203
pub const SCD_TRANS_TBL_MEM_LOWER_BOUND: u32 = SCD_MEM_LOWER_BOUND + 0x7E0; // iwl-prph.h:211
pub const SCD_QUEUE_STTS_REG_POS_TXF: u32 = 0; // iwl-prph.h:185
pub const SCD_QUEUE_STTS_REG_POS_ACTIVE: u32 = 3; // iwl-prph.h:186
pub const SCD_QUEUE_STTS_REG_POS_WSL: u32 = 4; // iwl-prph.h:187
pub const SCD_QUEUE_STTS_REG_POS_SCD_ACT_EN: u32 = 19; // iwl-prph.h:188
pub const SCD_QUEUE_STTS_REG_MSK: u32 = 0x017F_0000; // iwl-prph.h:189
pub const SCD_QUEUE_CTX_REG2_WIN_SIZE: u32 = 0x0000_007F; // iwl-prph.h:195
pub const SCD_QUEUE_CTX_REG2_FRAME_LIMIT: u32 = 0x007F_0000; // iwl-prph.h:196

/// `SCD_CONTEXT_QUEUE_OFFSET(x)`. // iwl-prph.h:214
pub const fn scd_context_queue_offset(q: u32) -> u32 {
    SCD_CONTEXT_MEM_LOWER_BOUND + q * 8
}
/// `SCD_TRANS_TBL_OFFSET_QUEUE(x)`. // iwl-prph.h:220
pub const fn scd_trans_tbl_offset_queue(q: u32) -> u32 {
    (SCD_TRANS_TBL_MEM_LOWER_BOUND + q * 2) & 0xfffc
}
/// `SCD_QUEUE_RDPTR(chnl)`, queues below 20. // iwl-scd.h:60-66
pub const fn scd_queue_rdptr(q: u32) -> u32 {
    SCD_BASE + 0x68 + q * 4
}
/// `SCD_QUEUE_STATUS_BITS(chnl)`, queues below 20. // iwl-scd.h:68-74
pub const fn scd_queue_status_bits(q: u32) -> u32 {
    SCD_BASE + 0x10c + q * 4
}

// ── queues ── cfg/9000.c, fw/api/txq.h, iwl-trans.h, iwl-config.h, iwl-fh.h
/// `num_of_queues` of the 9000 family. // cfg/9000.c:@.num_of_queues
pub const IWL9000_NUM_OF_QUEUES: u32 = 31; // cfg/9000.c:31
/// `max_tfd_queue_size`: the TFD ring of every gen1 queue has this many entries. // cfg/9000.c:32
pub const MAX_TFD_QUEUE_SIZE: u16 = 256; // cfg/9000.c:32
/// `IWL_MVM_DQA_CMD_QUEUE` and `IWL_MVM_TX_FIFO_CMD`. // fw/api/txq.h:35
pub const IWL_MVM_DQA_CMD_QUEUE: u32 = 0; // fw/api/txq.h:35
pub const IWL_MVM_TX_FIFO_CMD: u32 = 7; // fw/api/txq.h:54
/// The command queue's window. // fw/api/txq.h:93
pub const IWL_CMD_QUEUE_SIZE: u16 = 32; // fw/api/txq.h:93
/// `IWL_FRAME_LIMIT`. // iwl-trans.h:273
pub const IWL_FRAME_LIMIT: u32 = 64; // iwl-trans.h:273
/// `IWL_DEF_WD_TIMEOUT`, ms. // iwl-config.h:86
pub const IWL_DEF_WD_TIMEOUT: u32 = 2500; // iwl-config.h:86
/// `sizeof(struct iwl_tfd)`: three reserved bytes, `num_tbs`, 20 six-byte TBs, four pad. // iwl-fh.h:692-697
pub const TFD_SIZE: usize = 128;
/// `IWL_NUM_OF_TBS`. // iwl-fh.h:598
pub const IWL_NUM_OF_TBS: usize = 20; // iwl-fh.h:598
/// `IWL_TX_DMA_MASK` is `DMA_BIT_MASK(36)`. // iwl-fh.h:597
pub const IWL_TX_DMA_BITS: u32 = 36; // iwl-fh.h:597
/// The first TB of a command is at most this long, copied to a 64-byte-aligned slot. // iwl-trans.h:712
pub const IWL_FIRST_TB_SIZE: usize = 20; // iwl-trans.h:712
pub const IWL_FIRST_TB_SIZE_ALIGN: usize = 64; // iwl-trans.h:713
/// `DEF_CMD_PAYLOAD_SIZE`. // iwl-trans.h:121
pub const DEF_CMD_PAYLOAD_SIZE: usize = 320; // iwl-trans.h:121
/// `TFD_MAX_PAYLOAD_SIZE` = sizeof(struct iwl_device_cmd): a short header and the payload. // iwl-trans.h:162
pub const TFD_MAX_PAYLOAD_SIZE: usize = 4 + DEF_CMD_PAYLOAD_SIZE;
/// `IWL_MAX_CMD_TBS_PER_TFD`. // iwl-trans.h:168
pub const IWL_MAX_CMD_TBS_PER_TFD: usize = 2; // iwl-trans.h:168
/// `IWL_ALWAYS_LONG_GROUP`. // fw/api/cmdhdr.h:57
pub const IWL_ALWAYS_LONG_GROUP: u8 = 1; // fw/api/cmdhdr.h:57
/// sizeof(struct iwl_cmd_header) and of `iwl_cmd_header_wide`. // fw/api/cmdhdr.h:65-122
pub const CMD_HDR_LEN: usize = 4;
pub const CMD_HDR_WIDE_LEN: usize = 8;
/// `HOST_COMPLETE_TIMEOUT` is `2 * HZ`. // pcie/gen1_2/tx.c:2548
pub const HOST_COMPLETE_TIMEOUT_MS: u32 = 2000; // pcie/gen1_2/tx.c:2548
