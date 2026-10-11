// SPDX-License-Identifier: GPL-2.0-only
//! `start_hw` and `stop_device` for the 9000 family in MSI mode — the sequences that bracket EVERY
//! firmware start (the 9260 runs INIT, stops, then runs REGULAR). Ported from Linux
//! `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/trans.c:1138-1147` (`iwl_pcie_conf_msix_hw`, MSI
//! arm), `:1184-1267` (`_iwl_trans_pcie_stop_device`), `:1734-1763`
//! (`iwl_trans_pcie_clear_persistence_bit`) and `:1786-1832` (`_iwl_trans_pcie_start_hw`), with the
//! interrupt helpers of `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/internal.h:827-850, :1019-1059`.
//!
//! Copyright (C) 2007-2015, 2018-2024 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.
//!
//! STATUS_DEVICE_ENABLED is load-bearing for the MSI select: `iwl_pcie_conf_msix_hw` writes
//! UREG_CHICK only while the device is enabled (trans.c:1143-1146). start_hw calls it right after
//! apm_init set the flag, so it writes; stop_device calls it after apm_stop cleared the flag, so it
//! does NOT (OpenBSD if_iwm.c `iwm_conf_msix_hw(sc, stopped)` makes the same distinction explicit).
//! [`TransState`] carries the flag so that ordering is tested, not assumed.

use crate::apm::{apm_init_9000, apm_stop, prepare_card_hw, sw_reset, ApmError};
use crate::io::{clear_bit, set_bit, TransIo};
use crate::regs::*;
use crate::tx_init::{rx_stop, tx_stop};

/// The transport status bits these sequences read and write. // iwl-trans.h STATUS_*; internal.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransState {
    /// STATUS_DEVICE_ENABLED. // trans.c:317, :466, :1208
    pub device_enabled: bool,
    /// trans_pcie->is_down. // trans.c:1190-1193, :1826
    pub is_down: bool,
    /// trans_pcie->use_ict. // rx.c:2232-2239
    pub use_ict: bool,
    /// The CSR_INT_MASK value last written (trans_pcie->inta_mask).
    pub inta_mask: u32,
}

impl Default for TransState {
    /// A freshly probed transport: down, disabled.
    fn default() -> Self {
        TransState { device_enabled: false, is_down: true, use_ict: false, inta_mask: 0 }
    }
}

/// `_iwl_disable_interrupts`, MSI arm: mask everything, ack everything. // internal.h:827-850
pub fn disable_interrupts<I: TransIo>(io: &mut I, st: &mut TransState) {
    io.csr_write(CSR_INT_MASK, 0x0000_0000); // internal.h:834
    io.csr_write(CSR_INT, 0xFFFF_FFFF); // internal.h:838
    io.csr_write(CSR_FH_INT_STATUS, 0xFFFF_FFFF); // internal.h:839
    st.inta_mask = 0;
}

/// `iwl_enable_rfkill_int`, MSI arm, family >= 9000. // internal.h:1019-1044
pub fn enable_rfkill_int<I: TransIo>(io: &mut I, st: &mut TransState) {
    st.inta_mask = CSR_INT_BIT_RF_KILL; // internal.h:1025
    io.csr_write(CSR_INT_MASK, st.inta_mask); // internal.h:1026
    set_bit(io, CSR_GP_CNTRL, CSR_GP_CNTRL_REG_FLAG_RFKILL_WAKE_L1A_EN); // internal.h:1035-1043
}

/// `iwl_is_rfkill_set`: the hardware switch is in KILL when HW_RF_KILL_SW reads CLEAR.
/// // internal.h:1048-1059
pub fn is_rfkill_set<I: TransIo>(io: &mut I) -> bool {
    io.csr_read(CSR_GP_CNTRL) & CSR_GP_CNTRL_REG_FLAG_HW_RF_KILL_SW == 0
}

