// SPDX-License-Identifier: GPL-2.0-only
//! The join half of the simulated RT firmware, and the access point on the air. Every command the join sends is checked
//! against the Linux struct it must be (written here as LITERALS, never through the driver's or the cores' constants) and
//! against the state the commands before it built: a MAC context cannot be bound before a PHY context exists, a station
//! needs its queue, a key needs its station. Frames the host puts on a data queue are fetched from the TFD ring the way the
//! card does (three transfer buffers), taken apart, checked field by field (the TX command, the 802.11 header, the CCMP
//! header, the scheduler byte-count entry), and answered by the access point: authentication, association, the 4-way
//! handshake (through the supplicant crate's test authenticator, which builds its frames from literal offsets), group
//! rekeys, and data. Every violation is recorded; the tests assert there are none.
use super::rtfw::{ie, le16, le32, rx_payload, want_len, Out};
use super::{fwmodel as fm, Ev, SeenCmd, World};
use wpa_crypto_core::kdf::KdfKind;
use wpa_supplicant_core::testkit::{rsne_for, Auth};
use wpa_supplicant_core::{Akm, Cipher};

pub const MAC_ID: u32 = 0;
pub const STA_ID: u8 = 0;
pub const QUEUE: usize = 5;
/// The unique id the firmware hands out for the session-protection time event.
pub const TE_UID: u32 = 0x77;

/// The access point on the air.
pub struct JoinAp {
    pub bssid: [u8; 6],
    pub ssid: Vec<u8>,
    pub channel: u8,
    /// `Some(pmk)`: WPA2-PSK with CCMP; `None`: open.
    pub pmk: Option<[u8; 32]>,
    pub auth_status: u16,
    pub assoc_status: u16,
    pub aid: u16,
    /// The first N authentication / association requests get no answer.
    pub ignore_auth: u8,
    pub ignore_assoc: u8,
    /// Do not start the 4-way handshake after association.
    pub no_m1: bool,
    /// What the TX status of every frame the host sends reports (1 = success).
    pub tx_status: u16,
    /// From the Nth frame the host sends on, the TX status reports a failure.
    pub fail_tx_from: Option<usize>,
    /// The `status` word of the TIME_EVENT_CMD response (Linux never reads it) and of the START notification
    /// ("true if scheduled, false otherwise (not executed)", fw/api/time-event.h:253).
    pub te_resp_status: u32,
    pub te_start_status: u32,
}

impl JoinAp {
    pub fn new(ssid: &str, last: u8, channel: u8, pmk: Option<[u8; 32]>) -> JoinAp {
        JoinAp { bssid: [0x02, 0xaa, 0xbb, 0xcc, 0xdd, last], ssid: ssid.as_bytes().to_vec(), channel, pmk, auth_status: 0, assoc_status: 0, aid: 3, ignore_auth: 0, ignore_assoc: 0, no_m1: false, tx_status: 1, fail_tx_from: None, te_resp_status: 1, te_start_status: 1 }
    }
    pub fn five(&self) -> bool {
        self.channel > 14
    }
}

/// A frame the host sent, taken apart.
#[derive(Debug, Clone)]
pub struct HostFrame {
    pub q: usize,
    pub idx: u16,
    pub hdr: Vec<u8>,
    pub body: Vec<u8>,
    pub rate: u32,
    pub sec_ctl: u8,
    pub key: [u8; 16],
}

#[derive(Default)]
pub struct Join {
    pub ap: Option<JoinAp>,
    pub auth: Option<Auth>,
    // ---- what the host built in the firmware ----
    pub phy: bool,
    pub mac_bssid: [u8; 6],
    pub mac_assoc: bool,
    pub binding: bool,
    pub q_on: [bool; 32],
    pub q_seen: [u16; 32],
    pub sta: bool,
    pub sta_aid: u16,
    pub te: bool,
    pub mcast: bool,
    pub keys: [Option<(Vec<u8>, bool)>; 16],
    // ---- the air ----
    pub authenticated: bool,
    pub associated: bool,
    pub auth_seen: u8,
    pub assoc_seen: u8,
    pub m2_seen: u32,
    pub m4_seen: bool,
    pub g2_seen: u32,
    pub deauths: Vec<u16>,
    pub frames: Vec<HostFrame>,
    /// Data frames from the host: (pn, ethertype, payload, protected).
    pub data: Vec<(u64, u16, Vec<u8>, bool)>,
    pub last_pn: u64,
    /// The next packet number the AP encrypts with, and its group counter.
    pub ap_pn: u64,
    pub ap_gpn: u64,
    /// The group key the AP currently uses: (id, key).
    pub gtk: Option<(u8, [u8; 16])>,
    pub bc_tbl_checked: u32,
    pub log: Vec<String>,
}

impl Join {
    pub fn reset_run(&mut self) {
        // The card restarted: its contexts are gone; the access point and the test's world stay.
        self.phy = false;
        self.mac_bssid = [0; 6];
        self.mac_assoc = false;
        self.binding = false;
        self.q_on = [false; 32];
        self.q_seen = [0; 32];
        self.sta = false;
        self.te = false;
        self.mcast = false;
        self.keys = Default::default();
    }
}

fn mac_of(w: &World) -> [u8; 6] {
    w.fw.rt.mac
}

fn post_cmd(w: &mut World, delay: u64, cmd: u8, seq: u16, payload: Vec<u8>) {
    w.schedule(delay, Ev::Post { group: 0, cmd, seq, payload });
}

fn post_rx(w: &mut World, delay: u64, mpdu: &[u8], status: u32, flags2: u8) {
    let ch = w.fw.rt.join.ap.as_ref().map_or(1, |a| a.channel);
    post_cmd(w, delay, 0xc1, 0x8000, rx_payload(mpdu, status, -45, ch, flags2));
}

// ------------------------------------------------------------------------------------------ commands

