// SPDX-License-Identifier: GPL-2.0-only
//! Pure SDHCI ACPI policy: HID/UID slot quirks and capability fix-ups.
//!
//! Ported mechanically from Linux `drivers/mmc/host/sdhci-acpi.c`, with bit
//! literals from `drivers/mmc/host/sdhci.h` and `include/linux/mmc/{host,pm}.h`.
//!
//! Copyright (c) 2012, Intel Corporation.
//! Copyright holders of the Linux SDHCI and MMC subsystems.
//!
//! This crate performs no ACPI calls, MMIO, allocation, or I/O. Callers pass
//! identifiers and capability words in and receive immutable policy values out.

#![no_std]
#![forbid(unsafe_code)]

pub mod caps;
pub mod slots;
