// SPDX-License-Identifier: GPL-2.0-only
//! [`Executor`]: typed register access over `coralos_hal::Mmio` with the three guarantees every plan needs.
//!
//! 1. WRITES go through `i915_common_core::WriteGuard` over [`allow::ALLOW`] - one short-lived guard per write, its counters folded
//!    into [`Stats`]. A register that is not on the list is not forwarded, is COUNTED, and stops the plan with
//!    `WRITE-NOT-ALLOWED reg=0x...` (a plan is never "mostly executed past" a refusal). GGTT entries are the one other write:
//!    8-byte stores into [`allow::GgttWindow`], checked here, closed unless `p2-memory` is compiled in.
//! 2. WAITS are bounded twice (`max_polls` reads AND the deadline) and call `Mmio::relax` between reads - a poll yields, it never
//!    spins (the rule display-link's `PollPlan` states as NO-POLL-BLOCK-ON-WAKE). Delays call `Mmio::park_ms`.
//! 3. FORCEWAKE is held around GT register access: [`Executor::awake`] takes the domains a plan's registers need per
//!    `i915_uncore_core`'s Gen9 range table, and every read/write is refused (`FORCEWAKE-NOT-HELD`) if its domains are not held.
//!
//! The ONLY register writes in this crate other than the forcewake protocol are the ones in this file and `memory.rs`'s
//! `ggtt_store` (check-i915-writes names them as the reviewed writer).
use super::allow::{self, GgttWindow};
use super::journal::{Phase, StepId};
use super::ExecError;
use crate::gt::ByRef;
use crate::log::{emit, Log};
use coralos_hal::Mmio;
use i915_common_core::{I915Error, Reg, RegIo, WriteGuard, WriteRefusal};
use i915_devinfo_core::PlatformDescriptor;
use i915_uncore_core::{read_fw_domains, write_fw_domains, Uncore, WRITE_ALLOWLIST};

/// What the executor has done to the device, for markers and for the tests' "refused by name and counted".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Register writes forwarded to the device through the guard (forcewake set/clear included).
    pub forwarded: u32,
    /// Writes REFUSED (register not on the allow-list, or a GGTT store outside the window) and not performed.
    pub refused: u32,
    /// The first refusal.
    pub first_refusal: Option<WriteRefusal>,
    /// GGTT entries stored (8 bytes each).
    pub ggtt_stores: u32,
    /// Register reads made by polls.
    pub polls: u32,
    /// Posting reads made.
    pub posting_reads: u32,
}

/// The executor. Owns the BAR0 [`Mmio`], the uncore (forcewake state) and the allow-list it was built with.
pub struct Executor<B: Mmio> {
    pub(super) bar: B,
    pub(super) uncore: Uncore,
    allow: &'static [Reg],
    pub(super) ggtt: Option<GgttWindow>,
    stats: Stats,
    seq: u32,
    open: Option<StepId>,
}

/// The most polls any executor-level wait makes when its plan names no bound of its own.
pub const DEFAULT_MAX_POLLS: u32 = 100_000;

impl<B: Mmio> Executor<B> {
    /// An executor with THIS build's allow-list (three forcewake registers unless `p2-memory` / `p3-exec` are compiled in).
    pub fn new(bar: B, desc: &PlatformDescriptor) -> Result<Self, I915Error> {
        Self::with_grant(bar, desc, &allow::ALLOW, allow::compiled_ggtt_window())
    }