pub(super) fn phy(w: &mut World, c: &SeenCmd, o: &mut Out) {
    if !want_len(o, c, 40, "PHY_CONTEXT_CMD (v1 layout, 8-byte channel info)") {
        return;
    }
    let p = &c.payload;
    let Some((ch, five)) = w.fw.rt.join.ap.as_ref().map(|a| (a.channel, a.five())) else {
        o.viol.push("PHY_CONTEXT_CMD with no access point on the air".into());
        return;
    };
    o.need(le32(p, 0) == 0, || "PHY_CONTEXT_CMD id_and_color is not phy 0 / color 0".into());
    o.need(le32(p, 8) == 0 && le32(p, 12) == 0, || "PHY_CONTEXT_CMD apply_time / tx_param_color are not zero".into());
    o.need(le32(p, 16) == ch as u32, || format!("PHY_CONTEXT_CMD channel {} is not the AP's {ch}", le32(p, 16)));
    o.need(p[20] == if five { 0 } else { 1 }, || format!("PHY_CONTEXT_CMD band {} (0 = 5 GHz, 1 = 2.4 GHz) for channel {ch}", p[20]));
    o.need(p[21] == 0 && p[22] == 0 && p[23] == 0, || "PHY_CONTEXT_CMD width/ctrl_pos/reserved are not a 20 MHz channel".into());
    o.need(le32(p, 24) == 3, || "PHY_CONTEXT_CMD txchain_info is not 3".into());
    o.need(le32(p, 28) == 0x1406, || format!("PHY_CONTEXT_CMD rxchain_info {:#x}, want valid 3 | idle 1 | active 1 = 0x1406", le32(p, 28)));
    o.need(le32(p, 32) == 0 && le32(p, 36) == 0, || "PHY_CONTEXT_CMD acquisition/dsp flags are not zero".into());
    let j = &mut w.fw.rt.join;
    match le32(p, 4) {
        1 => {
            o.need(!j.phy, || "PHY_CONTEXT_CMD ADD of a PHY context that exists".into());
            j.phy = true;
        }
        3 => {
            o.need(j.phy, || "PHY_CONTEXT_CMD REMOVE of a PHY context that does not exist".into());
            o.need(!j.binding, || "PHY_CONTEXT_CMD REMOVE while a binding still names it".into());
            j.phy = false;
        }
        a => o.viol.push(format!("PHY_CONTEXT_CMD action {a}")),
    }
}

pub(super) fn binding(w: &mut World, c: &SeenCmd, o: &mut Out) -> Vec<u8> {
    if !want_len(o, c, 24, "BINDING_CONTEXT_CMD v1") {
        return vec![0; 4];
    }
    let p = &c.payload;
    o.need(le32(p, 0) == 0 && le32(p, 20) == 0, || "BINDING_CONTEXT_CMD does not name PHY 0 / color 0".into());
    o.need(le32(p, 8) == MAC_ID, || "BINDING_CONTEXT_CMD macs[0] is not MAC 0".into());
    o.need(le32(p, 12) == 0xffff_ffff && le32(p, 16) == 0xffff_ffff, || "BINDING_CONTEXT_CMD macs[1..2] are not FW_CTXT_INVALID".into());
    o.need(w.fw.rt.mac_ctxt.is_some(), || "BINDING_CONTEXT_CMD before the MAC context exists".into());
    let j = &mut w.fw.rt.join;
    match le32(p, 4) {
        1 => {
            o.need(j.phy, || "BINDING_CONTEXT_CMD ADD before the PHY context".into());
            o.need(!j.binding, || "BINDING_CONTEXT_CMD ADD of a binding that exists".into());
            j.binding = true;
        }
        3 => {
            o.need(j.binding, || "BINDING_CONTEXT_CMD REMOVE of a binding that does not exist".into());
            o.need(!j.sta, || "BINDING_CONTEXT_CMD REMOVE while the station still exists".into());
            j.binding = false;
        }
        a => o.viol.push(format!("BINDING_CONTEXT_CMD action {a}")),
    }
    vec![0; 4] // status 0
}

/// The join queue's enable and disable. `Some` when the command is for the join queue (the caller handles the aux queue).
pub(super) fn scd(w: &mut World, c: &SeenCmd, o: &mut Out) {
    if !want_len(o, c, 12, "SCD_QUEUE_CFG") {
        return;
    }
    let p = &c.payload;
    let j = &mut w.fw.rt.join;
    if p[4] == 1 {
        // { token 0, sta_id 0, tid 8, queue 5, action ENABLE, aggregate 0, fifo BE (1), window 64, ssn 0 }
        let want = [0u8, STA_ID, 8, QUEUE as u8, 1, 0, 1, 64, 0, 0, 0, 0];
        o.need(p[..] == want, || format!("SCD_QUEUE_CFG is not the link queue enable: {:02x?}", p));
        o.need(w.fw.rt.q_wrptr[QUEUE] == Some(0), || "SCD_QUEUE_CFG before the link queue's write pointer was positioned".into());
        let j = &mut w.fw.rt.join;
        o.need(!j.q_on[QUEUE], || "SCD_QUEUE_CFG enables a queue that is on".into());
        j.q_on[QUEUE] = true;
        j.q_seen[QUEUE] = 0;
    } else {
        let want = [0u8, STA_ID, 8, QUEUE as u8, 0, 0, 0, 0, 0, 0, 0, 0];
        o.need(p[..] == want, || format!("SCD_QUEUE_CFG is not the link queue disable: {:02x?}", p));
        o.need(j.q_on[QUEUE], || "SCD_QUEUE_CFG disables a queue that is off".into());
        o.need(!j.sta, || "SCD_QUEUE_CFG disables the queue while the station exists".into());
        j.q_on[QUEUE] = false;
        w.fw.rt.q_wrptr[QUEUE] = None;
    }
}

