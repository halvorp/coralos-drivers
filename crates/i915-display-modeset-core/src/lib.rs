// SPDX-License-Identifier: GPL-2.0-only
//! i915-display-modeset-core: the Kaby Lake (Gen9.5, display ver 9) eDP MODESET as planners - an ordered pipe-enable transaction
//! that composes `display-clock-core` (power wells, CDCLK, DPLL, DDI clock), `display-link-core` (PPS, DDI buffers, sink power,
//! link training, PCH PWM backlight) and this crate's plane / watermark / DDB plans, each stage with a readback and a rollback; the
//! PCU memory-latency mailbox read the watermarks need; and the ADOPTION path - the first plane write, a same-layout surface
//! replacement whose whole write set is one register.
//!
//! WHY A NEW CRATE (PLAN §1 item 11 said "extend `i915-kbl-display-core`"): the transaction has to NAME `display-clock-core::
//! Transaction` and `display-link-core::Plan`, and both of those crates depend on `i915-kbl-display-core` (it homes the registers
//! they use). Putting the transaction inside kbl-display-core would be a dependency cycle. So the split is by the DAG:
//!  * `i915-kbl-display-core` (below) holds every VALUE Linux computes - `skl_plane`, `skl_wm`, `mode_write`, `skl_pfit` - as pure
//!    functions returning `(register, value)` pairs in Linux's order;
//!  * this crate (above) turns them into bounded, rollback-carrying effect plans and orders them with the other crates' plans.
//!
//! No MMIO, no `unsafe`. `devices/i915` alone executes a plan, through a `WriteGuard`; the allow-list it must use for the first
//! write increment is [`adopt::ADOPT_WRITE_ALLOWLIST`] (this task does not edit the device's own list).
//!
//! Modules: [`enable`] (the transaction), [`plane_plan`] (plane + watermark + cursor effect plans), [`latency`] (the PCU read),
//! [`adopt`] (P4), [`initial`] (what the firmware scans out, its GGTT extent, the one mode an adopting driver offers), [`plan`] (stage
//! vocabulary), [`error`] (named refusals).
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

pub mod adopt;
pub mod enable;
pub mod error;
pub mod initial;
pub mod latency;
pub mod plan;
pub mod plane_plan;

pub use error::ModesetError;
pub use plan::{Body, Interactive, Plan, Readback, Stage, StageId};
