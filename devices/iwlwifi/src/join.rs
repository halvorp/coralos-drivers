// SPDX-License-Identifier: GPL-2.0-only
//! Joining an access point: bind a PHY, a MAC and the AP station in the firmware, protect the session, authenticate,
//! associate, and for a protected network run the 4-way handshake and install the keys. Ported from Linux
//! `mvm/mac80211.c` (`iwl_mvm_mac_sta_state` NOTEXIST -> NONE -> AUTH -> ASSOC -> AUTHORIZED, `iwl_mvm_add_chanctx`,
//! `iwl_mvm_assign_vif_chanctx`, `iwl_mvm_mac_mgd_prepare_tx`), `mvm/phy-ctxt.c`, `mvm/binding.c`, `mvm/sta.c`
//! (`iwl_mvm_add_sta`, `iwl_mvm_rm_sta`), `mvm/time-event.c` and `net/mac80211/mlme.c` (the authentication and
//! association retry bounds), with OpenBSD `if_iwm.c` (`iwm_auth`, `iwm_run`) as the order of operations this
//! firmware is known to accept. Linux v7.0-rc6.
//!
//! The markers, in order: `[IWL-JOIN-REQ]` `[IWL-JOIN-PHY]` `[IWL-JOIN-MAC]` `[IWL-JOIN-BIND]` `[IWL-JOIN-TXQ]`
//! `[IWL-JOIN-STA]` `[IWL-JOIN-TE]` `[IWL-AUTH]` `[IWL-ASSOC]` `[IWL-JOIN-MAC assoc]` `[IWL-4WAY m2 sent]` `[IWL-4WAY m4
//! queued]` `[IWL-KEYS ptk]` `[IWL-KEYS gtk]` `[IWL-4WAY OK]` `[IWL-LINK-UP]`; a failure prints
//! `[IWL-JOIN FAIL stage=<s> <why>]`.
//!
//! WHAT IS NOT DONE, named: HT/VHT/HE and aggregation (the station associates as an 11a/g client), QoS and WMM,
//! power save, beacon filtering and the link-quality table (`LQ_CMD`), so every data frame goes out at ONE host-chosen
//! legacy rate ([`DATA_RATE_PREFERENCE`]) with no rate adaptation; a TKIP group cipher; management frame protection.

use crate::driver::Driver;
use crate::event::{Event, JoinRequest, LinkDown, Security};
use crate::host::Host;
use crate::link::*;
use crate::rt_up::StoredBeacon;
use alloc::boxed::Box;
use alloc::vec::Vec;
use iwlwifi_fw_tlv_core::capa::{CAPA_SESSION_PROT_CMD, CAPA_ULTRA_HB_CHANNELS};
use iwlwifi_mvm_join_core::bss::{offered_rates, parse_beacon, Bss};
use iwlwifi_mvm_join_core::ctxt::*;
use iwlwifi_mvm_join_core::dot11::{self, AssocReq, CAP_ESS, CAP_PRIVACY, CAP_SHORT_PREAMBLE, CAP_SHORT_SLOT};
use iwlwifi_mvm_join_core::ids::*;
use iwlwifi_mvm_join_core::rates;
use iwlwifi_mvm_rt_core::encode::encode_scd_queue_cfg_enable;
use iwlwifi_mvm_rt_core::ids::{ADD_STA, LEGACY_GROUP, MAC_CONTEXT_CMD, PHY_CONTEXT_CMD, SCD_QUEUE_CFG};
use iwlwifi_pcie_ring_core::tx::Chunk;
use wpa_supplicant_core::rsne::{self, Akm, Cipher, Rsne};
use wpa_supplicant_core::{Config as SupConfig, Supplicant};

/// Authentication attempts and the wait for each answer. // IEEE80211_AUTH_MAX_TRIES, IEEE80211_AUTH_TIMEOUT: net/mac80211/mlme.c
pub const AUTH_TRIES: u8 = 3;
pub const AUTH_TIMEOUT_MS: u32 = 200;
/// Association attempts and the wait for each answer. // IEEE80211_ASSOC_MAX_TRIES, IEEE80211_ASSOC_TIMEOUT: net/mac80211/mlme.c
pub const ASSOC_TRIES: u8 = 3;
pub const ASSOC_TIMEOUT_MS: u32 = 200;
/// The rate data frames go out at when the AP supports it, else the nearest below: 24 Mbit/s. The firmware's own rate
/// scaling needs `LQ_CMD`, which is not ported, so this is a fixed, conservative stopgap.
pub const DATA_RATE_PREFERENCE: usize = rates::RATE_24M;
/// The TX queue the link uses and the TX FIFO behind it: the first management queue, best effort. // fw/api/txq.h
pub const LINK_QUEUE: u8 = IWL_MVM_DQA_MIN_MGMT_QUEUE;
/// The PHY and MAC context ids of the one station interface (`mvmvif->id`, the first free PHY context).
const PHY_ID: u8 = 0;
const COLOR: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinReport {
    pub bssid: [u8; 6],
    pub channel: u8,
    pub aid: u16,
    pub secure: bool,
    /// The signal of the beacon the join was made from, dBm.
    pub rssi: i16,
    pub elapsed_ms: u64,
}