pub(super) fn add_sta(w: &mut World, c: &SeenCmd, o: &mut Out) -> Vec<u8> {
    if !want_len(o, c, 48, "ADD_STA v10") {
        return 1u32.to_le_bytes().to_vec();
    }
    let p = &c.payload;
    let Some((bssid, aid)) = w.fw.rt.join.ap.as_ref().map(|a| (a.bssid, a.aid)) else {
        o.viol.push("ADD_STA for the AP with no access point on the air".into());
        return 1u32.to_le_bytes().to_vec();
    };
    let update = p[0] == 1;
    let mut want = [0u8; 48];
    want[0] = update as u8;
    want[2] = 0xff;
    want[3] = 0xff; // tid_disable_tx: no aggregation
    want[8..14].copy_from_slice(&bssid);
    want[24..28].copy_from_slice(&0x3c02_0000u32.to_le_bytes()); // station_flags_msk: FAT_EN | MIMO_EN | RTS_MIMO_PROT
    want[40] = 1 << QUEUE;
    if update {
        want[36..38].copy_from_slice(&aid.to_le_bytes());
    }
    o.need(p[..] == want[..], || format!("ADD_STA is not the AP {} : {:02x?}", if update { "MODIFY" } else { "ADD" }, p));
    let j = &mut w.fw.rt.join;
    if update {
        o.need(j.sta, || "ADD_STA MODIFY of a station that does not exist".into());
        o.need(j.associated, || "ADD_STA MODIFY with an AID before the AP associated us".into());
        j.sta_aid = aid;
    } else {
        o.need(!j.sta, || "ADD_STA ADD of a station that exists".into());
        o.need(j.binding && j.phy, || "ADD_STA before the binding".into());
        o.need(j.q_on[QUEUE], || "ADD_STA names a queue (tfd_queue_msk) that was not enabled first".into());
        j.sta = true;
    }
    1u32.to_le_bytes().to_vec() // ADD_STA_SUCCESS
}

pub(super) fn remove_sta(w: &mut World, c: &SeenCmd, o: &mut Out) {
    if want_len(o, c, 4, "REMOVE_STA") {
        o.need(c.payload[..] == [STA_ID, 0, 0, 0], || format!("REMOVE_STA is not station 0: {:02x?}", c.payload));
    }
    let j = &mut w.fw.rt.join;
    o.need(j.sta, || "REMOVE_STA of a station that does not exist".into());
    o.need(j.keys.iter().all(|k| k.is_none()), || "REMOVE_STA while keys are still installed for it".into());
    j.sta = false;
    j.authenticated = false;
    j.associated = false;
}

pub(super) fn mcast(w: &mut World, c: &SeenCmd, o: &mut Out) {
    if want_len(o, c, 12, "MCAST_FILTER_CMD") {
        let b = w.fw.rt.join.ap.as_ref().map_or([0; 6], |a| a.bssid);
        let mut want = [0u8; 12];
        want[0] = 1;
        want[3] = 1;
        want[4..10].copy_from_slice(&b);
        o.need(c.payload[..] == want, || format!("MCAST_FILTER_CMD is not filter_own + pass_all for the BSSID: {:02x?}", c.payload));
    }
    o.need(w.fw.rt.join.associated, || "MCAST_FILTER_CMD before association".into());
    w.fw.rt.join.mcast = true;
}

pub(super) fn time_event(w: &mut World, c: &SeenCmd, o: &mut Out) -> Vec<u8> {
    if !want_len(o, c, 36, "TIME_EVENT_CMD v2") {
        return vec![0; 16];
    }
    let p = &c.payload;
    let action = le32(p, 4);
    if action == 1 {
        // { id_and_color 0, action ADD, id AGGRESSIVE_ASSOC (0), apply_time 0, max_delay, depends_on 0, interval 1, duration, repeat 1, frags 0, policy 0x0803 }
        o.need(le32(p, 0) == MAC_ID && le32(p, 8) == 0 && le32(p, 12) == 0 && le32(p, 20) == 0 && le32(p, 24) == 1, || format!("TIME_EVENT_CMD ADD fields: {:02x?}", p));
        o.need(p[32] == 1 && p[33] == 0 && le16(p, 34) == 0x0803, || "TIME_EVENT_CMD repeat/frags/policy are not 1 / NONE / START|END|START_IMMEDIATELY".into());
        o.need(le32(p, 28) == 600 && le32(p, 16) == 500, || format!("TIME_EVENT_CMD duration {} / max_delay {} (iwl_mvm_protect_assoc: 600 / 500, mac80211.c:2606,2626)", le32(p, 28), le32(p, 16)));
        let j = &mut w.fw.rt.join;
        o.need(j.sta && j.binding, || "TIME_EVENT_CMD before the station exists".into());
        o.need(!j.te, || "TIME_EVENT_CMD ADD while one is scheduled".into());
        let (resp_status, start_status) = j.ap.as_ref().map_or((1, 1), |a| (a.te_resp_status, a.te_start_status));
        // an event the scheduler could not place is gone in the firmware after its failed START (iwl_mvm_te_clear_data)
        j.te = start_status != 0;
        // the start notification: { timestamp, session_id, unique_id, id_and_color, action START (1), status }
        let mut n = vec![0u8; 24];
        n[8..12].copy_from_slice(&TE_UID.to_le_bytes());
        n[16..20].copy_from_slice(&1u32.to_le_bytes());
        n[20..24].copy_from_slice(&start_status.to_le_bytes());
        w.fw.notif_counter += 1;
        let seq = 0x8000 | w.fw.notif_counter;
        post_cmd(w, 700, 0x2a, seq, n);
        let mut r = vec![0u8; 16];
        r[0..4].copy_from_slice(&resp_status.to_le_bytes());
        r[8..12].copy_from_slice(&TE_UID.to_le_bytes());
        r
    } else {
        o.need(action == 3 && le32(p, 8) == TE_UID && le32(p, 0) == MAC_ID, || format!("TIME_EVENT_CMD is neither the ADD nor a REMOVE of uid {TE_UID:#x}: {:02x?}", p));
        o.need(w.fw.rt.join.te, || "TIME_EVENT_CMD REMOVE of an event that is not scheduled".into());
        w.fw.rt.join.te = false;
        let mut r = vec![0u8; 16];
        r[0..4].copy_from_slice(&1u32.to_le_bytes());
        r[8..12].copy_from_slice(&TE_UID.to_le_bytes());
        r
    }
}

