// SPDX-License-Identifier: GPL-2.0-only
//! The device's EXECUTOR layer: the one place where the port crates' effect plans meet hardware (plan KBL-I915-PORT-PLAN section 1,
//! "only devices/i915 executes effects"; increments P2 and P3; section 5, surviving a hang with no remote reset).
//!
//! * [`allow`]   - the write allow-list and the GGTT window, built at compile time from the `p2-memory` / `p3-exec` features.
//!   The default build still writes only the three forcewake SET registers.
//! * [`core`]    - [`Executor`]: typed reads/writes through the existing `WriteGuard`, posting reads, bounded polls that yield via
//!   `Mmio::relax` (never spin), forcewake held around GT register access per the uncore ranges, GGTT entry stores.
//! * [`plans`]   - adapters that run each crate's plan type: gt-core `Effect`, the reset coroutine, submit-core `Step`s,
//!   display-clock `WritePlan`/`Transaction`/pcode, display-link `Effect` (`plans_link`).
//! * [`dma`]     - [`dma::GpuAlloc`]: table pages, ring, context image and HWSP over `Dma::alloc` (addr_width from the platform,
//!   budget from the endowment), `cpu_mapping` kept beside `device_addr`, never identity.
//! * [`memory`]  - P2: execute a `BindPlan` in order (AwaitRetire, entries, flushes), prove the GGTT entries read back, the marker.
//! * [`engine`]  - P3: execlists bring-up, request submission, CSB/seqno service, hang declaration, engine and full reset.
//! * [`journal`] - plan section 5: a `[I915-STEP begun id=0x...]` record before every risky step, `done`/`failed` after, and the
//!   next-boot refusal of a step that began and never ended.
//!
//! WHAT THE EXECUTOR DOES NOT DO: it does not decide. Every sequence, ordering and bound comes from a planner; the executor adds
//! only the three things a planner cannot know - the clock (deadline + yield), the memory (CPU mapping of a DEVICE address) and the
//! grant (the allow-list). A plan the allow-list does not cover is stopped at its first uncovered write, by name.
//!
//! CONTRACT GAPS A LATER ABI BUMP SHOULD CLOSE (the adapters here are thin on purpose; the core crates are untouched): gt-core
//! `Effect::{Write,Rmw,Poll,PostingRead,DelayUs}`, display-clock `Step::{Write,Rmw,PostingRead,Poll}` and display-link
//! `Effect::{Write,Rmw,PostingRead,Poll,DelayUs}` differ only cosmetically and should be ONE `i915-common-core` effect type; the
//! three `PollPlan`s differ only in unit (`timeout_us` vs `interval_us`), name field and `OnTimeout`; `MemError`, `SubmitError`, `GtError` and `ClockError` fold into `I915Error::BadValue` today.
pub mod allow;
pub mod core;
pub mod dma;
pub mod engine;
pub mod journal;
pub mod memory;
pub mod plans;
pub mod plans_link;

pub use self::core::{Executor, Stats};
pub use journal::{StepId, StepRecord};

use coralos_hal::DmaRefusal;
use i915_common_core::{I915Error, Reg};
use i915_display_clock_core::ClockError;
use i915_gt_core::GtError;
use i915_memory_core::MemError;
use i915_submit_core::SubmitError;

/// Why an executor operation stopped. Every variant has a stable [`ExecError::name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecError {
    /// The register layer: a refused write (`WRITE-NOT-ALLOWED`), forcewake not held, an unreliable BAR, a bad value.
    Hw(I915Error),
    Mem(MemError),
    Submit(SubmitError),
    Gt(GtError),
    Clock(ClockError),
    Dma(DmaRefusal),
    /// A bounded poll ran out (`max_polls` reads or the deadline, whichever came first).
    PollTimeout { what: &'static str, reg: Reg, mask: u32, want: u32, got: u32, polls: u32 },
    /// A GGTT entry store outside the open window (or any store while the window is closed).
    GgttStoreRefused { offset: u32 },
    /// A CPU-side store or load whose device address is not inside a DMA grant this device holds.
    OutsideGrant { device_addr: u64, len: u64 },
    /// A victim VMA never retired.
    AwaitRetireTimeout { vma: u32, polls: u32 },
    /// The device's DMA endowment cannot cover the request.
    EndowmentExhausted { want: usize, left: usize },
    /// The grant table could not grow (the heap is exhausted).
    GrantTableFull { have: usize },
    /// A step began on a previous boot and never ended.
    PreviousStepNotDone { id: u32 },
    /// A step began while another is open.
    StepNested { open: u32, new: u32 },
    /// An executor-level bound (steps of a coroutine, GGTT entries of a plan) was hit.
    Bound { what: &'static str, cap: u32 },
    /// The object a context needs (ring, image, status page) is not what the engine was built for.
    Layout { what: &'static str },
}

impl ExecError {
    pub const fn name(&self) -> &'static str {
        match self {
            ExecError::Hw(e) => e.name(),
            ExecError::Mem(e) => e.name(),
            ExecError::Submit(e) => e.name(),
            ExecError::Gt(e) => e.name(),
            ExecError::Clock(e) => e.name(),
            ExecError::Dma(e) => dma_refusal_name(e),
            ExecError::PollTimeout { .. } => "POLL-TIMEOUT",
            ExecError::GgttStoreRefused { .. } => "GGTT-STORE-NOT-ALLOWED",
            ExecError::OutsideGrant { .. } => "OUTSIDE-DMA-GRANT",
            ExecError::AwaitRetireTimeout { .. } => "AWAIT-RETIRE-TIMEOUT",
            ExecError::EndowmentExhausted { .. } => "DMA-ENDOWMENT-EXHAUSTED",
            ExecError::GrantTableFull { .. } => "GRANT-TABLE-FULL",
            ExecError::PreviousStepNotDone { .. } => "PREVIOUS-STEP-NOT-DONE",
            ExecError::StepNested { .. } => "STEP-NESTED",
            ExecError::Bound { .. } => "EXECUTOR-BOUND",
            ExecError::Layout { .. } => "LAYOUT",
        }
    }
}

/// The stable token of a DMA refusal (`DmaRefusal` carries none).
pub const fn dma_refusal_name(e: &DmaRefusal) -> &'static str {
    match e {
        DmaRefusal::ZeroLength => "DMA-ZERO-LENGTH",
        DmaRefusal::BadAlign { .. } => "DMA-BAD-ALIGN",
        DmaRefusal::Misplaced { .. } => "DMA-MISPLACED",
        DmaRefusal::OverQuota { .. } => "DMA-OVER-QUOTA",
        DmaRefusal::OutOfMemory { .. } => "DMA-OUT-OF-MEMORY",
        DmaRefusal::CacheTypeUnavailable => "DMA-CACHE-TYPE-UNAVAILABLE",
        DmaRefusal::BadAddrWidth { .. } => "DMA-BAD-ADDR-WIDTH",
        DmaRefusal::BeyondAddrWidth { .. } => "DMA-BEYOND-ADDR-WIDTH",
        DmaRefusal::Unsupported => "DMA-FREE-UNSUPPORTED",
        DmaRefusal::NotGranted { .. } => "DMA-NOT-GRANTED",
        DmaRefusal::Host(_) => "DMA-HOST-REFUSED",
    }
}

macro_rules! from_err {
    ($($t:ty => $v:ident),*) => {$(impl From<$t> for ExecError { fn from(e: $t) -> Self { ExecError::$v(e) } })*};
}
from_err!(I915Error => Hw, MemError => Mem, SubmitError => Submit, GtError => Gt, ClockError => Clock, DmaRefusal => Dma);
