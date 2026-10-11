// SPDX-License-Identifier: GPL-2.0-only
//! Sending a command to a PS/2 device and collecting its reply — Linux `drivers/input/serio/libps2.c`.
//!
//! Linux runs this half in two contexts: `__ps2_command` sleeps in `wait_event_timeout` while
//! `ps2_interrupt` feeds it bytes from the IRQ. A polled host has one context, so [`Ps2Dev::wait`]
//! reads the controller's output buffer itself and hands each keyboard byte to the SAME routing
//! `ps2_interrupt` does (libps2.c:610-617; atkbd's pre-receive hook is `PS2_PROCESS` on x86,
//! atkbd.c:434-439). Bytes from the AUX port belong to another serio and are dropped.
//! Copyright (c) 1999-2002 Vojtech Pavlik; Copyright (c) 2004 Dmitry Torokhov.

use crate::controller::{Controller, Fail, Source};
use crate::regs::CTL_POLL_US;
use crate::Port;

pub const CMD_EX_SETLEDS: u16 = 0x20eb; // libps2.c:25
pub const CMD_SETLEDS: u16 = 0x10ed; // libps2.c:26
pub const CMD_GETID: u16 = 0x02f2; // libps2.c:27
pub const CMD_SETREP: u16 = 0x10f3; // libps2.c:28
pub const CMD_RESET_BAT: u16 = 0x02ff; // libps2.c:29

pub const RET_BAT: u8 = 0xaa; // libps2.c:31
pub const RET_ID: u8 = 0x00; // libps2.c:32
pub const RET_ACK: u8 = 0xfa; // libps2.c:33
pub const RET_NAK: u8 = 0xfe; // libps2.c:34
pub const RET_ERR: u8 = 0xfc; // libps2.c:35

pub const FLAG_ACK: u8 = 1 << 0; // libps2.c:37
pub const FLAG_CMD: u8 = 1 << 1; // libps2.c:38
pub const FLAG_CMD1: u8 = 1 << 2; // libps2.c:39
pub const FLAG_WAITID: u8 = 1 << 3; // libps2.c:40
pub const FLAG_NAK: u8 = 1 << 4; // libps2.c:41
pub const FLAG_PASS_NOACK: u8 = 1 << 5; // libps2.c:42

/// The ACK wait for a command byte; RESET_BAT gets longer because "Some devices (Synaptics) perform the
/// reset before ACKing the reset command".
pub const ACK_TIMEOUT_MS: u32 = 200; // libps2.c:326
pub const RESET_ACK_TIMEOUT_MS: u32 = 1000; // libps2.c:326
/// The ACK wait for each parameter byte.
pub const PARAM_ACK_TIMEOUT_MS: u32 = 200; // libps2.c:334
/// Sends per byte while the device NAKs.
pub const SEND_ATTEMPTS: u32 = 2; // libps2.c:328
/// The wait for the reply's first byte; "The reset command takes a long time to execute."
pub const REPLY_TIMEOUT_MS: u32 = 500; // libps2.c:344
pub const RESET_REPLY_TIMEOUT_MS: u32 = 4000; // libps2.c:344
/// After RESET_BAT's first reply byte, the rest "will come soon (keyboard) or not at all (mouse)".
pub const RESET_TAIL_TIMEOUT_MS: u32 = 100; // libps2.c:214-215
/// The GETID first byte a Microsoft Natural Elite answers "as it were a mouse".
pub const GETID_MOUSE_LIKE: u8 = 0xaa; // libps2.c:225
/// `cmdbuf` capacity (`u8 cmdbuf[8]` in struct ps2dev).
pub const CMDBUF_LEN: usize = 8; // libps2.h:53

/// The first GETID bytes of keyboards Linux knows.
pub const KEYBOARD_IDS: [u8; 6] = [0xab, 0xac, 0x2b, 0x5d, 0x60, 0x47]; // libps2.c:185-190

/// `ps2_is_keyboard_id` (libps2.c:182-194).
pub fn is_keyboard_id(id_byte: u8) -> bool {
    KEYBOARD_IDS.contains(&id_byte)
}

/// A refusal, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2Error {
    /// The controller timed out under the byte (serio_write failed).
    Controller(Fail),
    /// The device NAKed every attempt (Linux -EAGAIN, reported to callers as -EPROTO).
    Nak { byte: u8 },
    /// NAK then 0xfc: the device refuses the byte (Linux -EPROTO).
    DeviceError { byte: u8 },
    /// Neither ACK nor NAK arrived in time (Linux -EIO, `nak` still 1).
    NoAck { byte: u8 },
    /// The reply came up short: `missing` bytes never arrived (Linux -EPROTO).
    ShortReply { command: u16, missing: usize },
    /// More reply bytes than `cmdbuf` holds, or a parameter buffer too short (Linux -EINVAL + WARN_ON).
    BadCommand { command: u16 },
}

