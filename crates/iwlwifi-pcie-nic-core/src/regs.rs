// SPDX-License-Identifier: GPL-2.0-only
//! Registers the nic-core touches beyond `iwlwifi-pcie-trans-core::regs`, ported from Linux
//! `iwl-csr.h`, `iwl-fh.h`, `iwl-prph.h`, `fw/file.h`. Every citation names the file and line of the
//! `#define`; the values are the C literals evaluated.
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016 Intel Deutschland GmbH.

// ── CSR (BAR0) ── iwl-csr.h
pub const CSR_INT_COALESCING: u32 = 0x004; // iwl-csr.h:30
pub const CSR_INT_PERIODIC_REG: u32 = 0x005; // iwl-csr.h:40
pub const CSR_HW_REV: u32 = 0x028; // iwl-csr.h:50
pub const CSR_UCODE_DRV_GP1: u32 = 0x054; // iwl-csr.h:81
pub const CSR_UCODE_DRV_GP1_CLR: u32 = 0x05C; // iwl-csr.h:83
pub const CSR_HW_RF_ID: u32 = 0x09C; // iwl-csr.h:61
pub const CSR_MAC_SHADOW_REG_CTRL: u32 = 0x0A8; // iwl-csr.h:90
pub const CSR_INT_PERIODIC_DIS: u8 = 0x00; // iwl-csr.h:191
pub const CSR_INT_PERIODIC_ENA: u8 = 0xFF; // iwl-csr.h:192
/// iwl_enable_fw_load_int-era default for CSR_INT_COALESCING. // iwl-csr.h:570
pub const IWL_HOST_INT_TIMEOUT_DEF: u8 = 0x40; // iwl-csr.h:570

pub const CSR_INT_BIT_FH_RX: u32 = 1 << 31; // iwl-csr.h:196
pub const CSR_INT_BIT_HW_ERR: u32 = 1 << 29; // iwl-csr.h:197
pub const CSR_INT_BIT_RX_PERIODIC: u32 = 1 << 28; // iwl-csr.h:198
pub const CSR_INT_BIT_FH_TX: u32 = 1 << 27; // iwl-csr.h:199
pub const CSR_INT_BIT_SCD: u32 = 1 << 26; // iwl-csr.h:200
pub const CSR_INT_BIT_SW_ERR: u32 = 1 << 25; // iwl-csr.h:201
pub const CSR_INT_BIT_RF_KILL: u32 = 1 << 7; // iwl-csr.h:202
pub const CSR_INT_BIT_CT_KILL: u32 = 1 << 6; // iwl-csr.h:203
pub const CSR_INT_BIT_SW_RX: u32 = 1 << 3; // iwl-csr.h:204
pub const CSR_INT_BIT_RESET_DONE: u32 = 1 << 2; // iwl-csr.h:205
pub const CSR_INT_BIT_WAKEUP: u32 = 1 << 1; // iwl-csr.h:206
pub const CSR_INT_BIT_ALIVE: u32 = 1 << 0; // iwl-csr.h:207
/// `CSR_INI_SET_MASK`, the mask `_iwl_enable_interrupts` writes in MSI mode. The composite is
/// asserted below as the OR of the named bits, and each bit is pinned by value above.
pub const CSR_INI_SET_MASK: u32 = CSR_INT_BIT_FH_RX // iwl-csr.h:209
    | CSR_INT_BIT_HW_ERR
    | CSR_INT_BIT_FH_TX
    | CSR_INT_BIT_SW_ERR
    | CSR_INT_BIT_RF_KILL
    | CSR_INT_BIT_SW_RX
    | CSR_INT_BIT_WAKEUP
    | CSR_INT_BIT_RESET_DONE
    | CSR_INT_BIT_ALIVE
    | CSR_INT_BIT_RX_PERIODIC;

pub const CSR_FH_INT_BIT_HI_PRIOR: u32 = 1 << 30; // iwl-csr.h:222
pub const CSR_FH_INT_BIT_RX_CHNL1: u32 = 1 << 17; // iwl-csr.h:223
pub const CSR_FH_INT_BIT_RX_CHNL0: u32 = 1 << 16; // iwl-csr.h:224
pub const CSR_FH_INT_BIT_TX_CHNL1: u32 = 1 << 1; // iwl-csr.h:225
pub const CSR_FH_INT_BIT_TX_CHNL0: u32 = 1 << 0; // iwl-csr.h:226
pub const CSR_FH_INT_RX_MASK: u32 = CSR_FH_INT_BIT_HI_PRIOR | CSR_FH_INT_BIT_RX_CHNL1 | CSR_FH_INT_BIT_RX_CHNL0; // iwl-csr.h:228
pub const CSR_FH_INT_TX_MASK: u32 = CSR_FH_INT_BIT_TX_CHNL1 | CSR_FH_INT_BIT_TX_CHNL0; // iwl-csr.h:232

