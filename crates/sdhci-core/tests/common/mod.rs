// SPDX-License-Identifier: GPL-2.0-or-later

//! Shared test doubles for sdhci-core.
//!
//! ONE home for the mocks. They started as dead copies inside tests/recovery.rs — constructed by a
//! helper that never used them, which is how four of that file's five compile errors arose. They
//! live here now because tests/executor.rs actually drives them, and a second copy would be the
//! duplication the doctrine forbids.
//!
//! `RecordingRegs` differs from a plain register file in the one way that matters for the executor:
//! it keeps an ORDERED LOG of writes. The reducer vectors assert emitted `Action`s; only a write
//! log can assert that those Actions reach the BUS, in order, with the right widths.

use sdhci_core::executor::{Bus, Time};

/// One observed register write: (register, value, access width in bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Write {
    pub reg: u16,
    pub val: u32,
    pub width: u8,
}

pub struct RecordingRegs {
    regs: [u32; 0x100],
    pub writes: Vec<Write>,
    pub reads: Vec<u16>,
    /// Model SOFTWARE_RESET as a STUCK controller: the reset bits stay set forever.
    /// Default false = the hardware completes the reset immediately.
    pub hold_reset_bits: bool,
}

impl Default for RecordingRegs {
    fn default() -> Self {
        RecordingRegs {
            regs: [0; 0x100],
            writes: Vec::new(),
            reads: Vec::new(),
            hold_reset_bits: false,
        }
    }
}

impl RecordingRegs {
    /// The writes to one register, in order — the usual assertion shape.
    pub fn writes_to(&self, reg: u16) -> Vec<u32> {
        self.writes.iter().filter(|w| w.reg == reg).map(|w| w.val).collect()
    }
}

impl Bus for RecordingRegs {
    fn r8(&mut self, reg: u16) -> u8 {
        self.reads.push(reg);
        (self.regs[reg as usize] & 0xFF) as u8
    }
    fn r16(&mut self, reg: u16) -> u16 {
        self.reads.push(reg);
        (self.regs[reg as usize] & 0xFFFF) as u16
    }
    fn r32(&mut self, reg: u16) -> u32 {
        self.reads.push(reg);
        self.regs[reg as usize]
    }
    fn w8(&mut self, reg: u16, val: u8) {
        self.writes.push(Write { reg, val: val as u32, width: 8 });
        // SOFTWARE_RESET IS SELF-CLEARING. Linux's sdhci_reset() polls it with the comment "hw
        // clears the bit when it's done" (sdhci.c:217-234) — software sets the bit, the controller
        // performs the reset and clears it. A mock that simply stores the value models a
        // PERMANENTLY STUCK controller, and the reducer then correctly polls forever. That is not
        // a hypothetical: the first executor vector written against a plain store-everything mock
        // read SOFTWARE_RESET 16 times and never reached the DATA reset, which reads exactly like
        // an executor defect and is not one. Keep the distinction available rather than hidden:
        // hold_reset_bits = true is the STUCK controller, on purpose, for the ResetStuck path.
        if reg == 0x2F && !self.hold_reset_bits {
            self.regs[reg as usize] = 0;
            return;
        }
        self.regs[reg as usize] = (self.regs[reg as usize] & !0xFF) | val as u32;
    }
    fn w16(&mut self, reg: u16, val: u16) {
        self.writes.push(Write { reg, val: val as u32, width: 16 });
        self.regs[reg as usize] = (self.regs[reg as usize] & !0xFFFF) | val as u32;
    }
    fn w32(&mut self, reg: u16, val: u32) {
        self.writes.push(Write { reg, val, width: 32 });
        self.regs[reg as usize] = val;
    }
}

/// A time source that never expires unless a test says so. Deliberately NOT a real clock: a
/// deadline that expires on its own would make these vectors timing-dependent, which is the exact
/// property that makes a witness unreliable.
pub struct MockTime {
    pub now: u64,
}

impl Default for MockTime {
    fn default() -> Self {
        MockTime { now: 0 }
    }
}

impl Time for MockTime {
    type Deadline = u64;
    fn deadline_after_ms(&mut self, ms: u64) -> u64 {
        self.now + ms
    }
    fn expired(&mut self, deadline: u64) -> bool {
        self.now > deadline
    }
    fn delay_us(&mut self, us: u32) {
        self.now += us as u64 / 1000;
    }
    fn park_ms(&mut self, ms: u64) {
        self.now += ms;
    }
}