fn mac_str(m: &[u8; 6]) -> [u8; 6] {
    *m
}

impl<H: Host> Driver<H> {
    /// `Request::Join`: the events the wrapper turns into replies.
    pub fn join_events(&mut self, req: &JoinRequest) -> Vec<Event> {
        match self.join(req) {
            Ok(r) => {
                let mut ev = self.take_events();
                if !ev.iter().any(|e| matches!(e, Event::LinkUp { .. })) {
                    ev.push(Event::LinkUp { bssid: r.bssid, channel: r.channel, aid: r.aid });
                }
                ev
            }
            Err(e) => {
                let mut ev = self.take_events();
                ev.push(Event::JoinFailed { stage: e.stage, code: e.code() });
                ev
            }
        }
    }

    /// `Request::Leave`.
    pub fn leave_events(&mut self) -> Vec<Event> {
        self.leave();
        self.take_events()
    }

    /// Leave the network: deauthenticate, then undo every firmware context. A no-op without a link.
    pub fn leave(&mut self) {
        if self.link.is_some() {
            self.teardown(Some(wpa_supplicant_core::refusal::REASON_AUTH_LEAVE));
            self.pending_events.push(Event::LinkDown { reason: LinkDown::Left });
        }
    }

    /// Join `req`. Returns when the link is up (keys installed for a protected network) or the first step fails; a
    /// failure leaves no firmware context behind.
    pub fn join(&mut self, req: &JoinRequest) -> Result<JoinReport, JoinError> {
        if self.link.is_some() {
            // A second join while a link exists: refused, and the link is left alone.
            return Err(JoinError::new("request", JoinWhy::Busy));
        }
        let r = self.join_inner(req);
        if let Err(e) = &r {
            self.log(format_args!("[IWL-JOIN FAIL stage={} {:?}]", e.stage, e.why));
            if self.link.is_some() {
                self.teardown(e.deauth_reason());
            }
            if self.t.fault.is_some() {
                self.dump_fw_error(iwlwifi_fw_alive_core::dump::Image::Regular);
            }
        }
        r
    }

    fn pick_beacon(&self, req: &JoinRequest) -> Option<StoredBeacon> {
        let want = req.ssid.as_bytes();
        let mut best: Option<&StoredBeacon> = None;
        for b in &self.rt.beacons {
            if let Some(w) = req.bssid {
                if b.bssid != w {
                    continue;
                }
            }
            let Ok(p) = parse_beacon(&b.mpdu, b.padded) else { continue };
            if p.ssid != want {
                continue;
            }
            if best.is_none_or(|o| b.rssi > o.rssi) {
                best = Some(b);
            }
        }
        best.cloned()
    }

