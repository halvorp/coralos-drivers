// SPDX-License-Identifier: GPL-2.0-only
//! Intel Low Power Subsystem PWM controller core.
//!
//! Mechanically ported from Linux `drivers/pwm/pwm-lpss.c` and
//! `drivers/pwm/pwm-lpss.h`: register definitions, board data, the duty/period
//! conversion, and the update/wait ordering.
//!
//! Copyright (C) 2014 Intel Corporation.
//! Original authors: Mika Westerberg, Chew Kean Ho, Chang Rebecca Swee Fun,
//! Chew Chiau Ee, and Alan Cox.
//!
//! This crate performs no MMIO and no I/O. Callers supply sampled register
//! values and execute the returned writes or sequencing actions.

#![no_std]
#![forbid(unsafe_code)]

pub mod board;
pub mod encode;
pub mod regs;
pub mod sequence;
