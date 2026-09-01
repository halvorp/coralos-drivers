// SPDX-License-Identifier: GPL-2.0-only
//! Software autorepeat state machine from Linux `drivers/input/input.c`.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

/// Linux's registration defaults (input.c:2351-2352).
pub const DEFAULT_DELAY_MS: u32 = 250; // input.c:2352
pub const DEFAULT_PERIOD_MS: u32 = 33; // input.c:2352

/// Pure state corresponding to `repeat_key`, `rep[]`, and the repeat timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoRepeat {
    pub delay_ms: u32,
    pub period_ms: u32,
    pub repeat_key: Option<u16>,
    pub timer_armed_ms: Option<u32>,
}

/// Facts Linux checks before a timer callback emits repeat events (input.c:2221-2233).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepeatTickContext {
    pub inhibited: bool,
    pub key_down: bool,
    pub key_supported: bool,
}

/// Timer callback result. Emission means EV_KEY value 2 followed by SYN_REPORT (input.c:2227-2228).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickOutcome {
    Idle,
    Emit { key: u16, next_ms: Option<u32> },
}

impl AutoRepeat {
    /// Construct software repeat settings as `input_enable_softrepeat` does (input.c:2244-2249).
    pub const fn new(delay_ms: u32, period_ms: u32) -> Self {
        Self { delay_ms, period_ms, repeat_key: None, timer_armed_ms: None }
    }

    /// Linux defaults only when BOTH driver values are zero (input.c:2348-2352).
    pub const fn with_registration_defaults(delay_ms: u32, period_ms: u32) -> Self {
        if delay_ms == 0 && period_ms == 0 {
            Self::new(DEFAULT_DELAY_MS, DEFAULT_PERIOD_MS)
        } else {
            Self::new(delay_ms, period_ms)
        }
    }

    /// Process a delivered key event after handler filtering (input.c:136-146).
    ///
    /// Value 2 bypasses the machine. A nonzero press starts only when EV_REP, period, delay, and a
    /// timer function are all present (input.c:87-96); a release deletes the timer (input.c:98-101).
    pub fn on_key_event(
        &mut self,
        code: u16,
        value: i32,
        ev_rep_enabled: bool,
        timer_available: bool,
    ) {
        if value == 2 {
            return;
        }
        if value == 0 {
            self.timer_armed_ms = None;
            return;
        }
        if ev_rep_enabled
            && self.period_ms != 0
            && self.delay_ms != 0
            && timer_available
        {
            self.repeat_key = Some(code);
            self.timer_armed_ms = Some(self.delay_ms);
        }
    }

    /// Fire the repeat timer according to `input_repeat_key` (input.c:2216-2234).
    pub fn tick(&mut self, context: RepeatTickContext) -> TickOutcome {
        let Some(key) = self.repeat_key else { return TickOutcome::Idle };
        if self.timer_armed_ms.is_none()
            || context.inhibited
            || !context.key_down
            || !context.key_supported
        {
            self.timer_armed_ms = None;
            return TickOutcome::Idle;
        }
        let next_ms = (self.period_ms != 0).then_some(self.period_ms);
        self.timer_armed_ms = next_ms;
        TickOutcome::Emit { key, next_ms }
    }
}
