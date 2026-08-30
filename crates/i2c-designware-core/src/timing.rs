// SPDX-License-Identifier: GPL-2.0-only
//! SCL clock counts — how long the controller holds the line high and low.
//!
//! Ported from Linux `drivers/i2c/busses/`:
//!   * `i2c_dw_scl_hcnt` / `i2c_dw_scl_lcnt` (i2c-designware-common.c:527, :550)
//!   * the per-mode timing values passed by `i2c_dw_set_timings_master`
//!     (i2c-designware-master.c: standard :60-:73, fast :115-:128, fast-plus :92-:105,
//!     high-speed :165-:178)
//!
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.
//!
//! This is the arithmetic that decides whether the bus meets its timing spec. It is pure — no
//! register touches, no hardware — which means it can be pinned EXACTLY against Linux's own
//! formula, and getting it wrong produces a bus that mostly works and fails under load or at
//! temperature. That is the worst failure mode this crate can ship, so it is tested first.

/// `MICRO` (include/linux/units.h:19) — the formulas work in nanoseconds over a Hz clock.
pub const MICRO: u64 = 1_000_000;

/// Linux's default fall time when the platform does not supply one, in nanoseconds.
///
/// i2c-designware-master.c:53-54: `sda_falling_time = t->sda_fall_ns ?: 300` and likewise for scl.
/// The comment in `i2c_dw_scl_lcnt` says "Default tf value should be 0.3 us, for safety."
pub const DEFAULT_FALL_NS: u32 = 300;

/// `DIV_ROUND_CLOSEST_ULL(x, d)` — round to nearest, halves away from zero.
///
/// Truncating instead (plain `x / d`) shortens every count by up to one tick, which is a timing
/// violation the bus will not report — it simply misbehaves.
fn div_round_closest(x: u64, d: u64) -> u64 {
    (x + d / 2) / d
}

/// `i2c_dw_scl_hcnt` (i2c-designware-common.c:547).
///
/// `hcnt = DIV_ROUND_CLOSEST(ic_clk * (t_symbol + tf), MICRO) - 3 + offset`
///
/// THE MINUS THREE IS NOT THE MINUS ONE IN [`scl_lcnt`]. Linux's comment explains it: the tHD;STA
/// period turned out to be proportional to (HCNT + 3), so the register is pre-compensated by three
/// ticks and the low count by one. A port that factors these into one helper with a shared constant
/// gets one of the two wrong, and the bus still mostly works.
pub fn scl_hcnt(ic_clk_hz: u32, t_symbol_ns: u32, tf_ns: u32, offset: i32) -> i64 {
    let raw = div_round_closest(ic_clk_hz as u64 * (t_symbol_ns as u64 + tf_ns as u64), MICRO);
    raw as i64 - 3 + offset as i64
}

/// `i2c_dw_scl_lcnt` (i2c-designware-common.c:567).
///
/// `lcnt = DIV_ROUND_CLOSEST(ic_clk * (t_low + tf), MICRO) - 1 + offset`
///
/// The fall time is part of the LOW period by construction: the core starts counting as soon as it
/// pulls SCL, so the time the line takes to actually fall is already being counted.
pub fn scl_lcnt(ic_clk_hz: u32, t_low_ns: u32, tf_ns: u32, offset: i32) -> i64 {
    let raw = div_round_closest(ic_clk_hz as u64 * (t_low_ns as u64 + tf_ns as u64), MICRO);
    raw as i64 - 1 + offset as i64
}

/// The timing values Linux passes for one bus speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeTiming {
    /// tHD;STA / tHIGH, nanoseconds.
    pub t_high_ns: u32,
    /// tLOW, nanoseconds.
    pub t_low_ns: u32,
}

/// Standard mode — i2c-designware-master.c:64, :71.
pub const STANDARD: ModeTiming = ModeTiming { t_high_ns: 4000, t_low_ns: 4700 };
/// Fast mode — i2c-designware-master.c:119, :126.
pub const FAST: ModeTiming = ModeTiming { t_high_ns: 600, t_low_ns: 1300 };
/// Fast mode plus — i2c-designware-master.c:96, :103.
pub const FAST_PLUS: ModeTiming = ModeTiming { t_high_ns: 260, t_low_ns: 500 };

/// Both counts for a mode, using Linux's default fall times.
///
/// THE TWO FALL TIMES ARE NOT THE SAME INPUT. `i2c_dw_scl_hcnt` is called with
/// `sda_falling_time` and `i2c_dw_scl_lcnt` with `scl_falling_time` (master.c:66/:73 and
/// throughout). They default to the same 300 ns, which is exactly why collapsing them into one
/// parameter passes every default-configured test and then silently ignores a platform that
/// specifies only one of them.
pub fn counts_for(mode: ModeTiming, ic_clk_hz: u32, sda_fall_ns: u32, scl_fall_ns: u32) -> (i64, i64) {
    (
        scl_hcnt(ic_clk_hz, mode.t_high_ns, sda_fall_ns, 0),
        scl_lcnt(ic_clk_hz, mode.t_low_ns, scl_fall_ns, 0),
    )
}

/// Whether a computed count can actually be programmed.
///
/// `ic_clk == 0` means "read the count back from the register instead of computing it"
/// (i2c-designware-common.c:530, :553) — a distinct case that must NOT reach the arithmetic, since
/// the formula would return a negative number and a cast would turn it into an enormous count.
pub fn is_programmable(count: i64) -> bool {
    count > 0 && count <= u16::MAX as i64
}
