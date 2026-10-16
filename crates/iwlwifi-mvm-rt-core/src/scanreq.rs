// SPDX-License-Identifier: GPL-2.0-only
//! `SCAN_CFG_CMD` (the v1 layout, `iwl_mvm_legacy_config_scan`) and the passive `SCAN_REQ_UMAC` in the v8 layout
//! (`iwl_mvm_scan_umac`, the generic handler `iwl_mvm_scan` falls back to for the SCAN_REQ_UMAC command version 8
//! this firmware reports, mvm/scan.c:2840-2873). The layouts are the ones selected by the API bits the firmware file
//! advertises: ADAPTIVE_DWELL (32) and ADAPTIVE_DWELL_V2 (42) set, SCAN_EXT_CHAN_VER (58), HB_N_APS (57),
//! REDUCED_SCAN_CONFIG (56), FRAG_EBS (44) clear, and the 9000 family is not CDB and not 22000.

use crate::encode::{check, put16, put32, EncodeError};
use crate::ids::*;

/// What `iwl_mvm_legacy_config_scan` puts in `struct iwl_scan_config_v1`.
#[derive(Debug, Clone, Copy)]
pub struct ScanConfig<'a> {
    pub tx_chains: u32,
    pub rx_chains: u32,
    pub mac_addr: [u8; 6],
    pub bcast_sta_id: u8,
    /// The channel numbers of `iwl_mvm_fill_channels`: 2.4 GHz band first, then 5 GHz, at most `n_scan_channels`.
    pub channels: &'a [u8],
    /// The `IWL_UCODE_TLV_N_SCAN_CHANNELS` value: the trailing `channel_array[]` is exactly this long.
    pub n_scan_channels: usize,
}

/// `iwl_mvm_legacy_config_scan` + `iwl_mvm_fill_scan_config_v1`. Scan type UNASSOC: out-of-channel and suspend times 0
/// (`scan_timing[IWL_SCAN_TYPE_UNASSOC]`, mvm/scan.c:59-63); dwell 10/110/44/90; the legacy rates of both bands.
/// // mvm/scan.c:1156-1246 (iwl_mvm_legacy_config_scan), :1036-1103 (rates, dwell, v1 fill); fw/api/scan.h:538-550
pub fn encode_scan_config_v1(c: &ScanConfig<'_>, buf: &mut [u8]) -> Result<usize, EncodeError> {
    if c.channels.len() > c.n_scan_channels {
        return Err(EncodeError::TooManyChannels { n: c.channels.len(), max: c.n_scan_channels });
    }
    let len = SCAN_CONFIG_V1_HDR_LEN + c.n_scan_channels;
    check(buf, len)?;
    let flags = SCAN_CONFIG_FLAG_ACTIVATE
        | SCAN_CONFIG_FLAG_ALLOW_CHUB_REQS
        | SCAN_CONFIG_FLAG_SET_TX_CHAINS
        | SCAN_CONFIG_FLAG_SET_RX_CHAINS
        | SCAN_CONFIG_FLAG_SET_AUX_STA_ID
        | SCAN_CONFIG_FLAG_SET_ALL_TIMES
        | SCAN_CONFIG_FLAG_SET_LEGACY_RATES
        | SCAN_CONFIG_FLAG_SET_MAC_ADDR
        | SCAN_CONFIG_FLAG_SET_CHANNEL_FLAGS
        | ((c.channels.len() as u32) << SCAN_CONFIG_N_CHANNELS_POS)
        | SCAN_CONFIG_FLAG_CLEAR_FRAGMENTED; // mvm/scan.c:1207-1224
    put32(buf, 0, flags);
    put32(buf, 4, c.tx_chains);
    put32(buf, 8, c.rx_chains);
    put32(buf, 12, SCAN_CONFIG_RATES_ALL | (SCAN_CONFIG_RATES_ALL << SCAN_CONFIG_SUPPORTED_RATE_POS)); // iwl_mvm_scan_config_rates
    put32(buf, 16, 0); // out_of_channel_time
    put32(buf, 20, 0); // suspend_time
    buf[24] = IWL_SCAN_DWELL_ACTIVE; // iwl_mvm_fill_scan_dwell, mvm/scan.c:1055-1063
    buf[25] = IWL_SCAN_DWELL_PASSIVE;
    buf[26] = IWL_SCAN_DWELL_FRAGMENTED;
    buf[27] = IWL_SCAN_DWELL_EXTENDED;
    buf[28..34].copy_from_slice(&c.mac_addr);
    buf[34] = c.bcast_sta_id;
    buf[35] = IWL_CHANNEL_FLAG_EBS | IWL_CHANNEL_FLAG_ACCURATE_EBS | IWL_CHANNEL_FLAG_EBS_ADD | IWL_CHANNEL_FLAG_PRE_SCAN_PASSIVE2ACTIVE; // mvm/scan.c:1220-1223
    buf[SCAN_CONFIG_V1_HDR_LEN..SCAN_CONFIG_V1_HDR_LEN + c.channels.len()].copy_from_slice(c.channels);
    Ok(len)
}

