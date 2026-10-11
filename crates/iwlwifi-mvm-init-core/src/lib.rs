// SPDX-License-Identifier: GPL-2.0-only
//! The INIT-image flow of the iwlwifi MVM op-mode for the NON-unified 9000-family image (Wireless-AC
//! 9260): the host-command encoders (`TX_ANT_CONFIGURATION_CMD`, `PHY_CONFIGURATION_CMD` v1,
//! `NVM_ACCESS_CMD`), the decoders (`NVM_ACCESS_CMD` response, `CALIB_RES_NOTIF_PHY_DB`) and a pure
//! state machine that mirrors `iwl_run_init_mvm_ucode` — ALIVE wait, the NVM walk, the rfkill check,
//! the two PHY commands and the wait for `INIT_COMPLETE_NOTIF`.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/mvm/fw.c` (`iwl_run_init_mvm_ucode`,
//! `iwl_send_tx_ant_cfg`, `iwl_send_phy_cfg_cmd`, `iwl_wait_phy_db_entry`,
//! `iwl_mvm_load_ucode_wait_alive`), `mvm/nvm.c` (`iwl_nvm_init`, `iwl_nvm_read_section`,
//! `iwl_nvm_read_chunk`, `iwl_parse_nvm_sections`), `mvm/mvm.h` (`iwl_mvm_get_phy_config`),
//! `mvm/utils.c` (`iwl_mvm_send_cmd`), `fw/api/{commands,config,nvm-reg,cmdhdr}.h`, `fw/file.h`,
//! `fw/notif-wait.c`, `iwl-phy-db.c` and `iwl-drv.c` (Linux v7.0-rc6).
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! No MMIO, no allocation, no I/O: the caller performs each [`machine::Action`] and feeds the
//! outcome back as a [`machine::Event`].
//!
//! Left out on purpose (Linux does it, this crate does not): the `< IWL_DEVICE_FAMILY_8000` BT-init
//! branch (`fw.c:733-737`, the 9000 family never takes it); the unified-image path
//! (`iwl_run_unified_mvm_ucode`); `iwl_nvm_fixups`; the external NVM file (`nvm_file_name`,
//! `iwl_mvm_load_nvm_to_nic`); the NVM-version WARN (`fw.c:755`); the fake `nvm_data` on failure
//! (`fw.c:806-820`); PNVM load and `iwl_trans_fw_alive` after ALIVE; PHY-DB storage (the channel
//! group allocation of `iwl_phy_db_set_section`); `iwl_parse_nvm_data` itself.

#![no_std]
#![forbid(unsafe_code)]

pub mod decode;
pub mod encode;
pub mod ids;
pub mod machine;
