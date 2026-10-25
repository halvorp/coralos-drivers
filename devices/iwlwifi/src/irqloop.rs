// SPDX-License-Identifier: GPL-2.0-only
//! The interrupt service and the waits built on it. Ported from Linux `pcie/gen1_2/rx.c:1890-2150`
//! (`iwl_pcie_irq_handler`, MSI, non-ICT), over nic-core's cause decode. Linux splits the work between a
//! hard handler that masks the interrupt and a threaded handler that services and unmasks; this driver is
//! one task that sleeps in [`Host::wait_irq`], so it reads, acknowledges and services in one pass and puts
//! the mask back at the end (`iwlwifi_pcie_nic_core::irq::reenable`).
use crate::host::{Host, Wake};
use crate::transport::*;
use iwlwifi_pcie_nic_core::irq::{ack_fh_tx, ack_rx, reenable, service, Cause};
use iwlwifi_pcie_trans_core::hw::{disable_interrupts, is_rfkill_set};

/// What one pass of [`Transport::service_irq`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Serviced {
    /// `CSR_INT` held nothing we unmask (a shared line, a coalesced interrupt already serviced).
    pub nothing: bool,
    /// Packets taken off the receive queue.
    pub packets: u32,
    /// The firmware-load channel completed a chunk.
    pub fh_tx: bool,
    /// The RF-kill switch changed (see `Transport::rfkill_hw`).
    pub rfkill: bool,
    /// ALIVE was raised (the notification itself is an RX packet).
    pub alive: bool,
}

impl<H: Host> Transport<H> {
    /// Service the card once. A hardware or firmware error is RECORDED in `self.fault` (and the
    /// handler carries on, as Linux does for SW_ERR, so the reply that explains it is still delivered);
    /// HW_ERR stops at once like Linux's `goto out`. // rx.c:1890-2150
    pub fn service_irq(&mut self) -> Result<Serviced, TransportError> {
        self.stats.irqs += 1;
        let c = match service(&mut self.nic, &self.st) {
            Cause::Nothing => {
                self.stats.spurious += 1;
                return Ok(Serviced { nothing: true, ..Serviced::default() });
            }
            Cause::HardwareGone { inta } => return Err(TransportError::HardwareGone { inta }),
            Cause::Some(c) => c,
        };
        let mut out = Serviced::default();
        if c.hw_err {
            // "Hardware error detected. Restarting." Tell the device to stop sending interrupts. // rx.c:1973-1986
            self.ctl.int_enabled = false;
            disable_interrupts(&mut self.nic, &mut self.st);
            self.fault = Some(Fault::HwError { inta: c.inta });
            return Ok(out);
        }
        out.alive = c.alive; // "Alive notification via Rx interrupt will do the real work", rx.c:1995-2007
        if c.rf_kill {
            self.rfkill_hw = is_rfkill_set(&mut self.nic); // iwl_pcie_handle_rfkill_irq, rx.c:2018-2022
            out.rfkill = true;
        }
        if c.sw_err {
            self.fault = Some(Fault::SwError { inta: c.inta }); // rx.c:2031-2043
        }
        if c.wants_rx() {
            ack_rx(&mut self.nic, &c); // rx.c:2057-2095
            out.packets = self.rx_handle()?;
        }
        if c.fh_tx {
            ack_fh_tx(&mut self.nic); // rx.c:2108-2109
            self.write_complete = true; // ucode_write_complete, rx.c:2113
            out.fh_tx = true;
        }
        reenable(&mut self.nic, &mut self.st, &mut self.ctl, &c); // rx.c:2133-2147
        Ok(out)
    }

    /// Block until `pred` matches a packet in the inbox (which is searched first, oldest first), the
    /// firmware or hardware reports an error, or `timeout_ms` passes. Packets that do not match stay
    /// queued for their own waiter. // fw/notif-wait.c semantics; mvm/fw.c:286-330
    pub fn wait_notif<F: Fn(&Rx) -> bool>(&mut self, timeout_ms: u32, pred: F) -> Result<Rx, TransportError> {
        let deadline = self.nic.bus.now_ms() + timeout_ms as u64;
        loop {
            if let Some(pos) = self.inbox.iter().position(|r| pred(r)) {
                return Ok(self.inbox.remove(pos).expect("position() found it"));
            }
            if let Some(f) = self.fault {
                return Err(TransportError::Fault(f));
            }
            // A completion whose interrupt was coalesced into an earlier pass would sleep here until the
            // deadline; look at the status block first. (The periodic RX interrupt is the hardware backstop.)
            if self.rx.is_some() && self.rx_handle()? > 0 {
                continue;
            }
            match self.nic.bus.wait_irq(deadline) {
                Wake::Deadline => return Err(TransportError::Timeout { what: "notification" }),
                Wake::Irq => {
                    self.service_irq()?;
                }
            }
        }
    }

    /// Service the card until `ms` milliseconds of the host's clock have passed or something is in the
    /// inbox. Used by the idle loop's tail to drain notifications that arrive with no waiter.
    pub fn pump(&mut self, ms: u32) -> Result<(), TransportError> {
        let deadline = self.nic.bus.now_ms() + ms as u64;
        loop {
            if !self.inbox.is_empty() || self.fault.is_some() {
                return Ok(());
            }
            match self.nic.bus.wait_irq(deadline) {
                Wake::Deadline => return Ok(()),
                Wake::Irq => {
                    self.service_irq()?;
                }
            }
        }
    }
}
