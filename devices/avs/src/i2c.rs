// SPDX-License-Identifier: GPL-2.0-only
//! The DesignWare I2C master (the codec's bus, `INT34C?` / PCI 00:19.2 on Atlas), driven transfer by transfer over a 32-bit register
//! window, built from the repo's own cores: `i2c-designware-master-core` decides the commands (`pump_tx`) and drains the receive FIFO
//! (`drain_rx`), `i2c-designware-core` supplies the register map, the abort decode and the disable rules.
//!
//! One transfer is the shape of Linux's `__i2c_dw_xfer_one_part`, polled: wait for the bus to be idle, disable, program the master
//! configuration and the target address, mask every interrupt (this master is polled: the codec's REGISTER traffic is bounded, short, and
//! never the idle loop), enable, clear the latched interrupts, then pump commands and drain the receive FIFO until STOP is detected or the
//! controller aborts. `Ok` means the target acknowledged its address and every byte, every requested byte came back and the bus reached
//! STOP; nothing else is success. After the transfer the controller is disabled again, so nothing carries into the next one.
//!
//! THE WAIT FOR COMPLETION BLOCKS ON THE CONTROLLER'S INTERRUPT when the launch row grants it (`STOP_DET` and `TX_ABRT` are unmasked for
//! the transfer and masked again after it); a host with no I2C line falls back to a timed sleep between status reads. Either way every
//! wait is bounded twice (the sleep without a line is the PollPlan-style exception of `rig::Rig::sleep_ms`: a blocking timed wait on a hardware
//! status that has no wakeup source here): by [`XFER_BUDGET_MS`] on the host's clock and by [`SPIN_LIMIT`] iterations, and none of them spins.
use crate::machine::I2cFault;
use i2c_designware_core::abort::{verdict, AbortVerdict, CAPTURE_THEN_CLEAR};
use i2c_designware_core::enable::{disable_took_effect, next_step, DisableStep, DISABLE_ATTEMPTS};
use i2c_designware_core::regs::{bits, off};
use i2c_designware_core::xfer::{rx_fifo_depth, tx_fifo_depth};
use i2c_designware_master_core::fifo::{drain_rx, pump_tx, Command, RxState, TxState};
use i2c_designware_master_core::setup::{master_config, next_part_len, target_setup, Message, I2C_M_RD};

/// A transfer that has not finished in this long has failed.
pub const XFER_BUDGET_MS: u64 = 50;
/// A backstop on loop iterations, independent of the clock.
pub const SPIN_LIMIT: u32 = 5000;
/// The most commands one pump call may produce (a codec transfer is at most 5 bytes).
const CMDS: usize = 16;

/// How a wait on the controller's interrupt ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqWait {
    Fired,
    TimedOut,
    /// The host has no interrupt line for this controller (or lost it): the caller sleeps and reads instead.
    NoLine,
}

/// The 32-bit register window of the controller, and what a wait needs of its host.
pub trait I2cHost {
    fn rd(&mut self, off: u32) -> u32;
    fn wr(&mut self, off: u32, value: u32);
    /// Block briefly (a timed wait of at least the clock's resolution) between status reads, for a host with no interrupt line.
    fn relax(&mut self);
    fn now_ms(&mut self) -> u64;
    /// Does the host hold the controller's interrupt line?
    fn has_irq(&self) -> bool {
        false
    }
    /// Block until the controller's interrupt fires or `timeout_ms` passes.
    fn wait_irq(&mut self, _timeout_ms: u64) -> IrqWait {
        IrqWait::NoLine
    }
    /// Re-arm the line after the cause was serviced (a level line stays masked at the host until then).
    fn ack_irq(&mut self) {}
}

/// The controller's timing, from the board record. Zero means "leave what the firmware programmed".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timing {
    pub bus_hz: u32,
    pub fs_hcnt: u16,
    pub fs_lcnt: u16,
    pub sda_hold: u32,
}

pub struct DwMaster {
    timing: Timing,
    tx_depth: u32,
    rx_depth: u32,
    master_cfg: u32,
    identified: bool,
}

/// What a transfer left in the registers, for the status line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LastAbort {
    pub source: u32,
}

impl DwMaster {
    pub const fn new(timing: Timing) -> Self {
        Self { timing, tx_depth: 2, rx_depth: 2, master_cfg: 0, identified: false }
    }

