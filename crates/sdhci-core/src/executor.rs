// SPDX-License-Identifier: GPL-2.0-or-later

//! Reference executor that turns `Action`s into bus/time calls and feeds
//! `ReadComplete` / `DeadlineExpired` events back into `Recovery::step`.
//!
//! The executor is `no_std`-compatible: all working state lives in
//! fixed-size stack arrays whose capacity is bounded by the maximum
//! number of concurrent actions the `Recovery` reducer can produce
//! in a single `step()` call.

use crate::core::*;
use crate::regs::*;

/// Bus abstraction: register-width read/write.
pub trait Bus {
    fn r8(&mut self, reg: u16) -> u8;
    fn r16(&mut self, reg: u16) -> u16;
    fn r32(&mut self, reg: u16) -> u32;
    fn w8(&mut self, reg: u16, val: u8);
    fn w16(&mut self, reg: u16, val: u16);
    fn w32(&mut self, reg: u16, val: u32);
}

/// Time abstraction: deadlines and delays.
pub trait Time {
    type Deadline: Copy;
    fn deadline_after_ms(&mut self, ms: u64) -> Self::Deadline;
    fn expired(&mut self, deadline: Self::Deadline) -> bool;
    fn delay_us(&mut self, us: u32);
    fn park_ms(&mut self, ms: u64);
}

/// Maximum number of actions emitted by a single `step()` call.
///
/// Bounded by the longest known chain:
///   ack + write8 + arm_deadline + read8  (start_reset = 3 actions)
/// or:
///   ack + send_stop  (2 actions)
/// The margin below is generous.
const MAX_ACTIONS: usize = 16;

/// Maximum concurrent deadlines (reset CMD + reset DATA ≈ 2).
const MAX_DEADLINES: usize = 4;

/// Maximum pending events per iteration.
const MAX_EVENTS: usize = 16;

/// Reference executor that drives a `Recovery` state machine to completion.
///
/// The executor owns a `Bus` and a `Time` implementation.  Call `run` with an
/// initial event (typically `Event::InterruptStatus { raw }`) and the executor
/// will loop, performing side effects and feeding back read completions and
/// deadline expirations until the request is completed or no further progress
/// is possible.
pub struct Executor<B: Bus, T: Time> {
    bus: B,
    time: T,
    /// Token → deadline handle, paired for later expiry checks.
    deadlines_tok: [Token; MAX_DEADLINES],
    deadlines_hdl: [T::Deadline; MAX_DEADLINES],
    deadlines_len: usize,
}

impl<B: Bus, T: Time> Executor<B, T> {
    pub fn new(bus: B, time: T) -> Self {
        use core::mem::MaybeUninit;
        Executor {
            bus,
            time,
            // SAFETY: Token(0) and uninitialised Deadline are overwritten before use.
            deadlines_tok: unsafe { MaybeUninit::zeroed().assume_init() },
            deadlines_hdl: unsafe { MaybeUninit::zeroed().assume_init() },
            deadlines_len: 0,
        }
    }

    /// Drive the recovery state machine with the given initial event.
    ///
    /// Returns when the state machine has no more events to process and no
    /// pending deadlines have expired.  In a real system the executor would
    /// block waiting for the next interrupt or deadline; this reference
    /// implementation polls deadlines once after each action batch.
    pub fn run(&mut self, recovery: &mut Recovery, initial: Event) {
        let mut events: [Event; MAX_EVENTS] = unsafe { core::mem::MaybeUninit::zeroed().assume_init() };
        let mut ev_head: usize = 0;
        let mut ev_tail: usize = 0;

        // Seed with the initial event.
        events[0] = initial;
        ev_tail = 1;

        while ev_head < ev_tail {
            let ev = events[ev_head].clone();
            ev_head += 1;

            let mut actions: [Action; MAX_ACTIONS] = unsafe { core::mem::MaybeUninit::zeroed().assume_init() };
            let mut act_count: usize = 0;
            recovery.step(ev, &mut |a| {
                if act_count < MAX_ACTIONS {
                    actions[act_count] = a;
                    act_count += 1;
                }
            });

            for i in 0..act_count {
                self.handle_action(&actions[i], &mut events, &mut ev_tail);
            }

            // Check for expired deadlines after processing all actions from this event.
            self.check_deadlines(&mut events, &mut ev_tail);
        }
    }

