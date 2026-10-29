// SPDX-License-Identifier: GPL-2.0-only
//! The link to one access point after the firmware contexts exist: what the station knows about it, the receive
//! classifier (management responses, data, EAPOL), the transmit helpers (management frames, data frames with the
//! host-built CCMP header), the supplicant glue (EAPOL in and out, key installation through `ADD_STA_KEY`), and the
//! `TX_CMD` response handler that reclaims the queue and releases message 4.
//!
//! Ported from Linux `mvm/rxmq.c` (`iwl_mvm_rx_mpdu_mq`, `iwl_mvm_rx_crypto`), `mvm/tx.c` (`iwl_mvm_tx_skb`,
//! `iwl_mvm_set_tx_cmd*`, `iwl_mvm_rx_tx_cmd_single`) and `mvm/sta.c` (`iwl_mvm_set_sta_key`), with the packet-number
//! check of mac80211 `wpa.c` and the 802.1X port rules of OpenBSD `ieee80211_input.c`. The supplicant is
//! `wpa-supplicant-core`; this file performs the work it asks for.

use crate::driver::{Driver, DriverError};
use crate::event::{Event, LinkDown};
use crate::host::Host;
use alloc::vec::Vec;
use iwlwifi_fw_tlv_core::capa::API_TKIP_MIC_KEYS;
use iwlwifi_mvm_join_core::dot11::{self, Mac, DataIn, ETHERTYPE_EAPOL};
use iwlwifi_mvm_join_core::ids::*;
use iwlwifi_mvm_join_core::rates::rate_n_flags_v1;
use iwlwifi_mvm_join_core::rx::{rx_security, PnReplay, RxSec};
use iwlwifi_mvm_join_core::txcmd::{decode_tx_resp, TxParams};
use iwlwifi_mvm_rt_core::decode::{add_sta_ok, decode_add_sta_status, decode_rx_mpdu_v1};
use iwlwifi_mvm_rt_core::ids::{ADD_STA, LEGACY_GROUP, REPLY_RX_MPDU_CMD};
use iwlwifi_mvm_sta_key_core::encode as keyenc;
use iwlwifi_mvm_sta_key_core::ids::ADD_STA_KEY;
use iwlwifi_mvm_sta_key_core::slots::KeySlots;
use iwlwifi_pcie_ring_core::tx::Chunk;
use wpa_supplicant_core::supplicant::MAX_TX;
use wpa_supplicant_core::{Entropy, GroupKey, InstallError, KeyInstaller, PairwiseKey, Reply, State, Supplicant};