    /// An executor with an explicit grant, for a build that wants a NARROWER one than it compiled in (never wider: the BAR0 window
    /// grant is `allow::BAR0_WRITE_RANGES`, and the loader refuses what it does not cover) and for the tests that prove the default
    /// build's refusals inside a feature-ON test run.
    ///
    /// The forcewake SET registers must be on `allow`: a grant without them cannot wake the GT (the protocol would write, be refused
    /// and wait for an ack that never comes), so it is refused here by name (`BAD-VALUE`).
    pub fn with_grant(bar: B, desc: &PlatformDescriptor, allow: &'static [Reg], ggtt: Option<GgttWindow>) -> Result<Self, I915Error> {
        if WRITE_ALLOWLIST.iter().any(|fw| !allow.contains(fw)) {
            return Err(I915Error::BadValue { what: "allow-list lacks a forcewake set register", value: allow.len() as u32 });
        }
        Ok(Executor { bar, uncore: Uncore::for_platform(desc)?, allow, ggtt, stats: Stats::default(), seq: 0, open: None })
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }
    pub fn allow_list(&self) -> &'static [Reg] {
        self.allow
    }
    pub fn ggtt_window(&self) -> Option<GgttWindow> {
        self.ggtt
    }
    /// Narrow the GGTT window to the probed table. Cannot widen: a window wider than the grant is refused.
    pub fn set_ggtt_window(&mut self, w: GgttWindow) -> Result<(), ExecError> {
        match self.ggtt {
            Some(g) if w.base >= g.base && w.base as u64 + w.bytes as u64 <= g.base as u64 + g.bytes as u64 => {
                self.ggtt = Some(w);
                Ok(())
            }
            _ => Err(ExecError::GgttStoreRefused { offset: w.base }),
        }
    }
    /// Give the register file back (host tests read the chip's state from their own handle; the device never calls this).
    pub fn into_bar(self) -> B {
        self.bar
    }

    // ---- forcewake -----------------------------------------------------------------------------------------------------------

    /// The forcewake domains a plan touching `regs` needs: for each register the domains of its read AND its write.
    pub fn domains_for(&self, regs: impl IntoIterator<Item = Reg>) -> u32 {
        regs.into_iter().fold(0, |m, r| m | read_fw_domains(r.offset()) | write_fw_domains(r.offset())) & self.uncore.domains()
    }

    /// Hold `domains` for the duration of `f`, then release them and OBSERVE the release (the ack clears). The body's error wins over a
    /// release error; a failed take runs no body.
    pub fn awake<R>(&mut self, domains: u32, f: impl FnOnce(&mut Self) -> Result<R, ExecError>) -> Result<R, ExecError> {
        let domains = domains & self.uncore.domains();
        if domains == 0 {
            return f(self);
        }
        self.fw_take(domains)?;
        let body = f(self);
        let rel = self.fw_release(domains);
        match (body, rel) {
            (Ok(v), Ok(())) => Ok(v),
            (Err(e), _) => Err(e),
            (Ok(_), Err(e)) => Err(e),
        }
    }

    fn fw_take(&mut self, domains: u32) -> Result<(), ExecError> {
        let mut g = WriteGuard::new(ByRef(&mut self.bar), self.allow);
        let r = self.uncore.forcewake_get(&mut g, domains).and_then(|()| g.check());
        self.stats.forwarded += g.writes();
        self.stats.refused += g.refusals();
        r.map_err(ExecError::Hw)
    }

    fn fw_release(&mut self, domains: u32) -> Result<(), ExecError> {
        let mut g = WriteGuard::new(ByRef(&mut self.bar), self.allow);
        let r = self.uncore.forcewake_put(&mut g, domains).and_then(|()| g.check());
        self.stats.forwarded += g.writes();
        self.stats.refused += g.refusals();
        r?;
        self.uncore.wait_released(&mut self.bar, domains)?;
        Ok(())
    }

    fn need_awake(&self, reg: Reg, write: bool) -> Result<(), ExecError> {
        let off = reg.offset();
        let need = (if write { write_fw_domains(off) } else { read_fw_domains(off) }) & self.uncore.domains();
        if need & !self.uncore.active() != 0 {
            return Err(ExecError::Hw(I915Error::ForcewakeNotHeld { offset: off, domains: need & !self.uncore.active() }));
        }
        Ok(())
    }

    // ---- registers -----------------------------------------------------------------------------------------------------------

    /// `intel_uncore_read`: refused unless the register's forcewake domains are held.
    pub fn rd(&mut self, reg: Reg) -> Result<u32, ExecError> {
        self.uncore.read32(&mut self.bar, reg).map_err(ExecError::Hw)
    }

    /// `intel_uncore_write` through the write guard. A register off the allow-list is refused, counted and returned as
    /// `WRITE-NOT-ALLOWED`; nothing reaches the device.
    pub fn wr(&mut self, reg: Reg, value: u32) -> Result<(), ExecError> {
        self.need_awake(reg, true)?;
        let mut g = WriteGuard::new(ByRef(&mut self.bar), self.allow);
        g.wr(reg, value);
        self.stats.forwarded += g.writes();
        self.stats.refused += g.refusals();
        if self.stats.first_refusal.is_none() {
            self.stats.first_refusal = g.first_refusal();
        }
        g.check().map_err(ExecError::Hw)
    }

    /// `intel_uncore_rmw`: read, `(old & !clear) | set`, write unconditionally (the clock-gating flavour).
    pub fn rmw(&mut self, reg: Reg, clear: u32, set: u32) -> Result<u32, ExecError> {
        let v = (self.rd(reg)? & !clear) | set;
        self.wr(reg, v)?;
        Ok(v)
    }

    /// `intel_uncore_posting_read`: read `reg` and discard it, to flush the preceding write.
    pub fn posting_read(&mut self, reg: Reg) -> Result<(), ExecError> {
        self.rd(reg)?;
        self.stats.posting_reads += 1;
        Ok(())
    }

    /// `udelay`/`usleep_range`: at least `us` microseconds, as whole milliseconds of `Mmio::park_ms` (a delay is a floor; 1 ms for
    /// 50 us is slower and safe). Zero returns at once.
    pub fn delay_us(&mut self, us: u32) {
        if us != 0 {
            self.bar.park_ms(us.div_ceil(1000) as u64);
        }
    }

    /// A bounded wait for `(read(reg) & mask) == want`. Reads at most `max_polls` times and not past `timeout_us` (rounded up to
    /// whole milliseconds), yielding with `relax` between reads. The condition is checked before either bound (`__wait_for`).
    /// An all-ones read is a dead BAR (`MMIO-UNRELIABLE`), never an answer.
    pub fn poll(&mut self, reg: Reg, mask: u32, want: u32, timeout_us: u32, max_polls: u32, what: &'static str) -> Result<u32, ExecError> {
        if max_polls == 0 {
            return Err(ExecError::Hw(I915Error::BadValue { what: "poll max_polls", value: 0 }));
        }
        self.bar.arm_deadline(timeout_us.div_ceil(1000).max(1) as u64);
        let mut polls = 0u32;
        loop {
            let got = self.rd(reg)?;
            polls += 1;
            self.stats.polls += 1;
            if got & mask == want {
                return Ok(got);
            }
            if got == u32::MAX {
                return Err(ExecError::Hw(I915Error::MmioUnreliable { domain: 0, reg }));
            }
            if polls >= max_polls || self.bar.deadline_expired() {
                return Err(ExecError::PollTimeout { what, reg, mask, want, got, polls });
            }
            self.bar.relax();
        }
    }

    /// Is the deadline armed by the last [`Executor::poll`]-style wait over?
    pub fn deadline_expired(&mut self) -> bool {
        self.bar.deadline_expired()
    }
    pub fn arm_deadline_us(&mut self, us: u32) {
        self.bar.arm_deadline(us.div_ceil(1000).max(1) as u64);
    }
    pub fn relax(&mut self) {
        self.bar.relax();
    }

    // ---- GGTT entries --------------------------------------------------------------------------------------------------------

    /// One 64-bit GGTT entry store at BAR0 offset `off` (`gen8_set_pte`, gt/intel_ggtt.c:243-246 `writeq`). `Mmio` has no 64-bit
    /// access, so the entry is two 32-bit stores, HIGH dword first: the PRESENT bit is in the low dword, so the entry is never
    /// valid with a stale upper half. Refused (counted, `GGTT-STORE-NOT-ALLOWED`) outside the window or while it is closed.
    pub fn ggtt_store(&mut self, off: u32, entry: u64) -> Result<(), ExecError> {
        match self.ggtt {
            Some(w) if w.holds_entry(off) => {
                self.bar.w32(off + 4, (entry >> 32) as u32);
                self.bar.w32(off, entry as u32);
                self.stats.ggtt_stores += 1;
                Ok(())
            }
            _ => {
                self.stats.refused += 1;
                if self.stats.first_refusal.is_none() {
                    self.stats.first_refusal = Some(WriteRefusal { offset: off, width: 8, value: entry as u32 });
                }
                Err(ExecError::GgttStoreRefused { offset: off })
            }
        }
    }

    /// Read a GGTT entry back (BAR0 reads are not gated): the readback that proves a bind landed.
    pub fn ggtt_load(&mut self, off: u32) -> u64 {
        let lo = self.bar.r32(off) as u64;
        let hi = self.bar.r32(off + 4) as u64;
        hi << 32 | lo
    }

    // ---- the durable "begun" record ------------------------------------------------------------------------------------------

    /// Run `f` as a risky STEP: print `[I915-STEP begun id=... seq=...]` BEFORE it, `done` or `failed why=...` after. A step that
    /// begins while another is open is refused (`STEP-NESTED`).
    pub fn step<L: Log, R>(&mut self, log: &mut L, id: StepId, name: &'static str, f: impl FnOnce(&mut Self, &mut L) -> Result<R, ExecError>) -> Result<R, ExecError> {
        if let Some(open) = self.open {
            return Err(ExecError::StepNested { open: open.0, new: id.0 });
        }
        self.seq += 1;
        let seq = self.seq;
        self.open = Some(id);
        emit(log, format_args!("[I915-STEP {} id={:#x} name={} seq={}]", Phase::Begun.word(), id.0, name, seq));
        let r = f(self, log);
        self.open = None;
        match &r {
            Ok(_) => emit(log, format_args!("[I915-STEP {} id={:#x} name={} seq={}]", Phase::Done.word(), id.0, name, seq)),
            Err(e) => emit(log, format_args!("[I915-STEP {} id={:#x} name={} seq={} why={}]", Phase::Failed.word(), id.0, name, seq, e.name())),
        }
        r
    }
    /// The id of the step in progress, if any.
    pub fn open_step(&self) -> Option<StepId> {
        self.open
    }
}
