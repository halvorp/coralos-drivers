// SPDX-License-Identifier: GPL-2.0-only
//! Every register constant pinned by VALUE against the Linux literal (PORT_CONVENTIONS.md 3a.4/3a.6: the expected side is
//! the C literal, never the constant). `iwl-csr.h`, `iwl-fh.h`, `iwl-prph.h`, `fw/file.h`, `trans.c` at Linux v7.0-rc6.
use iwlwifi_pcie_nic_core::bus::{csr_needs_grab, GRAB_SETTLE_US, GRAB_TIMEOUT_US, READ_FAILED, RFH_WIDX_TRG_HI, RFH_WIDX_TRG_LO};
use iwlwifi_pcie_nic_core::load::*;
use iwlwifi_pcie_nic_core::nicinit::*;
use iwlwifi_pcie_nic_core::regs::*;

#[test]
fn csr_registers_and_bits() {
    assert_eq!(CSR_INT_COALESCING, 0x004); // iwl-csr.h:30
    assert_eq!(CSR_INT_PERIODIC_REG, 0x005); // iwl-csr.h:40
    assert_eq!(CSR_HW_REV, 0x028); // iwl-csr.h:50
    assert_eq!(CSR_UCODE_DRV_GP1, 0x054); // iwl-csr.h:81
    assert_eq!(CSR_UCODE_DRV_GP1_CLR, 0x05c); // iwl-csr.h:83
    assert_eq!(CSR_HW_RF_ID, 0x09c); // iwl-csr.h:61
    assert_eq!(CSR_MAC_SHADOW_REG_CTRL, 0x0a8); // iwl-csr.h:90
    assert_eq!((CSR_INT_PERIODIC_DIS, CSR_INT_PERIODIC_ENA), (0x00, 0xFF)); // iwl-csr.h:191-192
    assert_eq!(IWL_HOST_INT_TIMEOUT_DEF, 0x40); // iwl-csr.h:570
    assert_eq!(CSR_RESET_REG_FLAG_FORCE_NMI, 0x2); // iwl-csr.h:242
    assert_eq!(CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY, 0x1); // iwl-csr.h:287
    assert_eq!(CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ, 0x8); // iwl-csr.h:289
    assert_eq!(CSR_GP_CNTRL_REG_FLAG_GOING_TO_SLEEP, 0x10); // iwl-csr.h:290
    assert_eq!(CSR_GP_CNTRL_REG_VAL_MAC_ACCESS_EN, 0x1); // iwl-csr.h:293
    assert_eq!(CSR_UCODE_SW_BIT_RFKILL, 0x2); // iwl-csr.h:433
    assert_eq!(CSR_UCODE_DRV_GP1_BIT_CMD_BLOCKED, 0x4); // iwl-csr.h:434
}

