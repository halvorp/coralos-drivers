// SPDX-License-Identifier: GPL-2.0-only
//! Adapter: run an `i915-display-link-core` effect list over the [`Executor`] (that crate builds its plans as `Vec<Effect>`; the device
//! has a heap now, `coralos_rt::heap`). Its waits are `PollPlan`s with an `interval_us` and a hard
//! `max_polls`: read, `judge`, and between reads sleep `interval_us` through `Mmio::park_ms` - never spin (the rule display-link's
//! own `PollPlan` documents as the NO-POLL-BLOCK-ON-WAKE exception).
use super::core::Executor;
use super::ExecError;
use coralos_hal::Mmio;
use i915_display_link_core::plan::{Effect as LinkEffect, PollPlan as LinkPoll, PollStep};

impl<B: Mmio> Executor<B> {
    /// Run a display-link effect list. Its waits are `PollPlan`s with an `interval_us` and a hard `max_polls`: read, `judge`, and
    /// between reads sleep `interval_us` through `Mmio::park_ms` (never spin).
    pub fn run_link_effects(&mut self, effects: &[LinkEffect]) -> Result<(), ExecError> {
        let regs = effects.iter().filter_map(|e| match *e {
            LinkEffect::Write { reg, .. } | LinkEffect::Rmw { reg, .. } | LinkEffect::ClearIfSet { reg, .. } | LinkEffect::PostingRead { reg } => Some(reg),
            LinkEffect::Poll(p) => Some(p.reg),
            LinkEffect::DelayUs(_) => None,
        });
        let d = self.domains_for(regs);
        self.awake(d, |ex| ex.link_effects_awake(effects))
    }

    fn link_effects_awake(&mut self, effects: &[LinkEffect]) -> Result<(), ExecError> {
        for e in effects {
            match *e {
                LinkEffect::Write { reg, value } => self.wr(reg, value)?,
                LinkEffect::Rmw { reg, clear, set } => {
                    self.rmw(reg, clear, set)?;
                }
                LinkEffect::ClearIfSet { reg, bits } => {
                    let v = self.rd(reg)?;
                    if v & bits != 0 {
                        self.wr(reg, v & !bits)?;
                    }
                }
                LinkEffect::PostingRead { reg } => self.posting_read(reg)?,
                LinkEffect::Poll(p) => self.link_poll(&p)?,
                LinkEffect::DelayUs(us) => self.delay_us(us),
            }
        }
        Ok(())
    }

    fn link_poll(&mut self, p: &LinkPoll) -> Result<(), ExecError> {
        let mut polls = 0u32;
        loop {
            let v = self.rd(p.reg)?;
            polls += 1;
            match p.judge(polls, v) {
                PollStep::Satisfied => return Ok(()),
                PollStep::Again => self.delay_us(p.interval_us),
                PollStep::TimedOut(t) => return Err(ExecError::PollTimeout { what: t.what, reg: t.reg, mask: t.mask, want: t.want, got: t.got, polls: t.polls }),
            }
        }
    }
}