impl Ps2Error {
    pub const fn name(self) -> &'static str {
        match self {
            Ps2Error::Controller(f) => f.name(),
            Ps2Error::Nak { .. } => "PS2-NAK",
            Ps2Error::DeviceError { .. } => "PS2-DEVICE-ERROR",
            Ps2Error::NoAck { .. } => "PS2-NO-ACK",
            Ps2Error::ShortReply { .. } => "PS2-SHORT-REPLY",
            Ps2Error::BadCommand { .. } => "PS2-BAD-COMMAND",
        }
    }
}

/// `struct ps2dev`'s command state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ps2Dev {
    pub flags: u8,
    pub nak: u8,
    pub cmdbuf: [u8; CMDBUF_LEN],
    pub cmdcnt: usize,
}

impl Default for Ps2Dev {
    fn default() -> Self {
        Self::new()
    }
}

impl Ps2Dev {
    pub const fn new() -> Self {
        Ps2Dev { flags: 0, nak: 0, cmdbuf: [0; CMDBUF_LEN], cmdcnt: 0 }
    }

    /// `ps2_interrupt`'s PS2_PROCESS routing (libps2.c:610-617).
    pub fn interrupt(&mut self, data: u8, rx: &mut dyn FnMut(u8)) {
        if self.flags & FLAG_ACK != 0 {
            self.handle_ack(data, rx);
        } else if self.flags & FLAG_CMD != 0 {
            self.handle_response(data);
        } else {
            rx(data);
        }
    }

    /// `ps2_handle_response` (libps2.c:482-497).
    fn handle_response(&mut self, data: u8) {
        if self.cmdcnt != 0 {
            self.cmdcnt -= 1;
            self.cmdbuf[self.cmdcnt] = data;
        }
        if self.flags & FLAG_CMD1 != 0 {
            self.flags &= !FLAG_CMD1;
        }
        if self.cmdcnt == 0 {
            self.flags &= !FLAG_CMD;
        }
    }

    /// `ps2_handle_ack` (libps2.c:504-566).
    fn handle_ack(&mut self, data: u8, rx: &mut dyn FnMut(u8)) {
        let mut fall = false;
        match data {
            RET_ACK => self.nak = 0,
            RET_NAK => {
                self.flags |= FLAG_NAK;
                self.nak = RET_NAK;
            }
            RET_ERR if self.flags & FLAG_NAK != 0 => {
                self.flags &= !FLAG_NAK;
                self.nak = RET_ERR;
            }
            _ => fall = true,
        }
        // case 0x00 / 0x03 / 0x04 (and an ERR without a NAK before it): mice that do not ACK GETID.
        if fall {
            if matches!(data, RET_ERR | 0x00 | 0x03 | 0x04) && self.flags & FLAG_WAITID != 0 {
                self.nak = 0;
            } else {
                // "unexpected" — not an ACK; for SETLEDS/SETREP it goes to the receive handler.
                if self.flags & FLAG_PASS_NOACK != 0 {
                    rx(data);
                }
                self.flags &= !(FLAG_WAITID | FLAG_PASS_NOACK);
                return;
            }
        }
        if self.nak == 0 {
            self.flags &= !FLAG_NAK;
        }
        self.flags &= !FLAG_ACK;
        if self.nak == 0 && data != RET_ACK {
            self.handle_response(data);
        }
    }

    /// `wait_event_timeout` over a polled controller: route every keyboard byte until `done` holds.
    /// Returns the polls LEFT (0 = timed out), the unit `ps2_adjust_timeout` shortens.
    fn wait<P: Port>(
        &mut self,
        c: &mut Controller<P>,
        polls: u32,
        rx: &mut dyn FnMut(u8),
        done: fn(&Ps2Dev) -> bool,
    ) -> u32 {
        let mut left = polls;
        while !done(self) {
            if left == 0 {
                return 0;
            }
            // Every iteration spends one poll, a byte or not: a device that streams forever (Linux bounds
            // the wait by wall time) cannot hold the wait open.
            left -= 1;
            match c.read_byte() {
                Some((b, Source::Kbd)) => self.interrupt(b, rx),
                Some((_, Source::Aux)) => {}
                None => c.io.udelay(CTL_POLL_US),
            }
        }
        left.max(1)
    }