pub const CSR_RESET_REG_FLAG_FORCE_NMI: u32 = 0x0000_0002; // iwl-csr.h:242
pub const CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY: u32 = 0x0000_0001; // iwl-csr.h:287
pub const CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ: u32 = 0x0000_0008; // iwl-csr.h:289
pub const CSR_GP_CNTRL_REG_FLAG_GOING_TO_SLEEP: u32 = 0x0000_0010; // iwl-csr.h:290
pub const CSR_GP_CNTRL_REG_VAL_MAC_ACCESS_EN: u32 = 0x0000_0001; // iwl-csr.h:293
pub const CSR_UCODE_SW_BIT_RFKILL: u32 = 0x0000_0002; // iwl-csr.h:433
pub const CSR_UCODE_DRV_GP1_BIT_CMD_BLOCKED: u32 = 0x0000_0004; // iwl-csr.h:434
pub const CSR_HW_IF_CONFIG_REG_MSK_PHY_TYPE: u32 = 0x0000_0C00; // iwl-csr.h:168
pub const CSR_HW_IF_CONFIG_REG_MSK_PHY_DASH: u32 = 0x0000_3000; // iwl-csr.h:169
pub const CSR_HW_IF_CONFIG_REG_MSK_PHY_STEP: u32 = 0x0000_C000; // iwl-csr.h:170
pub const CSR_HW_IF_CONFIG_REG_POS_PHY_TYPE: u32 = 10; // iwl-csr.h:175
pub const CSR_HW_IF_CONFIG_REG_POS_PHY_DASH: u32 = 12; // iwl-csr.h:176
pub const CSR_HW_IF_CONFIG_REG_POS_PHY_STEP: u32 = 14; // iwl-csr.h:177
pub const CSR_HW_IF_CONFIG_REG_MSK_MAC_STEP_DASH: u32 = 0x0000_000F; // iwl-csr.h:162

/// The 8-bit-wide hardware revision STEP_DASH nibble `CSR_HW_REV_STEP_DASH` extracts. // iwl-csr.h:309
pub const CSR_HW_REV_STEP_DASH_MASK: u32 = 0x0000_000F; // iwl-csr.h:309
pub const CSR_HW_REV_STEP_DASH_SHIFT: u32 = 0;

// ── HBUS (BAR0 + 0x400): the indirect accessors ── iwl-csr.h
pub const HBUS_TARG_MEM_RADDR: u32 = 0x40C; // iwl-csr.h:522
pub const HBUS_TARG_MEM_WADDR: u32 = 0x410; // iwl-csr.h:523
pub const HBUS_TARG_MEM_WDAT: u32 = 0x418; // iwl-csr.h:524
pub const HBUS_TARG_MEM_RDAT: u32 = 0x41C; // iwl-csr.h:525
pub const HBUS_TARG_PRPH_WADDR: u32 = 0x444; // iwl-csr.h:539
pub const HBUS_TARG_PRPH_RADDR: u32 = 0x448; // iwl-csr.h:540
pub const HBUS_TARG_PRPH_WDAT: u32 = 0x44C; // iwl-csr.h:541
pub const HBUS_TARG_PRPH_RDAT: u32 = 0x450; // iwl-csr.h:542
pub const HBUS_TARG_WRPTR: u32 = 0x460; // iwl-csr.h:554

