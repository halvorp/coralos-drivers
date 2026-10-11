// SPDX-License-Identifier: GPL-2.0-only
//! Every register constant by value against its Linux literal, and the register-address functions
//! driven across their whole range.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/iwl-csr.h`, `iwl-fh.h`, `iwl-prph.h`,
//! `iwl-trans.h` and `cfg/9000.c`. Copyright (C) 2005-2014, 2018-2025 Intel Corporation, and the
//! Linux iwlwifi authors.

use iwlwifi_pcie_trans_core::regs::*;

#[test]
fn csr_registers_and_bits_are_iwl_csr_h() {
    assert_eq!(CSR_HW_IF_CONFIG_REG, 0x000); // iwl-csr.h:29 — zero-valued, also pinned by the sim
    assert_eq!(CSR_INT, 0x008); // iwl-csr.h:31
    assert_eq!(CSR_INT_MASK, 0x00C); // iwl-csr.h:32
    assert_eq!(CSR_FH_INT_STATUS, 0x010); // iwl-csr.h:33
    assert_eq!(CSR_RESET, 0x020); // iwl-csr.h:35
    assert_eq!(CSR_GP_CNTRL, 0x024); // iwl-csr.h:36
    assert_eq!(CSR_GIO_REG, 0x03C); // iwl-csr.h:73
    assert_eq!(CSR_MBOX_SET_REG, 0x088); // iwl-csr.h:86
    assert_eq!(CSR_GIO_CHICKEN_BITS, 0x100); // iwl-csr.h:108
    assert_eq!(CSR_DBG_HPET_MEM_REG, 0x240); // iwl-csr.h:151
    assert_eq!(CSR_DBG_LINK_PWR_MGMT_REG, 0x250); // iwl-csr.h:152
    assert_eq!(CSR_HW_IF_CONFIG_REG_HAP_WAKE, 0x00080000); // iwl-csr.h:179
    assert_eq!(CSR_HW_IF_CONFIG_REG_PCI_OWN_SET, 0x00400000); // iwl-csr.h:182
    assert_eq!(CSR_HW_IF_CONFIG_REG_WAKE_ME, 0x08000000); // iwl-csr.h:185
    assert_eq!(CSR_MBOX_SET_REG_OS_ALIVE, 0x20); // iwl-csr.h:189
    assert_eq!(CSR_INT_BIT_RF_KILL, 0x80); // iwl-csr.h:202
    assert_eq!(CSR_RESET_REG_FLAG_SW_RESET, 0x00000080); // iwl-csr.h:243
    assert_eq!(CSR_RESET_REG_FLAG_MASTER_DISABLED, 0x00000100); // iwl-csr.h:244
    assert_eq!(CSR_RESET_REG_FLAG_STOP_MASTER, 0x00000200); // iwl-csr.h:245
    assert_eq!(CSR_RESET_LINK_PWR_MGMT_DISABLED, 0x80000000); // iwl-csr.h:246
    assert_eq!(CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY, 0x00000001); // iwl-csr.h:287
    assert_eq!(CSR_GP_CNTRL_REG_FLAG_INIT_DONE, 0x00000004); // iwl-csr.h:288
    assert_eq!(CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ, 0x00000008); // iwl-csr.h:289
    assert_eq!(CSR_GP_CNTRL_REG_FLAG_RFKILL_WAKE_L1A_EN, 0x04000000); // iwl-csr.h:296
    assert_eq!(CSR_GP_CNTRL_REG_FLAG_HW_RF_KILL_SW, 0x08000000); // iwl-csr.h:297
    assert_eq!(CSR_GIO_REG_VAL_L0S_DISABLED, 0x00000002); // iwl-csr.h:402
    assert_eq!(CSR_GIO_CHICKEN_BITS_REG_BIT_L1A_NO_L0S_RX, 0x00800000); // iwl-csr.h:449
    assert_eq!(CSR_DBG_HPET_MEM_REG_VAL, 0xFFFF0000); // iwl-csr.h:461
}

