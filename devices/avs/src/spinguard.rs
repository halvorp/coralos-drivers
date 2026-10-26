// SPDX-License-Identifier: GPL-2.0-only
//! The serve loop's busy-wake guard (IDLE-WAKES-ONLY-FOR-WORK): a wait that returns at once, step after step, with the clock not moving is a
//! loop burning a core, whatever woke it (a listen or request endpoint that reads ready and is never drained, a wait set that came out
//! empty, a deadline already past). The guard counts consecutive wakes that ended in the SAME millisecond the previous one did; past
//! [`LIMIT`] it names the wake at firings 1, 2, 4, 8, ... (powers of two; a per-N marker flooded the ring and evicted evidence) and asks the loop to park for [`PARK_MS`], which turns a 100 % core into a bounded poll
//! whose marker names the culprit.
use crate::machine::Wake;

/// Consecutive same-millisecond wakes tolerated: a legitimate burst (a stream period, a request and its reply) is a handful, never this.
pub const LIMIT: u32 = 256;
/// How long the loop parks once the guard fires.
pub const PARK_MS: u64 = 5;

#[derive(Default)]
pub struct SpinGuard {
    last_ms: u64,
    run: u32,
    guarded: u64,
}

/// What the loop does after a step.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Fine,
    /// Park for [`PARK_MS`]; `say` is true when this firing is to be named.
    Park { say: bool, steps: u64 },
}

impl SpinGuard {
    pub fn note(&mut self, now_ms: u64) -> Verdict {
        if now_ms != self.last_ms {
            self.last_ms = now_ms;
            self.run = 0;
            return Verdict::Fine;
        }
        self.run = self.run.saturating_add(1);
        if self.run < LIMIT {
            return Verdict::Fine;
        }
        self.run = 0;
        self.guarded += 1;
        Verdict::Park { say: self.guarded.is_power_of_two(), steps: self.guarded }
    }
}

pub const fn wake_name(w: Wake) -> &'static str {
    match w {
        Wake::AvsIrq => "avs-irq",
        Wake::Jack => "jack",
        Wake::Request => "request",
        Wake::Accept => "accept",
        Wake::PeerDead => "peer-dead",
        Wake::JackGone => "jack-gone",
        Wake::Deadline => "deadline",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clock_that_moves_never_fires() {
        let mut g = SpinGuard::default();
        for t in 1..10_000u64 {
            assert_eq!(g.note(t), Verdict::Fine);
        }
    }

    #[test]
    fn a_burst_inside_one_millisecond_is_fine_but_a_spin_is_parked_and_named_once() {
        let mut g = SpinGuard::default();
        g.note(5);
        for _ in 0..LIMIT - 1 {
            assert_eq!(g.note(5), Verdict::Fine);
        }
        assert_eq!(g.note(5), Verdict::Park { say: true, steps: 1 });
        let mut said = 1;
        for n in 2..=1024u64 {
            for _ in 0..LIMIT - 1 {
                g.note(5);
            }
            match g.note(5) {
                Verdict::Park { say, steps } => {
                    assert_eq!(steps, n);
                    assert_eq!(say, n.is_power_of_two(), "firing {n}");
                    said += say as u32;
                }
                v => panic!("{v:?}"),
            }
        }
        assert_eq!(said, 11, "1024 firings are named 11 times (1,2,4,...,1024), not 6");
    }
}