    /// `ps2_do_sendbyte` (libps2.c:44-95).
    fn sendbyte<P: Port>(
        &mut self,
        c: &mut Controller<P>,
        byte: u8,
        timeout_ms: u32,
        max_attempts: u32,
        rx: &mut dyn FnMut(u8),
    ) -> Result<(), Ps2Error> {
        let mut attempt = 0;
        loop {
            self.nak = 1;
            self.flags |= FLAG_ACK;
            if let Err(e) = c.kbd_write(byte) {
                self.flags &= !FLAG_ACK;
                return Err(Ps2Error::Controller(e));
            }
            self.wait(c, polls(timeout_ms), rx, |d| d.flags & FLAG_ACK == 0);
            attempt += 1;
            if !(self.nak == RET_NAK && attempt < max_attempts) {
                break;
            }
        }
        self.flags &= !FLAG_ACK;
        match self.nak {
            0 => Ok(()),
            RET_NAK => Err(Ps2Error::Nak { byte }),
            RET_ERR => Err(Ps2Error::DeviceError { byte }),
            _ => Err(Ps2Error::NoAck { byte }),
        }
    }

    /// `ps2_adjust_timeout` (libps2.c:202-249).
    fn adjust_timeout(&mut self, command: u16, left: u32) -> u32 {
        match command {
            CMD_RESET_BAT => left.min(polls(RESET_TAIL_TIMEOUT_MS)),
            CMD_GETID => {
                let mut left = left;
                if self.cmdbuf[1] == GETID_MOUSE_LIKE {
                    self.flags = 0;
                    left = 0;
                }
                if !is_keyboard_id(self.cmdbuf[1]) {
                    self.flags = 0;
                    self.cmdcnt = 0;
                    left = 0;
                }
                left
            }
            _ => left,
        }
    }

    /// `__ps2_command` (libps2.c:264-387): send the command byte and its parameters, each ACKed, then
    /// collect the reply into `param`. `rx` receives scancodes that arrive while the command runs.
    pub fn command<P: Port>(
        &mut self,
        c: &mut Controller<P>,
        param: &mut [u8],
        command: u16,
        rx: &mut dyn FnMut(u8),
    ) -> Result<(), Ps2Error> {
        let send = ((command >> 12) & 0xf) as usize;
        let receive = ((command >> 8) & 0xf) as usize;
        if receive > CMDBUF_LEN || param.len() < send.max(receive) {
            return Err(Ps2Error::BadCommand { command });
        }
        let mut send_param = [0u8; 16];
        send_param[..send].copy_from_slice(&param[..send]);
        self.cmdcnt = receive;
        self.flags = match command {
            CMD_GETID => FLAG_WAITID,
            CMD_SETLEDS | CMD_EX_SETLEDS | CMD_SETREP => FLAG_PASS_NOACK,
            _ => 0,
        };
        if receive != 0 {
            self.flags |= FLAG_CMD | FLAG_CMD1;
            for i in 0..receive {
                self.cmdbuf[(receive - 1) - i] = param[i];
            }
        }
        let r = self.command_body(c, param, command, &send_param[..send], receive, rx);
        self.flags = 0;
        r
    }

    fn command_body<P: Port>(
        &mut self,
        c: &mut Controller<P>,
        param: &mut [u8],
        command: u16,
        send_param: &[u8],
        receive: usize,
        rx: &mut dyn FnMut(u8),
    ) -> Result<(), Ps2Error> {
        let reset = command == CMD_RESET_BAT;
        let t = if reset { RESET_ACK_TIMEOUT_MS } else { ACK_TIMEOUT_MS };
        self.sendbyte(c, command as u8, t, SEND_ATTEMPTS, rx)?;
        for &p in send_param {
            self.sendbyte(c, p, PARAM_ACK_TIMEOUT_MS, SEND_ATTEMPTS, rx)?;
        }
        let t = if reset { RESET_REPLY_TIMEOUT_MS } else { REPLY_TIMEOUT_MS };
        let left = self.wait(c, polls(t), rx, |d| d.flags & FLAG_CMD1 == 0);
        if self.cmdcnt != 0 && self.flags & FLAG_CMD1 == 0 {
            let left = self.adjust_timeout(command, left);
            self.wait(c, left, rx, |d| d.flags & FLAG_CMD == 0);
        }
        for i in 0..receive {
            param[i] = self.cmdbuf[(receive - 1) - i];
        }
        if self.cmdcnt != 0 && (!reset || self.cmdcnt != 1) {
            return Err(Ps2Error::ShortReply { command, missing: self.cmdcnt });
        }
        Ok(())
    }
}

/// Milliseconds as [`CTL_POLL_US`] polls.
pub const fn polls(ms: u32) -> u32 {
    ms * (1000 / CTL_POLL_US)
}
