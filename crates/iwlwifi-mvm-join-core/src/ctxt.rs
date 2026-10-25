// SPDX-License-Identifier: GPL-2.0-only
//! Host-command payload encoders for the join: PHY context, MAC context (station), binding, AP
//! station, time event, multicast filter. Each writes the little-endian `__packed` Linux layout
//! into a caller buffer and returns the length.

use crate::ids::*;
use crate::rates::{self, ack_rates};
use crate::{check, le32, put16, put32, put64, EncodeError};

/// `FW_CMD_ID_AND_COLOR(id, color)`. // fw/api/context.h
pub fn id_and_color(id: u8, color: u8) -> u32 {
    ((id as u32) << FW_CTXT_ID_POS) | ((color as u32) << FW_CTXT_COLOR_POS)
}

// ------------------------------------------------------------------------------------------ PHY

/// The channel and chains a PHY context carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhyCtxt {
    pub id: u8,
    pub color: u8,
    pub channel: u8,
    pub band_5ghz: bool,
    /// `iwl_mvm_get_valid_tx_ant` (bit mask of chains).
    pub valid_tx_ant: u8,
    /// `iwl_mvm_get_valid_rx_ant`.
    pub valid_rx_ant: u8,
    /// `chains_static` -> `PHY_RX_CHAIN_CNT` and `chains_dynamic` -> `PHY_RX_CHAIN_MIMO_CNT`.
    pub chains_static: u8,
    pub chains_dynamic: u8,
}

/// `PHY_CONTEXT_CMD` version 2: `struct iwl_phy_context_cmd_v1` (20 MHz channel, no HT). `uhb` is
/// the firmware's `ULTRA_HB_CHANNELS` capability: it selects the 8-byte channel info (the 9260 -46
/// file has it) over the 4-byte one. // iwl_mvm_phy_ctxt_apply ver < 3 branch and
/// iwl_mvm_phy_ctxt_cmd_data_v1, mvm/phy-ctxt.c
pub fn phy_context(action: u32, c: &PhyCtxt, uhb: bool, out: &mut [u8]) -> Result<usize, EncodeError> {
    let len = if uhb { PHY_CTX_CMD_V1_UHB_LEN } else { PHY_CTX_CMD_V1_LEN };
    check(out, len)?;
    put32(out, 0, id_and_color(c.id, c.color));
    put32(out, 4, action);
    // apply_time @8 and tx_param_color @12 stay zero.
    let band = if c.band_5ghz { PHY_BAND_5 } else { PHY_BAND_24 };
    let tail;
    if uhb {
        put32(out, 16, c.channel as u32); // iwl_fw_channel_info: le32 channel, band, width, ctrl_pos, reserved
        out[20] = band;
        out[21] = IWL_PHY_CHANNEL_MODE20;
        out[22] = 0; // ctrl_pos: iwl_mvm_get_ctrl_pos() is 0 for a 20 MHz channel
        tail = 24;
    } else {
        out[16] = band; // iwl_fw_channel_info_v1: u8 band, channel, width, ctrl_pos
        out[17] = c.channel;
        out[18] = IWL_PHY_CHANNEL_MODE20;
        out[19] = 0;
        tail = 20;
    }
    // struct iwl_phy_context_cmd_tail { txchain_info, rxchain_info, acquisition_data, dsp_cfg_flags }
    put32(out, tail, c.valid_tx_ant as u32);
    let rx = ((c.valid_rx_ant as u32) << PHY_RX_CHAIN_VALID_POS)
        | ((c.chains_static as u32) << PHY_RX_CHAIN_CNT_POS)
        | ((c.chains_dynamic as u32) << PHY_RX_CHAIN_MIMO_CNT_POS);
    put32(out, tail + 4, rx);
    Ok(len)
}

// -------------------------------------------------------------------------------------- binding

