// SPDX-License-Identifier: GPL-2.0-only
//! The MSI interrupt masks and the cause decode of `iwl_pcie_irq_handler`, for the non-ICT path.
//! Ported from Linux `pcie/gen1_2/internal.h:827-850, :895-925, :942-980` (`_iwl_disable_interrupts`,
//! `_iwl_enable_interrupts`, `iwl_enable_fw_load_int`, `iwl_enable_fw_load_int_ctx_info`),
//! `pcie/gen1_2/rx.c:1717-1727, :1890-2150` (`iwl_pcie_int_cause_non_ict`, `iwl_pcie_irq_handler`).
//!
//! Copyright (C) 2003-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.
//!
//! DECISION, named: the ICT (interrupt cause table in host DRAM, rx.c:1733-2250) is NOT used. It is a
//! host-side optimisation — the device writes causes into a table so the ISR avoids a slow CSR read —
//! and Linux itself runs without it until `iwl_trans_pcie_fw_alive` calls `iwl_pcie_reset_ict`
//! (`use_ict` false → `iwl_pcie_int_cause_non_ict`). Every interrupt this driver takes is one CSR read
//! of `CSR_INT`. That is correct on the 9260 and costs microseconds per interrupt; the table is left
//! for a measurement to ask for.

use crate::bus::NicIo;
use crate::regs::*;
use iwlwifi_pcie_trans_core::hw::{enable_rfkill_int, TransState};
use iwlwifi_pcie_trans_core::regs::{is_hw_error_value, CSR_FH_INT_STATUS, CSR_INT, CSR_INT_MASK};

/// `STATUS_INT_ENABLED`, the one status bit the end of the handler consults. // internal.h:895-900
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IrqCtl {
    pub int_enabled: bool,
}

impl IrqCtl {
    /// `_iwl_enable_interrupts`, MSI arm. // internal.h:895-903
    pub fn enable_all<I: NicIo>(&mut self, io: &mut I, st: &mut TransState) {
        self.int_enabled = true;
        st.inta_mask = CSR_INI_SET_MASK;
        io.csr_write(CSR_INT_MASK, st.inta_mask);
    }

    /// `_iwl_disable_interrupts`, MSI arm. // internal.h:827-840
    pub fn disable_all<I: NicIo>(&mut self, io: &mut I, st: &mut TransState) {
        self.int_enabled = false;
        iwlwifi_pcie_trans_core::hw::disable_interrupts(io, st);
    }
}

/// `iwl_enable_fw_load_int`, MSI arm: only FH_TX, the channel the upload completes on.
/// // internal.h:942-949
pub fn enable_fw_load_int<I: NicIo>(io: &mut I, st: &mut TransState) {
    st.inta_mask = CSR_INT_BIT_FH_TX;
    io.csr_write(CSR_INT_MASK, st.inta_mask);
}

/// `iwl_enable_fw_load_int_ctx_info(trans, false)`, MSI arm: ALIVE and FH_RX, so the ALIVE
/// notification (an RX frame) can arrive. // internal.h:966-979
pub fn enable_alive_int<I: NicIo>(io: &mut I, st: &mut TransState) {
    st.inta_mask = CSR_INT_BIT_ALIVE | CSR_INT_BIT_FH_RX;
    io.csr_write(CSR_INT_MASK, st.inta_mask);
}

/// What one `CSR_INT` read said, after the mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Causes {
    /// The masked INTA the cause bits below came from.
    pub inta: u32,
    pub hw_err: bool,
    pub sw_err: bool,
    pub alive: bool,
    pub rf_kill: bool,
    pub ct_kill: bool,
    pub wakeup: bool,
    pub reset_done: bool,
    pub fh_rx: bool,
    pub sw_rx: bool,
    pub rx_periodic: bool,
    pub fh_tx: bool,
    pub scd: bool,
    /// Bits Linux logs as "Unhandled INTA bits". // rx.c:2123-2126 (IWL_ERR at :2124)
    pub unhandled: u32,
}

impl Causes {
    /// The RX path is to run. // rx.c:2060-2062
    pub fn wants_rx(&self) -> bool {
        self.fh_rx || self.sw_rx || self.rx_periodic
    }

    /// Any cause the handler would have acted on.
    pub fn any(&self) -> bool {
        self.inta != 0
    }
}

/// The bits `iwl_pcie_irq_handler` handles (`handled |= …`), in the order it tests them.
const HANDLED: u32 = CSR_INT_BIT_HW_ERR
    | CSR_INT_BIT_ALIVE
    | CSR_INT_BIT_RESET_DONE
    | CSR_INT_BIT_RF_KILL
    | CSR_INT_BIT_CT_KILL
    | CSR_INT_BIT_SW_ERR
    | CSR_INT_BIT_WAKEUP
    | CSR_INT_BIT_FH_RX
    | CSR_INT_BIT_SW_RX
    | CSR_INT_BIT_RX_PERIODIC
    | CSR_INT_BIT_FH_TX
    | CSR_INT_BIT_SCD;