    /// Check the identity register once and read the FIFO depths.
    fn identify<H: I2cHost>(&mut self, h: &mut H) -> Result<(), I2cFault> {
        if self.identified {
            return Ok(());
        }
        if h.rd(off::COMP_TYPE) != bits::COMP_TYPE_VALUE {
            return Err(I2cFault::NoController);
        }
        let p = h.rd(off::COMP_PARAM_1);
        self.tx_depth = tx_fifo_depth(p);
        self.rx_depth = rx_fifo_depth(p);
        self.master_cfg = master_config(if self.timing.bus_hz == 0 { 400_000 } else { self.timing.bus_hz });
        self.identified = true;
        Ok(())
    }

    /// `__i2c_dw_disable`: abort a held bus first, then clear ENABLE and wait for ENABLE_STATUS.
    fn disable<H: I2cHost>(&mut self, h: &mut H) -> Result<(), I2cFault> {
        let raw = h.rd(off::RAW_INTR_STAT);
        let status = h.rd(off::STATUS);
        let en = h.rd(off::ENABLE);
        match next_step(raw, status, en, if self.timing.bus_hz == 0 { 400_000 } else { self.timing.bus_hz }) {
            DisableStep::EnableThenSettle { .. } => {
                h.wr(off::ENABLE, bits::ENABLE_ENABLE);
                h.relax();
                h.wr(off::ENABLE, bits::ENABLE_ENABLE | bits::ENABLE_ABORT);
            }
            DisableStep::SetAbort => h.wr(off::ENABLE, bits::ENABLE_ENABLE | bits::ENABLE_ABORT),
            DisableStep::DisableNow => {}
        }
        for _ in 0..DISABLE_ATTEMPTS {
            h.wr(off::ENABLE, 0);
            if disable_took_effect(h.rd(off::ENABLE_STATUS)) {
                return Ok(());
            }
            h.relax();
        }
        Err(I2cFault::Timeout)
    }

    fn abort_fault<H: I2cHost>(&mut self, h: &mut H) -> I2cFault {
        // the source register is cleared by reading CLR_TX_ABRT: capture it first (i2c-designware-master.c:611-618)
        let source = h.rd(CAPTURE_THEN_CLEAR[0]);
        let _ = h.rd(CAPTURE_THEN_CLEAR[1]);
        match verdict(source) {
            AbortVerdict::NoAck => I2cFault::Nak,
            _ => I2cFault::Abort { source },
        }
    }

