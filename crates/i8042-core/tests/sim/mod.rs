// SPDX-License-Identifier: GPL-2.0-only
//! A scripted i8042 + AT keyboard for the vectors: the controller's status / CTR / output buffer and
//! commands (RCTR, WCTR, CTL_TEST, KBD_TEST), the keyboard's command protocol (ACK/NAK, GETID, SETLEDS,
//! SETREP, SSCANSET, ENABLE, RESET_DIS, RESET_BAT), and the controller's set 2 -> set 1 translation when
//! CTR_XLATE is set. Knobs make every named refusal reachable.
#![allow(dead_code)]

use i8042_core::regs::*;
use i8042_core::Port;
use std::collections::VecDeque;

/// Set 1 make code for each set 2 code a 101-key keyboard emits (the inverse of atkbd.c:125-134's table,
/// written out for the codes the vectors type).
pub fn set2_to_set1(code: u8) -> u8 {
    match code {
        0x1c => 0x1e, // a
        0x32 => 0x30, // b
        0x24 => 0x12, // e
        0x33 => 0x23, // h
        0x43 => 0x17, // i
        0x4b => 0x26, // l
        0x44 => 0x18, // o
        0x2d => 0x13, // r
        0x16 => 0x02, // 1
        0x29 => 0x39, // space
        0x5a => 0x1c, // enter
        0x76 => 0x01, // esc
        0x66 => 0x0e, // backspace
        0x12 => 0x2a, // left shift
        0x59 => 0x36, // right shift
        0x14 => 0x1d, // left ctrl (E0 14 = right ctrl)
        0x11 => 0x38, // left alt (E0 11 = right alt)
        0x1f => 0x5b, // E0 1F = left meta
        0x75 => 0x48, // E0 75 = up
        0x6b => 0x4b, // E0 6B = left
        0x74 => 0x4d, // E0 74 = right
        0x72 => 0x50, // E0 72 = down
        0x6c => 0x47, // E0 6C = home
        0x69 => 0x4f, // E0 69 = end
        0x05 => 0x3b, // F1
        0x06 => 0x3c, // F2
        0x09 => 0x44, // F10
        0x83 => 0x41, // F7
        0x77 => 0x45, // E1 14 77 = pause
        other => panic!("sim: no set-1 code written out for set-2 {other:#04x}"),
    }
}

pub struct Sim {
    pub absent: bool,
    pub ibf_stuck: bool,
    pub selftest_answer: Option<u8>,
    pub kbd_test_answer: u8,
    pub ctr_unstable: bool,
    pub keylock_active: bool,
    /// The keyboard never answers anything.
    pub no_keyboard: bool,
    /// NAK every keyboard byte this many times before ACKing it.
    pub naks_before_ack: u32,
    /// After a NAK, send 0xfc (the device refuses the byte).
    pub err_after_nak: bool,
    /// The keyboard sends this byte forever instead of answering.
    pub chatter: Option<u8>,
    /// GETID's reply bytes after the ACK; empty = no reply.
    pub getid_reply: Vec<u8>,
    /// RESET_BAT's reply after the ACK; empty = no BAT.
    pub reset_reply: Vec<u8>,
    /// Scancodes the keyboard emits just before it ACKs the next SETLEDS (the PASS_NOACK path).
    pub before_ack: Vec<u8>,
    pub ctr: u8,
    pub out: VecDeque<(u8, bool)>,
    expect_ctr: bool,
    pending: Option<u8>,
    naked: u32,
    pub scanning: bool,
    pub leds: u8,
    pub repeat: u8,
    pub set: u8,
    pub kbd_log: Vec<u8>,
    pub cmd_log: Vec<u8>,
    pub elapsed_us: u64,
    ctr_reads: u32,
}

impl Sim {
    /// A healthy q35-shaped controller: translation on, keyboard interrupt on, keyboard answering 0xab83.
    pub fn new() -> Self {
        Sim {
            absent: false,
            ibf_stuck: false,
            selftest_answer: Some(0x55),
            kbd_test_answer: 0x00,
            ctr_unstable: false,
            keylock_active: false,
            no_keyboard: false,
            naks_before_ack: 0,
            err_after_nak: false,
            chatter: None,
            getid_reply: vec![0xab, 0x83],
            reset_reply: vec![0xaa],
            before_ack: Vec::new(),
            ctr: CTR_XLATE | 0x04 | CTR_KBDINT,
            out: VecDeque::new(),
            expect_ctr: false,
            pending: None,
            naked: 0,
            scanning: true,
            leds: 0xff,
            repeat: 0xff,
            set: 2,
            kbd_log: Vec::new(),
            cmd_log: Vec::new(),
            elapsed_us: 0,
            ctr_reads: 0,
        }
    }

