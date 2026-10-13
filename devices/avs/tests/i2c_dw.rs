// SPDX-License-Identifier: GPL-2.0-only
//! The DesignWare master against a model of the controller and one target: what a transfer writes, what it reads back, how it fails.
use avs_device::allow::I2C_WRITE_REGS;
use avs_device::i2c::*;
use avs_device::machine::I2cFault;
use i2c_designware_core::regs::{bits, off};
use std::collections::HashMap;

struct Dw {
    regs: HashMap<u32, u32>,
    enabled: bool,
    cmds: Vec<u32>,
    rx: Vec<u8>,
    raw: u32,
    abort_src: u32,
    target_addr: u16,
    target: [u8; 256],
    ptr: Option<u8>,
    pub writes: Vec<(u32, u32)>,
    pub relaxes: u32,
    now: u64,
    // knobs
    comp_type: u32,
    never_stop: bool,
    always_busy: bool,
    data_nak: bool,
    // interrupt mode
    irq: bool,
    irq_waits: u32,
    acks: u32,
    /// STOP appears only after this many waits (None: as soon as the stop command lands)
    stop_after_waits: Option<u32>,
    pending_stop: bool,
}

impl Dw {
    fn new() -> Self {
        Self { regs: HashMap::new(), enabled: false, cmds: vec![], rx: vec![], raw: 0, abort_src: 0, target_addr: 0x1a, target: [0; 256], ptr: None, writes: vec![], relaxes: 0, now: 0, comp_type: bits::COMP_TYPE_VALUE, never_stop: false, always_busy: false, data_nak: false, irq: false, irq_waits: 0, acks: 0, stop_after_waits: None, pending_stop: false }
    }
    fn run(&mut self) {
        // a transfer is evaluated when its STOP command arrives
        let tar = *self.regs.get(&off::TAR).unwrap_or(&0) as u16;
        if self.cmds.last().is_none_or(|c| c & bits::DATA_CMD_STOP == 0) {
            return;
        }
        let cmds = std::mem::take(&mut self.cmds);
        if tar != self.target_addr {
            self.raw |= bits::INTR_TX_ABRT;
            self.abort_src = bits::TX_ABRT_7B_ADDR_NOACK;
            return;
        }
        if self.data_nak {
            self.raw |= bits::INTR_TX_ABRT;
            self.abort_src = bits::TX_ABRT_TXDATA_NOACK;
            return;
        }
        let mut first_write = true;
        for c in cmds {
            if c & bits::DATA_CMD_READ != 0 {
                let p = self.ptr.unwrap_or(0);
                self.rx.push(self.target[p as usize]);
                self.ptr = Some(p.wrapping_add(1));
                first_write = true;
            } else if first_write {
                self.ptr = Some(c as u8);
                first_write = false;
            } else {
                let p = self.ptr.unwrap_or(0);
                self.target[p as usize] = c as u8;
                self.ptr = Some(p.wrapping_add(1));
            }
        }
        if self.stop_after_waits.is_some() {
            self.pending_stop = true;
        } else if !self.never_stop {
            self.raw |= bits::INTR_STOP_DET;
        }
    }
}

impl I2cHost for Dw {
    fn rd(&mut self, o: u32) -> u32 {
        match o {
            off::COMP_TYPE => self.comp_type,
            off::COMP_PARAM_1 => (15 << 16) | (15 << 8),
            off::RAW_INTR_STAT => self.raw,
            off::TXFLR => 0,
            off::RXFLR => self.rx.len() as u32,
            off::DATA_CMD => {
                if self.rx.is_empty() {
                    0
                } else {
                    self.rx.remove(0) as u32
                }
            }
            off::TX_ABRT_SOURCE => self.abort_src,
            off::CLR_TX_ABRT => {
                self.raw &= !bits::INTR_TX_ABRT;
                self.abort_src = 0;
                0
            }
            off::CLR_INTR => {
                self.raw = 0;
                0
            }
            off::STATUS => {
                if self.always_busy {
                    bits::STATUS_MASTER_ACTIVITY
                } else {
                    0
                }
            }
            off::ENABLE => self.enabled as u32,
            off::ENABLE_STATUS => self.enabled as u32,
            o => *self.regs.get(&o).unwrap_or(&0),
        }
    }
    fn wr(&mut self, o: u32, v: u32) {
        self.writes.push((o, v));
        match o {
            off::ENABLE => {
                self.enabled = v & 1 != 0;
                if !self.enabled {
                    self.cmds.clear();
                }
            }
            off::DATA_CMD => {
                assert!(self.enabled, "a command written to a disabled controller");
                self.cmds.push(v);
                self.run();
            }
            o => {
                self.regs.insert(o, v);
            }
        }
    }
    fn relax(&mut self) {
        self.relaxes += 1;
        self.now += 1;
    }
    fn now_ms(&mut self) -> u64 {
        self.now
    }
    fn has_irq(&self) -> bool {
        self.irq
    }
    fn wait_irq(&mut self, timeout_ms: u64) -> IrqWait {
        if !self.irq {
            return IrqWait::NoLine;
        }
        self.irq_waits += 1;
        if let Some(n) = self.stop_after_waits {
            if self.irq_waits >= n && self.pending_stop {
                self.pending_stop = false;
                self.raw |= bits::INTR_STOP_DET;
            }
        }
        let mask = *self.regs.get(&off::INTR_MASK).unwrap_or(&0);
        if self.raw & mask != 0 {
            IrqWait::Fired
        } else {
            self.now += timeout_ms;
            IrqWait::TimedOut
        }
    }
    fn ack_irq(&mut self) {
        self.acks += 1;
    }
}

