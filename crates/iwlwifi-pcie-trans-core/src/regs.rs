// SPDX-License-Identifier: GPL-2.0-only
//! The CSR, FH and periphery (PRPH) registers the 9000-family power sequences touch, ported from Linux
//! `drivers/net/wireless/intel/iwlwifi/iwl-csr.h`, `iwl-fh.h`, `iwl-prph.h`, `iwl-trans.h` and
//! `cfg/9000.c`.
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016 Intel Deutschland GmbH.

// ── CSR (BAR0) ── iwl-csr.h
pub const CSR_HW_IF_CONFIG_REG: u32 = 0x000; // iwl-csr.h:29
pub const CSR_INT: u32 = 0x008; // iwl-csr.h:31
pub const CSR_INT_MASK: u32 = 0x00C; // iwl-csr.h:32
pub const CSR_FH_INT_STATUS: u32 = 0x010; // iwl-csr.h:33
pub const CSR_RESET: u32 = 0x020; // iwl-csr.h:35
pub const CSR_GP_CNTRL: u32 = 0x024; // iwl-csr.h:36
pub const CSR_GIO_REG: u32 = 0x03C; // iwl-csr.h:73
pub const CSR_MBOX_SET_REG: u32 = 0x088; // iwl-csr.h:86
pub const CSR_GIO_CHICKEN_BITS: u32 = 0x100; // iwl-csr.h:108
pub const CSR_DBG_HPET_MEM_REG: u32 = 0x240; // iwl-csr.h:151
pub const CSR_DBG_LINK_PWR_MGMT_REG: u32 = 0x250; // iwl-csr.h:152

pub const CSR_HW_IF_CONFIG_REG_HAP_WAKE: u32 = 0x0008_0000; // iwl-csr.h:179
pub const CSR_HW_IF_CONFIG_REG_PCI_OWN_SET: u32 = 0x0040_0000; // iwl-csr.h:182
pub const CSR_HW_IF_CONFIG_REG_WAKE_ME: u32 = 0x0800_0000; // iwl-csr.h:185
pub const CSR_MBOX_SET_REG_OS_ALIVE: u32 = 1 << 5; // iwl-csr.h:189
pub const CSR_INT_BIT_RF_KILL: u32 = 1 << 7; // iwl-csr.h:202
pub const CSR_RESET_REG_FLAG_SW_RESET: u32 = 0x0000_0080; // iwl-csr.h:243
pub const CSR_RESET_REG_FLAG_MASTER_DISABLED: u32 = 0x0000_0100; // iwl-csr.h:244
pub const CSR_RESET_REG_FLAG_STOP_MASTER: u32 = 0x0000_0200; // iwl-csr.h:245
pub const CSR_RESET_LINK_PWR_MGMT_DISABLED: u32 = 0x8000_0000; // iwl-csr.h:246
pub const CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY: u32 = 0x0000_0001; // iwl-csr.h:287
pub const CSR_GP_CNTRL_REG_FLAG_INIT_DONE: u32 = 0x0000_0004; // iwl-csr.h:288
pub const CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ: u32 = 0x0000_0008; // iwl-csr.h:289
pub const CSR_GP_CNTRL_REG_FLAG_RFKILL_WAKE_L1A_EN: u32 = 0x0400_0000; // iwl-csr.h:296
/// SET = the radio is enabled (the switch is NOT in kill). // internal.h:1057-1058
pub const CSR_GP_CNTRL_REG_FLAG_HW_RF_KILL_SW: u32 = 0x0800_0000; // iwl-csr.h:297
pub const CSR_GIO_REG_VAL_L0S_DISABLED: u32 = 0x0000_0002; // iwl-csr.h:402
pub const CSR_GIO_CHICKEN_BITS_REG_BIT_L1A_NO_L0S_RX: u32 = 0x0080_0000; // iwl-csr.h:449
pub const CSR_DBG_HPET_MEM_REG_VAL: u32 = 0xFFFF_0000; // iwl-csr.h:461