/// The station MAC context after the bring-up ADD: the BSSID, then association, then the reset.
pub(super) fn mac_modify(w: &mut World, c: &SeenCmd, o: &mut Out) {
    if !want_len(o, c, 148, "MAC_CONTEXT_CMD") {
        return;
    }
    let p = &c.payload;
    let node = mac_of(w);
    let Some((bssid, aid)) = w.fw.rt.join.ap.as_ref().map(|a| (a.bssid, a.aid)) else {
        o.viol.push("MAC_CONTEXT_CMD MODIFY with no access point on the air".into());
        return;
    };
    o.need(le32(p, 0) == MAC_ID && le32(p, 4) == 2 && le32(p, 8) == 5 && le32(p, 12) == 0, || format!("MAC_CONTEXT_CMD head is not MAC 0 / MODIFY / BSS_STA / TSF A: {:02x?}", &p[..16]));
    o.need(p[16..22] == node && p[22..24] == [0, 0], || "MAC_CONTEXT_CMD node_addr is not the station".into());
    let reset = p[24..30] == [0; 6];
    let aid_in = le32(p, 136);
    if reset {
        o.need(le32(p, 100) == 0 && le32(p, 116) == 0 && le32(p, 124) == 0 && aid_in == 0 && le32(p, 52) == 0x44, || format!("MAC_CONTEXT_CMD reset is not an unassociated, bssid-less context: {:02x?}", &p[96..148]));
        let j = &mut w.fw.rt.join;
        j.mac_bssid = [0; 6];
        j.mac_assoc = false;
        return;
    }
    o.need(p[24..30] == bssid && p[30..32] == [0, 0], || format!("MAC_CONTEXT_CMD bssid {:02x?} is not the AP's", &p[24..30]));
    o.need(le32(p, 32) == 0xf && le32(p, 36) == 0x15, || format!("MAC_CONTEXT_CMD ack rates cck {:#x} ofdm {:#x}, want 0xf / 0x15", le32(p, 32), le32(p, 36)));
    o.need(le32(p, 40) == 0, || "MAC_CONTEXT_CMD protection flags set for an AP without ERP protection".into());
    o.need(le32(p, 44) == 0 && le32(p, 48) == 0x10, || "MAC_CONTEXT_CMD preamble/slot flags are not long preamble + short slot (cap 0x0401)".into());
    o.need(le32(p, 56) == 0, || "MAC_CONTEXT_CMD qos_flags set for a non-QoS BSS".into());
    // ac[VO, VI, BE, BK, CAB]
    let ac: [[u8; 8]; 5] = [[3, 0, 7, 0, 2, 8, 0xe0, 0x05], [7, 0, 15, 0, 2, 4, 0xc0, 0x0b], [15, 0, 0xff, 3, 3, 2, 0, 0], [15, 0, 0xff, 3, 7, 1, 0, 0], [0; 8]];
    for (i, a) in ac.iter().enumerate() {
        o.need(p[60 + 8 * i..68 + 8 * i] == a[..], || format!("MAC_CONTEXT_CMD ac[{i}] is {:02x?}, want {a:02x?}", &p[60 + 8 * i..68 + 8 * i]));
    }
    o.need(le32(p, 116) == 100 && le32(p, 124) == 200 && le32(p, 128) == 0 && le32(p, 132) == 10, || format!("MAC_CONTEXT_CMD bi/dtim_interval/data_policy/listen {:02x?}", &p[116..136]));
    let j = &mut w.fw.rt.join;
    o.need(j.phy && j.mac_bssid == [0; 6] || j.mac_bssid != [0; 6], || "MAC_CONTEXT_CMD MODIFY before the PHY context".into());
    if aid_in == 0 {
        o.need(le32(p, 100) == 0 && le32(p, 52) == 0x44, || "MAC_CONTEXT_CMD (before association) is_assoc set or beacons filtered".into());
        j.mac_bssid = bssid;
    } else {
        o.need(aid_in == aid as u32, || format!("MAC_CONTEXT_CMD assoc_id {aid_in}, the AP gave {aid}"));
        o.need(le32(p, 100) == 1 && le32(p, 52) == 0x04, || format!("MAC_CONTEXT_CMD (associated) is_assoc {} filter {:#x}", le32(p, 100), le32(p, 52)));
        // the beacon in the scan had TSF 0 and device time 0: the next DTIM is dtim_count beacon intervals on
        let off = 0u64; // dtim_count 0 in the scan's TIM
        o.need(le32(p, 104) as u64 == off && u64::from_le_bytes(p[108..116].try_into().unwrap()) == off && le32(p, 140) == 0, || format!("MAC_CONTEXT_CMD dtim_time/dtim_tsf/arrive {:02x?}", &p[104..116]));
        o.need(j.associated, || "MAC_CONTEXT_CMD marks the MAC associated before the AP associated us".into());
        j.mac_assoc = true;
    }
}

pub(super) fn add_sta_key(w: &mut World, c: &SeenCmd, o: &mut Out) -> Vec<u8> {
    if !want_len(o, c, 64, "ADD_STA_KEY v1") {
        return 1u32.to_le_bytes().to_vec();
    }
    let p = &c.payload;
    let (sta, slot, flags) = (p[0], p[1] as usize, le16(p, 2));
    o.need(sta == STA_ID, || format!("ADD_STA_KEY names station {sta}, the AP is 0"));
    o.need(slot < 16, || format!("ADD_STA_KEY key_offset {slot} is outside the 16 slots"));
    let j = &mut w.fw.rt.join;
    o.need(j.sta, || "ADD_STA_KEY before the station exists".into());
    if slot >= 16 {
        return 1u32.to_le_bytes().to_vec();
    }
    if flags & 0x0800 != 0 {
        // removal: { keyid, WEP_KEY_MAP, NOT_VALID, [MULTICAST] }, no key material
        o.need(flags & 0x0808 == 0x0808 && flags & 7 == 0, || format!("ADD_STA_KEY removal flags {flags:#06x}"));
        o.need(p[4..].iter().all(|&b| b == 0), || "ADD_STA_KEY removal carries key material".into());
        o.need(j.keys[slot].is_some(), || format!("ADD_STA_KEY removes slot {slot}, which holds no key"));
        j.keys[slot] = None;
        return 1u32.to_le_bytes().to_vec();
    }
    o.need(flags & 0x00f8 == 0x0008 && flags & 7 == 2, || format!("ADD_STA_KEY flags {flags:#06x} are not CCM | WEP_KEY_MAP | keyid | [MULTICAST]"));
    o.need(j.keys[slot].is_none(), || format!("ADD_STA_KEY installs into slot {slot}, which is in use"));
    let mcast = flags & 0x4000 != 0;
    let kid = ((flags >> 8) & 3) as u8;
    let key = &p[4..20];
    o.need(p[20..36].iter().all(|&b| b == 0), || "ADD_STA_KEY key[16..32] is not zero for a 128-bit key".into());
    if let Some(a) = j.auth.as_ref() {
        if mcast {
            let want = j.gtk.map(|(id, k)| (id, k.to_vec()));
            o.need(want == Some((kid, key.to_vec())), || format!("ADD_STA_KEY group key id {kid} is not the GTK the AP sent ({want:02x?})"));
            // rx_secur_seq_cnt for the Key RSC the AP sent (9): hole format, MSB-first PN bytes 00 00 | 00 00 | 00 00 00 09
            o.need(p[36..44] == [0, 0, 0, 0, 0, 0, 0, 9], || format!("ADD_STA_KEY GTK rx_secur_seq_cnt {:02x?}", &p[36..44]));
        } else {
            let want = a.ptk.as_ref().map(|k| k.tk.to_vec());
            o.need(want.as_deref() == Some(key), || "ADD_STA_KEY pairwise key is not the PTK's TK".into());
            o.need(kid == 0, || "ADD_STA_KEY pairwise key id is not 0".into());
            o.need(p[36..44] == [0; 8], || format!("ADD_STA_KEY PTK rx_secur_seq_cnt {:02x?}: a GTK rides in message 3, so the pairwise RSC is 0", &p[36..44]));
        }
    } else {
        o.viol.push("ADD_STA_KEY on an open network".into());
    }
    o.need(p[44..].iter().all(|&b| b == 0), || "ADD_STA_KEY v1 tkip fields are not zero".into());
    j.keys[slot] = Some((key.to_vec(), mcast));
    1u32.to_le_bytes().to_vec()
}

