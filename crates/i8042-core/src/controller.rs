// SPDX-License-Identifier: GPL-2.0-only
//! The i8042 controller: bounded waits, flush, command execution, self-test, CTR init, port enable.
//!
//! Ported from Linux `drivers/input/serio/i8042.c` (`i8042_wait_read`, `i8042_wait_write`, `i8042_flush`,
//! `__i8042_command`, `i8042_kbd_write`, `i8042_controller_check`, `i8042_controller_selftest`,
//! `i8042_controller_init`, `i8042_enable_kbd_port`, the port-disable half of `i8042_controller_reset`).
//! Copyright (c) 1999-2004 Vojtech Pavlik.
//!
//! The one addition is [`Controller::kbd_test`]: Linux defines `I8042_CMD_KBD_TEST` (i8042.h:19) but only
//! ever runs the AUX twin; its answer is read with the AUX test's documented codes (i8042.c:827-836).

use crate::regs::*;
use crate::Port;

/// A refusal, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fail {
    /// The flush read more than [`BUFFER_SIZE`] bytes: nothing answers at the status register.
    NoController { flushed: usize },
    /// OBF never rose within [`CTL_TIMEOUT`] polls.
    WaitReadTimeout,
    /// IBF never cleared within [`CTL_TIMEOUT`] polls.
    WaitWriteTimeout,
    /// [`CTR_READS_MAX`] CTR reads never produced two equal values.
    CtrUnstable { last: [u8; 2] },
}

impl Fail {
    pub const fn name(self) -> &'static str {
        match self {
            Fail::NoController { .. } => "NO-CONTROLLER",
            Fail::WaitReadTimeout => "WAIT-READ-TIMEOUT",
            Fail::WaitWriteTimeout => "WAIT-WRITE-TIMEOUT",
            Fail::CtrUnstable { .. } => "CTR-UNSTABLE",
        }
    }
}

/// A controller or interface test's verdict. `Tolerated` is Linux's x86 posture (i8042.c:965-973): a
/// wrong answer is NAMED and the bring-up continues "in hopes that keyboard port will still be
/// functional"; a timeout is a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestVerdict {
    Ok,
    Tolerated { answer: u8 },
}

/// What `i8042_controller_init` learned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CtrFacts {
    /// The CTR as the firmware left it (Linux keeps it to restore on unload).
    pub initial_ctr: u8,
    /// Translation is on: the keyboard's set 2 reaches the host as set 1. Linux keeps the firmware's
    /// choice unless `i8042.direct` (i8042.c:1033-1050); so does this port.
    pub translated: bool,
    /// The keylock is engaged (STR_KEYLOCK clear). Linux warns unless `i8042.unlock`.
    pub keylock: bool,
}

/// Which port a byte in the output buffer came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Kbd,
    Aux,
}

pub struct Controller<P: Port> {
    pub io: P,
    /// The driver's image of the CTR (Linux `i8042_ctr`).
    pub ctr: u8,
}

impl<P: Port> Controller<P> {
    pub const fn new(io: P) -> Self {
        Controller { io, ctr: 0 }
    }

    pub fn read_status(&mut self) -> u8 {
        self.io.inb(STATUS_REG)
    }

    pub fn read_data(&mut self) -> u8 {
        self.io.inb(DATA_REG)
    }

    /// `i8042_wait_read` (i8042.c:230-239).
    pub fn wait_read(&mut self) -> Result<(), Fail> {
        let mut i = 0;
        while (!self.read_status() & STR_OBF != 0) && i < CTL_TIMEOUT {
            self.io.udelay(CTL_POLL_US);
            i += 1;
        }
        if i == CTL_TIMEOUT { Err(Fail::WaitReadTimeout) } else { Ok(()) }
    }

    /// `i8042_wait_write` (i8042.c:241-250).
    pub fn wait_write(&mut self) -> Result<(), Fail> {
        let mut i = 0;
        while (self.read_status() & STR_IBF != 0) && i < CTL_TIMEOUT {
            self.io.udelay(CTL_POLL_US);
            i += 1;
        }
        if i == CTL_TIMEOUT { Err(Fail::WaitWriteTimeout) } else { Ok(()) }
    }

    /// `i8042_flush` (i8042.c:257-275): returns the bytes discarded.
    pub fn flush(&mut self) -> Result<usize, Fail> {
        let mut count = 0usize;
        while self.read_status() & STR_OBF != 0 {
            if count >= BUFFER_SIZE {
                return Err(Fail::NoController { flushed: count });
            }
            count += 1;
            self.io.udelay(CTL_POLL_US);
            let _ = self.read_data();
        }
        Ok(count)
    }

    /// `__i8042_command` (i8042.c:285-326): parameters from `param`, replies into it, both counted by the
    /// command word. (The AUX_LOOP special cases are not ported: this port never issues AUX_LOOP.)
    pub fn command(&mut self, param: &mut [u8], command: u16) -> Result<(), Fail> {
        self.wait_write()?;
        self.io.outb(COMMAND_REG, command as u8);
        for i in 0..((command >> 12) & 0xf) as usize {
            self.wait_write()?;
            self.io.outb(DATA_REG, param[i]);
        }
        for i in 0..((command >> 8) & 0xf) as usize {
            self.wait_read()?;
            param[i] = self.read_data();
        }
        Ok(())
    }

