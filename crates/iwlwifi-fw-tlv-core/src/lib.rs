// SPDX-License-Identifier: GPL-2.0-only
//! The iwlwifi firmware file's TLV container and the capability/parameter TLVs the 9000-family
//! (Wireless-AC 9260) bring-up branches on.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/iwl-drv.c` (`iwl_parse_tlv_firmware`,
//! `iwl_set_ucode_api_flags`, `iwl_set_ucode_capabilities`), with the layouts and enums from
//! `drivers/net/wireless/intel/iwlwifi/fw/file.h`, `fw/img.h` and `fw/api/alive.h` (Linux v7.0-rc6).
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! The crate parses bytes the caller already holds. It owns no buffer, performs no I/O and does not
//! touch the uCode SECTIONS (their contents are firmware, never inspected); it reads only the
//! documented container and the scalar TLVs.

#![no_std]
#![forbid(unsafe_code)]

pub mod capa;
pub mod container;
pub mod params;