/// `BINDING_CONTEXT_CMD` version 1 (`struct iwl_binding_cmd_v1`): one MAC bound to one PHY.
/// The firmware has no `BINDING_CDB_SUPPORT`, so no `lmac_id`. // iwl_mvm_binding_update, mvm/binding.c
pub fn binding_v1(action: u32, phy_id: u8, phy_color: u8, mac_id: u8, mac_color: u8, out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, BINDING_CMD_V1_LEN)?;
    let phy = id_and_color(phy_id, phy_color);
    put32(out, 0, phy);
    put32(out, 4, action);
    put32(out, 8, id_and_color(mac_id, mac_color));
    for i in 1..MAX_MACS_IN_BINDING {
        put32(out, 8 + 4 * i, FW_CTXT_INVALID);
    }
    put32(out, 8 + 4 * MAX_MACS_IN_BINDING, phy);
    Ok(BINDING_CMD_V1_LEN)
}

// ------------------------------------------------------------------------------------ time event

/// `TIME_EVENT_CMD` that protects the session while authenticating and associating: type
/// `TE_BSS_STA_AGGRESSIVE_ASSOC`, start immediately, notify host on start and end.
/// `duration` and `max_delay` are in TU (1024 us). // iwl_mvm_protect_session, mvm/time-event.c:645-698
pub fn time_event_protect(mac_id: u8, mac_color: u8, duration: u32, max_delay: u32, out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, TIME_EVENT_CMD_LEN)?;
    put32(out, 0, id_and_color(mac_id, mac_color));
    put32(out, 4, FW_CTXT_ACTION_ADD);
    put32(out, 8, TE_BSS_STA_AGGRESSIVE_ASSOC);
    put32(out, 12, 0); // apply_time
    put32(out, 16, max_delay);
    put32(out, 20, 0); // depends_on
    put32(out, 24, 1); // interval: "why do we need interval = bi if it is not periodic?"
    put32(out, 28, duration);
    out[32] = 1; // repeat
    out[33] = TE_V2_FRAG_NONE;
    put16(out, 34, TE_V2_NOTIF_HOST_EVENT_START | TE_V2_NOTIF_HOST_EVENT_END | TE_V2_START_IMMEDIATELY);
    Ok(TIME_EVENT_CMD_LEN)
}

/// Remove the time event `unique_id` the add returned. // iwl_mvm_stop_session_protection / OpenBSD iwm_unprotect_session
pub fn time_event_remove(mac_id: u8, mac_color: u8, unique_id: u32, out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, TIME_EVENT_CMD_LEN)?;
    put32(out, 0, id_and_color(mac_id, mac_color));
    put32(out, 4, FW_CTXT_ACTION_REMOVE);
    put32(out, 8, unique_id);
    Ok(TIME_EVENT_CMD_LEN)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeEventRefusal {
    /// The response is not a `struct iwl_time_event_resp`; carries its length.
    Length(usize),
    /// Status bit 0 clear: the firmware refused the event; carries the status word.
    NotScheduled(u32),
}

/// The response to `TIME_EVENT_CMD`: `{ status, id, unique_id, id_and_color }`; returns the unique id.
/// Status bit 0 is success. // struct iwl_time_event_resp, fw/api/time-event.h; iwl_mvm_time_event_response
pub fn time_event_resp(payload: &[u8]) -> Result<u32, TimeEventRefusal> {
    if payload.len() != TIME_EVENT_RESP_LEN {
        return Err(TimeEventRefusal::Length(payload.len()));
    }
    let status = le32(payload, 0);
    if status & 1 == 0 {
        return Err(TimeEventRefusal::NotScheduled(status));
    }
    Ok(le32(payload, 8))
}

// ------------------------------------------------------------------------------------ MAC context

/// A station MAC context. `bssid` is zero before the join; `assoc` carries what the firmware needs
/// to treat the MAC as associated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacCtxt {
    pub id: u8,
    pub color: u8,
    pub node_addr: [u8; 6],
    pub bssid: [u8; 6],
    /// Basic-rate bitmap by rate index (bit `i` = rate index `i`).
    pub basic_rates: u16,
    pub short_preamble: bool,
    pub short_slot: bool,
    /// ERP "use protection" (`bss_conf.use_cts_prot`).
    pub use_cts_prot: bool,
    /// Beacon interval in TU and DTIM period, zero before the join.
    pub beacon_int: u16,
    pub dtim_period: u8,
    pub assoc: Option<MacAssoc>,
}