// ── FH (flow handler) ── iwl-fh.h
pub const FH_MEM_LOWER_BOUND: u32 = 0x1000; // iwl-fh.h:22
pub const FH_TFDIB_LOWER_BOUND: u32 = FH_MEM_LOWER_BOUND + 0x900; // iwl-fh.h:438
pub const FH_SRVC_LOWER_BOUND: u32 = FH_MEM_LOWER_BOUND + 0x9C8; // iwl-fh.h:545
pub const FH_TCSR_LOWER_BOUND: u32 = FH_MEM_LOWER_BOUND + 0xD00; // iwl-fh.h:465
pub const FH_SRVC_CHNL: u32 = 9; // iwl-fh.h:544
pub const FH_MEM_TFDIB_DRAM_ADDR_LSB_MSK: u32 = 0xFFFF_FFFF; // iwl-fh.h:437
pub const FH_MEM_TFDIB_REG1_ADDR_BITSHIFT: u32 = 28; // iwl-fh.h:326
pub const FH_MEM_TB_MAX_LENGTH: u32 = 0x0002_0000; // iwl-fh.h:327
pub const FH_TCSR_CHNL_NUM: u32 = 8; // iwl-fh.h:469
pub const FH_TCSR_TX_CONFIG_REG_VAL_DMA_CREDIT_DISABLE: u32 = 0x0000_0000; // iwl-fh.h:482
pub const FH_TCSR_TX_CONFIG_REG_VAL_DMA_CREDIT_ENABLE: u32 = 0x0000_0008; // iwl-fh.h:483
pub const FH_TCSR_TX_CONFIG_REG_VAL_CIRQ_HOST_ENDTFD: u32 = 0x0010_0000; // iwl-fh.h:486
pub const FH_TCSR_TX_CONFIG_REG_VAL_DMA_CHNL_PAUSE: u32 = 0x0000_0000; // iwl-fh.h:493
pub const FH_TCSR_TX_CONFIG_REG_VAL_DMA_CHNL_ENABLE: u32 = 0x8000_0000; // iwl-fh.h:495
pub const FH_TCSR_CHNL_TX_BUF_STS_REG_VAL_TFDB_VALID: u32 = 0x0000_0003; // iwl-fh.h:499
pub const FH_TCSR_CHNL_TX_BUF_STS_REG_POS_TB_NUM: u32 = 20; // iwl-fh.h:501
pub const FH_TCSR_CHNL_TX_BUF_STS_REG_POS_TB_IDX: u32 = 12; // iwl-fh.h:502
pub const FH_TX_CHICKEN_BITS_REG: u32 = FH_MEM_LOWER_BOUND + 0xE98; // iwl-fh.h:550
pub const FH_TX_CHICKEN_BITS_SCD_AUTO_RETRY_EN: u32 = 0x0000_0002; // iwl-fh.h:556

/// `FH_TFDIB_CTRL0_REG(_chnl)`. // iwl-fh.h:440
pub const fn fh_tfdib_ctrl0_reg(chnl: u32) -> u32 {
    FH_TFDIB_LOWER_BOUND + 0x8 * chnl
}
/// `FH_TFDIB_CTRL1_REG(_chnl)`. // iwl-fh.h:441
pub const fn fh_tfdib_ctrl1_reg(chnl: u32) -> u32 {
    FH_TFDIB_LOWER_BOUND + 0x8 * chnl + 0x4
}
/// `FH_TCSR_CHNL_TX_CONFIG_REG(_chnl)`. // iwl-fh.h:472
pub const fn fh_tcsr_chnl_tx_config_reg(chnl: u32) -> u32 {
    FH_TCSR_LOWER_BOUND + 0x20 * chnl
}
/// `FH_TCSR_CHNL_TX_BUF_STS_REG(_chnl)`. // iwl-fh.h:476
pub const fn fh_tcsr_chnl_tx_buf_sts_reg(chnl: u32) -> u32 {
    FH_TCSR_LOWER_BOUND + 0x20 * chnl + 0x8
}
/// `FH_SRVC_CHNL_SRAM_ADDR_REG(_chnl)`. // iwl-fh.h:547
pub const fn fh_srvc_chnl_sram_addr_reg(chnl: u32) -> u32 {
    FH_SRVC_LOWER_BOUND + (chnl - 9) * 0x4
}

// ── PRPH (periphery, through HBUS_TARG_PRPH_*) ── iwl-prph.h, pcie/gen1_2/trans.c
pub const RELEASE_CPU_RESET: u32 = 0x300C; // iwl-prph.h:84
pub const RELEASE_CPU_RESET_BIT: u32 = 1 << 24; // iwl-prph.h:85
pub const WFPM_GP2: u32 = 0xA0_30B4; // iwl-prph.h:359
pub const LMPM_CHICK: u32 = 0xA0_1FF8; // iwl-prph.h:422
pub const LMPM_CHICK_EXTENDED_ADDR_SPACE: u32 = 1 << 0; // iwl-prph.h:424
/// The load-status mailbox the secure loader polls; a CSR-space address (read/written with
/// `iwl_read_direct32` / `iwl_write_direct32`). // iwl-prph.h:243
pub const FH_UCODE_LOAD_STATUS: u32 = 0x1AF0; // iwl-prph.h:243
/// Destinations in this window need LMPM_CHICK's extended-address bit around their chunk.
pub const IWL_FW_MEM_EXTENDED_START: u32 = 0x40000; // trans.c:75
pub const IWL_FW_MEM_EXTENDED_END: u32 = 0x57FFF; // trans.c:76

/// `iwl_trans_pcie_prph_msk`: the periphery address mask of a < AX210 part. // pcie/gen1_2/trans.c:1886
pub const PRPH_MASK_9000: u32 = 0x000F_FFFF;
/// The "dword access" field OR'd above the periphery address (`3 << 24`). // pcie/gen1_2/trans.c:1894
pub const PRPH_DWORD_ACCESS: u32 = 3 << 24;
