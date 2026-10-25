// SPDX-License-Identifier: GPL-2.0-only
//! Scan (M3) - NOT BUILT. This module is deliberately empty; nothing here is claimed.
//!
//! What M3 needs, in order, each with its Linux/OpenBSD source (the breakdown the next session starts from):
//!  1. RT firmware restart after INIT: `iwl_mvm_up` mvm/fw.c (load REGULAR, ALIVE v3 again, paging download fw/paging.c).
//!  2. The post-ALIVE commands: PHY_DB send, TX_ANT_CONFIGURATION, PHY_CONFIGURATION, MCC_UPDATE (28 bytes, v3 reply),
//!     SCAN_CFG_CMD (`iwl_scan_config`, OpenBSD `iwm_config_umac_scan` if_iwm.c:7837).
//!  3. SCAN_REQ_UMAC v8 passive (OpenBSD `iwm_umac_scan`: PASS_ALL|ITER_COMPLETE|PASSIVE, dwell 10/110, one iteration).
//!  4. Async command send and the SCAN_COMPLETE_UMAC notification; RX_MPDU v1 parse; beacon IE walk into [`crate::event::Bss`].
//!  5. A simulated RT firmware in tests/sim answering all of the above.
//! The aux station and data queues are not needed for a passive scan and stay out.