/// The association-time half of the station data. // iwl_mvm_set_fw_dtim_tbtt, mvm/mac-ctxt.c:593-630
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacAssoc {
    pub aid: u16,
    /// `sync_dtim_count`: beacon intervals until the next DTIM, from the beacon heard at association.
    pub dtim_count: u8,
    /// TSF of that beacon (`sync_tsf`) and the device timestamp at which it arrived (`sync_device_ts`).
    pub sync_tsf: u64,
    pub sync_device_ts: u32,
}

/// One `struct iwl_ac_qos` per EDCA access category, from the mac80211 default parameters a
/// non-QoS BSS gets (`ieee80211_set_wmm_default`, net/mac80211/util.c): BK `cw 15/1023 aifs 7`,
/// BE `15/1023 aifs 3`, VI `7/15 aifs 2 txop 3008us`, VO `3/7 aifs 2 txop 1504us`. The driver
/// stores `txop * 32` microseconds in `edca_txop` (`iwl_mvm_set_fw_qos_params`, mac-ctxt.c:480).
/// The array is indexed by the firmware AC (`iwl_mvm_mac80211_ac_to_ucode_ac`, mvm/utils.c: VO 0,
/// VI 1, BE 2, BK 3).
const EDCA: [(u16, u16, u8, u16); 4] = [
    // (cw_min, cw_max, aifsn, txop in microseconds) for AC_VO, AC_VI, AC_BE, AC_BK
    (3, 7, 2, 1504),
    (7, 15, 2, 3008),
    (15, 1023, 3, 0),
    (15, 1023, 7, 0),
];
/// `fifos_mask` bit of each ucode AC: the TX FIFO of mac80211 AC `VO/VI/BE/BK`.
const AC_FIFO: [u8; 4] = [IWL_MVM_TX_FIFO_VO, IWL_MVM_TX_FIFO_VI, IWL_MVM_TX_FIFO_BE, IWL_MVM_TX_FIFO_BK];

/// `MAC_CONTEXT_CMD` version 4 for a station, ADD or MODIFY. // iwl_mvm_mac_ctxt_cmd_common and
/// iwl_mvm_mac_ctxt_cmd_sta, mvm/mac-ctxt.c:537-760
pub fn mac_context_sta(action: u32, c: &MacCtxt, out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, MAC_CTX_CMD_LEN)?;
    put32(out, 0, id_and_color(c.id, c.color));
    put32(out, 4, action);
    put32(out, 8, iwlwifi_mvm_rt_core::ids::FW_MAC_TYPE_BSS_STA);
    put32(out, 12, iwlwifi_mvm_rt_core::ids::TSF_ID_A);
    out[16..22].copy_from_slice(&c.node_addr);
    out[24..30].copy_from_slice(&c.bssid);
    let (cck, ofdm) = ack_rates(c.basic_rates);
    put32(out, 32, cck);
    put32(out, 36, ofdm);
    put32(out, 40, if c.use_cts_prot { MAC_PROT_FLG_TGG_PROTECT } else { 0 });
    put32(out, 44, if c.short_preamble { MAC_FLG_SHORT_PREAMBLE } else { 0 });
    put32(out, 48, if c.short_slot { MAC_FLG_SHORT_SLOT } else { 0 });
    let mut filter = iwlwifi_mvm_rt_core::ids::MAC_FILTER_ACCEPT_GRP;
    if c.assoc.is_none() || c.dtim_period == 0 {
        filter |= iwlwifi_mvm_rt_core::ids::MAC_FILTER_IN_BEACON; // beacons pass until the DTIM period is known
    }
    put32(out, 52, filter);
    put32(out, 56, 0); // qos_flags: mac80211 reports no QoS BSS here, so no UPDATE_EDCA
    for (i, &(cw_min, cw_max, aifsn, txop)) in EDCA.iter().enumerate() {
        let o = 60 + 8 * i;
        put16(out, o, cw_min);
        put16(out, o + 2, cw_max);
        out[o + 4] = aifsn;
        out[o + 5] = 1 << AC_FIFO[i];
        put16(out, o + 6, txop);
    }
    // struct iwl_mac_data_sta at 100.
    let (assoc_ok, dtim_time, dtim_tsf, arrive, aid) = match c.assoc {
        Some(a) if c.dtim_period != 0 => {
            let off = (a.dtim_count as u64) * (c.beacon_int as u64) * 1024; // TU -> us
            (1u32, a.sync_device_ts.wrapping_add(off as u32), a.sync_tsf.wrapping_add(off), a.sync_device_ts, a.aid as u32)
        }
        Some(a) => (0, 0, 0, 0, a.aid as u32),
        None => (0, 0, 0, 0, 0),
    };
    put32(out, 100, assoc_ok);
    put32(out, 104, dtim_time);
    put64(out, 108, dtim_tsf);
    put32(out, 116, c.beacon_int as u32);
    // reserved1 @120
    put32(out, 124, c.beacon_int as u32 * c.dtim_period as u32);
    // data_policy @128 stays 0
    put32(out, 132, LISTEN_INTERVAL);
    put32(out, 136, aid);
    put32(out, 140, arrive);
    Ok(MAC_CTX_CMD_LEN)
}