fn master() -> DwMaster {
    DwMaster::new(Timing { bus_hz: 400_000, fs_hcnt: 0x96, fs_lcnt: 0xe4, sda_hold: 0 })
}

#[test]
fn a_register_write_lands_in_the_target_and_the_controller_is_left_disabled() {
    let mut h = Dw::new();
    let mut m = master();
    assert_eq!(m.write(&mut h, 0x1a, &[0x20, 0xab, 0xcd]), Ok(()));
    assert_eq!(&h.target[0x20..0x22], &[0xab, 0xcd]);
    assert!(!h.enabled, "nothing carries into the next transfer");
    assert_eq!(h.regs[&off::TAR], 0x1a);
    // fast mode, restart enabled, slave disabled
    assert_eq!(h.regs[&off::CON], bits::CON_MASTER | bits::CON_SLAVE_DISABLE | bits::CON_RESTART_EN | bits::CON_SPEED_FAST);
    assert_eq!((h.regs[&off::FS_SCL_HCNT], h.regs[&off::FS_SCL_LCNT]), (0x96, 0xe4), "the board's timing was programmed");
    assert_eq!(h.regs[&off::INTR_MASK], 0, "polled: every interrupt masked");
}

#[test]
fn a_register_read_is_a_write_of_the_register_then_a_repeated_start_read() {
    let mut h = Dw::new();
    h.target[0x81] = 0x23;
    h.target[0x82] = 0x93;
    let mut m = master();
    let mut b = [0u8; 2];
    assert_eq!(m.write_read(&mut h, 0x1a, &[0x81], &mut b), Ok(()));
    assert_eq!(b, [0x23, 0x93]);
    // the command stream: the register address, then two read commands, the first with RESTART, the last with STOP
    let cmds: Vec<u32> = h.writes.iter().filter(|(o, _)| *o == off::DATA_CMD).map(|(_, v)| *v).collect();
    assert_eq!(cmds, vec![0x81, bits::DATA_CMD_RESTART | bits::DATA_CMD_READ, bits::DATA_CMD_STOP | bits::DATA_CMD_READ]);
}

#[test]
fn an_absent_target_is_a_nak_and_the_abort_source_was_captured_before_it_was_cleared() {
    let mut h = Dw::new();
    h.target_addr = 0x55;
    let mut m = master();
    assert_eq!(m.write(&mut h, 0x1a, &[1, 2]), Err(I2cFault::Nak));
    assert!(!h.enabled);
    assert_eq!(h.raw & bits::INTR_TX_ABRT, 0, "the abort was cleared");
    // a data NAK is a NAK too
    let mut h = Dw::new();
    h.data_nak = true;
    assert_eq!(master().write(&mut h, 0x1a, &[1, 2]), Err(I2cFault::Nak));
}

#[test]
fn the_next_transfer_after_a_nak_works() {
    let mut h = Dw::new();
    let mut m = master();
    h.target_addr = 0x55;
    assert_eq!(m.write(&mut h, 0x1a, &[1, 2]), Err(I2cFault::Nak));
    h.target_addr = 0x1a;
    assert_eq!(m.write(&mut h, 0x1a, &[0x10, 7]), Ok(()));
    assert_eq!(h.target[0x10], 7);
}

#[test]
fn a_window_that_is_not_a_designware_core_is_no_controller_and_nothing_is_written() {
    let mut h = Dw::new();
    h.comp_type = 0xffff_ffff;
    assert_eq!(master().write(&mut h, 0x1a, &[1, 2]), Err(I2cFault::NoController));
    assert!(h.writes.is_empty());
}