    fn handle_action(&mut self, action: &Action, events: &mut [Event], ev_tail: &mut usize) {
        match action {
            Action::Write32 { reg, value } => self.bus.w32(*reg, *value),
            Action::Write16 { reg, value } => self.bus.w16(*reg, *value),
            Action::Write8 { reg, value } => self.bus.w8(*reg, *value),
            Action::Read32 { reg, token } => {
                let val = self.bus.r32(*reg);
                self.push_event(events, ev_tail, Event::ReadComplete { token: *token, value: val });
            }
            Action::Read16 { reg, token } => {
                let val = self.bus.r16(*reg) as u32;
                self.push_event(events, ev_tail, Event::ReadComplete { token: *token, value: val });
            }
            Action::Read8 { reg, token } => {
                let val = self.bus.r8(*reg) as u32;
                self.push_event(events, ev_tail, Event::ReadComplete { token: *token, value: val });
            }
            Action::ArmDeadline { token, ms } => {
                let deadline = self.time.deadline_after_ms(*ms);
                if self.deadlines_len < MAX_DEADLINES {
                    self.deadlines_tok[self.deadlines_len] = *token;
                    self.deadlines_hdl[self.deadlines_len] = deadline;
                    self.deadlines_len += 1;
                }
            }
            Action::DelayUs { us } => self.time.delay_us(*us),
            Action::SendStop { id: _, stop } => {
                // UNDER-SOURCED: sdhci_send_command (sdhci.c:1653-1742).
                // Simplified implementation: write argument, clear auto-CMD bits,
                // write command register.
                // Also omits PRESENT_STATE test, timeout setup, timer,
                // card-presence retry, and currently fails to encode
                // STOP_WITH_TC busy unless the reducer mutates `StopCtx.flags`.
                self.bus.w32(SDHCI_ARGUMENT, stop.arg);
                let mode = self.bus.r16(SDHCI_TRANSFER_MODE);
                let mode = mode & !(SDHCI_TRNS_AUTO_CMD12 | SDHCI_TRNS_AUTO_CMD23);
                self.bus.w16(SDHCI_TRANSFER_MODE, mode);
                // Build command flags.
                let mut flags = SDHCI_CMD_RESP_SHORT | SDHCI_CMD_CRC | SDHCI_CMD_INDEX;
                if stop.flags & MMC_RSP_BUSY != 0 {
                    flags = SDHCI_CMD_RESP_SHORT_BUSY | SDHCI_CMD_CRC | SDHCI_CMD_INDEX;
                }
                let cmd = SDHCI_MAKE_CMD(stop.opcode as u16, flags);
                self.bus.w16(SDHCI_COMMAND, cmd);
            }
            Action::AdmaPost => {
                // UNDER-SOURCED: sdhci_adma_table_post (sdhci.c:844-882).
                // Abstract action; no bus writes in this reference executor.
            }
            Action::AdmaWorkaround => {
                // UNDER-SOURCED: ops->adma_workaround (sdhci.c:3495-3496).
                // Abstract action.
            }
            Action::ClockKick => {
                // UNDER-SOURCED: host->ops->set_clock (sdhci.c:3184-3189).
                // Abstract action.
            }
            Action::CompleteRequest { .. } => {
                // The core emits this when the request is done; no bus action.
            }
            Action::ReportError { .. } => {
                // CoralOS extension; no bus action.
            }
        }
    }

    fn push_event(&self, events: &mut [Event], ev_tail: &mut usize, ev: Event) {
        if *ev_tail < MAX_EVENTS {
            events[*ev_tail] = ev;
            *ev_tail += 1;
        }
    }

    fn check_deadlines(&mut self, events: &mut [Event], ev_tail: &mut usize) {
        let mut i = 0;
        while i < self.deadlines_len {
            if self.time.expired(self.deadlines_hdl[i]) {
                let token = self.deadlines_tok[i];
                self.push_event(events, ev_tail, Event::DeadlineExpired { token });
                // Swap-remove.
                self.deadlines_len -= 1;
                self.deadlines_tok[i] = self.deadlines_tok[self.deadlines_len];
                self.deadlines_hdl[i] = self.deadlines_hdl[self.deadlines_len];
            } else {
                i += 1;
            }
        }
    }
}