// ------------------------------------------------------------------------------------------ the air

fn le64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// The host moved queue `q`'s write pointer to `wr`: fetch every new TFD.
pub fn data_doorbell(w: &mut World, q: usize, wr: u16) {
    let base = (w.fh.get(&super::regs::cbbc_queue(q as u32)).copied().unwrap_or(0) as u64) << 8;
    let bc_base = (w.prph.get(&(super::regs::P_SCD_SRAM_BASE + 8)).copied().unwrap_or(0) as u64) << 10;
    let mut i = w.fw.rt.join.q_seen[q];
    while i != wr {
        let mut tfd = [0u8; 128];
        if !w.dma_read(base + i as u64 * 128, &mut tfd) {
            w.violation(format!("data TFD {i} of queue {q} at {:#x}: not host memory", base + i as u64 * 128));
            return;
        }
        let n = (tfd[3] & 0x1f) as usize;
        let mut tbs: Vec<(u64, usize)> = vec![];
        let mut bytes: Vec<Vec<u8>> = vec![];
        for k in 0..n {
            let o = 4 + 6 * k;
            let lo = u32::from_le_bytes([tfd[o], tfd[o + 1], tfd[o + 2], tfd[o + 3]]) as u64;
            let hl = le16(&tfd, o + 4);
            let addr = lo | ((hl & 0xf) as u64) << 32;
            let len = (hl >> 4) as usize;
            let mut buf = vec![0u8; len];
            if !w.dma_read(addr, &mut buf) {
                w.violation(format!("TB {k} of data TFD {i}: {len} bytes at {addr:#x} is not host memory"));
                return;
            }
            tbs.push((addr, len));
            bytes.push(buf);
        }
        let bc = {
            let mut e = [0u8; 2];
            let ok = w.dma_read(bc_base + ((q * 320 + i as usize) * 2) as u64, &mut e);
            let mut d = [0u8; 2];
            let okd = i < 64 && w.dma_read(bc_base + ((q * 320 + 256 + i as usize) * 2) as u64, &mut d);
            if !ok || (i < 64 && (!okd || d != e)) {
                w.violation(format!("byte-count entry of queue {q} slot {i} missing or its duplicate differs"));
            }
            u16::from_le_bytes(e)
        };
        on_tx(w, q, i, &tbs, &bytes, bc);
        i = (i + 1) & 255;
        w.fw.rt.join.q_seen[q] = i;
    }
}

