// SPDX-License-Identifier: GPL-2.0-only
//! PCI ownership, the APM (adapter power management) up/down sequences and the SW reset, ported from
//! Linux `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/trans.c:78-95` (`iwl_trans_pcie_sw_reset`),
//! `:205-320` (`iwl_pcie_apm_config`, `iwl_pcie_apm_init`), `:413-483` (`iwl_pcie_apm_stop_master`,
//! `iwl_pcie_apm_stop`), `:522-598` (`iwl_pcie_set_hw_ready`, `iwl_pcie_prepare_card_hw`) and
//! `:4320-4360` (`iwl_pcie_gen1_2_activate_nic`).
//!
//! Copyright (C) 2007-2015, 2018-2024 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.
//!
//! 9000-family branches: no L0S-exit-timer bit (trans.c:243, family >= 8000), no PLL cfg (:267), no
//! APMG clock writes (:303, apmg_not_supported, cfg/9000.c:42), no BISR delay (:4326), CSR_RESET
//! carries the reset (:85-89, < BZ), no lp_xtal workaround (:471, cfg/7000.c only). The CSME/SAP arm
//! of prepare_card_hw (:577-587) is not modelled: it needs AMT, which the discrete 9260 on a
//! Chromebook does not have, and an AMT-owned card reaches the same named refusal.

use crate::io::{clear_bit, poll_bits, set_bit, TransIo};
use crate::regs::*;

/// `HW_READY_TIMEOUT`, µs. // trans.c:522
pub const HW_READY_TIMEOUT: u32 = 50; // trans.c:522
/// prepare_card_hw's retry ladder. // trans.c:561-592
pub const PREPARE_LINK_PM_SETTLE_US: u32 = 1000; // trans.c:561
pub const PREPARE_ROUNDS: u32 = 10; // trans.c:563
pub const PREPARE_RETRY_US: u32 = 200; // trans.c:589-590
pub const PREPARE_ROUND_BUDGET_US: u32 = 150_000; // trans.c:591
pub const PREPARE_ROUND_GAP_US: u32 = 25_000; // trans.c:592
/// activate_nic's MAC-clock wait. // trans.c:4354
pub const MAC_CLOCK_READY_TIMEOUT_US: u32 = 25_000; // trans.c:4354
/// stop_master's wait. // trans.c:430-431
pub const STOP_MASTER_TIMEOUT_US: u32 = 100; // trans.c:431
/// The SW-reset settle, < BZ. // trans.c:88
pub const SW_RESET_SETTLE_US: u32 = 5000; // trans.c:88

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApmError {
    /// "Couldn't prepare the card": PCI_OWN_SET never stuck after `attempts` claims. // trans.c:595
    CardNotReady { attempts: u32 },
    /// "Failed to wake NIC": MAC_CLOCK_READY did not rise within 25 ms of INIT_DONE. // trans.c:4354-4358
    MacClockNotReady { gp_cntrl: u32 },
}

/// `iwl_pcie_set_hw_ready`: claim, see whether it stuck, announce OS_ALIVE if so. // trans.c:525-542
pub fn set_hw_ready<I: TransIo>(io: &mut I) -> bool {
    set_bit(io, CSR_HW_IF_CONFIG_REG, CSR_HW_IF_CONFIG_REG_PCI_OWN_SET); // trans.c:529-530
    let ok = poll_bits(io, CSR_HW_IF_CONFIG_REG, CSR_HW_IF_CONFIG_REG_PCI_OWN_SET, HW_READY_TIMEOUT); // :533-535
    if ok {
        set_bit(io, CSR_MBOX_SET_REG, CSR_MBOX_SET_REG_OS_ALIVE); // trans.c:537-538
    }
    ok
}

/// `iwl_pcie_prepare_card_hw`. // trans.c:545-598
pub fn prepare_card_hw<I: TransIo>(io: &mut I) -> Result<(), ApmError> {
    let mut attempts = 1;
    if set_hw_ready(io) {
        return Ok(()); // trans.c:552-557
    }
    set_bit(io, CSR_DBG_LINK_PWR_MGMT_REG, CSR_RESET_LINK_PWR_MGMT_DISABLED); // trans.c:559-560
    io.sleep_us(PREPARE_LINK_PM_SETTLE_US); // trans.c:561
    for _ in 0..PREPARE_ROUNDS {
        set_bit(io, CSR_HW_IF_CONFIG_REG, CSR_HW_IF_CONFIG_REG_WAKE_ME); // trans.c:567-568
        let mut t = 0;
        loop {
            attempts += 1;
            if set_hw_ready(io) {
                return Ok(()); // trans.c:571-575
            }
            io.sleep_us(PREPARE_RETRY_US); // trans.c:589
            t += PREPARE_RETRY_US; // trans.c:590
            if t >= PREPARE_ROUND_BUDGET_US {
                break; // trans.c:591
            }
        }
        io.sleep_us(PREPARE_ROUND_GAP_US); // trans.c:592
    }
    Err(ApmError::CardNotReady { attempts })
}