    fn join_inner(&mut self, req: &JoinRequest) -> Result<JoinReport, JoinError> {
        let t0 = self.t.host().now_ms();
        if !self.rt.up {
            return Err(JoinError::new("request", JoinWhy::NotUp));
        }
        if self.link.is_some() {
            return Err(JoinError::new("request", JoinWhy::Busy));
        }
        let sb = self.pick_beacon(req).ok_or(JoinError::new("request", JoinWhy::NoScanResult))?;
        let bss = parse_beacon(&sb.mpdu, sb.padded).map_err(|e| JoinError::new("request", JoinWhy::Beacon(e)))?;
        let secure = match (req.security, &req.pmk) {
            (Security::Open, None) => false,
            (Security::Wpa2Psk, Some(_)) => true,
            _ => return Err(JoinError::new("request", JoinWhy::Unsupported("security and credential disagree"))),
        };
        // What the AP offers must be something this station runs.
        let mut own_rsne = [0u8; 32];
        let mut own_len = 0;
        if secure {
            let ie = bss.rsne.ok_or(JoinError::new("request", JoinWhy::Unsupported("no RSN element")))?;
            let r = Rsne::parse(ie).map_err(|_| JoinError::new("request", JoinWhy::Unsupported("RSN element does not parse")))?;
            if !r.offers_akm(Akm::Psk) || !r.offers_pairwise(Cipher::Ccmp) {
                return Err(JoinError::new("request", JoinWhy::Unsupported("AP does not offer WPA2-PSK with CCMP")));
            }
            if !r.group_is(Cipher::Ccmp) {
                return Err(JoinError::new("request", JoinWhy::Unsupported("group cipher is not CCMP")));
            }
            if r.caps & 0x0040 != 0 {
                return Err(JoinError::new("request", JoinWhy::Unsupported("management frame protection is required")));
            }
            own_len = rsne::build(&mut own_rsne, Cipher::Ccmp, Akm::Psk).map_err(|_| JoinError::new("request", JoinWhy::Unsupported("RSNE")))?;
        } else if bss.rsne.is_some() || bss.capability & CAP_PRIVACY != 0 {
            return Err(JoinError::new("request", JoinWhy::Unsupported("network is protected")));
        }
        if secure {
            // No handshake without entropy: ask once now so the refusal is a join refusal, not a mid-handshake one.
            let mut probe = [0u8; 8];
            if !self.t.host().random(&mut probe) {
                return Err(JoinError::new("request", JoinWhy::NoEntropy));
            }
        }

        let channel = bss.channel.unwrap_or(sb.rx_channel);
        let five = channel > 14;
        let beacon_int = if bss.beacon_interval == 0 { 100 } else { bss.beacon_interval };
        let (dtim_count, dtim_period) = bss.dtim.unwrap_or((0, 1));
        let dtim_period = dtim_period.max(1);
        let mgmt_rate = rates::lowest_rate(bss.basic_rates, !five);
        let data_rate = pick_data_rate(bss.supported_rates, five);
        let mac = self.mac.ok_or(JoinError::new("request", JoinWhy::NotUp))?;
        let ant = self.valid_tx_ant;
        self.log(format_args!(
            "[IWL-JOIN-REQ ssid-len={} bssid={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} ch={channel} band={} sec={} bi={beacon_int} dtim={dtim_period} rates={:#05x} basic={:#05x} mgmt-rate={mgmt_rate} data-rate={data_rate}]",
            bss.ssid.len(),
            bss.bssid[0], bss.bssid[1], bss.bssid[2], bss.bssid[3], bss.bssid[4], bss.bssid[5],
            if five { "5" } else { "2.4" },
            if secure { "psk" } else { "open" },
            bss.supported_rates,
            bss.basic_rates
        ));
        let _ = (ant, mac_str(&mac));
        {
            let cv = self.fw.cmd_versions();
            let sp = self.fw.info.capa.has(CAPA_SESSION_PROT_CMD);
            self.log(format_args!(
                "[IWL-JOIN-FW session_prot_capa={} uhb={} te_ver={} te_notif_ver={} phy_ver={} binding_ver={} mac_ver={} add_sta_ver={}]",
                sp as u8,
                self.fw.info.capa.has(CAPA_ULTRA_HB_CHANNELS) as u8,
                cv.cmd_ver(LEGACY_GROUP, TIME_EVENT_CMD, 0),
                cv.notif_ver(LEGACY_GROUP, TIME_EVENT_NOTIFICATION, 0),
                cv.cmd_ver(LEGACY_GROUP, PHY_CONTEXT_CMD, 0),
                cv.cmd_ver(LEGACY_GROUP, BINDING_CONTEXT_CMD, 0),
                cv.cmd_ver(LEGACY_GROUP, MAC_CONTEXT_CMD, 0),
                cv.cmd_ver(LEGACY_GROUP, ADD_STA, 0),
            ));
            if sp {
                // iwl_mvm_protect_assoc would send SESSION_PROTECTION_CMD (MAC_CONF_GROUP), which is not ported.
                return Err(JoinError::new("request", JoinWhy::Unsupported("firmware wants SESSION_PROTECTION_CMD")));
            }
        }

        let link = Link {
            bssid: bss.bssid,
            ssid: bss.ssid.to_vec(),
            channel,
            five_ghz: five,
            our_mac: mac,
            aid: 0,
            secure,
            up: false,
            mgmt_rate,
            data_rate,
            ant: 0,
            qid: LINK_QUEUE,
            beacon_int,
            dtim_period,
            basic_rates: bss.basic_rates,
            short_preamble: bss.capability & CAP_SHORT_PREAMBLE != 0 && !bss.barker_preamble,
            short_slot: bss.capability & CAP_SHORT_SLOT != 0,
            use_cts_prot: bss.use_protection,
            sup: None,
            ptk: None,
            gtks: [None, None, None, None],
            slots: iwlwifi_mvm_sta_key_core::slots::KeySlots::new(),
            pn_tx: 0,
            pending_m4: None,
            te_uid: None,
            done: Done::default(),
            stats: LinkStats::default(),
        };
        self.link = Some(Box::new(link));
        self.pending_events.clear();

        self.join_contexts(&bss)?;
        self.join_time_event(beacon_int)?;
        self.join_auth(&bss)?;
        let ie = &own_rsne[..own_len];
        let aid = self.join_assoc(&bss, if secure { Some(ie) } else { None })?;
        self.link_mut("assoc")?.aid = aid;
        self.join_assoc_contexts(&bss, aid, dtim_count, sb.gp2)?;

        if secure {
            let pmk = wpa_crypto_core::Pmk::from_bytes(req.pmk.expect("secure implies a PMK"));
            let mut own = [0u8; 257];
            own[..own_len].copy_from_slice(ie);
            let apr = bss.rsne.expect("checked above");
            let cfg = SupConfig { aa: bss.bssid, spa: mac, akm: Akm::Psk, group: Cipher::Ccmp, own_rsne: &own[..own_len], ap_rsne: apr };
            let now = self.t.host().now_ms();
            let sup = Supplicant::new(&cfg, pmk, now).map_err(|r| JoinError::new("4way", JoinWhy::Handshake(r)))?;
            self.link_mut("4way")?.sup = Some(sup);
            self.join_handshake()?;
        } else {
            self.link_up();
        }
        // Linux stops the session protection once the station is authorized. Failure here is harmless.
        self.join_stop_te();
        let l = self.link.as_ref().expect("up");
        let rep = JoinReport { bssid: l.bssid, channel: l.channel, aid: l.aid, secure: l.secure, rssi: sb.rssi, elapsed_ms: self.t.host().now_ms().saturating_sub(t0) };
        Ok(rep)
    }