#[test]
fn interrupt_causes_and_masks() {
    assert_eq!(CSR_INT_BIT_FH_RX, 0x8000_0000); // iwl-csr.h:196
    assert_eq!(CSR_INT_BIT_HW_ERR, 0x2000_0000); // iwl-csr.h:197
    assert_eq!(CSR_INT_BIT_RX_PERIODIC, 0x1000_0000); // iwl-csr.h:198
    assert_eq!(CSR_INT_BIT_FH_TX, 0x0800_0000); // iwl-csr.h:199
    assert_eq!(CSR_INT_BIT_SCD, 0x0400_0000); // iwl-csr.h:200
    assert_eq!(CSR_INT_BIT_SW_ERR, 0x0200_0000); // iwl-csr.h:201
    assert_eq!(CSR_INT_BIT_RF_KILL, 0x80); // iwl-csr.h:202
    assert_eq!(CSR_INT_BIT_CT_KILL, 0x40); // iwl-csr.h:203
    assert_eq!(CSR_INT_BIT_SW_RX, 0x8); // iwl-csr.h:204
    assert_eq!(CSR_INT_BIT_RESET_DONE, 0x4); // iwl-csr.h:205
    assert_eq!(CSR_INT_BIT_WAKEUP, 0x2); // iwl-csr.h:206
    assert_eq!(CSR_INT_BIT_ALIVE, 0x1); // iwl-csr.h:207
    // CSR_INI_SET_MASK (iwl-csr.h:209-218) = FH_RX|HW_ERR|FH_TX|SW_ERR|RF_KILL|SW_RX|WAKEUP|RESET_DONE|ALIVE|RX_PERIODIC, summed by hand:
    assert_eq!(CSR_INI_SET_MASK, 0xBA00_008F);
    assert_eq!(CSR_INI_SET_MASK & CSR_INT_BIT_SCD, 0, "SCD and CT_KILL are not unmasked");
    assert_eq!(CSR_INI_SET_MASK & CSR_INT_BIT_CT_KILL, 0);
    assert_eq!((CSR_FH_INT_BIT_HI_PRIOR, CSR_FH_INT_BIT_RX_CHNL1, CSR_FH_INT_BIT_RX_CHNL0), (1 << 30, 1 << 17, 1 << 16)); // iwl-csr.h:222-224
    assert_eq!((CSR_FH_INT_BIT_TX_CHNL1, CSR_FH_INT_BIT_TX_CHNL0), (2, 1)); // iwl-csr.h:225-226
    assert_eq!(CSR_FH_INT_RX_MASK, 0x4003_0000); // iwl-csr.h:228-230
    assert_eq!(CSR_FH_INT_TX_MASK, 0x3); // iwl-csr.h:232-233
}

#[test]
fn hw_if_config_fields() {
    assert_eq!(CSR_HW_IF_CONFIG_REG_MSK_PHY_TYPE, 0x0C00); // iwl-csr.h:168
    assert_eq!(CSR_HW_IF_CONFIG_REG_MSK_PHY_DASH, 0x3000); // iwl-csr.h:169
    assert_eq!(CSR_HW_IF_CONFIG_REG_MSK_PHY_STEP, 0xC000); // iwl-csr.h:170
    assert_eq!(CSR_HW_IF_CONFIG_REG_MSK_MAC_STEP_DASH, 0xF); // iwl-csr.h:162
    assert_eq!((CSR_HW_IF_CONFIG_REG_POS_PHY_TYPE, CSR_HW_IF_CONFIG_REG_POS_PHY_DASH, CSR_HW_IF_CONFIG_REG_POS_PHY_STEP), (10, 12, 14)); // iwl-csr.h:175-177
    // each field's mask is its width at its position: the positions and masks agree
    assert_eq!(CSR_HW_IF_CONFIG_REG_MSK_PHY_TYPE, 0x3 << 10);
    assert_eq!(CSR_HW_IF_CONFIG_REG_MSK_PHY_DASH, 0x3 << 12);
    assert_eq!(CSR_HW_IF_CONFIG_REG_MSK_PHY_STEP, 0x3 << 14);
}

#[test]
fn hbus_indirection_block() {
    assert_eq!(HBUS_TARG_MEM_RADDR, 0x40c); // iwl-csr.h:522
    assert_eq!(HBUS_TARG_MEM_WADDR, 0x410); // iwl-csr.h:523
    assert_eq!(HBUS_TARG_MEM_WDAT, 0x418); // iwl-csr.h:524
    assert_eq!(HBUS_TARG_MEM_RDAT, 0x41c); // iwl-csr.h:525
    assert_eq!(HBUS_TARG_PRPH_WADDR, 0x444); // iwl-csr.h:539
    assert_eq!(HBUS_TARG_PRPH_RADDR, 0x448); // iwl-csr.h:540
    assert_eq!(HBUS_TARG_PRPH_WDAT, 0x44c); // iwl-csr.h:541
    assert_eq!(HBUS_TARG_PRPH_RDAT, 0x450); // iwl-csr.h:542
    assert_eq!(HBUS_TARG_WRPTR, 0x460); // iwl-csr.h:554
    assert_eq!(PRPH_MASK_9000, 0xFFFFF); // trans.c:1886
    assert_eq!(PRPH_DWORD_ACCESS, 0x0300_0000); // trans.c:1894 (3 << 24)
}