// ---------------------------------------------------------------------------------------- station

/// `ADD_STA` version 10 for the AP, ADD or MODIFY. `tfd_queue_msk` names the TX queues the station
/// uses (old TX API); `aid` is zero until associated. // iwl_mvm_sta_send_to_fw, mvm/sta.c
pub fn add_sta_ap(update: bool, sta_id: u8, mac_id: u8, mac_color: u8, addr: &[u8; 6], tfd_queue_msk: u32, aid: u16, out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, ADD_STA_CMD_LEN)?;
    // add_modify@0 awake_acs@1 tid_disable_tx@2 mac_id_n_color@4 addr@8 reserved2@14 sta_id@16 modify_mask@17 reserved3@18
    // station_flags@20 station_flags_msk@24 add_immediate_ba_tid@28 remove_immediate_ba_tid@29 add_immediate_ba_ssn@30
    // sleep_tx_count@32 sleep_state_flags@34 station_type@35 assoc_id@36 beamform_flags@38 tfd_queue_msk@40
    // rx_ba_window@44 sp_length@46 uapsd_acs@47
    out[0] = if update { STA_MODE_MODIFY } else { STA_MODE_ADD };
    put16(out, 2, 0xffff); // tid_disable_agg: no aggregation until a BA agreement
    put32(out, 4, id_and_color(mac_id, mac_color));
    out[8..14].copy_from_slice(addr);
    out[16] = sta_id;
    put32(out, 24, STA_FLG_FAT_EN_MSK | STA_FLG_MIMO_EN_MSK | STA_FLG_RTS_MIMO_PROT);
    out[35] = IWL_STA_LINK;
    put16(out, 36, aid);
    put32(out, 40, tfd_queue_msk);
    Ok(ADD_STA_CMD_LEN)
}

/// `REMOVE_STA`: `{ sta_id, reserved[3] }`. // struct iwl_mvm_rm_sta_cmd
pub fn remove_sta(sta_id: u8, out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, REMOVE_STA_CMD_LEN)?;
    out[0] = sta_id;
    Ok(REMOVE_STA_CMD_LEN)
}

/// `MCAST_FILTER_CMD` that lets every multicast and broadcast frame through for the BSS: `filter_own 1,
/// port_id 0, count 0, pass_all 1, bssid`. // iwl_mvm_recalc_multicast / OpenBSD iwm_allow_mcast;
/// struct iwl_mcast_filter_cmd, fw/api/filter.h
pub fn mcast_filter_pass_all(bssid: &[u8; 6], out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, 12)?;
    out[0] = 1;
    out[1] = 0;
    out[2] = 0;
    out[3] = 1;
    out[4..10].copy_from_slice(bssid);
    Ok(12) // rounded up to a dword as iwm_allow_mcast does
}

/// Re-exported so callers need one import.
pub fn lowest_rate(basic: u16, is_2ghz: bool) -> usize {
    rates::lowest_rate(basic, is_2ghz)
}

/// `SCD_QUEUE_CFG` that disables a queue: `{ token 0, sta_id, tid, scd_queue, action SCD_CFG_DISABLE_QUEUE (0),
/// the rest zero }` (12 bytes). // iwl_mvm_disable_txq, mvm/sta.c:352-420; fw/api/tx.h (SCD_CFG_DISABLE_QUEUE)
pub fn scd_queue_disable(queue: u8, sta_id: u8, tid: u8, out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, 12)?;
    out[1] = sta_id;
    out[2] = tid;
    out[3] = queue;
    Ok(12)
}