/// `iwl_pcie_conf_msix_hw`, MSI arm on an mq_rx_supported part (cfg/9000.c:74): UREG_CHICK ←
/// MSI_ENABLE, only while the device is enabled. Returns whether it wrote. // trans.c:1142-1147
pub fn conf_msi_hw<I: TransIo>(io: &mut I, st: &TransState) -> bool {
    if st.device_enabled {
        io.prph_write(UREG_CHICK, UREG_CHICK_MSI_ENABLE); // trans.c:1145-1146
    }
    st.device_enabled
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwError {
    Apm(ApmError),
    /// "Error, can not clear persistence bit": HPM_DEBUG has PERSISTENCE_BIT and the WFPM write
    /// protection is on. // trans.c:1753-1757
    PersistenceLocked { hpm: u32, wprot: u32 },
}

impl From<ApmError> for HwError {
    fn from(e: ApmError) -> Self {
        HwError::Apm(e)
    }
}

/// `iwl_trans_pcie_clear_persistence_bit`, the IWL_DEVICE_FAMILY_9000 arm. Returns whether it cleared
/// the bit. // trans.c:1734-1763
pub fn clear_persistence_bit<I: TransIo>(io: &mut I) -> Result<bool, HwError> {
    let hpm = io.prph_read(HPM_DEBUG); // trans.c:1749
    if is_hw_error_value(hpm) || hpm & PERSISTENCE_BIT == 0 {
        return Ok(false); // trans.c:1750
    }
    let wprot = io.prph_read(PREG_PRPH_WPROT_9000); // trans.c:1739-1740, :1751
    if wprot & PREG_WFPM_ACCESS != 0 {
        return Err(HwError::PersistenceLocked { hpm, wprot }); // trans.c:1753-1757
    }
    io.prph_write(HPM_DEBUG, hpm & !PERSISTENCE_BIT); // trans.c:1758-1759
    Ok(true)
}

/// `_iwl_trans_pcie_start_hw` for the 9000 family (trans.c:1786-1832): claim the card, clear the
/// persistence bit, SW-reset and reclaim, APM up (→ DEVICE_ENABLED), the MSI select, the rfkill
/// interrupt, then report the rfkill switch (`iwl_pcie_check_hw_rf_kill`, :1829). Returns `true`
/// when the radio is killed. The 22000-integrated power-gating arm (:1807-1812) is not this family.
pub fn start_hw<I: TransIo>(io: &mut I, st: &mut TransState) -> Result<bool, HwError> {
    prepare_card_hw(io)?; // trans.c:1793-1797
    clear_persistence_bit(io)?; // trans.c:1799-1801
    sw_reset(io, true)?; // trans.c:1803-1805
    apm_init_9000(io)?; // trans.c:1814-1816
    st.device_enabled = true; // trans.c:317
    conf_msi_hw(io, st); // iwl_pcie_init_msix → conf_msix_hw, trans.c:1818
    enable_rfkill_int(io, st); // trans.c:1821
    st.is_down = false; // trans.c:1826
    Ok(is_rfkill_set(io)) // trans.c:1829
}

/// What a stop observed. Linux logs each of these and carries on; the port reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopReport {
    /// `is_down` was already set: nothing was touched. // trans.c:1190-1191
    pub already_down: bool,
    /// The DMA stops ran (the device was enabled), with their idle verdicts. // trans.c:1208-1215
    pub tx_stop: Option<Result<(), u32>>,
    pub rx_stop: Option<Result<(), u32>>,
    /// stop_master's MASTER_DISABLED verdict. // trans.c:434-435
    pub master_stopped: bool,
    /// The ownership retake after the final SW reset. // trans.c:1237
    pub retake: Result<(), ApmError>,
}

/// `_iwl_trans_pcie_stop_device` for the 9000 family, MSI mode (trans.c:1184-1267).
pub fn stop_device<I: TransIo>(io: &mut I, st: &mut TransState) -> StopReport {
    let mut r = StopReport { already_down: false, tx_stop: None, rx_stop: None, master_stopped: false, retake: Ok(()) };
    if st.is_down {
        r.already_down = true; // trans.c:1190-1191
        return r;
    }
    st.is_down = true; // trans.c:1193
    disable_interrupts(io, st); // trans.c:1196
    st.use_ict = false; // iwl_pcie_disable_ict, trans.c:1199
    if st.device_enabled {
        st.device_enabled = false; // test_and_clear_bit, trans.c:1208
        r.tx_stop = Some(tx_stop(io)); // trans.c:1214
        r.rx_stop = Some(rx_stop(io)); // trans.c:1215
        // APMG_CLK_DIS (trans.c:1218-1222): apmg_not_supported on the 9000 family.
    }
    clear_bit(io, CSR_GP_CNTRL, CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ); // trans.c:1230-1231
    st.device_enabled = false; // apm_stop clears it, trans.c:466
    r.master_stopped = apm_stop(io); // trans.c:1234
    r.retake = sw_reset(io, true); // trans.c:1237
    conf_msi_hw(io, st); // trans.c:1246 — the device is disabled now, so this writes nothing
    disable_interrupts(io, st); // trans.c:1255
    enable_rfkill_int(io, st); // trans.c:1266
    r
}