#[test]
fn flow_handler_service_channel() {
    assert_eq!(FH_MEM_LOWER_BOUND, 0x1000); // iwl-fh.h:22
    assert_eq!(FH_SRVC_CHNL, 9); // iwl-fh.h:544
    assert_eq!(FH_MEM_TB_MAX_LENGTH, 0x20000); // iwl-fh.h:327
    assert_eq!(FH_MEM_TFDIB_REG1_ADDR_BITSHIFT, 28); // iwl-fh.h:326
    assert_eq!(FH_MEM_TFDIB_DRAM_ADDR_LSB_MSK, 0xFFFF_FFFF); // iwl-fh.h:437
    assert_eq!(FH_TCSR_CHNL_NUM, 8); // iwl-fh.h:469
    // FH_TFDIB_CTRL0_REG(9) = 0x1000 + 0x900 + 0x8 * 9 (iwl-fh.h:438-441)
    assert_eq!(fh_tfdib_ctrl0_reg(9), 0x1948);
    assert_eq!(fh_tfdib_ctrl1_reg(9), 0x194c);
    // FH_TCSR_CHNL_TX_CONFIG_REG(c) = 0x1000 + 0xD00 + 0x20 * c; BUF_STS = + 0x8 (iwl-fh.h:465-477)
    assert_eq!((fh_tcsr_chnl_tx_config_reg(0), fh_tcsr_chnl_tx_config_reg(7), fh_tcsr_chnl_tx_config_reg(9)), (0x1d00, 0x1de0, 0x1e20));
    assert_eq!(fh_tcsr_chnl_tx_buf_sts_reg(9), 0x1e28);
    // FH_SRVC_CHNL_SRAM_ADDR_REG(9) = FH_SRVC_LOWER_BOUND (0x1000 + 0x9C8) + 0 (iwl-fh.h:545-548)
    assert_eq!(fh_srvc_chnl_sram_addr_reg(9), 0x19c8);
    assert_eq!(FH_TX_CHICKEN_BITS_REG, 0x1e98); // iwl-fh.h:550
    assert_eq!(FH_TX_CHICKEN_BITS_SCD_AUTO_RETRY_EN, 0x2); // iwl-fh.h:556
    assert_eq!(FH_TCSR_TX_CONFIG_REG_VAL_DMA_CREDIT_DISABLE, 0x0); // iwl-fh.h:482
    assert_eq!(FH_TCSR_TX_CONFIG_REG_VAL_DMA_CREDIT_ENABLE, 0x8); // iwl-fh.h:483
    assert_eq!(FH_TCSR_TX_CONFIG_REG_VAL_CIRQ_HOST_ENDTFD, 0x0010_0000); // iwl-fh.h:486
    assert_eq!(FH_TCSR_TX_CONFIG_REG_VAL_DMA_CHNL_PAUSE, 0x0); // iwl-fh.h:493
    assert_eq!(FH_TCSR_TX_CONFIG_REG_VAL_DMA_CHNL_ENABLE, 0x8000_0000); // iwl-fh.h:495
    assert_eq!(FH_TCSR_CHNL_TX_BUF_STS_REG_VAL_TFDB_VALID, 0x3); // iwl-fh.h:499
    assert_eq!((FH_TCSR_CHNL_TX_BUF_STS_REG_POS_TB_NUM, FH_TCSR_CHNL_TX_BUF_STS_REG_POS_TB_IDX), (20, 12)); // iwl-fh.h:501-502
}