/// A passive, one-iteration, all-pass scan request.
#[derive(Debug, Clone, Copy)]
pub struct ScanRequest<'a> {
    /// The firmware scan id (`uid`): `iwl_mvm_scan_uid_by_status(mvm, 0)`, the first free slot — 0 for the first scan.
    pub uid: u32,
    /// The channels to listen on, in order. // iwl_mvm_umac_scan_cfg_channels
    pub channels: &'a [u8],
    /// `IWL_UCODE_TLV_N_SCAN_CHANNELS`: the per-channel config array always has this many entries.
    pub n_scan_channels: usize,
    /// The station address used as the probe request's source (never transmitted by a passive scan).
    pub sa: [u8; 6],
    /// `scan_start_mac_id`: Linux passes the scanning vif's id; no MAC context exists here, so 0.
    pub scan_start_mac_id: u8,
    /// `capa->flags & IWL_UCODE_TLV_FLAGS_EBS_SUPPORT` (and `last_ebs_successful`): channel flags EBS|ACCURATE|CACHE_ADD.
    pub use_ebs: bool,
}

/// Total command length: the v8 head, one channel config per firmware channel slot, the v1 tail.
/// // mvm/scan.c:3401 (iwl_mvm_scan_size); fw/api/scan.h:940, :788-796
pub fn scan_req_umac_len(n_scan_channels: usize) -> usize {
    IWL_SCAN_REQ_UMAC_SIZE_V8 + SCAN_CHANNEL_CFG_UMAC_LEN * n_scan_channels + SCAN_REQ_UMAC_TAIL_V1_LEN
}

/// The probe request template `iwl_mvm_build_scan_probe` builds: a 24-byte probe-request header from the station
/// address to the broadcast address with a wildcard SSID element (`mac_header`, 26 bytes), a DS Parameter Set
/// placeholder on 2.4 GHz (the firmware can do RRM scans: DS_PARAM_SET_IE_SUPPORT) as `band_data[0]`, nothing for 5 GHz,
/// and the WFA TPC report element as the common data (the firmware lacks WFA_TPC_REP_IE_SUPPORT, so the driver adds it).
/// Returns the template bytes and its segment table `(offset, len)` for mac_header, band_data[0], band_data[1], common.
/// // mvm/scan.c:751-818 (iwl_mvm_build_scan_probe), :696-748 (DS and TPC elements)
fn probe_template(sa: [u8; 6]) -> ([u8; 40], [(u16, u16); 4], usize) {
    let mut f = [0u8; 40];
    f[0..2].copy_from_slice(&0x0040u16.to_le_bytes()); // IEEE80211_STYPE_PROBE_REQ, frame_control
    f[4..10].fill(0xff); // da = broadcast
    f[10..16].copy_from_slice(&sa); // sa
    f[16..22].fill(0xff); // bssid = req->bssid, the cfg80211 default is broadcast
    // seq_ctrl = 0 (f[22..24]); SSID element, wildcard:
    f[24] = 0; // WLAN_EID_SSID
    f[25] = 0;
    let mac_header = (0u16, 26u16);
    f[26] = 3; // WLAN_EID_DS_PARAMS placeholder: id, len 1, channel inserted by the firmware
    f[27] = 1;
    f[28] = 0;
    let band0 = (26u16, 3u16);
    let band1 = (29u16, 0u16);
    // iwl_mvm_add_tpc_report_ie: vendor specific, len 7, OUI 00:50:f2 type 8, then 3 zero bytes
    f[29] = 221;
    f[30] = 7;
    f[31] = 0x00;
    f[32] = 0x50;
    f[33] = 0xf2;
    f[34] = 8;
    let common = (29u16, 9u16);
    (f, [mac_header, band0, band1, common], 38)
}