/// Decode a masked INTA. // rx.c:1973-2108
pub fn decode(inta: u32) -> Causes {
    let b = |m: u32| inta & m != 0;
    Causes {
        inta,
        hw_err: b(CSR_INT_BIT_HW_ERR),                 // rx.c:1973
        scd: b(CSR_INT_BIT_SCD),                       // rx.c:1988
        alive: b(CSR_INT_BIT_ALIVE),                   // rx.c:1995
        reset_done: b(CSR_INT_BIT_RESET_DONE),         // rx.c:2009
        rf_kill: b(CSR_INT_BIT_RF_KILL),               // rx.c:2018
        ct_kill: b(CSR_INT_BIT_CT_KILL),               // rx.c:2024
        sw_err: b(CSR_INT_BIT_SW_ERR),                 // rx.c:2031
        wakeup: b(CSR_INT_BIT_WAKEUP),                 // rx.c:2045
        fh_rx: b(CSR_INT_BIT_FH_RX),                   // rx.c:2057-2062 (the FH_RX | SW_RX | RX_PERIODIC test)
        sw_rx: b(CSR_INT_BIT_SW_RX),                   // rx.c:2057-2062
        rx_periodic: b(CSR_INT_BIT_RX_PERIODIC),       // rx.c:2066
        fh_tx: b(CSR_INT_BIT_FH_TX),                   // rx.c:2108
        unhandled: inta & !HANDLED,
    }
}

/// What the handler's prologue found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// "Ignore interrupt, inta == 0" — not ours (a shared line) or masked. // rx.c:1928-1940
    Nothing,
    /// "HARDWARE GONE?? INTA == 0x%08x": all-ones or the bus-error pattern. // rx.c:1943-1950
    HardwareGone { inta: u32 },
    /// A real interrupt, acknowledged.
    Some(Causes),
}

/// The prologue of `iwl_pcie_irq_handler` on the non-ICT path: read `CSR_INT`, mask it with the
/// software mask, refuse nothing/gone, and acknowledge — `CSR_INT ← inta | ~inta_mask`, which also
/// clears the causes the hardware raises despite the mask (rx.c:1952-1964). // rx.c:1909-1964
pub fn service<I: NicIo>(io: &mut I, st: &TransState) -> Cause {
    let raw = io.csr_read(CSR_INT); // rx.c:1717-1727 (iwl_pcie_int_cause_non_ict)
    let inta = raw & st.inta_mask; // rx.c:1923
    if inta == 0 {
        return Cause::Nothing;
    }
    if raw == 0xFFFF_FFFF || is_hw_error_value(raw) {
        // DEVIATION, named: rx.c:1943 tests the MASKED inta, so an all-ones read (a dead link) is
        // only caught when the mask itself is all-ones — otherwise it is mistaken for HW_ERR | SW_ERR
        // and "restarted". The port tests the raw read: all-ones is a link that is gone, not a fault.
        return Cause::HardwareGone { inta: raw };
    }
    io.csr_write(CSR_INT, inta | !st.inta_mask); // rx.c:1964
    Cause::Some(decode(inta))
}

/// The RX arm of the handler: ack the FH RX bits, the one-shot periodic timer. // rx.c:2057-2095
pub fn ack_rx<I: NicIo>(io: &mut I, c: &Causes) {
    if c.fh_rx || c.sw_rx {
        io.csr_write(CSR_FH_INT_STATUS, CSR_FH_INT_RX_MASK); // rx.c:2063-2064
    }
    if c.rx_periodic {
        io.csr_write(CSR_INT, CSR_INT_BIT_RX_PERIODIC); // rx.c:2066-2069
    }
    // Disable the periodic interrupt; it is only a one-shot. // rx.c:2083-2084
    io.write8(CSR_INT_PERIODIC_REG, CSR_INT_PERIODIC_DIS);
    // Re-arm it for 8 ms only after a REAL rx interrupt, to catch a dangling one. // rx.c:2093-2095
    if c.fh_rx || c.sw_rx {
        io.write8(CSR_INT_PERIODIC_REG, CSR_INT_PERIODIC_ENA);
    }
}

/// The FH_TX arm: ack the TX channel bits. The caller wakes the upload waiter. // rx.c:2108-2115
pub fn ack_fh_tx<I: NicIo>(io: &mut I) {
    io.csr_write(CSR_FH_INT_STATUS, CSR_FH_INT_TX_MASK);
}

/// The end of the handler (the non-polling arm): put the mask back to what the current state wants.
/// // rx.c:2133-2147
pub fn reenable<I: NicIo>(io: &mut I, st: &mut TransState, ctl: &mut IrqCtl, handled: &Causes) {
    if ctl.int_enabled {
        ctl.enable_all(io, st); // `_iwl_enable_interrupts`
    } else if handled.fh_tx {
        enable_fw_load_int(io, st);
    } else if handled.rf_kill {
        enable_rfkill_int(io, st);
    } else if handled.alive || handled.fh_rx {
        enable_alive_int(io, st);
    }
}
