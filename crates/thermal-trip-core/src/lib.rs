// SPDX-License-Identifier: GPL-2.0-only
//! Thermal zone trip-point logic, ported from Linux thermal core sources.
//!
//! Ported mechanically from:
//!   * `drivers/thermal/thermal_core.c` — directional crossing and threshold ordering
//!   * `drivers/thermal/thermal_trip.c` — trip type names
//!   * `drivers/thermal/thermal_sysfs.c` — trip temperature and hysteresis validation
//!   * `include/linux/thermal.h` — trip layout and invalid-temperature sentinel
//!   * `include/uapi/linux/thermal.h` — trip type values
//!
//! Copyright (C) 2008 Intel Corp
//! Copyright (C) 2008 Zhang Rui <rui.zhang@intel.com>
//! Copyright (C) 2008 Sujith Thomas <sujith.thomas@intel.com>
//! Copyright 2022 Linaro Limited
//!
//! This crate is pure arithmetic over caller-supplied temperatures. It performs no MMIO or I/O.

#![no_std]
#![forbid(unsafe_code)]

pub mod trip;