#[test]
fn fh_rfh_and_prph_registers_are_their_headers() {
    assert_eq!(FH_MEM_LOWER_BOUND, 0x1000); // iwl-fh.h:22
    assert_eq!(FH_KW_MEM_ADDR_REG, 0x1000 + 0x97C); // iwl-fh.h:44
    assert_eq!(FH_MEM_CBBC_0_15_LOWER_BOUND, 0x1000 + 0x9D0); // iwl-fh.h:61
    assert_eq!(FH_MEM_CBBC_16_19_LOWER_BOUND, 0x1000 + 0xBF0); // iwl-fh.h:63
    assert_eq!(FH_MEM_CBBC_20_31_LOWER_BOUND, 0x1000 + 0xB20); // iwl-fh.h:65
    assert_eq!(FH_TCSR_LOWER_BOUND, 0x1000 + 0xD00); // iwl-fh.h:465
    assert_eq!(FH_TCSR_CHNL_NUM, 8); // iwl-fh.h:469
    assert_eq!(FH_TSSR_LOWER_BOUND, 0x1000 + 0xEA0); // iwl-fh.h:516
    assert_eq!(FH_TSSR_TX_STATUS_REG, 0x1000 + 0xEA0 + 0x010); // iwl-fh.h:519
    assert_eq!(IWL_KW_SIZE, 0x1000); // iwl-fh.h:714
    assert_eq!(RFH_GEN_STATUS, 0xA09808); // iwl-fh.h:380
    assert_eq!(RXF_DMA_IDLE, 0x8000_0000); // iwl-fh.h:384
    assert_eq!(RFH_RXF_DMA_CFG, 0xA09820); // iwl-fh.h:387
    assert_eq!(PRPH_BASE, 0); // iwl-prph.h:15 — zero-valued; SCD_BASE below proves the sum
    assert_eq!(SCD_BASE, 0xa02c00); // iwl-prph.h:223
    assert_eq!(SCD_TXFACT, 0xa02c00 + 0x10); // iwl-prph.h:228
    assert_eq!(SCD_GP_CTRL, 0xa02c00 + 0x1a8); // iwl-prph.h:234
    assert_eq!(SCD_GP_CTRL_ENABLE_31_QUEUES, 1 << 0); // iwl-prph.h:199
    assert_eq!(SCD_GP_CTRL_AUTO_ACTIVE_MODE, 1 << 18); // iwl-prph.h:200
    assert_eq!(PREG_PRPH_WPROT_9000, 0xA04CE0); // iwl-prph.h:394
    assert_eq!(UREG_CHICK, 0xA05C00); // iwl-prph.h:449
    assert_eq!(UREG_CHICK_MSI_ENABLE, 1 << 24); // iwl-prph.h:450
    assert_eq!(UREG_CHICK_MSIX_ENABLE, 1 << 25); // iwl-prph.h:451
    assert_eq!(HPM_DEBUG, 0xA03440); // iwl-prph.h:467
    assert_eq!(PERSISTENCE_BIT, 1 << 12); // iwl-prph.h:468
    assert_eq!(PREG_WFPM_ACCESS, 1 << 12); // iwl-prph.h:469
    assert_eq!(IWL9000_NUM_OF_QUEUES, 31); // cfg/9000.c:31
}

#[test]
fn every_tx_channel_config_and_idle_bit() {
    // iwl-fh.h:472-473 and :541 for all FH_TCSR_CHNL_NUM channels, written out.
    let cfg = [0x1D00, 0x1D20, 0x1D40, 0x1D60, 0x1D80, 0x1DA0, 0x1DC0, 0x1DE0];
    let idle = [0x1_0000, 0x2_0000, 0x4_0000, 0x8_0000, 0x10_0000, 0x20_0000, 0x40_0000, 0x80_0000];
    for ch in 0..8u32 {
        assert_eq!(fh_tcsr_chnl_tx_config_reg(ch), cfg[ch as usize]);
        assert_eq!(fh_tssr_tx_status_reg_msk_chnl_idle(ch), idle[ch as usize]);
    }
}

#[test]
fn every_gen1_cbbc_queue_register_and_the_refusal_past_31() {
    // iwl-fh.h:71-84 for all 32 queues, written out from the three ranges.
    let mut want = vec![];
    for q in 0..16 {
        want.push(0x19D0 + 4 * q);
    }
    for q in 0..4 {
        want.push(0x1BF0 + 4 * q);
    }
    for q in 0..12 {
        want.push(0x1B20 + 4 * q);
    }
    for q in 0..32u32 {
        assert_eq!(fh_mem_cbbc_queue(q), Some(want[q as usize]), "queue {q}");
    }
    assert_eq!(fh_mem_cbbc_queue(32), None);
}

#[test]
fn hw_error_values_are_the_two_bus_patterns_with_any_low_nibble() {
    // iwl-trans.h:1169-1172
    for v in [0xa5a5a5a0, 0xa5a5a5a2, 0xa5a5a5af, 0x5a5a5a50, 0x5a5a5a5f] {
        assert!(is_hw_error_value(v), "{v:#x}");
    }
    for v in [0, 0xa5a5a5b0, 0xa5a5a590, 0x5a5a5a60, 0xffffffff, 0x1000] {
        assert!(!is_hw_error_value(v), "{v:#x}");
    }
}