fn on_tx(w: &mut World, q: usize, idx: u16, tbs: &[(u64, usize)], bytes: &[Vec<u8>], bc: u16) {
    let mut v: Vec<String> = vec![];
    if tbs.len() < 2 || tbs[0].1 != 20 || tbs[1].1 != 64 {
        w.violation(format!("data TFD {idx}: transfer buffers {:?}, want TB0 20 bytes, TB1 64 bytes (command rest + 24-byte header), TB2 payload", tbs.iter().map(|t| t.1).collect::<Vec<_>>()));
        return;
    }
    let all: Vec<u8> = bytes.iter().flatten().copied().collect();
    let (cmdhdr, params, rest) = (&all[..4], &all[4..60], &all[60..]);
    let (hdr, body) = rest.split_at(24);
    let seq = (q as u16) << 8 | idx;
    if cmdhdr != [0x1c, 0, seq as u8, (seq >> 8) as u8] {
        v.push(format!("TX_CMD header {cmdhdr:02x?}, want TX_CMD / group 0 / sequence {seq:#06x}"));
    }
    let fc1 = hdr[1];
    let protected = fc1 & 0x40 != 0;
    let sec_ctl = params[17];
    let flags = le32(params, 4);
    let rate = le32(params, 12);
    let ap = w.fw.rt.join.ap.as_ref();
    let (bssid, five) = ap.map_or(([0; 6], false), |a| (a.bssid, a.five()));
    let is_mgmt = hdr[0] & 0x0c == 0;
    let unicast = hdr[4] & 1 == 0 && hdr[16] & 1 == 0;
    let da_group = hdr[16] & 1 != 0;
    // ---- the TX command ----
    if le16(params, 0) as usize != 24 + body.len() {
        v.push(format!("TX_CMD len {} is not header + body = {}", le16(params, 0), 24 + body.len()));
    }
    let mh = if protected { 16u16 } else { 12 };
    if le16(params, 2) != mh << 8 {
        v.push(format!("TX_CMD offload_assist {:#06x}, want mac header {} words << 8", le16(params, 2), mh));
    }
    let bt = if (is_mgmt && !five) || (da_group && !five) { 3 } else { 0 };
    let want_flags = (if unicast || is_mgmt { 8 } else { 0 }) | 0x2000 | (bt << 11);
    if flags != want_flags {
        v.push(format!("TX_CMD tx_flags {flags:#x}, want {want_flags:#x} (ACK for unicast, SEQ_CTL, BT priority)"));
    }
    let want_rate = if is_mgmt { if five { 0x400d } else { 0x420a } } else { 0x4009 };
    if rate != want_rate {
        v.push(format!("TX_CMD rate_n_flags {rate:#x}, want {want_rate:#x}"));
    }
    if params[16] != STA_ID {
        v.push(format!("TX_CMD sta_id {}", params[16]));
    }
    if (sec_ctl == 2) != protected || (sec_ctl != 0 && sec_ctl != 2) {
        v.push(format!("TX_CMD sec_ctl {sec_ctl:#x} for a frame with the Protected bit {protected}"));
    }
    let mut key = [0u8; 16];
    key.copy_from_slice(&params[20..36]);
    if protected {
        let want = w.fw.rt.join.auth.as_ref().and_then(|a| a.ptk.as_ref().map(|k| k.tk));
        if want != Some(key) {
            v.push("TX_CMD key is not the PTK's temporal key".into());
        }
    } else if key != [0; 16] {
        v.push("TX_CMD carries key bytes for an unprotected frame".into());
    }
    if le32(params, 40) != 0xffff_ffff {
        v.push("TX_CMD life_time is not infinite".into());
    }
    let scratch = tbs[0].0 + 12;
    if le32(params, 44) as u64 | (params[48] as u64) << 32 != scratch {
        v.push(format!("TX_CMD dram scratch pointer is not TB0 + 12 ({scratch:#x})"));
    }
    if params[49] != 60 || params[50] != 15 {
        v.push(format!("TX_CMD retry limits {}/{}", params[49], params[50]));
    }
    let eapol = !is_mgmt && body.len() >= if protected { 16 } else { 8 } && {
        let o = if protected { 8 } else { 0 };
        body[o + 6..o + 8] == [0x88, 0x8e]
    };
    let want_tid = if is_mgmt { 8 } else { 0 };
    if params[51] != want_tid {
        v.push(format!("TX_CMD tid_tspec {}", params[51]));
    }
    let subtype = hdr[0] & 0xf0;
    let want_pm = if is_mgmt { if subtype == 0x00 { 3 } else { 2 } } else if eapol { 2 } else { 0 };
    if le16(params, 52) != want_pm {
        v.push(format!("TX_CMD pm_frame_timeout {}, want {want_pm}", le16(params, 52)));
    }
    // ---- the byte-count entry: (frame + CRC + delimiter + MIC) in dwords, station in bits 12..15 ----
    let want_bc = ((24 + body.len() + 8 + if protected { 8 } else { 0 }).div_ceil(4)) as u16;
    if bc != want_bc {
        v.push(format!("byte-count entry {bc:#06x}, want {want_bc:#06x}"));
    }
    w.fw.rt.join.bc_tbl_checked += 1;
    // ---- the 802.11 header ----
    if hdr[2..4] != [0, 0] || hdr[22..24] != [0, 0] {
        v.push("802.11 duration or sequence control is not zero (the firmware assigns them)".into());
    }
    if hdr[4..10] != bssid || hdr[10..16] != mac_of(w) {
        v.push(format!("802.11 addresses a1 {:02x?} a2 {:02x?}", &hdr[4..10], &hdr[10..16]));
    }
    for m in v {
        w.violation(format!("TX frame on queue {q} slot {idx}: {m}"));
    }
    w.fw.rt.join.frames.push(HostFrame { q, idx, hdr: hdr.to_vec(), body: body.to_vec(), rate, sec_ctl, key });

    // ---- the TX status the firmware reports ----
    let nframes = w.fw.rt.join.frames.len();
    let status = w.fw.rt.join.ap.as_ref().map_or(1, |a| if a.fail_tx_from.is_some_and(|n| nframes > n) { 0x83 } else { a.tx_status });
    let mut r = vec![0u8; 44];
    r[0] = 1;
    r[4..8].copy_from_slice(&rate.to_le_bytes());
    r[33] = 0; // ra 0, tid 0
    r[36..38].copy_from_slice(&status.to_le_bytes());
    r[40..44].copy_from_slice(&(((idx + 1) & 255) as u32 | 0xf000).to_le_bytes()); // SCD ssn: the next slot (upper bits are other fields)
    post_cmd(w, 150, 0x1c, seq, r);

    // ---- what the access point does with it ----
    if is_mgmt {
        on_mgmt(w, hdr, body);
    } else {
        on_data(w, hdr, body, protected);
    }
}