// ── FH (BAR0, flow handler) ── iwl-fh.h
pub const FH_MEM_LOWER_BOUND: u32 = 0x1000; // iwl-fh.h:22
pub const FH_KW_MEM_ADDR_REG: u32 = FH_MEM_LOWER_BOUND + 0x97C; // iwl-fh.h:44
pub const FH_MEM_CBBC_0_15_LOWER_BOUND: u32 = FH_MEM_LOWER_BOUND + 0x9D0; // iwl-fh.h:61
pub const FH_MEM_CBBC_16_19_LOWER_BOUND: u32 = FH_MEM_LOWER_BOUND + 0xBF0; // iwl-fh.h:63
pub const FH_MEM_CBBC_20_31_LOWER_BOUND: u32 = FH_MEM_LOWER_BOUND + 0xB20; // iwl-fh.h:65
pub const FH_TCSR_LOWER_BOUND: u32 = FH_MEM_LOWER_BOUND + 0xD00; // iwl-fh.h:465
pub const FH_TCSR_CHNL_NUM: u32 = 8; // iwl-fh.h:469
pub const FH_TSSR_LOWER_BOUND: u32 = FH_MEM_LOWER_BOUND + 0xEA0; // iwl-fh.h:516
pub const FH_TSSR_TX_STATUS_REG: u32 = FH_TSSR_LOWER_BOUND + 0x010; // iwl-fh.h:519
/// The keep-warm buffer the NIC is pointed at. // iwl-fh.h:714
pub const IWL_KW_SIZE: u32 = 0x1000; // iwl-fh.h:714

/// `FH_TCSR_CHNL_TX_CONFIG_REG(_chnl)` = FH_TCSR_LOWER_BOUND + 0x20 * chnl. // iwl-fh.h:472-473
pub fn fh_tcsr_chnl_tx_config_reg(chnl: u32) -> u32 {
    FH_TCSR_LOWER_BOUND + 0x20 * chnl
}

/// `FH_TSSR_TX_STATUS_REG_MSK_CHNL_IDLE(_chnl)` = (1 << chnl) << 16. // iwl-fh.h:541
pub fn fh_tssr_tx_status_reg_msk_chnl_idle(chnl: u32) -> u32 {
    (1 << chnl) << 16
}

/// `FH_MEM_CBBC_QUEUE(trans, chnl)` for a gen1 part (iwl-fh.h:71-84); `None` past queue 31 (Linux
/// WARNs at :82 and computes an address anyway).
pub fn fh_mem_cbbc_queue(chnl: u32) -> Option<u32> {
    match chnl {
        0..=15 => Some(FH_MEM_CBBC_0_15_LOWER_BOUND + 4 * chnl), // iwl-fh.h:78-79
        16..=19 => Some(FH_MEM_CBBC_16_19_LOWER_BOUND + 4 * (chnl - 16)), // iwl-fh.h:80-81
        20..=31 => Some(FH_MEM_CBBC_20_31_LOWER_BOUND + 4 * (chnl - 20)), // iwl-fh.h:82-83
        _ => None,
    }
}

// ── RFH (PRPH, the 9000 multi-queue RX) ── iwl-fh.h
pub const RFH_GEN_STATUS: u32 = 0xA0_9808; // iwl-fh.h:380
pub const RXF_DMA_IDLE: u32 = 1 << 31; // iwl-fh.h:384
pub const RFH_RXF_DMA_CFG: u32 = 0xA0_9820; // iwl-fh.h:387

// ── PRPH ── iwl-prph.h
pub const PRPH_BASE: u32 = 0x00000; // iwl-prph.h:15
pub const SCD_GP_CTRL_ENABLE_31_QUEUES: u32 = 1 << 0; // iwl-prph.h:199
pub const SCD_GP_CTRL_AUTO_ACTIVE_MODE: u32 = 1 << 18; // iwl-prph.h:200
pub const SCD_BASE: u32 = PRPH_BASE + 0xA0_2C00; // iwl-prph.h:223
pub const SCD_TXFACT: u32 = SCD_BASE + 0x10; // iwl-prph.h:228
pub const SCD_GP_CTRL: u32 = SCD_BASE + 0x1A8; // iwl-prph.h:234
pub const PREG_PRPH_WPROT_9000: u32 = 0xA0_4CE0; // iwl-prph.h:394
pub const UREG_CHICK: u32 = 0xA0_5C00; // iwl-prph.h:449
pub const UREG_CHICK_MSI_ENABLE: u32 = 1 << 24; // iwl-prph.h:450
pub const UREG_CHICK_MSIX_ENABLE: u32 = 1 << 25; // iwl-prph.h:451
pub const HPM_DEBUG: u32 = 0xA0_3440; // iwl-prph.h:467
pub const PERSISTENCE_BIT: u32 = 1 << 12; // iwl-prph.h:468
pub const PREG_WFPM_ACCESS: u32 = 1 << 12; // iwl-prph.h:469

// ── Family parameters ── cfg/9000.c
/// `iwl9000_base.num_of_queues`. // cfg/9000.c:31
pub const IWL9000_NUM_OF_QUEUES: u32 = 31; // cfg/9000.c:31

/// `iwl_trans_is_hw_error_value`: a read that returns the bus-error pattern. // iwl-trans.h:1169-1172
pub fn is_hw_error_value(val: u32) -> bool {
    (val & !0xF) == 0xA5A5_A5A0 || (val & !0xF) == 0x5A5A_5A50
}
