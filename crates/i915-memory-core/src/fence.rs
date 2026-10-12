// SPDX-License-Identifier: GPL-2.0-only
//! Fence registers (tiled-surface detiling for CPU aperture access), Gen6+ layout. Ported from gt/intel_ggtt_fencing.c
//! (`i965_write_fence_reg`, :61; `intel_ggtt_init_fences`, :841) and the register block of i915_reg.h:205-219.
//!
//! A fence write is a four-step sequence (the 64-bit register is updated as two dwords, so the fence is switched OFF first and
//! ON last): the planner returns the steps; the executor performs them in order under the render forcewake the range needs.

use crate::regs::*;
use crate::MemError;
use i915_common_core::Reg;

/// `I915_TILING_*` (include/uapi/drm/i915_drm.h, `I915_TILING_NONE` = 0, `_X` = 1, `_Y` = 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tiling {
    None = 0,
    X = 1,
    Y = 2,
}

/// One step of a fence update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FenceStep {
    Write { reg: Reg, value: u32 },
    /// A posting read of the register, so the write has landed before the next step (`intel_uncore_posting_read_fw`).
    PostingRead { reg: Reg },
}

/// The five steps of `i965_write_fence_reg` (:106-111): LO <- 0, read LO, HI, LO, read LO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FencePlan {
    /// KEPT FIXED (D10): the fence write sequence is 5 steps (`fence_write` bracket) - hardware.
    pub steps: [FenceStep; 5],
}

/// Plan programming fence `id` for the GGTT range `[start, start + size)` with `tiling` and `stride` bytes (`Tiling::None`
/// clears the fence, as Linux does with `val = 0`).
pub fn plan_fence(id: u32, start: u64, size: u64, tiling: Tiling, stride: u32) -> Result<FencePlan, MemError> {
    if id >= GEN7_NUM_FENCES {
        return Err(MemError::BadFence { what: "fence id", value: id as u64 });
    }
    let (lo, hi) = (fence_lo(id), fence_hi(id));
    let mut val = 0u64;
    if tiling != Tiling::None {
        if size == 0 || size % I965_FENCE_PAGE != 0 {
            return Err(MemError::BadFence { what: "size", value: size });
        }
        if start % I965_FENCE_PAGE != 0 {
            return Err(MemError::BadFence { what: "start", value: start });
        }
        if stride == 0 || stride % 128 != 0 {
            return Err(MemError::BadFence { what: "stride", value: stride as u64 }); // GEM_BUG_ON(!IS_ALIGNED(stride, 128)), :82
        }
        if stride / 128 > GEN7_FENCE_MAX_PITCH_VAL {
            return Err(MemError::BadFence { what: "pitch", value: stride as u64 });
        }
        if start.checked_add(size).map_or(true, |e| e > u32::MAX as u64 + 1) {
            return Err(MemError::BadFence { what: "range past 4 GiB", value: start.wrapping_add(size) });
        }
        val = start + size - I965_FENCE_PAGE; // :84
        val <<= 32; // :85
        val |= start; // :86
        val |= (((stride / 128) - 1) as u64) << GEN6_FENCE_PITCH_SHIFT; // :87
        if tiling == Tiling::Y {
            val |= 1u64 << I965_FENCE_TILING_Y_SHIFT; // :88-89
        }
        val |= I965_FENCE_REG_VALID as u64; // :90
    }
    Ok(FencePlan {
        steps: [
            FenceStep::Write { reg: lo, value: 0 },
            FenceStep::PostingRead { reg: lo },
            FenceStep::Write { reg: hi, value: (val >> 32) as u32 },
            FenceStep::Write { reg: lo, value: val as u32 },
            FenceStep::PostingRead { reg: lo },
        ],
    })
}
