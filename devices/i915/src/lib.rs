// SPDX-License-Identifier: GPL-2.0-only
//! i915.device — the Intel integrated GPU as a ring-3 CoralOS `.device`. STEP 2 of Astra's Kaby Lake GPU ruling
//! (2026-10-12): the public device boundary, identity, grant-bounded MMIO and a READ-ONLY display readout; no register
//! is written, no firmware loaded, nothing programmed.
//!
//! WHAT IT DOES, in order: [`startup`] reads the PCI configuration page (slot 1, read-only) and checks the id against the
//! Kaby Lake table (`i915-kbl-display-core`) - any other id is refused BY NAME, never aliased to another generation's
//! row; it opens BAR0 (slot 0, read-only, at least the display register block long); it runs the display readout and
//! LOGS it: one `[I915-KBL-READOUT pipe=…]` line per pipe, then the timing of each active pipe, the power wells, CDCLK,
//! the DDIs and DPLLs that are on, any extra plane or scaler. Then [`service`] parks and answers every wire request
//! with an explicit UNSUPPORTED ([`wire`]).
//!
//! WHAT IT HOLDS: BAR0 and the config page, both read-only (`coralos_rt::mmio::ReadWindow` has no write). Nothing else.
//!
//! The readout is validated against Linux by the reconnaissance boot (see i915-kbl-display-core's crate docs).
//!
//! Copyright (c) Intel Corporation and the i915 authors (the core).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod identity;
pub mod log;
pub mod regio;
pub mod report;
#[cfg(target_arch = "x86_64")]
pub mod service;
pub mod startup;
pub mod wire;

pub use startup::{startup, Ready, StartError};
#[cfg(target_arch = "x86_64")]
pub use service::{on_panic, service};
