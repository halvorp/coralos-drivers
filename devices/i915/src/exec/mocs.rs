// SPDX-License-Identifier: GPL-2.0-only
//! MOCS: run an `i915-gt-core` MOCS program ([`MocsWrites`]) and read every word back, with the `[I915-MOCS-PROGRAM ...]` marker.
//!
//! Linux programs the MOCS from two places (gt/intel_mocs.c): `intel_mocs_init` (:666-685, from `intel_gt_init_hw`, gt/intel_gt.c:219 -
//! at bring-up and again in a full reset, gt/intel_reset.c:1264) writes the global table where there is one and the L3CC words; and
//! `intel_mocs_init_engine` (:631-649, from `execlists_resume`, gt/intel_execlists_submission.c:2952-2954 - every engine resume, so
//! before the engine's first request and after every engine or full reset) writes the engine's own control table and, for the render
//! engine, the L3CC words again. Which table and which registers is the planner's (`mocs_settings`, `get_mocs_settings` :454-534); this
//! file only executes the words and checks them, as `program_private_ppat` does for the PAT. Linux writes and never reads back; the
//! readback is this port's evidence (a word the register does not hold is a MISMATCH named by offset, not hidden).
use super::core::Executor;
use super::journal::StepId;
use super::ExecError;
use crate::log::{emit, Log};
use coralos_hal::Mmio;
use i915_gt_core::mocs::{gt_init_writes, MocsSettings, MocsWrites};

/// What one MOCS program did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MocsReport {
    /// The Linux table (`skl`, `bxt`).
    pub table: &'static str,
    /// `gt` (`intel_mocs_init`) or the engine's name (`intel_mocs_init_engine`).
    pub scope: &'static str,
    /// Control registers written.
    pub entries: u32,
    /// `LNCFCMOCS` words written.
    pub l3cc: u32,
    /// Words that did not read back as written.
    pub mismatches: u32,
    /// The first of them: `(offset, wrote, read)`.
    pub first_bad: Option<(u32, u32, u32)>,
}

impl<B: Mmio> Executor<B> {
    /// `intel_mocs_init` as its own step, `MOCS-PROGRAM` (on Gen9: the L3CC words only - no global MOCS).
    pub fn program_gt_mocs<L: Log>(&mut self, log: &mut L, s: &MocsSettings) -> Result<MocsReport, ExecError> {
        let plan = gt_init_writes(s)?;
        self.step(log, StepId::MOCS_SETUP, "MOCS-PROGRAM", |ex, log| ex.run_mocs(log, s, plan, "gt"))
    }

    /// Write `plan` in order (forcewake held for the registers it names), posting-read the last word, read every word back and emit
    /// the marker. No step record: an engine resume runs this inside its own `ENGINE-ENABLE` / `*-RESET` step (steps do not nest).
    /// A write the allow-list refuses stops the program by name; a readback mismatch does not (it is reported, as the PAT's is).
    pub fn run_mocs<L: Log>(&mut self, log: &mut L, s: &MocsSettings, plan: MocsWrites, scope: &'static str) -> Result<MocsReport, ExecError> {
        // EXPERIMENT (Atlas 2026-10-30, branch atlas12-exp-nomocs, NEVER LANDED): leave the firmware's MOCS / LNCFCMOCS untouched to test
        // whether programming skl_mocs_table caused the cross-window GPU corruption first seen on "amber mountain marsh".
        let _ = (s, &plan);
        crate::log::emit(log, format_args!("[I915-MOCS-PROGRAM table=firmware scope={} skipped EXPERIMENT nomocs]", scope));
        return Ok(MocsReport { table: "firmware", scope, entries: 0, l3cc: 0, mismatches: 0, first_bad: None });
        #[allow(unreachable_code)]
        let (entries, l3cc) = (plan.control_entries(), plan.l3cc_words());
        let d = self.domains_for(plan.map(|w| w.reg));
        let (mismatches, first_bad) = self.awake(d, |ex| {
            let mut last = None;
            for w in plan {
                ex.wr(w.reg, w.value)?;
                last = Some(w.reg);
            }
            if let Some(r) = last {
                ex.posting_read(r)?;
            }
            let (mut n, mut first) = (0u32, None);
            for w in plan {
                let got = ex.rd(w.reg)?;
                if got != w.value {
                    n += 1;
                    first = first.or(Some((w.reg.offset(), w.value, got)));
                }
            }
            Ok((n, first))
        })?;
        let rep = MocsReport { table: s.table.name, scope, entries, l3cc, mismatches, first_bad };
        match first_bad {
            None => emit(log, format_args!("[I915-MOCS-PROGRAM table={} engine={} entries={} l3cc={} readback=ok]", rep.table, scope, entries, l3cc)),
            Some((off, want, got)) => emit(
                log,
                format_args!(
                    "[I915-MOCS-PROGRAM table={} engine={} entries={} l3cc={} readback=MISMATCH bad={} first_bad={:#x} want={:#010x} got={:#010x}]",
                    rep.table, scope, entries, l3cc, mismatches, off, want, got
                ),
            ),
        }
        Ok(rep)
    }
}
