// SPDX-License-Identifier: GPL-2.0-only
//! How long the main loop waits for IRQ 1 before looking at the controller anyway.
//!
//! IRQ 1 is the normal path. Some boards' EC-emulated 8042 never raises it to the OS (the Babymega's did not reach the
//! kernel's handler either), and a lost edge on any board would otherwise hold a key until the next one. So the loop also
//! looks at the status register on a timeout: slowly while the IRQ works, and fast the first time a timeout turns up data
//! (the IRQ is not reaching us) — until the IRQ is seen working again.

pub const SLOW_MS: u64 = 250;
pub const FAST_MS: u64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// IRQ 1 delivers; the timeout is a safety net.
    Irq,
    /// IRQ 1 does not deliver (or is not held): the controller is polled.
    Polling,
}

#[derive(Clone, Copy, Debug)]
pub struct Pacer {
    mode: Mode,
}

impl Pacer {
    pub const fn new() -> Self {
        Pacer { mode: Mode::Irq }
    }
    pub const fn mode(&self) -> Mode {
        self.mode
    }
    pub const fn timeout_ms(&self) -> u64 {
        match self.mode {
            Mode::Irq => SLOW_MS,
            Mode::Polling => FAST_MS,
        }
    }
    /// The IRQ wait was refused (no capability): poll for good.
    pub fn irq_unavailable(&mut self) -> Option<Mode> {
        self.switch(Mode::Polling)
    }
    /// After a wake. `by_irq`: the line woke us (not the timeout); `bytes`: what the drain found. Returns the new mode
    /// when it changed.
    pub fn observe(&mut self, by_irq: bool, bytes: usize) -> Option<Mode> {
        match (by_irq, bytes) {
            (false, b) if b > 0 => self.switch(Mode::Polling),
            (true, _) => self.switch(Mode::Irq),
            _ => None,
        }
    }
    fn switch(&mut self, to: Mode) -> Option<Mode> {
        (self.mode != to).then(|| {
            self.mode = to;
            to
        })
    }
}

impl Default for Pacer {
    fn default() -> Self {
        Self::new()
    }
}