/// `iwl_pcie_gen1_2_activate_nic`, < BZ: INIT_DONE moves the adapter D0U* → D0A*, then the MAC clock.
/// // trans.c:4320-4360
pub fn activate_nic<I: TransIo>(io: &mut I) -> Result<(), ApmError> {
    set_bit(io, CSR_GP_CNTRL, CSR_GP_CNTRL_REG_FLAG_INIT_DONE); // trans.c:4341-4342
    if !poll_bits(io, CSR_GP_CNTRL, CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY, MAC_CLOCK_READY_TIMEOUT_US) {
        return Err(ApmError::MacClockNotReady { gp_cntrl: io.csr_read(CSR_GP_CNTRL) }); // trans.c:4354-4358
    }
    Ok(())
}

/// `iwl_pcie_apm_init` on the 9000 branch, with `iwl_pcie_apm_config`'s CSR write inlined (its
/// LNKCTL / DEVCTL2 reads only record pm_support / ltr_enabled — config space is the service's).
/// On success the caller marks STATUS_DEVICE_ENABLED. // trans.c:231-320
pub fn apm_init_9000<I: TransIo>(io: &mut I) -> Result<(), ApmError> {
    set_bit(io, CSR_GIO_CHICKEN_BITS, CSR_GIO_CHICKEN_BITS_REG_BIT_L1A_NO_L0S_RX); // trans.c:251-252
    set_bit(io, CSR_DBG_HPET_MEM_REG, CSR_DBG_HPET_MEM_REG_VAL); // trans.c:255
    set_bit(io, CSR_HW_IF_CONFIG_REG, CSR_HW_IF_CONFIG_REG_HAP_WAKE); // trans.c:261-262
    set_bit(io, CSR_GIO_REG, CSR_GIO_REG_VAL_L0S_DISABLED); // apm_config, trans.c:214
    activate_nic(io) // trans.c:270-272
}

/// `iwl_trans_pcie_sw_reset`, < BZ: CSR_RESET.SW_RESET, settle, and (when `retake_ownership`)
/// prepare_card_hw again. // trans.c:78-95
pub fn sw_reset<I: TransIo>(io: &mut I, retake_ownership: bool) -> Result<(), ApmError> {
    set_bit(io, CSR_RESET, CSR_RESET_REG_FLAG_SW_RESET); // trans.c:86-87
    io.sleep_us(SW_RESET_SETTLE_US); // trans.c:88
    if retake_ownership {
        return prepare_card_hw(io); // trans.c:91-92
    }
    Ok(())
}

/// `iwl_pcie_apm_stop_master`, < BZ. Linux only WARNs when the master never reports disabled
/// (trans.c:434-435); the port returns whether it did so the caller can log it by name.
/// // trans.c:413-438
pub fn apm_stop_master<I: TransIo>(io: &mut I) -> bool {
    set_bit(io, CSR_RESET, CSR_RESET_REG_FLAG_STOP_MASTER); // trans.c:428
    poll_bits(io, CSR_RESET, CSR_RESET_REG_FLAG_MASTER_DISABLED, STOP_MASTER_TIMEOUT_US) // trans.c:430-431
}

/// `iwl_pcie_apm_stop(trans, op_mode_leave = false)`: the caller clears STATUS_DEVICE_ENABLED
/// (trans.c:466); DMA stops, the device is reset without retaking ownership, INIT_DONE is cleared
/// (D0A* → D0U*). Returns stop_master's verdict. // trans.c:440-483
pub fn apm_stop<I: TransIo>(io: &mut I) -> bool {
    let master_stopped = apm_stop_master(io); // trans.c:469
    let _ = sw_reset(io, false); // trans.c:476 (cannot fail without retake)
    clear_bit(io, CSR_GP_CNTRL, CSR_GP_CNTRL_REG_FLAG_INIT_DONE); // trans.c:482
    master_stopped
}
