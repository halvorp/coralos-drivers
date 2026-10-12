// SPDX-License-Identifier: GPL-2.0-only
//! The cAVS IPC of Linux's AVS driver for Skylake / Kaby Lake: the message codec (global, module, reply and notification
//! headers; every request builder), the payload codecs (firmware/hardware configuration TLVs, the modules-info table, the
//! module configuration blobs), the interrupt-side planner of `avs_skl_ipc_interrupt`, and the request/reply transaction
//! state machine of `ipc.c` with every wait bounded.
//!
//! Ported from Linux `sound/soc/intel/avs/messages.h`, `messages.c`, `ipc.c`, `skl.c` and the HIPC rows of `registers.h` /
//! `core.c` (Linux v7.0-rc6). Copyright(c) 2021-2022 Intel Corporation; authors Cezary Rojewski and Amadeusz Slawinski.
//! The Linux files are `GPL-2.0-only`; this port is distributed under GPL-2.0-only.
//!
//! THE CRATE DOES NOT TOUCH THE DEVICE. Headers are `u64` (primary in the low dword, extension in the high dword, exactly
//! `union avs_global_msg`'s `val`). The transaction and the interrupt planner are coroutines: the caller performs what they
//! ask for and reports back. Where Linux silently truncates a bit-field, lets a TLV walk run past its buffer, or waits
//! without a bound (`wait_for_completion_killable`, ipc.c:337), this port refuses by name or bounds the wait; each such
//! place is listed in the doc of the function that does it.
//!
//! Not ported: the APL/CNL/ICL/MTL HIPC rows, D0ix (a no-op on cAVS 1.5: skl.c:153-163), probes, and the log/coredump handlers
//! (a notification that asks for them is surfaced as an event, the dump itself is the `.device`'s).

#![no_std]
#![forbid(unsafe_code)]

pub mod error;
pub mod header;
pub mod isr;
pub mod modcfg;
pub mod notify;
pub mod payload;
pub mod regs;
pub mod request;
pub mod txn;