    // ----------------------------------------------------------------------------------- the contexts

    fn join_contexts(&mut self, bss: &Bss<'_>) -> Result<(), JoinError> {
        let uhb = self.fw.info.capa.has(CAPA_ULTRA_HB_CHANNELS);
        let (channel, five, mac_addr) = {
            let l = self.link_mut("phy")?;
            (l.channel, l.five_ghz, l.our_mac)
        };
        let tx_ant = self.valid_tx_ant;
        let rx_ant = self.valid_rx_ant;
        let mut b = [0u8; 160];

        // PHY context on the AP's channel. // iwl_mvm_phy_ctxt_add, mvm/phy-ctxt.c
        let phy = PhyCtxt { id: PHY_ID, color: COLOR, channel, band_5ghz: five, valid_tx_ant: tx_ant, valid_rx_ant: rx_ant, chains_static: 1, chains_dynamic: 1 };
        let n = phy_context(FW_CTXT_ACTION_ADD, &phy, uhb, &mut b).map_err(|e| JoinError::new("phy", JoinWhy::Encode(e)))?;
        self.jcmd("phy", PHY_CONTEXT_CMD, &b[..n])?;
        self.link_mut("phy")?.done.phy = true;
        self.log(format_args!("[IWL-JOIN-PHY ok id={PHY_ID} ch={channel} band={}]", if five { 5 } else { 24 }));

        // The MAC context learns the BSSID and the rates. // iwl_mvm_mac_ctxt_changed (BSS_CHANGED_BSSID)
        let mc = self.mac_ctxt_for(bss.bssid, None);
        let n = mac_context_sta(FW_CTXT_ACTION_MODIFY, &mc, &mut b).map_err(|e| JoinError::new("mac", JoinWhy::Encode(e)))?;
        self.jcmd("mac", MAC_CONTEXT_CMD, &b[..n])?;
        self.link_mut("mac")?.done.mac = true;
        self.log(format_args!("[IWL-JOIN-MAC ok bssid-set]"));

        // Bind the MAC to the PHY. // iwl_mvm_binding_add_vif, mvm/binding.c
        let n = binding_v1(FW_CTXT_ACTION_ADD, PHY_ID, COLOR, self.rt.mac_id, COLOR, &mut b).map_err(|e| JoinError::new("binding", JoinWhy::Encode(e)))?;
        let rx = self.jcmd("binding", BINDING_CONTEXT_CMD, &b[..n])?;
        let status = rx.payload.get(..4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        match status {
            Some(0) => {}
            Some(s) => return Err(JoinError::new("binding", JoinWhy::Status(s))),
            None => return Err(JoinError::new("binding", JoinWhy::Decode("BINDING status"))),
        }
        self.link_mut("binding")?.done.binding = true;
        self.log(format_args!("[IWL-JOIN-BIND ok]"));

        // The transmit queue, then the AP station that owns it. // iwm_enable_txq before iwm_add_sta_cmd
        let qid = LINK_QUEUE;
        self.t.txq_enable(qid).map_err(|e| JoinError::new("txq", JoinWhy::Transport(e)))?;
        let n = encode_scd_queue_cfg_enable(qid, AP_STA_ID, IWL_MVM_TX_FIFO_BE, 0, &mut b).map_err(|_| JoinError::new("txq", JoinWhy::Decode("SCD_QUEUE_CFG")))?;
        self.jcmd("txq", SCD_QUEUE_CFG, &b[..n])?;
        self.link_mut("txq")?.done.queue = true;
        self.log(format_args!("[IWL-JOIN-TXQ ok qid={qid} fifo={IWL_MVM_TX_FIFO_BE}]"));

        let n = add_sta_ap(false, AP_STA_ID, self.rt.mac_id, COLOR, &bss.bssid, 1 << qid, 0, &mut b).map_err(|e| JoinError::new("sta", JoinWhy::Encode(e)))?;
        self.add_sta_cmd("sta", &b[..n])?;
        self.link_mut("sta")?.done.sta = true;
        self.log(format_args!("[IWL-JOIN-STA ok sta={AP_STA_ID} queues={:#x}]", 1u32 << qid));
        let _ = mac_addr;
        Ok(())
    }

    fn mac_ctxt_for(&self, bssid: [u8; 6], assoc: Option<MacAssoc>) -> MacCtxt {
        let l = self.link.as_ref().expect("link");
        MacCtxt {
            id: self.rt.mac_id,
            color: COLOR,
            node_addr: l.our_mac,
            bssid,
            basic_rates: l.basic_rates,
            short_preamble: l.short_preamble,
            short_slot: l.short_slot,
            use_cts_prot: l.use_cts_prot,
            beacon_int: l.beacon_int,
            dtim_period: l.dtim_period,
            assoc,
        }
    }

    /// Hold the channel while authenticating and associating. // iwl_mvm_protect_session, mvm/time-event.c
    ///
    /// The command response only hands back the unique id (Linux never reads its `status`); whether the firmware could
    /// schedule the event is the START notification's `status`. A start that was not executed is a named refusal: for a
    /// station that is not associated Linux reports connection loss (`iwl_mvm_te_check_disconnect`). A start that does
    /// not arrive within `max_delay` is logged and the join goes on, as `iwl_mvm_protect_session` does.
    fn join_time_event(&mut self, beacon_int: u16) -> Result<(), JoinError> {
        // Linux's numbers, not OpenBSD's (2 x beacon interval, max_delay half of one: 200 / 50 TU at 100 TU, which
        // left the firmware scheduler 50 TU to find a slot). // iwl_mvm_protect_assoc, mvm/mac80211.c:2604-2627
        let _ = beacon_int;
        let duration = TE_SESSION_PROTECTION_MAX_TIME;
        let max_delay = TE_PROTECT_ASSOC_MAX_DELAY;
        let mut b = [0u8; 40];
        let n = time_event_protect(self.rt.mac_id, COLOR, duration, max_delay, &mut b).map_err(|e| JoinError::new("te", JoinWhy::Encode(e)))?;
        let rx = self.jcmd("te", TIME_EVENT_CMD, &b[..n])?;
        let resp = match time_event_resp(&rx.payload, TE_BSS_STA_AGGRESSIVE_ASSOC) {
            Ok(r) => r,
            Err(e) => {
                // the firmware's answer is not a TE response for this event: both frames, so the boot names the field
                self.log(format_args!("[IWL-JOIN-TE-REFUSED cmd={:02x?} resp={:02x?}]", &b[..n], &rx.payload[..rx.payload.len().min(32)]));
                return Err(JoinError::new("te", JoinWhy::TimeEvent(e)));
            }
        };
        let uid = resp.unique_id;
        self.link_mut("te")?.te_uid = Some(uid);
        self.log(format_args!(
            "[IWL-JOIN-TE resp status={:#x} id={} uid={uid:#x} idc={:#x} duration={duration}TU max_delay={max_delay}TU]",
            resp.status, resp.id, resp.id_and_color
        ));
        let wait_ms = (max_delay as u64 * 1024 / 1000) as u32 + 5;
        let is_ours = move |r: &crate::transport::Rx| {
            r.group == LEGACY_GROUP && r.cmd == TIME_EVENT_NOTIFICATION && time_event_notif(&r.payload).is_ok_and(|n| n.unique_id == uid)
        };
        match self.t.wait_notif(wait_ms, is_ours) {
            Ok(rx) => match time_event_notif(&rx.payload) {
                Ok(nf) => {
                    self.log(format_args!("[IWL-JOIN-TE start action={:#x} status={} gp2={:#x}]", nf.action, nf.status, nf.timestamp));
                    if nf.status == 0 {
                        // "Time Event start notification failure": the scheduler found no slot. The event is gone in
                        // the firmware (iwl_mvm_te_clear_data), so there is nothing to remove.
                        self.link_mut("te")?.te_uid = None;
                        return Err(JoinError::new("te", JoinWhy::TimeEvent(TimeEventRefusal::NotExecuted(nf.action))));
                    }
                }
                Err(e) => return Err(JoinError::new("te", JoinWhy::TimeEvent(e))),
            },
            Err(crate::transport::TransportError::Timeout { .. }) => self.log(format_args!("[IWL-JOIN-TE start notification late wait_ms={wait_ms}]")),
            Err(e) => return Err(JoinError::new("te", JoinWhy::Transport(e))),
        }
        self.log(format_args!("[IWL-JOIN-TE ok uid={uid:#x} duration={duration}TU]"));
        Ok(())
    }

    fn join_stop_te(&mut self) {
        let Some(uid) = self.link.as_mut().and_then(|l| l.te_uid.take()) else { return };
        let mut b = [0u8; 40];
        if let Ok(n) = time_event_remove(self.rt.mac_id, COLOR, uid, &mut b) {
            let _ = self.jcmd("te-stop", TIME_EVENT_CMD, &b[..n]);
        }
    }

    // --------------------------------------------------------------------------- authenticate, associate

    fn join_auth(&mut self, bss: &Bss<'_>) -> Result<(), JoinError> {
        let t0 = self.t.host().now_ms();
        let mut frame = [0u8; 40];
        let our = self.link_mut("auth")?.our_mac;
        let n = dot11::auth_request(&mut frame, &bss.bssid, &our).map_err(|e| JoinError::new("auth", JoinWhy::Encode(e)))?;
        for _try in 0..AUTH_TRIES {
            self.send_mgmt(&frame[..n], false)?;
            let deadline = self.t.host().now_ms() + AUTH_TIMEOUT_MS as u64;
            loop {
                let left = deadline.saturating_sub(self.t.host().now_ms()) as u32;
                if left == 0 {
                    break;
                }
                match self.wait_frame(left)? {
                    Some(Frame::Auth { algo, seq, status }) if algo == dot11::AUTH_ALGO_OPEN && seq == 2 => {
                        if status != dot11::STATUS_SUCCESS {
                            return Err(JoinError::new("auth", JoinWhy::AuthRefused(status)));
                        }
                        let ms = self.t.host().now_ms().saturating_sub(t0);
                        self.log(format_args!("[IWL-AUTH ok ms={ms}]"));
                        return Ok(());
                    }
                    Some(Frame::Deauth(r)) | Some(Frame::Disassoc(r)) => return Err(JoinError::new("auth", JoinWhy::Dropped(r))),
                    Some(_) | None => {}
                }
            }
            self.log(format_args!("[IWL-AUTH retry]"));
        }
        Err(JoinError::new("auth", JoinWhy::Timeout("authentication response")))
    }

    fn join_assoc(&mut self, bss: &Bss<'_>, rsne: Option<&[u8]>) -> Result<u16, JoinError> {
        let t0 = self.t.host().now_ms();
        let l = self.link_mut("assoc")?;
        let (our, five) = (l.our_mac, l.five_ghz);
        let mut cap = CAP_ESS;
        if rsne.is_some() {
            cap |= CAP_PRIVACY;
        }
        if bss.capability & CAP_SHORT_SLOT != 0 {
            cap |= CAP_SHORT_SLOT;
        }
        if bss.capability & CAP_SHORT_PREAMBLE != 0 && !five {
            cap |= CAP_SHORT_PREAMBLE;
        }
        let mut offered = [0u8; 12];
        let no = offered_rates(bss.supported_rates, &mut offered);
        let req = AssocReq { sa: our, bssid: bss.bssid, capability: cap, listen_interval: LISTEN_INTERVAL as u16, ssid: bss.ssid, rates: &offered[..no], rsne };
        let mut frame = [0u8; 160];
        let n = dot11::assoc_request(&mut frame, &req).map_err(|e| JoinError::new("assoc", JoinWhy::Encode(e)))?;
        for _try in 0..ASSOC_TRIES {
            self.send_mgmt(&frame[..n], true)?;
            let deadline = self.t.host().now_ms() + ASSOC_TIMEOUT_MS as u64;
            loop {
                let left = deadline.saturating_sub(self.t.host().now_ms()) as u32;
                if left == 0 {
                    break;
                }
                match self.wait_frame(left)? {
                    Some(Frame::AssocResp { status, aid }) => {
                        if status != dot11::STATUS_SUCCESS {
                            return Err(JoinError::new("assoc", JoinWhy::AssocRefused(status)));
                        }
                        let ms = self.t.host().now_ms().saturating_sub(t0);
                        self.log(format_args!("[IWL-ASSOC aid={aid} ms={ms}]"));
                        return Ok(aid);
                    }
                    Some(Frame::Deauth(r)) | Some(Frame::Disassoc(r)) => return Err(JoinError::new("assoc", JoinWhy::Dropped(r))),
                    Some(_) | None => {}
                }
            }
            self.log(format_args!("[IWL-ASSOC retry]"));
        }
        Err(JoinError::new("assoc", JoinWhy::Timeout("association response")))
    }

    /// The station and the MAC become "associated". // iwl_mvm_mac_sta_state AUTH -> ASSOC, iwl_mvm_bss_info_changed_station_assoc;
    /// OpenBSD iwm_run (add_sta update, mac ctxt modify, allow mcast)
    fn join_assoc_contexts(&mut self, bss: &Bss<'_>, aid: u16, dtim_count: u8, gp2: u32) -> Result<(), JoinError> {
        let mut b = [0u8; 160];
        let n = add_sta_ap(true, AP_STA_ID, self.rt.mac_id, COLOR, &bss.bssid, 1 << LINK_QUEUE, aid, &mut b).map_err(|e| JoinError::new("sta-assoc", JoinWhy::Encode(e)))?;
        self.add_sta_cmd("sta-assoc", &b[..n])?;
        let mc = self.mac_ctxt_for(bss.bssid, Some(MacAssoc { aid, dtim_count, sync_tsf: bss.tsf, sync_device_ts: gp2 }));
        let n = mac_context_sta(FW_CTXT_ACTION_MODIFY, &mc, &mut b).map_err(|e| JoinError::new("mac-assoc", JoinWhy::Encode(e)))?;
        self.jcmd("mac-assoc", MAC_CONTEXT_CMD, &b[..n])?;
        self.log(format_args!("[IWL-JOIN-MAC assoc ok aid={aid}]"));
        let n = mcast_filter_pass_all(&bss.bssid, &mut b).map_err(|e| JoinError::new("mcast", JoinWhy::Encode(e)))?;
        self.jcmd("mcast", MCAST_FILTER_CMD, &b[..n])?;
        Ok(())
    }

    /// Run the 4-way handshake to the end: the AP sends message 1, the supplicant answers, the keys go in when message 4
    /// has left. Bounded by the supplicant's own timeout.
    fn join_handshake(&mut self) -> Result<(), JoinError> {
        loop {
            let up = self.link.as_ref().is_some_and(|l| l.up);
            if up {
                return Ok(());
            }
            let now = self.t.host().now_ms();
            let tick = self.link.as_mut().and_then(|l| l.sup.as_mut().map(|s| s.tick(now)));
            if let Some(Err(r)) = tick {
                let e = JoinError::new("4way", JoinWhy::Handshake(r));
                self.log(format_args!("[IWL-4WAY FAIL {:?}]", e.why));
                return Err(e);
            }
            match self.wait_frame(100)? {
                Some(Frame::Eapol(p)) => self.eapol_in(&p)?,
                Some(Frame::Deauth(r)) | Some(Frame::Disassoc(r)) => return Err(JoinError::new("4way", JoinWhy::Dropped(r))),
                Some(_) | None => {}
            }
        }
    }

    // ------------------------------------------------------------------------------------- teardown

    /// Undo every firmware context this link made, newest first. `deauth` sends a deauthentication with that reason
    /// first (best effort). Errors are logged, never returned: a teardown must always finish.
    pub(crate) fn teardown(&mut self, deauth: Option<u16>) {
        if self.link.is_none() {
            return;
        }
        if let Some(reason) = deauth {
            let (bssid, our, authed) = {
                let l = self.link.as_ref().expect("link");
                (l.bssid, l.our_mac, l.done.sta)
            };
            if authed {
                let mut f = [0u8; 32];
                if let Ok(n) = dot11::deauth(&mut f, &bssid, &our, reason) {
                    if self.send_mgmt(&f[..n], false).is_ok() {
                        // let the firmware put it on the air before the station goes away
                        let _ = self.wait_frame(20);
                    }
                }
            }
        }
        let (ptk, gtks) = {
            let l = self.link.as_mut().expect("link");
            let p = l.ptk.take().map(|k| k.slot);
            let g: Vec<(u8, u8)> = l.gtks.iter_mut().enumerate().filter_map(|(i, g)| g.take().map(|g| (i as u8, g.slot))).collect();
            (p, g)
        };
        if let Some(s) = ptk {
            self.remove_key(s, 0, false);
        }
        for (id, s) in gtks {
            self.remove_key(s, id, true);
        }
        self.join_stop_te();
        let done = self.link.as_ref().map(|l| l.done).unwrap_or_default();
        let mut b = [0u8; 160];
        if done.sta {
            if let Ok(n) = remove_sta(AP_STA_ID, &mut b) {
                let _ = self.jcmd("rm-sta", iwlwifi_mvm_join_core::ids::REMOVE_STA, &b[..n]);
            }
        }
        if done.queue {
            if let Ok(n) = scd_queue_disable(LINK_QUEUE, AP_STA_ID, IWL_MAX_TID_COUNT, &mut b) {
                let _ = self.jcmd("txq-off", SCD_QUEUE_CFG, &b[..n]);
            }
        }
        if done.binding {
            if let Ok(n) = binding_v1(FW_CTXT_ACTION_REMOVE, PHY_ID, COLOR, self.rt.mac_id, COLOR, &mut b) {
                let _ = self.jcmd("unbind", BINDING_CONTEXT_CMD, &b[..n]);
            }
        }
        if done.mac {
            let mut mc = self.mac_ctxt_for([0; 6], None);
            mc.dtim_period = 0;
            mc.beacon_int = 0;
            if let Ok(n) = mac_context_sta(FW_CTXT_ACTION_MODIFY, &mc, &mut b) {
                let _ = self.jcmd("mac-reset", MAC_CONTEXT_CMD, &b[..n]);
            }
        }
        if done.phy {
            let l = self.link.as_ref().expect("link");
            let phy = PhyCtxt { id: PHY_ID, color: COLOR, channel: l.channel, band_5ghz: l.five_ghz, valid_tx_ant: self.valid_tx_ant, valid_rx_ant: self.valid_rx_ant, chains_static: 1, chains_dynamic: 1 };
            let uhb = self.fw.info.capa.has(CAPA_ULTRA_HB_CHANNELS);
            if let Ok(n) = phy_context(FW_CTXT_ACTION_REMOVE, &phy, uhb, &mut b) {
                let _ = self.jcmd("phy-remove", PHY_CONTEXT_CMD, &b[..n]);
            }
        }
        self.link = None;
        self.t.inbox.retain(|r| !(r.group == LEGACY_GROUP && matches!(r.cmd, TX_CMD | iwlwifi_mvm_rt_core::ids::REPLY_RX_MPDU_CMD)));
        self.log(format_args!("[IWL-LINK-GONE]"));
        let _ = ADD_STA;
        let _ = Chunk { data: &[], nocopy: false };
    }
}

/// The data rate: the preference when the BSS supports it, else the highest supported rate below it, else the lowest.
pub fn pick_data_rate(supported: u16, five_ghz: bool) -> usize {
    let lo = if five_ghz { rates::FIRST_OFDM } else { rates::FIRST_CCK };
    let mut best = None;
    for i in lo..rates::RATE_COUNT {
        if supported & (1 << i) != 0 && i <= DATA_RATE_PREFERENCE {
            best = Some(i);
        }
    }
    if let Some(b) = best {
        return b;
    }
    (lo..rates::RATE_COUNT).find(|&i| supported & (1 << i) != 0).unwrap_or(if five_ghz { rates::RATE_6M } else { rates::RATE_1M })
}
