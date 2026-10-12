// SPDX-License-Identifier: GPL-2.0-only
//! INCREMENT P2 - memory: run an `i915-memory-core` [`BindPlan`] against the GGTT window and the DMA table pages, and qualify the
//! result with the `[I915-MEMORY ...]` marker (plan section 3).
//!
//! ORDER (the planner's guarantees, `plan.rs`): ops run in plan order; a table page is filled before the entry that links it; a
//! victim's `AwaitRetire` is waited on (bounded) before the scratch fills that unmap it; then the flushes, in order - CPU-cache
//! flush per touched table span, then the one TLB step. A GGTT entry is a store into the GGTT window of BAR0
//! ([`Executor::ggtt_store`], refused unless `p2-memory` is compiled in); a PPGTT entry is a CPU store into a DMA table page found
//! by DEVICE address through the grant table ([`GpuMem::store64`]). The `GgttInvalidate` flush is the one register write
//! (`GFX_FLSH_CNTL_GEN6`), through the guard, followed by a posting read.
use super::core::Executor;
use super::dma::{CpuMem, GpuMem};
use super::journal::StepId;
use super::ExecError;
use crate::log::{emit, Log};
use coralos_hal::{Dma, Irq, IrqWait, Mmio};
use i915_devinfo_core::PlatformDescriptor;
use i915_memory_core::{BindPlan, Flush, Gtt, Op, Target, VmaId};

/// How many times an `AwaitRetire` asks whether the VMA retired before giving up (`AWAIT-RETIRE-TIMEOUT`), yielding between asks.
pub const AWAIT_RETIRE_POLLS: u32 = 10_000;

/// What a bind did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BindReport {
    pub ggtt_entries: u32,
    pub table_entries: u32,
    pub cpu_flushes: u32,
    pub ggtt_invalidates: u32,
    /// The plan ended with `Flush::PpgttTlb`: the NEXT batch must carry the TLB-invalidating flush (an engine-level step).
    pub ppgtt_tlb_pending: bool,
    pub retire_waits: u32,
}

impl<B: Mmio> Executor<B> {
    /// Execute `plan` as one step (`GGTT-BIND` or `PPGTT-BIND`, by the plan's space), with the `[I915-STEP ...]` records.
    /// `retired(vma)` answers whether a victim has retired; it is asked at most [`AWAIT_RETIRE_POLLS`] times per victim.
    pub fn run_bind_step<L: Log, D: Dma, M: CpuMem>(&mut self, log: &mut L, gm: &mut GpuMem<D, M>, plan: &BindPlan<'_>, ggtt: bool, retired: &mut dyn FnMut(VmaId) -> bool) -> Result<BindReport, ExecError> {
        let (id, name) = if ggtt { (StepId::GGTT_BIND, "GGTT-BIND") } else { (StepId::PPGTT_BIND, "PPGTT-BIND") };
        self.step(log, id, name, |ex, _| ex.run_bind(gm, plan, retired))
    }

    /// Execute `plan` (no step record; see [`Executor::run_bind_step`]).
    pub fn run_bind<D: Dma, M: CpuMem>(&mut self, gm: &mut GpuMem<D, M>, plan: &BindPlan<'_>, retired: &mut dyn FnMut(VmaId) -> bool) -> Result<BindReport, ExecError> {
        let mut rep = BindReport::default();
        let mut writes = plan.writes();
        for op in plan.ops() {
            if let Op::AwaitRetire { vma } = *op {
                let mut polls = 0u32;
                while !retired(vma) {
                    polls += 1;
                    if polls >= AWAIT_RETIRE_POLLS {
                        return Err(ExecError::AwaitRetireTimeout { vma: vma.0, polls });
                    }
                    self.relax();
                }
                rep.retire_waits += 1;
                continue;
            }
            for _ in 0..op.entries() {
                let w = writes.next().ok_or(ExecError::Bound { what: "plan writes shorter than its ops", cap: 0 })?;
                match w.target {
                    Target::Ggtt { mmio_offset } => {
                        self.ggtt_store(mmio_offset, w.value)?;
                        rep.ggtt_entries += 1;
                    }
                    Target::Table { device_addr, byte_offset } => {
                        gm.store64(device_addr + byte_offset as u64, w.value)?;
                        rep.table_entries += 1;
                    }
                }
            }
        }
        for f in plan.flushes() {
            match f {
                Flush::CpuCache { device_addr, byte_offset, bytes } => {
                    gm.flush(device_addr + byte_offset as u64, bytes as u32)?;
                    rep.cpu_flushes += 1;
                }
                Flush::GgttInvalidate(w) => {
                    let d = self.domains_for([w.reg]);
                    self.awake(d, |ex| {
                        ex.wr(w.reg, w.value)?;
                        ex.posting_read(w.reg)
                    })?;
                    rep.ggtt_invalidates += 1;
                }
                Flush::PpgttTlb => rep.ppgtt_tlb_pending = true,
            }
        }
        Ok(rep)
    }

