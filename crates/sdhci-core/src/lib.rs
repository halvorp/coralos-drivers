// SPDX-License-Identifier: GPL-2.0-or-later

#![cfg_attr(not(test), no_std)]

//! Pure-Rust cited port of the Linux SDHCI error-recovery core.
//!
//! Pinned source: Linux 7.0.0-rc, commit `9147566d801602c9e7fc7f85e989735735bf38ba`
//! (`drivers/mmc/host/sdhci.c` and `drivers/mmc/host/sdhci.h`).
//!
//! Scope (Sol round 3): recovery of an already-issued request only. This crate
//! does not set up commands, calculate timeouts, or manage clock / power / tuning
//! / CQE / DMA mapping. It receives a `RequestCtx` describing the in-flight
//! request and drives the Linux recovery / completion ordering as a pure reducer.
//!
//! CoralOS extensions (explicitly marked in source):
//!
//! * `Error::ResetStuck(mask)` — Linux logs and returns void when a reset bit
//!   stays set; CoralOS reports it and continues, so a stuck CMD reset still
//!   proceeds to the DATA reset.
//! * `UNDER-SOURCED:` abstract actions — functions not shown in the supplied
//!   excerpts are surfaced as explicit actions rather than inferred.

pub mod regs;
pub mod core;
pub mod executor;