// SPDX-License-Identifier: GPL-2.0-only
//! The Kaby Lake / Skylake (cAVS 1.5 / 1.8) DSP firmware loader of Linux's AVS driver, as pure data and planners:
//! the firmware and library image envelope (extended manifest + manifest, version gate), the library table, the module
//! file name, the code-loader DMA (CLDMA) layout and transfer loop, and the base-firmware boot program.
//!
//! Ported from Linux `sound/soc/intel/avs/loader.c`, `skl.c`, `cldma.c`, `cldma.h`, `registers.h`, `dsp.c` and the
//! `skl_desc` rows of `core.c`, with the stream-descriptor constants of `include/sound/hda_register.h` (Linux v7.0-rc6).
//! Only the paths that apply to Kaby Lake (`HDA_KBL_LP` 8086:9d71 binds `skl_desc`, core.c:893-896) are ported: the CLDMA
//! path of `avs_cldma_load_basefw`. The APL/ICL/MTL host-DMA path (`avs_hda_load_basefw`, IMR) is NOT ported.
//!
//! Copyright(c) 2021-2022 Intel Corporation; authors Cezary Rojewski and Amadeusz Slawinski. The Linux files are
//! `GPL-2.0-only`; this port is distributed under GPL-2.0-only.
//!
//! THE CRATE DOES NOT TOUCH THE DEVICE. Every planner is a coroutine (`Runner`, `Transfer`, `BaseFwBoot`): the caller feeds
//! it what the last operation produced and it answers with the next [`effect::Op`]. Every wait is bounded TWICE - by Linux's
//! microsecond budget (carried in the op for the caller's clock) and by a read count that THIS crate counts and enforces, so
//! a caller with no clock still terminates with a NAMED failure. Linux's unbounded or unchecked spots are refused by name
//! (extended-manifest length past the image, BDL longer than the 256 entries one 4 KiB page holds, a zero-sized CLDMA
//! buffer, an SPIB beyond the buffer) and each is listed in the doc of the function that refuses it.
//!
//! The firmware BYTES are never inspected beyond the envelope (see [`manifest`]) and are never embedded or committed.

#![no_std]
#![forbid(unsafe_code)]

pub mod bdl;
pub mod boot;
pub mod cldma;
pub mod effect;
pub mod error;
pub mod library;
pub mod manifest;
pub mod programs;
pub mod regs;
pub mod runner;