/// `iwl_mvm_scan_umac` for a passive regular scan, v8 layout. The field values are Linux's: `iwl_mvm_scan_umac_dwell`
/// (adaptive dwell v2: social-channel and default AP counts, 300 TU full-scan budget, priority EXT_6, UNASSOC timings,
/// dwell 10 active / 110 passive on the low-band LMAC), `iwl_mvm_scan_umac_flags` (PASSIVE because `n_ssids == 0`,
/// PASS_ALL because a regular scan passes every frame, ADAPTIVE_DWELL), general_flags2 ALLOW_CHNL_REORDER, one
/// iteration scan plan, no direct SSIDs, per-channel iter_count 1.
/// // mvm/scan.c:1292-1392 (dwell), :2181-2258 (flags), :2292-2372 (iwl_mvm_scan_umac), :2260-2290 (sched params),
/// // :1587-1610 (channel config); fw/api/scan.h:870-940
pub fn encode_scan_req_umac_v8(r: &ScanRequest<'_>, buf: &mut [u8]) -> Result<usize, EncodeError> {
    if r.channels.len() > r.n_scan_channels {
        return Err(EncodeError::TooManyChannels { n: r.channels.len(), max: r.n_scan_channels });
    }
    let len = scan_req_umac_len(r.n_scan_channels);
    check(buf, len)?;
    // head
    put32(buf, 0, 0); // flags: not PREEMPTIVE (only scheduled scans), mvm/scan.c:2332-2333
    put32(buf, 4, r.uid);
    put32(buf, 8, IWL_SCAN_PRIORITY_EXT_6); // ooc_priority: regular scan, mvm/scan.c:1385-1388
    let gen_flags = IWL_UMAC_SCAN_GEN_FLAGS_PASSIVE | IWL_UMAC_SCAN_GEN_FLAGS_PASS_ALL | IWL_UMAC_SCAN_GEN_FLAGS_ADAPTIVE_DWELL;
    put16(buf, 12, gen_flags);
    buf[15] = r.scan_start_mac_id;
    // v8 @16
    buf[16] = IWL_SCAN_DWELL_ACTIVE; // active_dwell[SCAN_LB_LMAC_IDX]
    buf[19] = IWL_SCAN_ADWELL_DEFAULT_LB_N_APS; // adwell_default_n_aps
    buf[20] = IWL_SCAN_ADWELL_DEFAULT_N_APS_SOCIAL; // adwell_default_n_aps_social
    buf[21] = IWL_UMAC_SCAN_GEN_FLAGS2_ALLOW_CHNL_REORDER; // general_flags2
    put16(buf, 22, IWL_SCAN_ADWELL_MAX_BUDGET_FULL_SCAN); // adwell_max_budget
    put32(buf, 24, 0); // max_out_time[LB], UNASSOC
    put32(buf, 32, 0); // suspend_time[LB]
    put32(buf, 40, IWL_SCAN_PRIORITY_EXT_6); // scan_priority
    buf[44] = IWL_SCAN_DWELL_PASSIVE; // passive_dwell[SCAN_LB_LMAC_IDX]
    // num_of_fragments[] stay 0: not a fragmented scan.
    let ch_flags = if r.use_ebs { IWL_SCAN_CHANNEL_FLAG_EBS | IWL_SCAN_CHANNEL_FLAG_EBS_ACCURATE | IWL_SCAN_CHANNEL_FLAG_CACHE_ADD } else { 0 };
    buf[48] = ch_flags; // chan_param->flags
    buf[49] = r.channels.len() as u8; // chan_param->count
    // channel config array: flags = ssid_bitmap (0), channel_num, v1.iter_count 1, v1.iter_interval 0
    let mut o = IWL_SCAN_REQ_UMAC_SIZE_V8;
    for &ch in r.channels {
        put32(buf, o, 0);
        buf[o + 4] = ch;
        buf[o + 5] = 1; // iter_count
        o += SCAN_CHANNEL_CFG_UMAC_LEN;
    }
    // tail v1 after n_scan_channels entries
    let t = IWL_SCAN_REQ_UMAC_SIZE_V8 + SCAN_CHANNEL_CFG_UMAC_LEN * r.n_scan_channels;
    // schedule[0]: interval 0, iter_count 1; schedule[1] unused; delay 0
    buf[t + 2] = 1;
    // preq (iwl_scan_probe_req_v1): mac_header seg, band_data[2] segs, common seg, then the 512-byte buffer
    let (tmpl, segs, used) = probe_template(r.sa);
    let p = t + 12;
    let seg_put = |buf: &mut [u8], at: usize, s: (u16, u16)| {
        put16(buf, at, s.0);
        put16(buf, at + 2, s.1);
    };
    seg_put(buf, p, segs[0]); // mac_header
    seg_put(buf, p + 4, segs[1]); // band_data[0]
    seg_put(buf, p + 8, segs[2]); // band_data[1]
    seg_put(buf, p + 12, segs[3]); // common_data
    buf[p + 16..p + 16 + used].copy_from_slice(&tmpl[..used]);
    // direct_scan[] stay zero: no SSIDs
    Ok(len)
}
