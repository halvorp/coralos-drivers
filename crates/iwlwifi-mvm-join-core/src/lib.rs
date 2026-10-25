// SPDX-License-Identifier: GPL-2.0-only
//! Joining an access point with the iwlwifi MVM op-mode (station, legacy rates, CCMP): the host
//! commands that bind a PHY, a MAC and the AP station together (`PHY_CONTEXT_CMD`,
//! `MAC_CONTEXT_CMD`, `BINDING_CONTEXT_CMD`, `ADD_STA`, `TIME_EVENT_CMD`, `MCAST_FILTER_CMD`,
//! `REMOVE_STA`), the `TX_CMD` that carries every transmitted frame and the response that
//! reclaims it, the IEEE 802.11 frames of the join (authentication, association, deauthentication,
//! data with LLC/SNAP, the CCMP header), and the reduction of a received beacon to what the
//! join needs.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/mvm/{phy-ctxt,mac-ctxt,binding,time-event,
//! sta,tx,rxmq,quota}.c`, `fw/api/{phy-ctxt,context,binding,time-event,sta,tx,rx,mac,filter,rs,txq,
//! commands}.h`, `pcie/gen1_2/tx.c` and `iwl-trans.h` (Linux v7.0-rc6), with OpenBSD
//! `sys/dev/pci/if_iwm.c` (`iwm_auth`, `iwm_run`, `iwm_tx`, `iwm_ack_rates`) as the sequencing
//! oracle: it drives this same firmware. Frame layouts are IEEE Std 802.11-2020 9.2 and 9.3.
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! WHICH LAYOUTS. Read from the Atlas firmware `iwlwifi-9260-th-b0-jf-b0-46.ucode`: command
//! versions `PHY_CONTEXT_CMD` 2, `BINDING_CONTEXT_CMD` 1, `TIME_EVENT_CMD` 2, `ADD_STA` 10,
//! `MAC_CONTEXT_CMD` 4, `TX_CMD` 6 (rates version 1); capabilities `ULTRA_HB_CHANNELS` (the
//! 8-byte channel info) and `DYNAMIC_QUOTA` (no `TIME_QUOTA_CMD`, `mvm/quota.c:106`) set,
//! `BINDING_CDB_SUPPORT` and `CDB_SUPPORT` (no `lmac_id`) clear, `API_STA_TYPE` set,
//! `API_TKIP_MIC_KEYS` clear.
//!
//! No MMIO, no allocation, no I/O. Not ported: HT/VHT/HE (every frame is legacy 11a/g), A-MPDU,
//! QoS data, U-APSD, power save, beacon filtering and the link-quality table (`LQ_CMD`): data
//! frames are sent at one host-chosen rate, named in the caller.

#![no_std]
#![forbid(unsafe_code)]

pub mod bss;
pub mod ctxt;
pub mod dot11;
pub mod ids;
pub mod rates;
pub mod rx;
pub mod txcmd;

/// An encoder refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// The caller buffer cannot hold the command or frame.
    BufferTooSmall { need: usize, have: usize },
    /// A field is outside what the layout can carry; `what` names it.
    OutOfRange { what: &'static str, value: u32, max: u32 },
}

pub(crate) fn check(buf: &mut [u8], need: usize) -> Result<(), EncodeError> {
    if buf.len() < need {
        return Err(EncodeError::BufferTooSmall { need, have: buf.len() });
    }
    buf[..need].fill(0);
    Ok(())
}

pub(crate) fn put16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn put32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn put64(b: &mut [u8], off: usize, v: u64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn le16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
pub(crate) fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