fn on_mgmt(w: &mut World, hdr: &[u8], body: &[u8]) {
    let Some((bssid, ssid, aid, pmk, ignore_auth, ignore_assoc, auth_status, assoc_status, no_m1)) =
        w.fw.rt.join.ap.as_ref().map(|a| (a.bssid, a.ssid.clone(), a.aid, a.pmk, a.ignore_auth, a.ignore_assoc, a.auth_status, a.assoc_status, a.no_m1))
    else {
        return;
    };
    let sta = mac_of(w);
    let ap_hdr = |subtype: u8| {
        let mut h = vec![0u8; 24];
        h[0] = subtype;
        h[4..10].copy_from_slice(&sta);
        h[10..16].copy_from_slice(&bssid);
        h[16..22].copy_from_slice(&bssid);
        h
    };
    match hdr[0] & 0xf0 {
        0xb0 => {
            // authentication request
            w.fw.rt.join.auth_seen += 1;
            if body != [0, 0, 1, 0, 0, 0] {
                w.violation(format!("authentication body {body:02x?} is not open system / transaction 1 / success"));
            }
            if hdr[16..22] != bssid {
                w.violation("authentication addr3 is not the BSSID".into());
            }
            let j = &w.fw.rt.join;
            if !(j.phy && j.binding && j.sta && j.te && j.mac_bssid == bssid) {
                w.violation("authentication sent before the PHY, MAC, binding, station and time event exist".into());
            }
            if w.fw.rt.join.auth_seen <= ignore_auth {
                return;
            }
            let mut f = ap_hdr(0xb0);
            f.extend_from_slice(&[0, 0, 2, 0]);
            f.extend_from_slice(&auth_status.to_le_bytes());
            w.fw.rt.join.authenticated = auth_status == 0;
            post_rx(w, 500, &f, 3, 12);
        }
        0x00 => {
            w.fw.rt.join.assoc_seen += 1;
            if !w.fw.rt.join.authenticated {
                w.violation("association request before authentication succeeded".into());
            }
            let mut want_cap = 0x0001u16 | 0x0400; // ESS | short slot (the beacon's)
            if pmk.is_some() {
                want_cap |= 0x0010;
            }
            let cap = le16(body, 0);
            if cap != want_cap {
                w.violation(format!("association capability {cap:#06x}, want {want_cap:#06x}"));
            }
            if le16(body, 2) != 10 {
                w.violation("association listen interval is not 10".into());
            }
            let ies = &body[4..];
            let mut want = ie(0, &ssid);
            if w.fw.rt.join.ap.as_ref().is_some_and(|a| a.five()) {
                want.extend(ie(1, &[12, 18, 24, 36, 48, 72, 96, 108])); // OFDM only on 5 GHz
            } else {
                want.extend(ie(1, &[2, 4, 11, 22, 12, 18, 24, 36]));
                want.extend(ie(50, &[48, 72, 96, 108]));
            }
            if pmk.is_some() {
                want.extend(rsne_for(Akm::Psk, Cipher::Ccmp));
            }
            if ies != &want[..] {
                w.violation(format!("association elements {ies:02x?}, want {want:02x?}"));
            }
            if w.fw.rt.join.assoc_seen <= ignore_assoc {
                return;
            }
            let mut f = ap_hdr(0x10);
            f.extend_from_slice(&0x0401u16.to_le_bytes());
            f.extend_from_slice(&assoc_status.to_le_bytes());
            f.extend_from_slice(&(aid | 0xc000).to_le_bytes());
            f.extend(ie(1, &[0x82, 0x84, 0x8b, 0x96, 0x0c, 0x12, 0x18, 0x24]));
            w.fw.rt.join.associated = assoc_status == 0;
            post_rx(w, 500, &f, 3, 12);
            if assoc_status == 0 && pmk.is_some() && !no_m1 {
                let m1 = {
                    let a = w.fw.rt.join.auth.as_mut().expect("the test gave the AP a PMK and an authenticator");
                    a.m1()
                };
                let f = eapol_frame(&sta, &bssid, &m1, false);
                post_rx(w, 2_500, &f, 3, 12);
            }
        }
        0xc0 | 0xa0 => {
            let r = le16(body, 0);
            w.fw.rt.join.deauths.push(r);
            w.fw.rt.join.authenticated = false;
            w.fw.rt.join.associated = false;
        }
        s => w.violation(format!("management frame subtype {s:#x} from the host")),
    }
}

/// A From-DS data frame: the header, an optional CCMP header, LLC/SNAP and the payload.
fn from_ds(da: &[u8; 6], bssid: &[u8; 6], sa: &[u8; 6], ccmp: Option<[u8; 8]>, ethertype: u16, payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 24];
    f[0] = 0x08;
    f[1] = 0x02 | if ccmp.is_some() { 0x40 } else { 0 };
    f[4..10].copy_from_slice(da);
    f[10..16].copy_from_slice(bssid);
    f[16..22].copy_from_slice(sa);
    if let Some(c) = ccmp {
        f.extend_from_slice(&c);
    }
    f.extend_from_slice(&[0xaa, 0xaa, 0x03, 0, 0, 0]);
    f.extend_from_slice(&ethertype.to_be_bytes());
    f.extend_from_slice(payload);
    f
}

/// An unprotected EAPOL frame from the access point.
fn eapol_frame(sta: &[u8; 6], bssid: &[u8; 6], pdu: &[u8], _protected: bool) -> Vec<u8> {
    from_ds(sta, bssid, bssid, None, 0x888e, pdu)
}

fn on_data(w: &mut World, hdr: &[u8], body: &[u8], protected: bool) {
    let (bssid, pmk) = match w.fw.rt.join.ap.as_ref() {
        Some(a) => (a.bssid, a.pmk),
        None => return,
    };
    if hdr[0] != 0x08 || hdr[1] & 3 != 1 {
        w.violation(format!("data frame control {:02x?} is not To-DS data", &hdr[..2]));
        return;
    }
    let (pn, llc) = if protected {
        let c = &body[..8];
        if c[2] != 0 || c[3] != 0x20 {
            w.violation(format!("CCMP header {c:02x?}: reserved or ExtIV/key id bits wrong"));
        }
        let pn = (c[0] as u64) | (c[1] as u64) << 8 | (c[4] as u64) << 16 | (c[5] as u64) << 24 | (c[6] as u64) << 32 | (c[7] as u64) << 40;
        let last = w.fw.rt.join.last_pn;
        if pn != last + 1 {
            w.violation(format!("CCMP packet number {pn} does not follow {last}"));
        }
        w.fw.rt.join.last_pn = pn;
        (pn, &body[8..])
    } else {
        (0, body)
    };
    if llc[..6] != [0xaa, 0xaa, 0x03, 0, 0, 0] {
        w.violation("data frame without LLC/SNAP".into());
        return;
    }
    let et = u16::from_be_bytes([llc[6], llc[7]]);
    let payload = llc[8..].to_vec();
    if et != 0x888e {
        w.fw.rt.join.data.push((pn, et, payload, protected));
        return;
    }
    // EAPOL: the access point's authenticator answers
    let sta = mac_of(w);
    let Some(a) = w.fw.rt.join.auth.as_mut() else {
        w.violation("EAPOL from the host on a network without an authenticator".into());
        return;
    };
    let info = u16::from_be_bytes([payload[5], payload[6]]);
    let pairwise = info & 0x0008 != 0;
    let own_rsne = rsne_for(Akm::Psk, Cipher::Ccmp);
    let _ = pmk;
    if pairwise && info & 0x0040 == 0 && info & 0x0200 == 0 {
        // message 2: PAIRWISE | MIC, no INSTALL, no SECURE
        match a.check_m2(&payload, &own_rsne) {
            Ok(()) => {
                w.fw.rt.join.m2_seen += 1;
                let (m3, gtk) = {
                    let a = w.fw.rt.join.auth.as_mut().unwrap();
                    (a.m3(), (a.gtk_id, a.gtk))
                };
                w.fw.rt.join.gtk = Some(gtk);
                let f = eapol_frame(&sta, &bssid, &m3, false);
                post_rx(w, 1_500, &f, 3, 12);
            }
            Err(e) => w.fw.rt.join.log.push(format!("M2 refused by the authenticator: {e}")),
        }
    } else if pairwise && info & 0x0200 != 0 {
        // message 4: PAIRWISE | MIC | SECURE
        let want = w.fw.rt.join.auth.as_ref().map_or(0, |a| a.replay);
        match w.fw.rt.join.auth.as_ref().unwrap().check_m4(&payload, want) {
            Ok(()) => w.fw.rt.join.m4_seen = true,
            Err(e) => w.violation(format!("message 4: {e}")),
        }
        let keyed = w.fw.rt.join.keys.iter().flatten().any(|k| !k.1);
        if protected && !keyed {
            w.violation("message 4 was sent protected before any pairwise key was installed: the PTK goes in only after it has left".into());
        }
        if !protected && keyed {
            w.violation("a message 4 sent after the PTK was installed went out unprotected".into());
        }
    } else if !pairwise {
        let want = w.fw.rt.join.auth.as_ref().map_or(0, |a| a.replay);
        match w.fw.rt.join.auth.as_ref().unwrap().check_g2(&payload, want) {
            Ok(()) => w.fw.rt.join.g2_seen += 1,
            Err(e) => w.violation(format!("group message 2: {e}")),
        }
        if !protected {
            w.violation("group message 2 was sent unprotected after the PTK was installed".into());
        }
    }
}

