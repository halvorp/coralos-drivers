// SPDX-License-Identifier: GPL-2.0-only
//! The firmware IMAGES inside an iwlwifi TLV ucode file: which sections belong to the INIT and the
//! REGULAR (runtime) image, in what order, loaded to which device address; the default calibration
//! triggers; and the per-command version table the host consults before it encodes anything.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/iwl-drv.c` (`iwl_store_ucode_sec`,
//! `iwl_set_default_calib`, the SEC_RT / SEC_INIT / SECURE_SEC_* / DEF_CALIB / CMD_VERSIONS arms of
//! `iwl_parse_tlv_firmware`), `fw/file.h`, `fw/img.h` and `fw/img.c` (`iwl_fw_lookup_cmd_ver`,
//! `iwl_fw_lookup_notif_ver`). Linux v7.0-rc6.
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! The crate allocates nothing: a [`sections::Section`] borrows the blob, and the walk is an
//! iterator, so the caller owns whatever table it wants (a `Vec` in the device, which is where
//! Linux's `krealloc` lives). `iwl-drv.c` checks nothing about a section TLV shorter than its 4-byte
//! offset word (its `size - sizeof(offset)` underflows); the port refuses it by name.

#![no_std]
#![forbid(unsafe_code)]

pub mod calib;
pub mod cmdver;
pub mod sections;