#[test]
fn periphery_and_upload_registers() {
    assert_eq!(RELEASE_CPU_RESET, 0x300c); // iwl-prph.h:84
    assert_eq!(RELEASE_CPU_RESET_BIT, 1 << 24); // iwl-prph.h:85
    assert_eq!(WFPM_GP2, 0xA0_30B4); // iwl-prph.h:359
    assert_eq!(LMPM_CHICK, 0xA0_1FF8); // iwl-prph.h:422
    assert_eq!(LMPM_CHICK_EXTENDED_ADDR_SPACE, 1); // iwl-prph.h:424
    assert_eq!(FH_UCODE_LOAD_STATUS, 0x1af0); // iwl-prph.h:243
    assert_eq!((IWL_FW_MEM_EXTENDED_START, IWL_FW_MEM_EXTENDED_END), (0x40000, 0x57FFF)); // trans.c:75-76
}

#[test]
fn the_upload_constants() {
    assert_eq!(CHUNK_TIMEOUT_MS, 5000); // trans.c:648 (5 * HZ)
    assert_eq!(FIRST_SEC_NUM, 1); // trans.c:721
    assert_eq!((LOAD_STATUS_CPU1_DONE, LOAD_STATUS_CPU2_DONE), (0xFFFF, 0xFFFF_FFFF)); // trans.c:775-779
    assert_eq!(WFPM_GP2_PRELOAD, 0x0101_0101); // trans.c:993
    assert_eq!(FH_BOUNCE_BYTES, 0x20000); // trans.c:663
    assert_eq!((GRAB_TIMEOUT_US, GRAB_SETTLE_US), (15_000, 2)); // trans.c:2378, :2355-2356
    assert_eq!(READ_FAILED, 0x5a5a_5a5a); // iwl-io.c:142
    assert_eq!(SHADOW_REG_ENABLE_ALL, 0x800F_FFFF); // trans.c:515
    assert_eq!(CSR_HW_IF_CONFIG_REG_BIT_MAC_SI, 0x100); // iwl-csr.h:165
    assert_eq!(CSR_HW_IF_CONFIG_REG_BIT_RADIO_SI, 0x200); // iwl-csr.h:166
    assert_eq!(CSR_HW_IF_CONFIG_REG_D3_DEBUG, 0x200); // iwl-csr.h:167
    assert_eq!((FW_PHY_CFG_RADIO_TYPE, FW_PHY_CFG_RADIO_STEP, FW_PHY_CFG_RADIO_DASH), (0x3, 0xC, 0x30)); // fw/file.h:604-608
    assert_eq!((FW_PHY_CFG_RADIO_TYPE_POS, FW_PHY_CFG_RADIO_STEP_POS, FW_PHY_CFG_RADIO_DASH_POS), (0, 2, 4));
    // the RFH shadow write-index trigger window starts at RFH_Q0_FRBDCB_WIDX_TRG (iwl-fh.h:337)
    assert_eq!(RFH_WIDX_TRG_LO, 0x1c80);
}

#[test]
fn which_flow_handler_registers_need_the_mac_awake() {
    // iwl_write_direct32 (a grab) for the flow handler; the RFH shadow trigger is a plain iwl_write32 (rx.c:199-201).
    for r in [0x1000, 0x197c, 0x19d0, 0x1af0, 0x1e20, 0x1e98, 0x1eb0, 0x1fff, 0x1c7c, 0x1d00 + 0x20 * 31] {
        assert!(csr_needs_grab(r), "{r:#x}");
    }
    for r in [0x1c80, 0x1c84, 0x1cbc] {
        assert!(!csr_needs_grab(r), "{r:#x}: the shadow window");
    }
    for r in [0x000, 0x008, 0x024, 0x3fc, 0x460, 0xfff] {
        assert!(!csr_needs_grab(r), "{r:#x}: CSR/HBUS are plain iwl_write32");
    }
    assert!(RFH_WIDX_TRG_HI > 0x1c80 + 4 * 31, "all 31 queue triggers are inside the window");
}