    /// `i8042_kbd_write` (i8042.c:344-357): a byte for the keyboard device.
    pub fn kbd_write(&mut self, c: u8) -> Result<(), Fail> {
        self.wait_write()?;
        self.io.outb(DATA_REG, c);
        Ok(())
    }

    /// `i8042_controller_check` (i8042.c:931-939).
    pub fn controller_check(&mut self) -> Result<usize, Fail> {
        self.flush()
    }

    /// `i8042_controller_selftest`, x86 build (i8042.c:941-978): CTL_TEST until 0x55, at most six times
    /// 50 ms apart; a timeout refuses, a persistent wrong answer is tolerated by name.
    pub fn selftest(&mut self) -> Result<TestVerdict, Fail> {
        let mut i = 0;
        let mut last;
        loop {
            let mut p = [0u8; 1];
            self.command(&mut p, CMD_CTL_TEST)?;
            if p[0] == RET_CTL_TEST {
                return Ok(TestVerdict::Ok);
            }
            last = p[0];
            self.msleep(SELFTEST_RETRY_MS);
            let again = i < SELFTEST_RETRIES;
            i += 1;
            if !again {
                break;
            }
        }
        Ok(TestVerdict::Tolerated { answer: last })
    }

    /// The keyboard INTERFACE test (KBD_TEST, i8042.h:19), read with the AUX test's codes
    /// (i8042.c:829-830): 0x00, or 0xfa on notebooks that ignore the spec, is a pass; anything else is
    /// tolerated by name, as the AUX path tolerates "chipsets [that] return error on perfectly
    /// functioning" ports (i8042.c:831-833).
    pub fn kbd_test(&mut self) -> Result<TestVerdict, Fail> {
        let mut p = [0u8; 1];
        self.command(&mut p, CMD_KBD_TEST)?;
        Ok(match p[0] {
            PORT_TEST_OK | PORT_TEST_OK_NOTEBOOK => TestVerdict::Ok,
            answer => TestVerdict::Tolerated { answer },
        })
    }

    /// `i8042_controller_init` (i8042.c:986-1067), with `i8042_direct` and `i8042_unlock` false: read the
    /// CTR until two reads agree, disable the keyboard interface and interrupt, note the keylock, keep the
    /// firmware's translation mode, write the CTR back, flush.
    pub fn init(&mut self) -> Result<CtrFacts, Fail> {
        let mut ctr = [0u8; 2];
        let mut n = 0u32;
        loop {
            if n >= CTR_READS_MAX {
                return Err(Fail::CtrUnstable { last: ctr });
            }
            if n != 0 {
                self.io.udelay(CTR_READ_RETRY_US);
            }
            let mut p = [0u8; 1];
            self.command(&mut p, CMD_CTL_RCTR)?;
            ctr[(n % 2) as usize] = p[0];
            n += 1;
            if n >= 2 && ctr[0] == ctr[1] {
                break;
            }
        }
        let initial_ctr = ctr[0];
        self.ctr = initial_ctr;
        self.ctr |= CTR_KBDDIS; // i8042.c:1017
        self.ctr &= !CTR_KBDINT; // i8042.c:1018
        let keylock = !self.read_status() & STR_KEYLOCK != 0; // i8042.c:1025
        let translated = self.ctr & CTR_XLATE != 0; // i8042.c:1038-1039
        self.write_ctr()?;
        let _ = self.flush(); // i8042.c:1064
        Ok(CtrFacts { initial_ctr, translated, keylock })
    }

    /// CTL_WCTR with the image.
    pub fn write_ctr(&mut self) -> Result<(), Fail> {
        let mut p = [self.ctr];
        self.command(&mut p, CMD_CTL_WCTR)
    }

    /// The AUX half of `i8042_controller_reset`'s port disable (i8042.c:1082-1083), for a keyboard-only
    /// host (`i8042.noaux`): no mouse byte can then occupy the output buffer the keyboard shares.
    pub fn disable_aux_port(&mut self) -> Result<(), Fail> {
        self.ctr |= CTR_AUXDIS;
        self.ctr &= !CTR_AUXINT;
        self.write_ctr()
    }

    /// `i8042_enable_kbd_port` (i8042.c:617-631): on a failed write the image is restored.
    pub fn enable_kbd_port(&mut self) -> Result<(), Fail> {
        self.ctr &= !CTR_KBDDIS;
        self.ctr |= CTR_KBDINT;
        if let Err(e) = self.write_ctr() {
            self.ctr &= !CTR_KBDINT;
            self.ctr |= CTR_KBDDIS;
            return Err(e);
        }
        Ok(())
    }

    /// One byte from the output buffer, if any, with the port it came from (`i8042_handle_data`'s read,
    /// i8042.c:566-575 — `str & I8042_STR_AUXDATA` decides the port).
    pub fn read_byte(&mut self) -> Option<(u8, Source)> {
        let st = self.read_status();
        if st & STR_OBF == 0 {
            return None;
        }
        let data = self.read_data();
        Some((data, if st & STR_AUXDATA != 0 { Source::Aux } else { Source::Kbd }))
    }

    /// `msleep(ms)` as udelay steps.
    pub fn msleep(&mut self, ms: u32) {
        for _ in 0..ms {
            self.io.udelay(1000);
        }
    }
}
