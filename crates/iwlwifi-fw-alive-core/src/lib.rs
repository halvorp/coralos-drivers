// SPDX-License-Identifier: GPL-2.0-only
//! What the iwlwifi firmware tells the host about itself once it runs — and what to read when it
//! dies: the RX packet header, the ALIVE notification (v3 layout, the one the Wireless-AC 9260's
//! -46 firmware sends), the LMAC/UMAC error-event tables the ALIVE points at, and the one-line
//! markers a driver with no serial console logs them as.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/fw/api/alive.h`, `fw/api/cmdhdr.h`,
//! `fw/api/commands.h`, `iwl-trans.h`, `mvm/fw.c` (`iwl_alive_fn`, `iwl_mvm_load_ucode_wait_alive`,
//! `iwl_mvm_print_pd_notification`), `fw/dump.c` (`iwl_fwrt_dump_lmac_error_log`,
//! `iwl_fwrt_dump_umac_error_log`), `fw/img.c` (`iwl_fw_lookup_assert_desc`), `fw/img.h` and
//! `iwl-prph.h` (Linux v7.0-rc6).
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! Decode only: the caller reads device memory (`iwl_trans_read_mem_bytes`) and registers and hands
//! the words in. No I/O, no allocation — the markers are `core::fmt::Display`.

#![no_std]
#![forbid(unsafe_code)]

pub mod alive;
pub mod dump;
pub mod error_table;
pub mod packet;