    fn push(&mut self, b: u8) {
        self.out.push_back((b, false));
    }

    pub fn aux(&mut self, b: u8) {
        self.out.push_back((b, true));
    }

    /// The keyboard types set-2 bytes; with CTR_XLATE the controller hands the host set 1, folding F0 into
    /// bit 7 of the next code and passing E0/E1 through.
    pub fn type_set2(&mut self, bytes: &[u8]) {
        if !self.scanning {
            return;
        }
        if self.ctr & CTR_XLATE == 0 {
            for &b in bytes {
                self.push(b);
            }
            return;
        }
        let mut release = false;
        for &b in bytes {
            match b {
                0xf0 => release = true,
                0xe0 | 0xe1 => self.push(b),
                _ => {
                    let s1 = set2_to_set1(b) | if release { 0x80 } else { 0 };
                    self.push(s1);
                    release = false;
                }
            }
        }
    }

    fn ack(&mut self) -> bool {
        if self.naked < self.naks_before_ack {
            self.naked += 1;
            self.push(0xfe);
            if self.err_after_nak {
                self.push(0xfc);
            }
            return false;
        }
        self.naked = 0;
        self.push(0xfa);
        true
    }

    fn kbd_byte(&mut self, v: u8) {
        self.kbd_log.push(v);
        if self.no_keyboard || self.chatter.is_some() {
            return;
        }
        if let Some(cmd) = self.pending {
            if !self.ack() {
                return;
            }
            self.pending = None;
            match cmd {
                0xed => self.leds = v,
                0xf3 => self.repeat = v,
                0xf0 => self.set = v,
                _ => {}
            }
            return;
        }
        if v == 0xed && !self.before_ack.is_empty() {
            let pre = std::mem::take(&mut self.before_ack);
            for b in pre {
                self.push(b);
            }
        }
        if !self.ack() {
            return;
        }
        match v {
            0xf2 => {
                let r = self.getid_reply.clone();
                for b in r {
                    self.push(b);
                }
            }
            0xff => {
                self.scanning = true;
                let r = self.reset_reply.clone();
                for b in r {
                    self.push(b);
                }
            }
            0xf4 => self.scanning = true,
            0xf5 => self.scanning = false,
            0xed | 0xf3 | 0xf0 => self.pending = Some(v),
            _ => {}
        }
    }
}

impl Port for Sim {
    fn inb(&mut self, reg: u16) -> u8 {
        if self.absent {
            return 0xff;
        }
        match reg {
            STATUS_REG => {
                let mut s = 0x04;
                if !self.keylock_active {
                    s |= STR_KEYLOCK;
                }
                if self.ibf_stuck {
                    s |= STR_IBF;
                }
                if let Some(&(_, aux)) = self.out.front() {
                    s |= STR_OBF;
                    if aux {
                        s |= STR_AUXDATA;
                    }
                } else if self.chatter.is_some() && self.scanning {
                    s |= STR_OBF;
                }
                s
            }
            DATA_REG => match self.out.pop_front() {
                Some((b, _)) => b,
                None => self.chatter.unwrap_or(0),
            },
            _ => 0xff,
        }
    }

    fn outb(&mut self, reg: u16, v: u8) {
        if self.absent || self.ibf_stuck {
            return;
        }
        match reg {
            COMMAND_REG => {
                self.cmd_log.push(v);
                match v {
                    0x20 => {
                        self.ctr_reads += 1;
                        let c = if self.ctr_unstable && self.ctr_reads % 2 == 0 { self.ctr ^ 0x01 } else { self.ctr };
                        self.push(c);
                    }
                    0x60 => self.expect_ctr = true,
                    0xaa => {
                        if let Some(a) = self.selftest_answer {
                            self.push(a);
                        }
                    }
                    0xab => {
                        let a = self.kbd_test_answer;
                        self.push(a);
                    }
                    _ => {}
                }
            }
            DATA_REG => {
                if self.expect_ctr {
                    self.expect_ctr = false;
                    self.ctr = v;
                } else {
                    self.kbd_byte(v);
                }
            }
            _ => {}
        }
    }

    fn udelay(&mut self, us: u32) {
        self.elapsed_us += us as u64;
    }
}