#[test]
fn a_stop_that_never_comes_and_a_bus_that_never_idles_both_time_out_inside_their_bounds() {
    let mut h = Dw::new();
    h.never_stop = true;
    let r = master().write(&mut h, 0x1a, &[1, 2]);
    assert_eq!(r, Err(I2cFault::Timeout));
    assert!(h.relaxes as u64 <= XFER_BUDGET_MS + 200, "{} relaxes", h.relaxes);
    assert!(!h.enabled, "disabled even after a time-out");
    let mut h = Dw::new();
    h.always_busy = true;
    assert_eq!(master().write(&mut h, 0x1a, &[1, 2]), Err(I2cFault::Timeout));
    assert!(h.relaxes as u64 <= XFER_BUDGET_MS + 2);
    assert!(h.writes.is_empty(), "a busy bus is never programmed");
}

#[test]
fn only_the_registers_on_the_write_list_are_ever_written() {
    let mut h = Dw::new();
    let mut m = master();
    m.write(&mut h, 0x1a, &[0x20, 1]).unwrap();
    let mut b = [0u8; 1];
    m.write_read(&mut h, 0x1a, &[0x20], &mut b).unwrap();
    h.target_addr = 0x55;
    let _ = m.write(&mut h, 0x1a, &[1]);
    for (o, _) in &h.writes {
        assert!(I2C_WRITE_REGS.contains(o), "register {o:#x} is written but not on the list");
    }
    // and every register on the list is used by the shapes above (SDA hold only when the board asks)
    let used: std::collections::BTreeSet<u32> = h.writes.iter().map(|w| w.0).collect();
    for r in I2C_WRITE_REGS {
        if r != off::SDA_HOLD {
            assert!(used.contains(&r), "{r:#x} is on the list and never written");
        }
    }
    let mut h = Dw::new();
    DwMaster::new(Timing { bus_hz: 400_000, fs_hcnt: 0, fs_lcnt: 0, sda_hold: 0x1_0001 }).write(&mut h, 0x1a, &[0x20, 1]).unwrap();
    assert_eq!(h.regs[&off::SDA_HOLD], 0x1_0001);
    assert!(!h.regs.contains_key(&off::FS_SCL_HCNT), "zero counts mean: leave what the firmware set");
}

#[test]
fn a_read_larger_than_the_buffer_is_refused_before_the_bus_is_touched() {
    let mut h = Dw::new();
    let mut m = master();
    use i2c_designware_master_core::setup::{Message, I2C_M_RD};
    let msgs = [Message { addr: 0x1a, flags: 0, len: 1 }, Message { addr: 0x1a, flags: I2C_M_RD, len: 4 }];
    let mut small = [0u8; 2];
    assert!(m.xfer(&mut h, &msgs, &[&[0x10], &[]], &mut small).is_err());
}

#[test]
fn with_an_interrupt_line_the_wait_for_stop_blocks_on_it_and_never_sleeps_and_polls() {
    let mut h = Dw::new();
    h.irq = true;
    h.stop_after_waits = Some(1);
    h.target[0x81] = 0x23;
    let mut m = master();
    let mut b = [0u8; 1];
    assert_eq!(m.write_read(&mut h, 0x1a, &[0x81], &mut b), Ok(()));
    assert_eq!(b, [0x23]);
    assert!(h.irq_waits >= 1, "it waited on the interrupt");
    assert_eq!(h.relaxes, 0, "and slept on nothing else");
    assert_eq!(h.acks, 1, "the line was re-armed once, after the cause was masked");
    assert_eq!(h.regs[&off::INTR_MASK], 0, "masked again afterwards");
    // the mask during the transfer was exactly the two causes a transfer ends with
    let masks: Vec<u32> = h.writes.iter().filter(|(o, _)| *o == off::INTR_MASK).map(|(_, v)| *v).collect();
    assert_eq!(masks, vec![0, bits::INTR_STOP_DET | bits::INTR_TX_ABRT, 0]);
}

#[test]
fn an_abort_wakes_the_wait_too_and_a_stop_that_never_comes_times_out_inside_the_budget_without_relaxing() {
    let mut h = Dw::new();
    h.irq = true;
    h.target_addr = 0x55;
    assert_eq!(master().write(&mut h, 0x1a, &[1, 2]), Err(I2cFault::Nak));
    let mut h = Dw::new();
    h.irq = true;
    h.never_stop = true;
    assert_eq!(master().write(&mut h, 0x1a, &[1, 2]), Err(I2cFault::Timeout));
    assert_eq!(h.relaxes, 0);
    assert!(h.now <= XFER_BUDGET_MS + 1, "the model's clock moved {} ms", h.now);
    assert!(h.irq_waits <= 3, "{} waits: each is as long as the budget has left, not a tick", h.irq_waits);
}