    /// Read every GGTT entry `plan` stored back from BAR0 and compare: the proof that the stores landed (a closed window or a wrong
    /// offset would leave the old value). Returns the first mismatch as `(mmio_offset, expected, got)`.
    pub fn verify_ggtt(&mut self, plan: &BindPlan<'_>) -> Option<(u32, u64, u64)> {
        for w in plan.writes() {
            if let Target::Ggtt { mmio_offset } = w.target {
                let got = self.ggtt_load(mmio_offset);
                if got != w.value {
                    return Some((mmio_offset, w.value, got));
                }
            }
        }
        None
    }
}

/// `true` if the planner's model denies an access to `va`: an unbound, unbound-then-reused or guard address
/// (`Gtt::check_access`, the device-side denial the GPU's page walker would give with scratch).
pub fn stale_access_denied<const V: usize, const T: usize>(gtt: &Gtt<V, T>, va: u64, len: u64) -> bool {
    use i915_memory_core::MemError::{Beyond48Bit, BeyondSpace, GuardPageAccess, StaleMapping};
    matches!(gtt.check_access(va, len), Err(StaleMapping { .. } | GuardPageAccess { .. } | Beyond48Bit { .. } | BeyondSpace { .. }))
}

/// MSI qualification: did the line fire within `timeout_ms`, and was it re-armed? The CAUSE of the interrupt is the caller's (the
/// silicon step arms a GT interrupt through a planner and an allow-listed write); this only observes delivery.
pub fn msi_delivered<I: Irq>(irq: &mut I, timeout_ms: u64) -> bool {
    matches!(irq.wait(timeout_ms), Ok(IrqWait::Fired)) && irq.ack().is_ok()
}

/// The facts the `[I915-MEMORY ...]` marker certifies. Each is `Some(true)` only when the check really ran and passed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryProof {
    /// A GGTT bind executed and every entry read back from BAR0 equal to what the planner computed.
    pub ggtt_readback: bool,
    /// A PPGTT bind executed (entries stored in granted DMA table pages).
    pub ppgtt_bound: bool,
    /// An access to an unbound/guard address was denied by the planner's model.
    pub stale_denied: bool,
    /// MSI delivery was observed. `None` = not tested in this run.
    pub msi: Option<bool>,
}

/// Print the plan's marker, `[I915-MEMORY va_bits=48 stale_access=denied msi=ok]`, ONLY when every fact holds. Otherwise print one
/// `[I915-MEMORY INCOMPLETE ...]` line naming what did not (a marker that appears must mean the step happened). Returns whether the
/// marker was printed.
pub fn emit_memory_marker<L: Log>(log: &mut L, desc: &PlatformDescriptor, p: &MemoryProof) -> bool {
    let ok = p.ggtt_readback && p.ppgtt_bound && p.stale_denied && p.msi == Some(true);
    if ok {
        emit(log, format_args!("[I915-MEMORY va_bits={} stale_access=denied msi=ok]", desc.info.ppgtt_size));
    } else {
        let msi = match p.msi {
            Some(true) => "ok",
            Some(false) => "FAILED",
            None => "untested",
        };
        emit(
            log,
            format_args!("[I915-MEMORY INCOMPLETE ggtt_readback={} ppgtt_bound={} stale_access={} msi={}]", p.ggtt_readback as u8, p.ppgtt_bound as u8, if p.stale_denied { "denied" } else { "UNPROVEN" }, msi),
        );
    }
    ok
}
