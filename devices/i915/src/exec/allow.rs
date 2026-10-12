// SPDX-License-Identifier: GPL-2.0-only
//! The write allow-list, built at COMPILE TIME from the increment features (plan KBL-I915-PORT-PLAN sections 3 and 5).
//!
//! DEFAULT BUILD: exactly the three Gen9 forcewake SET registers ([`i915_uncore_core::WRITE_ALLOWLIST`]). Nothing else is writable,
//! the GGTT window is closed, and every other register write the executor is asked to make is REFUSED by name and counted.
//!
//! `p2-memory` adds [`P2_MEMORY_REGS`] and opens the GGTT window ([`GGTT_WINDOW`]); `p3-exec` (which implies `p2-memory`) adds
//! [`P3_EXEC_REGS`]. Each list is the write set of that increment's planners, no more: the tests in `tests/exec_allow.rs` RUN the
//! planners and require set equality, so a register that no planner writes cannot be granted and a planner write that is not
//! granted fails there before it can fail on silicon. Display registers are in no list: a clock or link plan is refused whole.
//!
//! The same lists size the BAR0 WINDOW grant ([`BAR0_WRITE_RANGES`]), so the hardware-facing half (`rt::mmio::WriteWindow`) and the
//! in-code half (`i915_common_core::WriteGuard`) can never disagree about what the increment may write.
use coralos_rt::mmio::WriteRange;
use i915_common_core::Reg;
use i915_gt_regs_core as r;
use i915_uncore_core::WRITE_ALLOWLIST;

/// Is increment P2 (memory) compiled in?
pub const P2_ON: bool = cfg!(feature = "p2-memory");
/// Is increment P3 (execlists + reset) compiled in?
pub const P3_ON: bool = cfg!(feature = "p3-exec");

/// The GGTT's place in BAR0 (`gen6_gttadr_offset`, gt/intel_ggtt.c:1192: GTTMMADR is 16 MiB and the GTT starts halfway) and the
/// most it can occupy (the upper half). The RUNTIME window is narrowed to the probed table size by [`GgttWindow`].
pub const GGTT_WINDOW: GgttWindow = GgttWindow { base: 8 << 20, bytes: 8 << 20 };

/// The byte range of BAR0 that GGTT entry stores may land in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GgttWindow {
    pub base: u32,
    pub bytes: u32,
}

impl GgttWindow {
    /// Does an 8-byte entry store at BAR0 offset `off` lie wholly inside the window? (4-byte aligned halves.)
    pub const fn holds_entry(&self, off: u32) -> bool {
        off % 8 == 0 && off >= self.base && off as u64 + 8 <= self.base as u64 + self.bytes as u64
    }
    /// The window narrowed to the probed GGTT table (`GgttGeometry::pte_base`, `entries`), never wider than [`GGTT_WINDOW`].
    pub const fn for_table(pte_base: u32, entries: u64) -> Option<GgttWindow> {
        let bytes = entries * 8;
        if pte_base != GGTT_WINDOW.base || bytes == 0 || bytes > GGTT_WINDOW.bytes as u64 {
            return None;
        }
        Some(GgttWindow { base: pte_base, bytes: bytes as u32 })
    }
}

/// P2: the planners' register write set. `BindPlan::flushes` ends a GGTT bind with `Flush::GgttInvalidate(GFX_FLSH_CNTL_GEN6 <-
/// GFX_FLSH_CNTL_EN)` (`gen8_ggtt_invalidate`, gt/intel_ggtt.c:217-230). That is the ONLY register P2 writes; the entries themselves
/// are stores into the GGTT window, not registers.
pub const P2_MEMORY_REGS: [Reg; 1] = [r::GFX_FLSH_CNTL_GEN6];

