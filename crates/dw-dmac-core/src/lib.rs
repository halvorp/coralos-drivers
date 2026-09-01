// SPDX-License-Identifier: GPL-2.0-only
//! Pure word construction and decoding for the Synopsys DesignWare AHB DMA controller.
//!
//! Ported mechanically from Linux `drivers/dma/dw/regs.h` and
//! `drivers/dma/dw/core.c`. Original copyright holders: Atmel Corporation,
//! ST Microelectronics, Intel Corporation, Haavard Skinnemoen, and Viresh Kumar.
//!
//! This crate performs no MMIO and no I/O. It only describes the LLI wire layout,
//! constructs CTL_LO/CTL_HI words, and decodes channel interrupt status words.

#![no_std]
#![forbid(unsafe_code)]

pub mod ctl;
pub mod lli;
pub mod status;