    /// One transfer: `msgs` (one or two, same address), the bytes of each write message, and the buffer the read message fills.
    pub fn xfer<H: I2cHost>(&mut self, h: &mut H, msgs: &[Message], writes: &[&[u8]], read: &mut [u8]) -> Result<(), I2cFault> {
        self.identify(h)?;
        if msgs.is_empty() || msgs.len() > 2 || next_part_len(msgs, false).map_err(|_| I2cFault::Abort { source: 0 })? != msgs.len() {
            return Err(I2cFault::Abort { source: 0 });
        }
        let start = h.now_ms();
        // wait for the bus to be idle
        let mut idle = false;
        for _ in 0..SPIN_LIMIT {
            if h.rd(off::STATUS) & bits::STATUS_MASTER_ACTIVITY == 0 {
                idle = true;
                break;
            }
            if h.now_ms().saturating_sub(start) >= XFER_BUDGET_MS {
                break;
            }
            h.relax();
        }
        if !idle {
            return Err(I2cFault::Timeout);
        }
        self.disable(h)?;
        let t = target_setup(msgs[0]);
        h.wr(off::CON, self.master_cfg | t.con_10bit_value);
        h.wr(off::TAR, t.tar_value);
        if self.timing.fs_hcnt != 0 && self.timing.fs_lcnt != 0 {
            h.wr(off::FS_SCL_HCNT, self.timing.fs_hcnt as u32);
            h.wr(off::FS_SCL_LCNT, self.timing.fs_lcnt as u32);
        }
        if self.timing.sda_hold != 0 {
            h.wr(off::SDA_HOLD, self.timing.sda_hold);
        }
        // polled hosts mask everything; a host with the line unmasks exactly the two causes a transfer ends with
        let irq_mode = h.has_irq();
        h.wr(off::INTR_MASK, 0);
        h.wr(off::ENABLE, bits::ENABLE_ENABLE);
        let _ = h.rd(off::CLR_INTR);
        if irq_mode {
            h.wr(off::INTR_MASK, bits::INTR_STOP_DET | bits::INTR_TX_ABRT);
        }

        let mut tx = TxState::new();
        let mut rx = RxState::new();
        let mut got = 0usize;
        let want: usize = msgs.iter().filter(|m| m.flags & I2C_M_RD != 0).map(|m| m.len).sum();
        if want > read.len() {
            let _ = self.disable(h);
            return Err(I2cFault::Abort { source: 0 });
        }
        let mut result = Err(I2cFault::Timeout);
        'outer: for _ in 0..SPIN_LIMIT {
            let raw = h.rd(off::RAW_INTR_STAT);
            if raw & bits::INTR_TX_ABRT != 0 {
                result = Err(self.abort_fault(h));
                break;
            }
            // commands
            if tx.message_index < msgs.len() {
                let mut cmds = [Command { word: 0, message_index: 0, byte_index: 0 }; CMDS];
                let (txfl, rxfl) = (h.rd(off::TXFLR), h.rd(off::RXFLR));
                match pump_tx(tx, msgs, writes, self.master_cfg, self.tx_depth, txfl, self.rx_depth, rxfl, &mut cmds) {
                    Ok(p) => {
                        for c in &cmds[..p.command_count] {
                            h.wr(off::DATA_CMD, c.word);
                        }
                        tx = p.state;
                    }
                    Err(_) => {
                        result = Err(I2cFault::Abort { source: 0 });
                        break 'outer;
                    }
                }
            }
            // receive FIFO
            let mut words = [0u32; CMDS];
            let mut n = 0;
            while n < CMDS && h.rd(off::RXFLR) > 0 {
                words[n] = h.rd(off::DATA_CMD);
                n += 1;
            }
            if n > 0 {
                // Linux keeps one `rx_outstanding` for the command pump and the drain; here it is handed across
                rx.rx_outstanding = tx.rx_outstanding;
                let mut out = [0u8; CMDS];
                match drain_rx(rx, msgs, &words[..n], &mut out, bits::INTR_MASTER_MASK) {
                    Ok(d) => {
                        rx = d.state;
                        tx.rx_outstanding = rx.rx_outstanding;
                        for b in &out[..d.consumed] {
                            if got < read.len() {
                                read[got] = *b;
                                got += 1;
                            }
                        }
                    }
                    Err(_) => {
                        result = Err(I2cFault::Abort { source: 0 });
                        break 'outer;
                    }
                }
            }
            if tx.message_index >= msgs.len() && got >= want && raw & bits::INTR_STOP_DET != 0 {
                result = Ok(());
                break;
            }
            let spent = h.now_ms().saturating_sub(start);
            if spent >= XFER_BUDGET_MS {
                break;
            }
            // block on the interrupt (or, with no line, sleep): the cause is read at the top of the loop
            match if irq_mode { h.wait_irq(XFER_BUDGET_MS - spent) } else { IrqWait::NoLine } {
                IrqWait::Fired | IrqWait::TimedOut => {}
                IrqWait::NoLine => h.relax(),
            }
        }
        if irq_mode {
            // mask first, then re-arm: the line is a level, and the cause is still latched until the controller is disabled
            h.wr(off::INTR_MASK, 0);
            h.ack_irq();
        }
        // a raised abort that arrived with the last poll still wins over a "complete" reading
        if result.is_ok() && h.rd(off::RAW_INTR_STAT) & bits::INTR_TX_ABRT != 0 {
            result = Err(self.abort_fault(h));
        }
        let off_result = self.disable(h);
        result.and(off_result)
    }

    pub fn write<H: I2cHost>(&mut self, h: &mut H, addr: u16, data: &[u8]) -> Result<(), I2cFault> {
        let m = [Message { addr, flags: 0, len: data.len() }];
        self.xfer(h, &m, &[data], &mut [])
    }

    pub fn write_read<H: I2cHost>(&mut self, h: &mut H, addr: u16, wr: &[u8], rd: &mut [u8]) -> Result<(), I2cFault> {
        let m = [Message { addr, flags: 0, len: wr.len() }, Message { addr, flags: I2C_M_RD, len: rd.len() }];
        self.xfer(h, &m, &[wr, &[]], rd)
    }
}