// ---------------------------------------------------------------------------------- test-side controls

/// Create the authenticator for the access point on the air, from its PMK.
pub fn start_authenticator(w: &mut World) {
    let (pmk, bssid) = {
        let ap = w.fw.rt.join.ap.as_ref().expect("an access point");
        (ap.pmk.expect("a PSK access point"), ap.bssid)
    };
    let mut a = Auth::new(Akm::Psk);
    a.aa = bssid;
    a.spa = w.fw.rt.mac;
    a.pmk = wpa_crypto_core::Pmk::from_bytes(pmk);
    a.kind = KdfKind::PrfSha1;
    a.ap_rsne = rsne_for(Akm::Psk, Cipher::Ccmp);
    // the group counter starts at the Key RSC the authenticator announced (9)
    w.fw.rt.join.ap_gpn = 9;
    w.fw.rt.join.auth = Some(a);
}

/// The access point sends the station one From-DS data frame. `protected` frames carry a CCMP header and arrive with the
/// descriptor flags of a frame the firmware decrypted; `group` frames use the group counter and key id 1.
pub fn deliver(w: &mut World, delay_us: u64, da: [u8; 6], sa: [u8; 6], ethertype: u16, payload: &[u8], protected: bool, group_key: Option<u8>) {
    let bssid = w.fw.rt.join.ap.as_ref().map(|a| a.bssid).expect("an access point");
    let j = &mut w.fw.rt.join;
    let ccmp = if protected {
        let (pn, kid) = match group_key {
            Some(id) => {
                j.ap_gpn += 1;
                (j.ap_gpn, id)
            }
            None => {
                j.ap_pn += 1;
                (j.ap_pn, 0)
            }
        };
        Some([pn as u8, (pn >> 8) as u8, 0, 0x20 | (kid << 6), (pn >> 16) as u8, (pn >> 24) as u8, (pn >> 32) as u8, (pn >> 40) as u8])
    } else {
        None
    };
    let f = from_ds(&da, &bssid, &sa, ccmp, ethertype, payload);
    // status: CRC OK | OVERRUN OK, and for a decrypted CCMP frame SEC_CCM | MIC_OK | DECRYPTED
    let status = 3 | if protected { (2 << 8) | 0x40 | 0x800 } else { 0 };
    post_rx(w, delay_us, &f, status, 12);
}

/// The access point starts a group-key handshake with a new GTK (always a protected frame).
pub fn group_rekey(w: &mut World, delay_us: u64, key: [u8; 16], id: u8) {
    let (bssid, sta) = (w.fw.rt.join.ap.as_ref().map(|a| a.bssid).expect("ap"), w.fw.rt.mac);
    let g1 = w.fw.rt.join.auth.as_mut().expect("authenticator").g1(&key, id);
    w.fw.rt.join.gtk = Some((id, key));
    deliver(w, delay_us, sta, bssid, 0x888e, &g1, true, None);
}

/// Silence the unused-import lint for items the sim shares with its siblings.
#[allow(dead_code)]
fn _uses() {
    let _ = fm::FwState::Off;
    let _ = le64(&[0; 8], 0);
}

/// The access point deauthenticates (or disassociates) the station.
pub fn ap_drop(w: &mut World, delay_us: u64, reason: u16, disassoc: bool) {
    let (bssid, sta) = (w.fw.rt.join.ap.as_ref().map(|a| a.bssid).expect("ap"), w.fw.rt.mac);
    let mut h = vec![0u8; 24];
    h[0] = if disassoc { 0xa0 } else { 0xc0 };
    h[4..10].copy_from_slice(&sta);
    h[10..16].copy_from_slice(&bssid);
    h[16..22].copy_from_slice(&bssid);
    h.extend_from_slice(&reason.to_le_bytes());
    post_rx(w, delay_us, &h, 3, 12);
}

/// The access point retransmits message 3 (a fresh replay counter), as hostapd does when it did not hear message 4.
/// It is protected: the PTK is installed by now.
pub fn resend_m3(w: &mut World, delay_us: u64) {
    let (bssid, sta) = (w.fw.rt.join.ap.as_ref().map(|a| a.bssid).expect("ap"), w.fw.rt.mac);
    let m3 = w.fw.rt.join.auth.as_mut().expect("authenticator").m3();
    deliver(w, delay_us, sta, bssid, 0x888e, &m3, true, None);
}