/// P3: the render engine's execlist registers and the reset handshake registers (each base from `intel_engine_cs.c:64-200` via
/// `i915-gt-regs-core`). Per planner:
///  * `execlists::elsp_writes` - `RING_ELSP(RCS0)` (gt/intel_execlists_submission.c:723-732, :905-939);
///  * `execlists::plan_enable_execlists` - `RING_HWSTAM`, `RING_MODE_GEN7`, `RING_MI_MODE`, `RING_HWS_PGA` of RCS0 (:2928-2950);
///  * `execlists::plan_reset_csb_pointers` - `RING_CONTEXT_STATUS_PTR(RCS0)` (:2804-2844);
///  * `reset::ResetPlanner` - `RING_RESET_CTL` of every engine of the GT (a full reset prepares all of them, gt/intel_reset.c:560-606)
///    and `GEN6_GDRST` (:266-316).
pub const P3_EXEC_REGS: [Reg; 11] = [
    r::ring_elsp(r::RENDER_RING_BASE),
    r::ring_hwstam(r::RENDER_RING_BASE),
    r::ring_mode_gen7(r::RENDER_RING_BASE),
    r::ring_mi_mode(r::RENDER_RING_BASE),
    r::ring_hws_pga(r::RENDER_RING_BASE),
    r::ring_context_status_ptr(r::RENDER_RING_BASE),
    r::ring_reset_ctl(r::RENDER_RING_BASE),
    r::ring_reset_ctl(r::BLT_RING_BASE),
    r::ring_reset_ctl(r::GEN6_BSD_RING_BASE),
    r::ring_reset_ctl(r::VEBOX_RING_BASE),
    r::GEN6_GDRST,
];

const fn on(b: bool, n: usize) -> usize {
    if b {
        n
    } else {
        0
    }
}

/// How many registers the compiled-in allow-list holds.
pub const N_ALLOW: usize = WRITE_ALLOWLIST.len() + on(P2_ON, P2_MEMORY_REGS.len()) + on(P3_ON, P3_EXEC_REGS.len());

const fn compose() -> [Reg; N_ALLOW] {
    let mut out = [Reg::new(0); N_ALLOW];
    let mut n = 0;
    let mut i = 0;
    while i < WRITE_ALLOWLIST.len() {
        out[n] = WRITE_ALLOWLIST[i];
        n += 1;
        i += 1;
    }
    if P2_ON {
        let mut i = 0;
        while i < P2_MEMORY_REGS.len() {
            out[n] = P2_MEMORY_REGS[i];
            n += 1;
            i += 1;
        }
    }
    if P3_ON {
        let mut i = 0;
        while i < P3_EXEC_REGS.len() {
            out[n] = P3_EXEC_REGS[i];
            n += 1;
            i += 1;
        }
    }
    out
}

/// THE allow-list the executor's `WriteGuard` is built over. Three registers in the default build.
pub static ALLOW: [Reg; N_ALLOW] = compose();

/// The GGTT window the executor may store entries into in THIS build (`None` = closed, every entry store is refused).
pub const fn compiled_ggtt_window() -> Option<GgttWindow> {
    if P2_ON {
        Some(GGTT_WINDOW)
    } else {
        None
    }
}

/// How many ranges the BAR0 window grant holds: one per allow-listed register, plus the GGTT window under P2.
pub const N_RANGES: usize = N_ALLOW + on(P2_ON, 1);

const fn ranges() -> [WriteRange; N_RANGES] {
    let mut out = [WriteRange::reg32(0); N_RANGES];
    let mut i = 0;
    while i < N_ALLOW {
        out[i] = WriteRange::reg32(ALLOW[i].offset());
        i += 1;
    }
    if P2_ON {
        out[N_ALLOW] = WriteRange { off: GGTT_WINDOW.base as u64, len: GGTT_WINDOW.bytes as u64 };
    }
    out
}

/// BAR0's MMIO write grant (the hardware-facing half), built from [`ALLOW`] so the two cannot drift. Default build: the three
/// forcewake SET registers, each one 32-bit register.
pub static BAR0_WRITE_RANGES: [WriteRange; N_RANGES] = ranges();

/// The part of BAR0 the whole device touches. Default: up to and including `GEN6_RP_STATE_CAP` (0x145998, intel_mchbar_regs.h:225)
/// and the GT forcewake ack (0x130044). With P2 the whole 16 MiB GTTMMADR (the GGTT is its upper half, gt/intel_ggtt.c:1187).
pub const BAR0_NEED_BYTES: u64 = if P2_ON { 16 << 20 } else { 0x14_59a0 };