/// Why a join step or a link operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinWhy {
    /// The RT image is not up.
    NotUp,
    /// A link is already up or being set up.
    Busy,
    /// No link.
    NoLink,
    /// The BSS is not in the last scan.
    NoScanResult,
    /// The stored frame is not a usable beacon.
    Beacon(iwlwifi_mvm_join_core::bss::BssRefusal),
    /// The network needs something this station does not do; the text names it.
    Unsupported(&'static str),
    /// The host has no entropy source: no handshake is started without one.
    NoEntropy,
    /// A command could not be encoded.
    Encode(iwlwifi_mvm_join_core::EncodeError),
    /// A command or a frame could not be sent or answered.
    Transport(crate::transport::TransportError),
    /// A command's response carried a status the stage refuses.
    Status(u32),
    /// A response that does not decode.
    Decode(&'static str),
    /// The firmware did not schedule the time event.
    TimeEvent(iwlwifi_mvm_join_core::ctxt::TimeEventRefusal),
    /// No answer within the bound; the text names what was awaited.
    Timeout(&'static str),
    /// The AP refused authentication; carries its status code.
    AuthRefused(u16),
    /// The AP refused association; carries its status code.
    AssocRefused(u16),
    /// The AP deauthenticated or disassociated us; carries the reason code.
    Dropped(u16),
    /// The key handshake ended; carries the supplicant's refusal.
    Handshake(wpa_supplicant_core::Refusal),
    /// The key table is full or the key command was refused.
    Key(&'static str),
    /// A frame to send is malformed (an Ethernet frame shorter than its header, or not from this station).
    BadFrame(&'static str),
    /// A fault latched in the card.
    Driver(DriverError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinError {
    pub stage: &'static str,
    pub why: JoinWhy,
}

impl JoinError {
    pub fn new(stage: &'static str, why: JoinWhy) -> JoinError {
        JoinError { stage, why }
    }
    /// The 802.11 reason code a failed join deauthenticates with: only a failed key handshake does (the AP holds our
    /// association by then); a refused or unanswered authentication or association leaves nothing to deauthenticate.
    pub fn deauth_reason(&self) -> Option<u16> {
        match &self.why {
            JoinWhy::Handshake(r) => Some(r.deauth_reason().unwrap_or(wpa_supplicant_core::refusal::REASON_AUTH_LEAVE)),
            _ => None,
        }
    }

    /// The number carried in `Event::JoinFailed`: the 802.11 status or reason when the AP sent one.
    pub fn code(&self) -> u32 {
        match &self.why {
            JoinWhy::AuthRefused(c) | JoinWhy::AssocRefused(c) | JoinWhy::Dropped(c) => *c as u32,
            JoinWhy::Handshake(r) => r.deauth_reason().unwrap_or(0) as u32,
            JoinWhy::Status(s) => *s,
            _ => 0,
        }
    }
}

pub(crate) struct PtkInfo {
    pub tk: [u8; 16],
    pub slot: u8,
    pub rx: PnReplay,
}

pub(crate) struct GtkInfo {
    pub slot: u8,
    pub rx: PnReplay,
}

/// What exists in the firmware for this link, so a teardown undoes exactly that.
#[derive(Default, Clone, Copy)]
pub(crate) struct Done {
    pub phy: bool,
    pub mac: bool,
    pub binding: bool,
    pub queue: bool,
    pub sta: bool,
}

/// Counters the markers and the tests read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinkStats {
    pub tx_ok: u32,
    pub tx_fail: u32,
    pub rx_data: u32,
    pub rx_replay: u32,
    pub rx_dropped_sec: u32,
    pub rx_dropped_port: u32,
}

pub struct Link {
    pub bssid: Mac,
    pub ssid: Vec<u8>,
    pub channel: u8,
    pub five_ghz: bool,
    pub our_mac: Mac,
    pub aid: u16,
    /// A protected network: frames are CCMP and the port opens with the keys.
    pub secure: bool,
    /// Authenticated, associated and (when secure) keyed: data may flow.
    pub up: bool,
    pub mgmt_rate: usize,
    pub data_rate: usize,
    pub ant: u8,
    pub qid: u8,
    pub beacon_int: u16,
    pub dtim_period: u8,
    pub basic_rates: u16,
    pub short_preamble: bool,
    pub short_slot: bool,
    pub use_cts_prot: bool,
    pub(crate) sup: Option<Supplicant>,
    pub(crate) ptk: Option<PtkInfo>,
    pub(crate) gtks: [Option<GtkInfo>; 4],
    pub(crate) slots: KeySlots,
    pub(crate) pn_tx: u64,
    /// The ring index of message 4 while the host waits for its `TX_CMD` response.
    pub(crate) pending_m4: Option<u16>,
    pub(crate) te_uid: Option<u32>,
    pub(crate) done: Done,
    pub stats: LinkStats,
}

/// A frame the classifier hands the join and the poll loop.
pub(crate) enum Frame {
    Auth { algo: u16, seq: u16, status: u16 },
    AssocResp { status: u16, aid: u16 },
    Deauth(u16),
    Disassoc(u16),
    Eapol(Vec<u8>),
    Data { da: Mac, sa: Mac, ethertype: u16, payload: Vec<u8> },
    Ignored,
}

enum KeyOp {
    Ptk { tk: [u8; 16], rsc: u64 },
    Gtk { id: u8, key: [u8; 16], tx: bool, rsc: u64 },
    OpenPort,
}

/// Records what the supplicant asks to install; the driver performs it once the supplicant call returns.
#[derive(Default)]
struct Recorder {
    ops: Vec<KeyOp>,
}

impl KeyInstaller for Recorder {
    fn install_ptk(&mut self, k: &PairwiseKey<'_>) -> Result<(), InstallError> {
        let tk: [u8; 16] = k.tk.try_into().map_err(|_| InstallError::Refused)?;
        self.ops.push(KeyOp::Ptk { tk, rsc: k.rsc });
        Ok(())
    }
    fn install_gtk(&mut self, k: &GroupKey<'_>) -> Result<(), InstallError> {
        let key: [u8; 16] = k.key.try_into().map_err(|_| InstallError::Refused)?;
        self.ops.push(KeyOp::Gtk { id: k.key_id, key, tx: k.tx, rsc: k.rsc });
        Ok(())
    }
    fn open_port(&mut self) {
        self.ops.push(KeyOp::OpenPort);
    }
}

/// The supplicant's SNonce source: the host's entropy, remembering a refusal.
struct HostEntropy<'a, H: Host> {
    h: &'a mut H,
    ok: bool,
}

impl<H: Host> Entropy for HostEntropy<'_, H> {
    fn fill(&mut self, out: &mut [u8]) {
        if !self.h.random(out) {
            self.ok = false;
            out.fill(0);
        }
    }
}

fn is_group(a: &Mac) -> bool {
    a[0] & 1 != 0
}

impl<H: Host> Driver<H> {
    pub(crate) fn link_mut(&mut self, stage: &'static str) -> Result<&mut Link, JoinError> {
        self.link.as_deref_mut().ok_or(JoinError::new(stage, JoinWhy::NoLink))
    }

    pub fn link(&self) -> Option<&Link> {
        self.link.as_deref()
    }

    /// Send a host command of the legacy group; names the stage on failure.
    pub(crate) fn jcmd(&mut self, stage: &'static str, cmd: u8, payload: &[u8]) -> Result<crate::transport::Rx, JoinError> {
        match self.t.send_cmd(LEGACY_GROUP, cmd, 0, &[Chunk { data: payload, nocopy: false }]) {
            Ok(rx) => {
                self.rt.cmds += 1;
                // every join command and its answer's first word, so a refusal names its step
                let w0 = rx.payload.get(..4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
                self.log(format_args!("[IWL-JCMD {stage} cmd={cmd:#04x} len={} resp_len={} w0={w0:#x?}]", payload.len(), rx.payload.len()));
                Ok(rx)
            }
            Err(e) => {
                self.log(format_args!("[IWL-JCMD {stage} cmd={cmd:#04x} len={} FAILED {e:?}]", payload.len()));
                let _ = self.nic_fault_check(stage);
                Err(JoinError::new(stage, JoinWhy::Transport(e)))
            }
        }
    }

    // ------------------------------------------------------------------------------------------ transmit

    fn ant_mask(&self) -> u8 {
        let v = self.valid_tx_ant & 3;
        if v == 0 { 1 } else { 1 << v.trailing_zeros() }
    }

    /// Put a management frame (24-byte header + body) on the link's queue. `assoc` selects the longer PM timeout.
    pub(crate) fn send_mgmt(&mut self, frame: &[u8], assoc: bool) -> Result<u16, JoinError> {
        let l = self.link.as_ref().ok_or(JoinError::new("tx", JoinWhy::NoLink))?;
        let (qid, rate, ant, five) = (l.qid, l.mgmt_rate, self.ant_mask(), l.five_ghz);
        let p = TxParams {
            sta_id: AP_STA_ID,
            tid: IWL_MAX_TID_COUNT,
            frame_len: frame.len() as u16,
            hdr_len: dot11::HDR_LEN as u8,
            rate_n_flags: rate_n_flags_v1(rate, ant),
            key: None,
            ack: true,
            fw_seq: true,
            pm_frame_timeout: if assoc { PM_FRAME_ASSOC } else { PM_FRAME_MGMT },
            bt_prio: if five { 0 } else { 3 }, // iwl_mvm_bt_coex_tx_prio: management frames on 2.4 GHz
        };
        self.t.txq_send(qid, &p, &frame[..dot11::HDR_LEN], &frame[dot11::HDR_LEN..]).map_err(|e| JoinError::new("tx", JoinWhy::Transport(e)))
    }

    /// Put one data frame on the queue: `da`, the Ethernet type and the payload, CCMP-protected with the pairwise key
    /// when one is installed. Returns the ring index.
    pub(crate) fn send_data(&mut self, da: &Mac, ethertype: u16, payload: &[u8]) -> Result<u16, JoinError> {
        let ant = self.ant_mask();
        let l = self.link.as_mut().ok_or(JoinError::new("tx", JoinWhy::NoLink))?;
        let mut hdr = [0u8; dot11::HDR_LEN];
        let protected = l.ptk.is_some();
        dot11::data_header_to_ds(&mut hdr, &l.bssid, &l.our_mac, da, protected).map_err(|e| JoinError::new("tx", JoinWhy::Encode(e)))?;
        let mut body = Vec::with_capacity(dot11::CCMP_HDR_LEN + dot11::LLC_SNAP_LEN + payload.len());
        let key = match &l.ptk {
            Some(k) => {
                l.pn_tx += 1; // iwl_mvm_set_tx_cmd_pn: atomic64_inc_return, so the first frame carries PN 1
                let mut c = [0u8; dot11::CCMP_HDR_LEN];
                dot11::ccmp_header(&mut c, l.pn_tx, 0).map_err(|e| JoinError::new("tx", JoinWhy::Encode(e)))?;
                body.extend_from_slice(&c);
                Some(k.tk)
            }
            None => None,
        };
        let mut s = [0u8; dot11::LLC_SNAP_LEN];
        dot11::llc_snap(&mut s, ethertype).map_err(|e| JoinError::new("tx", JoinWhy::Encode(e)))?;
        body.extend_from_slice(&s);
        body.extend_from_slice(payload);
        let eapol = ethertype == ETHERTYPE_EAPOL;
        let p = TxParams {
            sta_id: AP_STA_ID,
            tid: IWL_TID_NON_QOS,
            frame_len: (dot11::HDR_LEN + body.len()) as u16,
            hdr_len: dot11::HDR_LEN as u8,
            rate_n_flags: rate_n_flags_v1(l.data_rate, ant),
            key,
            ack: !is_group(da),
            fw_seq: true,
            pm_frame_timeout: if eapol { PM_FRAME_MGMT } else { PM_FRAME_NONE }, // IEEE80211_TX_CTRL_PORT_CTRL_PROTO
            bt_prio: if !l.five_ghz && is_group(da) { 3 } else { 0 },
        };
        let qid = l.qid;
        self.t.txq_send(qid, &p, &hdr, &body).map_err(|e| JoinError::new("tx", JoinWhy::Transport(e)))
    }

    // ------------------------------------------------------------------------------------------- receive

    /// Reduce one `REPLY_RX_MPDU_CMD` to what the link acts on. // iwl_mvm_rx_mpdu_mq, mvm/rxmq.c
    pub(crate) fn classify_rx(&mut self, payload: &[u8]) -> Frame {
        let Ok(m) = decode_rx_mpdu_v1(payload) else { return Frame::Ignored };
        if !m.crc_ok || m.mpdu.len() < dot11::HDR_LEN {
            return Frame::Ignored;
        }
        let status = u32::from_le_bytes([payload[RX_DESC_STATUS_OFF], payload[RX_DESC_STATUS_OFF + 1], payload[RX_DESC_STATUS_OFF + 2], payload[RX_DESC_STATUS_OFF + 3]]);
        let Some(l) = self.link.as_mut() else { return Frame::Ignored };
        let fc0 = m.mpdu[0];
        if fc0 & dot11::FC0_TYPE_MASK == dot11::FC0_TYPE_MGMT {
            let Ok(g) = dot11::parse_mgmt(m.mpdu, m.padded) else { return Frame::Ignored };
            if g.sa != l.bssid || g.bssid != l.bssid {
                return Frame::Ignored;
            }
            return match g.subtype {
                dot11::SUBTYPE_AUTH => dot11::parse_auth(g.body).map_or(Frame::Ignored, |(algo, seq, status)| Frame::Auth { algo, seq, status }),
                dot11::SUBTYPE_ASSOC_RESP => dot11::parse_assoc_resp(g.body).map_or(Frame::Ignored, |(_, status, aid)| Frame::AssocResp { status, aid }),
                dot11::SUBTYPE_DEAUTH => dot11::parse_reason(g.body).map_or(Frame::Ignored, Frame::Deauth),
                dot11::SUBTYPE_DISASSOC => dot11::parse_reason(g.body).map_or(Frame::Ignored, Frame::Disassoc),
                _ => Frame::Ignored,
            };
        }
        let Ok(d) = dot11::parse_data(m.mpdu, m.padded) else { return Frame::Ignored };
        if d.bssid != l.bssid {
            return Frame::Ignored;
        }
        match rx_security(status, d.protected) {
            Ok(RxSec::Clear) | Ok(RxSec::CcmpDecrypted) => {}
            Err(_) => {
                l.stats.rx_dropped_sec += 1;
                return Frame::Ignored;
            }
        }
        if let Some((pn, kid)) = d.pn {
            let ok = if is_group(&d.da) {
                l.gtks[(kid & 3) as usize].as_mut().is_some_and(|g| g.rx.accept(pn))
            } else {
                l.ptk.as_mut().is_some_and(|k| k.rx.accept(pn))
            };
            if !ok {
                l.stats.rx_replay += 1;
                return Frame::Ignored;
            }
        }
        l.stats.rx_data += 1;
        if d.ethertype == ETHERTYPE_EAPOL {
            return Frame::Eapol(d.payload.to_vec());
        }
        // The controlled port: nothing but EAPOL until the keys are in, and nothing unprotected once they are.
        if l.secure && (!l.up || !d.protected) {
            l.stats.rx_dropped_port += 1;
            return Frame::Ignored;
        }
        let DataIn { da, sa, ethertype, payload, .. } = d;
        Frame::Data { da, sa, ethertype, payload: payload.to_vec() }
    }

    /// Wait up to `timeout_ms` for the next frame the link cares about. `Ok(None)` is a timeout. `TX_CMD` responses and
    /// time-event notifications are consumed on the way.
    pub(crate) fn wait_frame(&mut self, timeout_ms: u32) -> Result<Option<Frame>, JoinError> {
        let deadline = self.t.host().now_ms() + timeout_ms as u64;
        loop {
            let left = deadline.saturating_sub(self.t.host().now_ms()) as u32;
            let r = self.t.wait_notif(left, |r| r.group == LEGACY_GROUP && matches!(r.cmd, REPLY_RX_MPDU_CMD | TX_CMD | TIME_EVENT_NOTIFICATION));
            match r {
                Ok(rx) => match rx.cmd {
                    TX_CMD => self.on_tx_resp(&rx.payload, rx.sequence)?,
                    TIME_EVENT_NOTIFICATION => self.on_time_event(&rx.payload),
                    _ => return Ok(Some(self.classify_rx(&rx.payload))),
                },
                Err(crate::transport::TransportError::Timeout { .. }) => return Ok(None),
                Err(e) => {
                    let _ = self.nic_fault_check("link");
                    return Err(JoinError::new("link", JoinWhy::Transport(e)));
                }
            }
        }
    }

    fn on_time_event(&mut self, p: &[u8]) {
        if p.len() >= 24 {
            let action = u32::from_le_bytes([p[16], p[17], p[18], p[19]]);
            let status = u32::from_le_bytes([p[20], p[21], p[22], p[23]]);
            self.log(format_args!("[IWL-JOIN-TE notif action={action} status={status}]"));
        }
    }

    /// The firmware reports `frame_count` frames of a queue sent or dropped: reclaim them, and release message 4.
    /// // iwl_mvm_rx_tx_cmd_single, mvm/tx.c:1547
    fn on_tx_resp(&mut self, payload: &[u8], sequence: u16) -> Result<(), JoinError> {
        let Ok(r) = decode_tx_resp(payload) else {
            self.log(format_args!("[IWL-TX-RESP undecodable len={}]", payload.len()));
            return Ok(());
        };
        let (qid, idx) = (((sequence >> 8) & 0x1f) as u8, sequence & 0xff);
        if let Err(e) = self.t.txq_reclaim(qid, r.ssn) {
            self.log(format_args!("[IWL-TX-RESP reclaim refused qid={qid} ssn={} {e:?}]", r.ssn));
            return Ok(());
        }
        if let Some(l) = self.link.as_mut() {
            if r.success() {
                l.stats.tx_ok += 1;
            } else {
                l.stats.tx_fail += 1;
                self.log(format_args!("[IWL-TX-FAIL qid={qid} idx={idx} status={:#x} retries={}]", r.status, r.failure_frame));
            }
        }
        let release = self.link.as_ref().is_some_and(|l| l.pending_m4 == Some(idx) && l.qid == qid);
        if release {
            self.link_mut("4way")?.pending_m4 = None;
            self.m4_sent(r.success())?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------ supplicant glue

    /// One EAPOL PDU from the AP. A reply is sent; key operations the supplicant asked for are performed.
    pub(crate) fn eapol_in(&mut self, pdu: &[u8]) -> Result<(), JoinError> {
        let now = self.t.host().now_ms();
        let mut sup = match self.link.as_mut().and_then(|l| l.sup.take()) {
            Some(s) => s,
            None => return Ok(()), // an open network, or the handshake is over: EAPOL is ignored
        };
        let mut tx = [0u8; MAX_TX];
        let mut rec = Recorder::default();
        let (r, entropy_ok) = {
            let mut ent = HostEntropy { h: self.t.host(), ok: true };
            let r = sup.on_eapol(pdu, &mut tx, now, &mut ent, &mut rec);
            (r, ent.ok)
        };
        if !entropy_ok {
            // Never send a message 2 built on a zero SNonce.
            self.log(format_args!("[IWL-4WAY FAIL no entropy]"));
            self.link_mut("4way")?.sup = Some(sup);
            return Err(JoinError::new("4way", JoinWhy::NoEntropy));
        }
        let was = sup.state();
        match r {
            Ok(Reply::Send(n)) => {
                let m4 = was == State::M4Sent;
                let est_before = self.link.as_ref().is_some_and(|l| l.up);
                self.link_mut("4way")?.sup = Some(sup);
                // Keys the supplicant asked for during this call (a group rekey) go in before the reply leaves.
                if let Err(e) = self.apply_key_ops(rec.ops) {
                    return self.handshake_failed(e);
                }
                let idx = self.send_data(&self.link.as_ref().map(|l| l.bssid).unwrap_or([0; 6]), ETHERTYPE_EAPOL, &tx[..n])?;
                if m4 {
                    self.link_mut("4way")?.pending_m4 = Some(idx);
                    self.log(format_args!("[IWL-4WAY m4 queued idx={idx}]"));
                } else if est_before {
                    self.log(format_args!("[IWL-GROUP-REKEY replied]"));
                } else {
                    self.log(format_args!("[IWL-4WAY m2 sent]"));
                }
                Ok(())
            }
            Ok(Reply::None) => {
                self.link_mut("4way")?.sup = Some(sup);
                Ok(())
            }
            Err(refusal) => {
                let fatal = refusal.severity() == wpa_supplicant_core::Severity::Fatal;
                self.log(format_args!("[IWL-4WAY refused {refusal:?} fatal={}]", fatal as u8));
                self.link_mut("4way")?.sup = Some(sup);
                if fatal {
                    return self.handshake_failed(JoinError::new("4way", JoinWhy::Handshake(refusal)));
                }
                Ok(())
            }
        }
    }

    /// Message 4 has been transmitted (`ok`) or has not (`!ok`).
    fn m4_sent(&mut self, ok: bool) -> Result<(), JoinError> {
        let mut sup = match self.link.as_mut().and_then(|l| l.sup.take()) {
            Some(s) => s,
            None => return Ok(()),
        };
        if !ok {
            sup.tx_failed();
            self.link_mut("4way")?.sup = Some(sup);
            return Ok(());
        }
        let mut rec = Recorder::default();
        let r = sup.tx_done(&mut rec);
        let established = sup.is_established();
        self.link_mut("4way")?.sup = Some(sup);
        if let Err(refusal) = r {
            return self.handshake_failed(JoinError::new("4way", JoinWhy::Handshake(refusal)));
        }
        if let Err(e) = self.apply_key_ops(rec.ops) {
            return self.handshake_failed(e);
        }
        if established {
            self.log(format_args!("[IWL-4WAY OK]"));
            self.link_up();
        }
        Ok(())
    }

    /// The handshake cannot continue: tell the AP and report the link down.
    fn handshake_failed(&mut self, e: JoinError) -> Result<(), JoinError> {
        let reason = match &e.why {
            JoinWhy::Handshake(r) => r.deauth_reason().unwrap_or(wpa_supplicant_core::refusal::REASON_AUTH_LEAVE),
            _ => wpa_supplicant_core::refusal::REASON_AUTH_LEAVE,
        };
        self.log(format_args!("[IWL-4WAY FAIL {:?} reason={reason}]", e.why));
        let was_up = self.link.as_ref().is_some_and(|l| l.up);
        self.teardown(Some(reason));
        if was_up {
            self.pending_events.push(Event::LinkDown { reason: LinkDown::HandshakeFailed(reason) });
        }
        Err(e)
    }

    pub(crate) fn link_up(&mut self) {
        let Some(l) = self.link.as_mut() else { return };
        if l.up {
            return;
        }
        l.up = true;
        let ev = Event::LinkUp { bssid: l.bssid, channel: l.channel, aid: l.aid };
        self.pending_events.push(ev);
        self.log(format_args!("[IWL-LINK-UP]"));
    }

    // ------------------------------------------------------------------------------------------- keys

    fn key_ver(&self) -> u8 {
        self.fw.cmd_versions().cmd_ver(LEGACY_GROUP, ADD_STA_KEY, 1)
    }

    fn send_key(&mut self, stage: &'static str, c: &keyenc::Cmd) -> Result<(), JoinError> {
        let rx = self.jcmd(stage, ADD_STA_KEY, c.as_slice())?;
        iwlwifi_mvm_sta_key_core::decode::status(&rx.payload).map_err(|_| JoinError::new(stage, JoinWhy::Key("ADD_STA_KEY refused")))
    }

    pub(crate) fn remove_key(&mut self, slot: u8, id: u8, mcast: bool) {
        let v2 = self.fw.info.api.has(API_TKIP_MIC_KEYS);
        if let Ok(c) = keyenc::remove(AP_STA_ID, slot, id, mcast, v2) {
            let _ = self.send_key("key-remove", &c);
        }
        if let Some(l) = self.link.as_mut() {
            let _ = l.slots.free(slot);
        }
    }

    /// Perform what the supplicant asked the installer for. // iwl_mvm_set_sta_key, mvm/sta.c:3841
    fn apply_key_ops(&mut self, ops: Vec<KeyOp>) -> Result<(), JoinError> {
        for op in ops {
            match op {
                KeyOp::Ptk { tk, rsc } => {
                    let ver = self.key_ver();
                    let slot = self.link_mut("keys")?.slots.alloc().map_err(|_| JoinError::new("keys", JoinWhy::Key("key table full")))?;
                    let c = keyenc::ccmp(ver, AP_STA_ID, slot, 0, false, &tk, rsc, 0).map_err(|_| JoinError::new("keys", JoinWhy::Key("encode")))?;
                    if let Err(e) = self.send_key("keys", &c) {
                        let _ = self.link_mut("keys")?.slots.free(slot);
                        return Err(e);
                    }
                    let old = self.link_mut("keys")?.ptk.replace(PtkInfo { tk, slot, rx: PnReplay::from_rsc(rsc) });
                    self.link_mut("keys")?.pn_tx = 0;
                    if let Some(o) = old {
                        self.remove_key(o.slot, 0, false);
                    }
                    self.log(format_args!("[IWL-KEYS ptk slot={slot} rsc={rsc}]"));
                }
                KeyOp::Gtk { id, key, tx, rsc } => {
                    let ver = self.key_ver();
                    let slot = self.link_mut("keys")?.slots.alloc().map_err(|_| JoinError::new("keys", JoinWhy::Key("key table full")))?;
                    let c = keyenc::ccmp(ver, AP_STA_ID, slot, id, true, &key, rsc, 0).map_err(|_| JoinError::new("keys", JoinWhy::Key("encode")))?;
                    if let Err(e) = self.send_key("keys", &c) {
                        let _ = self.link_mut("keys")?.slots.free(slot);
                        return Err(e);
                    }
                    let old = self.link_mut("keys")?.gtks[(id & 3) as usize].replace(GtkInfo { slot, rx: PnReplay::from_rsc(rsc) });
                    if let Some(o) = old {
                        self.remove_key(o.slot, id, true);
                    }
                    self.log(format_args!("[IWL-KEYS gtk id={id} slot={slot} tx={} rsc={rsc}]", tx as u8));
                }
                KeyOp::OpenPort => {
                    self.log(format_args!("[IWL-PORT open]"));
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------------ the poll

    /// Run the link for up to `timeout_ms` and return what happened as events: received data frames, group rekeys
    /// answered, the link going down. A `timeout_ms` of 0 does not wait: it drains what the card has already posted.
    /// With a timeout it returns at the first event or when the time is up.
    pub fn link_poll(&mut self, timeout_ms: u32) -> Result<Vec<Event>, JoinError> {
        let deadline = self.t.host().now_ms() + timeout_ms as u64;
        let drain = timeout_ms == 0;
        let mut out: Vec<Event> = core::mem::take(&mut self.pending_events);
        loop {
            if self.link.is_none() {
                return Ok(out);
            }
            let now = self.t.host().now_ms();
            // The handshake bound (and nothing else) runs on the poll's clock.
            let sup_tick = self.link.as_mut().and_then(|l| l.sup.as_mut().map(|s| s.tick(now)));
            if let Some(Err(refusal)) = sup_tick {
                let r = self.handshake_failed(JoinError::new("4way", JoinWhy::Handshake(refusal)));
                return self.settle(out, r);
            }
            let left = deadline.saturating_sub(now) as u32;
            if !drain && (left == 0 || !out.is_empty()) {
                return Ok(out);
            }
            let frame = match self.wait_frame(if drain { 0 } else { left.min(100) })? {
                None => {
                    if drain {
                        return Ok(out);
                    }
                    continue;
                }
                Some(f) => f,
            };
            match frame {
                Frame::Data { da, sa, ethertype, payload } => {
                    let mut f = Vec::with_capacity(14 + payload.len());
                    f.extend_from_slice(&da);
                    f.extend_from_slice(&sa);
                    f.extend_from_slice(&ethertype.to_be_bytes());
                    f.extend_from_slice(&payload);
                    out.push(Event::Rx(f));
                }
                Frame::Eapol(p) => {
                    let r = self.eapol_in(&p);
                    if r.is_err() {
                        return self.settle(out, r);
                    }
                }
                Frame::Deauth(reason) => {
                    self.log(format_args!("[IWL-LINK-DOWN deauth reason={reason}]"));
                    self.teardown(None);
                    out.push(Event::LinkDown { reason: LinkDown::Deauthenticated(reason) });
                }
                Frame::Disassoc(reason) => {
                    self.log(format_args!("[IWL-LINK-DOWN disassoc reason={reason}]"));
                    self.teardown(None);
                    out.push(Event::LinkDown { reason: LinkDown::Disassociated(reason) });
                }
                _ => {}
            }
            out.append(&mut self.pending_events);
        }
    }

    /// A failure inside a poll: if it took the link down, the events (the link-down among them) are the answer; if the
    /// link survives, the error is.
    fn settle(&mut self, mut out: Vec<Event>, r: Result<(), JoinError>) -> Result<Vec<Event>, JoinError> {
        out.append(&mut self.pending_events);
        match r {
            Err(e) if self.link.is_some() => Err(e),
            _ => Ok(out),
        }
    }

    /// Send one Ethernet II frame on the link (`dst`, `src`, ethertype, payload). The source must be this station.
    pub fn link_send(&mut self, eth: &[u8]) -> Result<(), JoinError> {
        let l = self.link.as_ref().ok_or(JoinError::new("tx", JoinWhy::NoLink))?;
        if !l.up {
            return Err(JoinError::new("tx", JoinWhy::NoLink));
        }
        if eth.len() < 14 {
            return Err(JoinError::new("tx", JoinWhy::BadFrame("shorter than an Ethernet header")));
        }
        if eth[6..12] != l.our_mac {
            return Err(JoinError::new("tx", JoinWhy::BadFrame("source is not this station")));
        }
        let da: Mac = eth[..6].try_into().expect("6 bytes");
        let et = u16::from_be_bytes([eth[12], eth[13]]);
        self.send_data(&da, et, &eth[14..]).map(|_| ())
    }

    /// Take the events queued by operations that have no caller to return them to.
    pub fn take_events(&mut self) -> Vec<Event> {
        core::mem::take(&mut self.pending_events)
    }

    /// Whether `add_sta_ok` accepts an ADD_STA response; shared by the join steps.
    pub(crate) fn add_sta_status(&mut self, stage: &'static str, payload: &[u8]) -> Result<(), JoinError> {
        let s = decode_add_sta_status(payload).map_err(|_| JoinError::new(stage, JoinWhy::Decode("ADD_STA status")))?;
        if add_sta_ok(s) {
            Ok(())
        } else {
            Err(JoinError::new(stage, JoinWhy::Status(s)))
        }
    }

    pub(crate) fn add_sta_cmd(&mut self, stage: &'static str, buf: &[u8]) -> Result<(), JoinError> {
        let rx = self.jcmd(stage, ADD_STA, buf)?;
        self.add_sta_status(stage, &rx.payload)
    }
}
