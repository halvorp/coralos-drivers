// SPDX-License-Identifier: GPL-2.0-only
//! The REGULAR (RT) image of the simulated card, and it is STRICT: every command is checked against the Linux struct
//! it must be and against the order `iwl_mvm_up` sends it in, and an out-of-order or malformed command is a recorded
//! violation (the tests assert there are none). What it models, from `mvm/fw.c:1542-1758`:
//!   paging command (the CPU2 paging image must be where the command says, byte for byte) before anything else;
//!   TX_ANT, PHY_DB (the sections the INIT image reported, in iwl_send_phy_db_data's order), PHY_CONFIGURATION, BT_CONFIG,
//!   SOC_CONFIGURATION, DQA_ENABLE; the aux queue (write pointer, then SCD_QUEUE_CFG) before ADD_STA of the aux station;
//!   TEMP thresholds, device power, MCC_UPDATE ("scan while LAR regdomain is not set" is refused), SCAN_CFG_CMD v1, and
//!   a SCAN_REQ_UMAC v8 checked field by field. A scan produces RX_MPDU notifications for the access points the test
//!   placed on the channels it asked for, plus junk the driver must skip, then SCAN_COMPLETE_UMAC.
//! The hardware side is written from the Linux structures as LITERALS, never through the driver's or the core's constants.
use super::fwmodel::{self as fm, FwImageModel, PAGING_SEP};
use super::{SeenCmd, World};

/// `iwl_ext_nvm_channels` (iwl-nvm-parse.c:`iwl_ext_nvm_channels`): the order the MCC response's flags follow.
pub const EXT_CHANNELS: [u8; 51] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 36, 40, 44, 48, 52, 56, 60, 64, 68, 72, 76, 80, 84, 88, 92, 96, 100, 104, 108, 112, 116, 120, 124, 128, 132, 136, 140, 144, 149, 153, 157, 161, 165, 169, 173, 177, 181,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApSec {
    Open,
    Wpa2Psk,
    /// RSN with SAE only.
    Wpa2Sae,
    /// RSN, PSK, TKIP-only pairwise.
    Wpa2Tkip,
    /// Privacy bit, no RSN.
    Wep,
    /// Privacy bit and the WPA vendor element.
    Wpa1,
}

#[derive(Clone, Debug)]
pub struct SimAp {
    pub ssid: Vec<u8>,
    pub bssid: [u8; 6],
    pub channel: u8,
    pub rssi: i16,
    pub sec: ApSec,
    /// The beacon carries a zero-length SSID; the probe response (also sent) carries the name.
    pub hidden: bool,
    /// Put the DS Parameter Set element in the frame (the 5 GHz beacons Linux sees have none).
    pub ds: bool,
}

impl SimAp {
    pub fn new(ssid: &str, last: u8, channel: u8, rssi: i16, sec: ApSec) -> SimAp {
        SimAp { ssid: ssid.as_bytes().to_vec(), bssid: [0x02, 0xaa, 0xbb, 0xcc, 0xdd, last], channel, rssi, sec, hidden: false, ds: channel <= 14 }
    }
}

pub struct RtFw {
    // ---- the world the test builds ----
    pub aps: Vec<SimAp>,
    /// The country the MCC response names.
    pub regdom: [u8; 2],
    /// Channels the regulatory domain disables (their MCC flags lose VALID).
    pub regdom_off: Vec<u8>,
    pub nvm_flags: Vec<u16>,
    pub n_scan: usize,
    pub mac: [u8; 6],
    pub expect_bt_modules: u32,
    // ---- fault injection ----
    pub never_complete: bool,
    pub complete_status: u8,
    pub add_sta_status: u32,
    pub junk: bool,
    pub chatter: bool,
    // ---- what the INIT image reported (filled by fwmodel) ----
    pub init_phydb: Vec<(u16, Vec<u8>)>,
    // ---- what the host sent, this run ----
    paging_expect: Option<(Vec<u8>, Vec<u8>)>,
    pub paging_done: bool,
    pub phydb_seen: Vec<(u16, Vec<u8>)>,
    pub tx_ant_done: bool,
    pub phydb_done: bool,
    pub phy_cfg_done: bool,
    pub bt_done: bool,
    pub soc_done: bool,
    pub dqa_done: bool,
    pub q_wrptr: [Option<u16>; 32],
    pub scd_done: bool,
    pub aux_sta: Option<u8>,
    pub temp_done: bool,
    pub power_done: bool,
    pub mcc_done: bool,
    pub scan_cfg: Option<Vec<u8>>,
    pub running_uid: Option<u32>,
    pub scans: Vec<Vec<u8>>,
    pub aborts: Vec<u32>,
    pub cmd_log: Vec<String>,
}

impl RtFw {
    pub fn new() -> RtFw {
        RtFw {
            aps: vec![],
            regdom: *b"US",
            regdom_off: vec![],
            nvm_flags: super::nvmdata::NvmSpec::atlas().channel_flags,
            n_scan: 52,
            mac: [0x12, 0x22, 0x33, 0x44, 0x55, 0x66],
            expect_bt_modules: 0x15,
            never_complete: false,
            complete_status: 1,
            add_sta_status: 1,
            junk: true,
            chatter: true,
            init_phydb: vec![],
            paging_expect: None,
            paging_done: false,
            phydb_seen: vec![],
            tx_ant_done: false,
            phydb_done: false,
            phy_cfg_done: false,
            bt_done: false,
            soc_done: false,
            dqa_done: false,
            q_wrptr: [None; 32],
            scd_done: false,
            aux_sta: None,
            temp_done: false,
            power_done: false,
            mcc_done: false,
            scan_cfg: None,
            running_uid: None,
            scans: vec![],
            aborts: vec![],
            cmd_log: vec![],
        }
    }

    /// The firmware restarted (a card reset): what the host told it is forgotten, the world stays.
    pub fn reset_run(&mut self) {
        self.paging_expect = None;
        self.paging_done = false;
        self.phydb_seen.clear();
        self.tx_ant_done = false;
        self.phydb_done = false;
        self.phy_cfg_done = false;
        self.bt_done = false;
        self.soc_done = false;
        self.dqa_done = false;
        self.q_wrptr = [None; 32];
        self.scd_done = false;
        self.aux_sta = None;
        self.temp_done = false;
        self.power_done = false;
        self.mcc_done = false;
        self.scan_cfg = None;
        self.running_uid = None;
    }

    /// The RT image started: remember where the paging image must end up. The layout is
    /// CPU1 .. sep .. CPU2 .. PAGING_SEP, CSS, paging image (fw/paging.c:118-127).
    pub fn on_boot(&mut self, model: Option<&FwImageModel>) {
        self.reset_run();
        self.paging_expect = model.and_then(|m| {
            let i = m.secs.iter().position(|s| s.offset == PAGING_SEP)?;
            Some((m.secs.get(i + 1)?.data.clone(), m.secs.get(i + 2)?.data.clone()))
        });
    }

    /// The channels the regulatory domain allows, in NVM order. // iwl_parse_nvm_mcc_info
    pub fn allowed(&self) -> Vec<u8> {
        EXT_CHANNELS.iter().enumerate().filter(|(i, c)| self.mcc_flags(*i, **c) & 1 != 0).map(|(_, c)| *c).collect()
    }

    fn mcc_flags(&self, idx: usize, ch: u8) -> u16 {
        let f = self.nvm_flags.get(idx).copied().unwrap_or(0);
        if self.regdom_off.contains(&ch) {
            f & !1
        } else {
            f
        }
    }
}

impl Default for RtFw {
    fn default() -> Self {
        Self::new()
    }
}

/// The host moved a data queue's write pointer (HBUS_TARG_WRPTR with queue > 0).
pub fn queue_wrptr(w: &mut World, q: u8, idx: u16) {
    if w.fw.state != fm::FwState::Regular {
        w.violation(format!("write pointer for queue {q} while the RT image is not running"));
        return;
    }
    if q >= 31 {
        w.violation(format!("write pointer for queue {q}: the 9000 family has 31 queues"));
        return;
    }
    if w.fw.rt.scd_done && w.fw.rt.q_wrptr[q as usize] != Some(idx) {
        w.violation(format!("queue {q} write pointer moved to {idx} after it was enabled: nothing was queued"));
    }
    w.fw.rt.q_wrptr[q as usize] = Some(idx);
}

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

struct Out {
    viol: Vec<String>,
    posts: Vec<(u64, u8, u8, u16, Vec<u8>)>,
    /// The scan whose completion frees its id when it is posted.
    complete_uid: Option<u32>,
}

impl Out {
    fn need(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        if !ok {
            self.viol.push(msg());
        }
    }
}

fn want_len(o: &mut Out, c: &SeenCmd, n: usize, what: &str) -> bool {
    o.need(c.payload.len() == n, || format!("{what} is {} bytes, not the {n} of its Linux struct", c.payload.len()));
    c.payload.len() == n
}

/// One host command to the RT image. Replies are scheduled here; violations are recorded.
pub fn handle(w: &mut World, c: &SeenCmd) {
    let mut o = Out { viol: vec![], posts: vec![], complete_uid: None };
    let mut reply: Vec<u8> = vec![];
    let key = (c.group, c.cmd);
    w.fw.rt.cmd_log.push(format!("{}.{:#x}", c.group, c.cmd));
    // Nothing but the paging command may come first when the image pages. // fw/paging.c (iwl_init_paging), mvm/fw.c:1554
    if w.fw.rt.paging_expect.is_some() && !w.fw.rt.paging_done && key != (1, 0x4f) {
        o.viol.push(format!("command {}.{:#x} before FW_PAGING_BLOCK_CMD: the firmware has no paging image", c.group, c.cmd));
    }
    match key {
        (1, 0x4f) => paging(w, c, &mut o),
        (0, 0x98) => {
            if want_len(&mut o, c, 4, "TX_ANT_CONFIGURATION_CMD") {
                o.need(le32(&c.payload, 0) == 3, || format!("TX_ANT valid mask {:#x}, the NVM and the firmware both say 3", le32(&c.payload, 0)));
            }
            o.need(!w.fw.rt.tx_ant_done, || "TX_ANT_CONFIGURATION_CMD sent twice".into());
            w.fw.rt.tx_ant_done = true;
        }
        (0, 0x6c) => {
            // struct iwl_phy_db_cmd { le16 type; le16 length; u8 data[]; }
            if c.payload.len() < 4 || le16(&c.payload, 2) as usize != c.payload.len() - 4 {
                o.viol.push(format!("PHY_DB_CMD length field does not match its data ({} bytes)", c.payload.len()));
            } else {
                w.fw.rt.phydb_seen.push((le16(&c.payload, 0), c.payload[4..].to_vec()));
            }
            o.need(w.fw.rt.tx_ant_done, || "PHY_DB_CMD before TX_ANT_CONFIGURATION_CMD".into());
        }
        (0, 0x6a) => {
            if want_len(&mut o, c, 12, "PHY_CONFIGURATION_CMD") {
                o.need(le32(&c.payload, 0) & 0x00ff_0000 != 0, || "PHY_CONFIGURATION_CMD phy_cfg names no TX chain".into());
            }
            o.need(w.fw.rt.tx_ant_done, || "PHY_CONFIGURATION_CMD before TX_ANT_CONFIGURATION_CMD".into());
            // iwl_send_phy_db_data came first, with exactly the sections the INIT image reported, in its order.
            let expect: Vec<(u16, Vec<u8>)> = w.fw.rt.init_phydb.iter().filter(|(_, d)| !d.is_empty()).cloned().collect();
            let seen = w.fw.rt.phydb_seen.clone();
            o.need(seen == expect, || format!("PHY_CONFIGURATION_CMD before the PHY database was replayed: sent {:?}, INIT reported {:?}", seen.iter().map(|s| (s.0, s.1.len())).collect::<Vec<_>>(), expect.iter().map(|s| (s.0, s.1.len())).collect::<Vec<_>>()));
            w.fw.rt.phydb_done = true;
            w.fw.rt.phy_cfg_done = true;
        }
        (0, 0x9b) => {
            if want_len(&mut o, c, 8, "BT_CONFIG") {
                o.need(le32(&c.payload, 0) == 1, || format!("BT_CONFIG mode {} is not BT_COEX_NW", le32(&c.payload, 0)));
                let m = w.fw.rt.expect_bt_modules;
                o.need(le32(&c.payload, 4) == m, || format!("BT_CONFIG enabled_modules {:#x}, expected {m:#x}", le32(&c.payload, 4)));
            }
            o.need(w.fw.rt.phy_cfg_done, || "BT_CONFIG before PHY_CONFIGURATION_CMD".into());
            w.fw.rt.bt_done = true;
        }
        (2, 1) => {
            if want_len(&mut o, c, 8, "SOC_CONFIGURATION_CMD") {
                o.need(le32(&c.payload, 0) == 1 && le32(&c.payload, 4) == 0, || "SOC_CONFIGURATION_CMD is not {DISCRETE, latency 0}".into());
            }
            o.need(w.fw.rt.bt_done, || "SOC_CONFIGURATION_CMD before BT_CONFIG".into());
            w.fw.rt.soc_done = true;
        }
        (5, 0) => {
            if want_len(&mut o, c, 4, "DQA_ENABLE_CMD") {
                o.need(le32(&c.payload, 0) == 0, || "DQA_ENABLE_CMD names a command queue other than 0".into());
            }
            w.fw.rt.dqa_done = true;
        }
        (0, 0x1d) => {
            if want_len(&mut o, c, 12, "SCD_QUEUE_CFG") {
                let p = &c.payload;
                // { token, sta_id, tid, scd_queue, action, aggregate, tx_fifo, window, le16 ssn, le16 reserved }
                let ok = p[0] == 0 && p[1] == 1 && p[2] == 8 && p[3] == 1 && p[4] == 1 && p[5] == 0 && p[6] == 5 && p[7] == 64 && le16(p, 8) == 0 && le16(p, 10) == 0;
                o.need(ok, || format!("SCD_QUEUE_CFG is not the aux queue enable: {:02x?}", p));
                o.need(w.fw.rt.q_wrptr[1] == Some(0), || "SCD_QUEUE_CFG before the aux queue's write pointer was positioned (iwl_trans_txq_enable_cfg)".into());
            }
            w.fw.rt.scd_done = true;
        }
        (0, 0x18) => {
            if want_len(&mut o, c, 48, "ADD_STA") {
                let p = &c.payload;
                let mut want = [0u8; 48];
                want[2] = 0xff;
                want[3] = 0xff;
                want[4] = 4;
                want[16] = 1;
                want[35] = 4;
                want[40] = 2;
                o.need(p[..] == want[..], || format!("ADD_STA is not the aux station: {:02x?}", p));
            }
            o.need(w.fw.rt.scd_done, || "ADD_STA of the aux station before its queue was enabled".into());
            o.need(w.fw.rt.dqa_done, || "ADD_STA before DQA_ENABLE_CMD".into());
            w.fw.rt.aux_sta = Some(1);
            reply = w.fw.rt.add_sta_status.to_le_bytes().to_vec(); // struct iwl_mvm_add_sta_rsp { le32 status }
        }
        (4, 4) => {
            if want_len(&mut o, c, 20, "TEMP_REPORTING_THRESHOLDS_CMD") {
                o.need(c.payload.iter().all(|&b| b == 0), || "TEMP_REPORTING_THRESHOLDS_CMD is not empty".into());
            }
            w.fw.rt.temp_done = true;
        }
        (0, 0x77) => {
            if want_len(&mut o, c, 4, "POWER_TABLE_CMD") {
                o.need(c.payload[..] == [1, 0, 0, 0], || format!("POWER_TABLE_CMD is not {{POWER_SAVE_ENA}}: {:02x?}", c.payload));
            }
            o.need(w.fw.rt.aux_sta.is_some(), || "POWER_TABLE_CMD before the aux station".into());
            w.fw.rt.power_done = true;
        }
        (0, 0xc8) => {
            if want_len(&mut o, c, 28, "MCC_UPDATE_CMD") {
                let p = &c.payload;
                o.need(p[0] == b'Z' && p[1] == b'Z' && p[2] == 0x10 && p[3..].iter().all(|&b| b == 0), || format!("MCC_UPDATE_CMD is not ZZ / GET_CURRENT: {:02x?}", p));
            }
            o.need(w.fw.rt.power_done, || "MCC_UPDATE_CMD before the device power command".into());
            w.fw.rt.mcc_done = true;
            reply = mcc_resp(&w.fw.rt);
        }
        (1, 0x0c) => scan_cfg(w, c, &mut o),
        (1, 0x0d) => scan_req(w, c, &mut o),
        (1, 0x0e) => {
            if want_len(&mut o, c, 8, "SCAN_ABORT_UMAC") {
                w.fw.rt.aborts.push(le32(&c.payload, 0));
                w.fw.rt.running_uid = None;
            }
        }
        _ => o.viol.push(format!("RT firmware: unexpected command {}.{:#x} ({} bytes)", c.group, c.cmd, c.payload.len())),
    }
    // the reply (every host command is answered with its sequence number)
    o.posts.push((80, c.group, c.cmd, c.seq, reply));
    if w.fw.rt.chatter && key == (0, 0x98) {
        // an unsolicited notification the driver has no use for, between its replies
        w.fw.notif_counter += 1;
        let seq = 0x8000 | w.fw.notif_counter;
        o.posts.push((400, 0, 0x51, seq, vec![1, 2, 3, 4]));
    }
    for v in o.viol {
        w.violation(v);
    }
    for (d, g, cmd, seq, p) in o.posts {
        if g == 0 && cmd == 0x0f {
            // the completion frees the scan id at the moment it is delivered
            w.schedule(d, super::Ev::Custom(Box::new(move |w: &mut World| {
                w.fw.rt.running_uid = None;
                fm::post(w, g, cmd, seq, &p);
            })));
        } else {
            w.schedule(d, super::Ev::Post { group: g, cmd, seq, payload: p });
        }
    }
}

fn paging(w: &mut World, c: &SeenCmd, o: &mut Out) {
    let Some((css, data)) = w.fw.rt.paging_expect.clone() else {
        o.viol.push("FW_PAGING_BLOCK_CMD but the image has no paging image".into());
        return;
    };
    if !want_len(o, c, 144, "FW_PAGING_BLOCK_CMD") {
        return;
    }
    let p = &c.payload;
    let pages = data.len().div_ceil(4096);
    let blocks = pages.div_ceil(8);
    let last = pages - 8 * (blocks - 1);
    // struct iwl_fw_paging_cmd { le32 flags; le32 block_size; le32 block_num; le32 device_phy_addr[33]; }
    o.need(le32(p, 0) == 0x300 | last as u32, || format!("paging flags {:#x}, want SECURED|ENABLED|{last}", le32(p, 0)));
    o.need(le32(p, 4) == 15, || format!("paging block_size 2^{}, want 2^15", le32(p, 4)));
    o.need(le32(p, 8) == blocks as u32, || format!("paging block_num {}, want {blocks}", le32(p, 8)));
    for i in 0..blocks + 1 {
        let addr = (le32(p, 12 + 4 * i) as u64) << 12;
        let (want, what): (&[u8], String) = if i == 0 { (&css[..], "CSS block".into()) } else { let off = (i - 1) * 32768; (&data[off..(off + 32768).min(data.len())], format!("paging block {i}")) };
        let mut got = vec![0u8; want.len()];
        if !w.dma_read(addr, &mut got) {
            o.viol.push(format!("{what}: {addr:#x} is not host memory"));
        } else if got != want {
            o.viol.push(format!("{what} at {addr:#x} does not hold the paging image bytes"));
        }
    }
    o.need(p[12 + 4 * (blocks + 1)..].iter().all(|&b| b == 0), || "paging cmd names blocks it does not have".into());
    w.fw.rt.paging_done = true;
}

fn mcc_resp(rt: &RtFw) -> Vec<u8> {
    // struct iwl_mcc_update_resp_v3 { le32 status; le16 mcc; u8 cap; u8 source_id; le16 time; le16 geo_info; le32 n_channels; le32 channels[]; }
    let mut r = vec![];
    r.extend_from_slice(&0u32.to_le_bytes());
    r.extend_from_slice(&(((rt.regdom[0] as u16) << 8) | rt.regdom[1] as u16).to_le_bytes());
    r.push(2);
    r.push(0x10);
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes());
    r.extend_from_slice(&(EXT_CHANNELS.len() as u32).to_le_bytes());
    for (i, ch) in EXT_CHANNELS.iter().enumerate() {
        r.extend_from_slice(&(rt.mcc_flags(i, *ch) as u32).to_le_bytes());
    }
    r
}

fn scan_cfg(w: &mut World, c: &SeenCmd, o: &mut Out) {
    let n_scan = w.fw.rt.n_scan;
    if !want_len(o, c, 36 + n_scan, "SCAN_CFG_CMD") {
        return;
    }
    let p = &c.payload;
    let allowed = w.fw.rt.allowed();
    // struct iwl_scan_config_v1 { le32 flags, tx_chains, rx_chains, legacy_rates, out_of_channel_time, suspend_time;
    //   struct { u8 active, passive, fragmented, extended } dwell; u8 mac_addr[6]; u8 bcast_sta_id; u8 channel_flags; u8 channel_array[]; }
    let want_flags = 0x0002_ef09u32 | ((allowed.len() as u32) << 26);
    o.need(le32(p, 0) == want_flags, || format!("SCAN_CFG flags {:#x}, want {want_flags:#x}", le32(p, 0)));
    o.need(le32(p, 4) == 3 && le32(p, 8) == 3, || "SCAN_CFG tx/rx chains are not 3/3".into());
    o.need(le32(p, 12) == 0x0fff_0fff, || format!("SCAN_CFG legacy_rates {:#x}", le32(p, 12)));
    o.need(le32(p, 16) == 0 && le32(p, 20) == 0, || "SCAN_CFG out_of_channel/suspend time are not the UNASSOC zeros".into());
    o.need(p[24..28] == [10, 110, 44, 90], || "SCAN_CFG dwell is not 10/110/44/90".into());
    let mac = w.fw.rt.mac;
    o.need(p[28..34] == mac, || format!("SCAN_CFG mac {:02x?} is not the station address", &p[28..34]));
    o.need(Some(p[34]) == w.fw.rt.aux_sta, || format!("SCAN_CFG bcast_sta_id {} is not the aux station {:?}", p[34], w.fw.rt.aux_sta));
    o.need(p[35] == 0x0f, || format!("SCAN_CFG channel_flags {:#x}", p[35]));
    o.need(p[36..36 + allowed.len()] == allowed[..], || format!("SCAN_CFG channels {:?} are not the regulatory domain's {:?}", &p[36..36 + allowed.len()], allowed));
    o.need(p[36 + allowed.len()..].iter().all(|&b| b == 0), || "SCAN_CFG channel_array is not zero past the channels".into());
    o.need(w.fw.rt.mcc_done, || "scan while LAR regdomain is not set: SCAN_CFG_CMD before MCC_UPDATE_CMD".into());
    o.need(w.fw.rt.aux_sta.is_some(), || "SCAN_CFG_CMD before the aux station".into());
    w.fw.rt.scan_cfg = Some(p[36..36 + allowed.len()].to_vec());
}

fn scan_req(w: &mut World, c: &SeenCmd, o: &mut Out) {
    let n_scan = w.fw.rt.n_scan;
    let total = 52 + 8 * n_scan + 1220;
    o.need(w.fw.rt.scan_cfg.is_some(), || "SCAN_REQ_UMAC before SCAN_CFG_CMD".into());
    o.need(w.fw.rt.mcc_done, || "scan while LAR regdomain is not set".into());
    if !want_len(o, c, total, "SCAN_REQ_UMAC v8") {
        return;
    }
    let p = &c.payload;
    let uid = le32(p, 4);
    o.need(w.fw.rt.running_uid != Some(uid), || format!("SCAN_REQ_UMAC reuses the running scan id {uid}"));
    o.need(le32(p, 0) == 0, || format!("SCAN_REQ_UMAC flags {:#x}: a regular scan is not preemptive", le32(p, 0)));
    o.need(le32(p, 8) == 6, || "SCAN_REQ_UMAC ooc_priority is not IWL_SCAN_PRIORITY_EXT_6".into());
    // PASSIVE bit3 | PASS_ALL bit2 | ADAPTIVE_DWELL bit13
    o.need(le16(p, 12) == 0x200c, || format!("SCAN_REQ_UMAC general_flags {:#x}, want PASSIVE|PASS_ALL|ADAPTIVE_DWELL", le16(p, 12)));
    o.need(p[14] == 0, || "SCAN_REQ_UMAC reserved byte set".into());
    o.need(p[16] == 10 && p[17] == 0 && p[18] == 0, || "SCAN_REQ_UMAC v8 active_dwell/reserved2 wrong".into());
    o.need(p[19] == 2 && p[20] == 10 && p[21] == 2, || format!("SCAN_REQ_UMAC adaptive dwell fields {:?}", &p[19..22]));
    o.need(le16(p, 22) == 300, || "SCAN_REQ_UMAC adwell_max_budget is not 300".into());
    o.need(p[24..40].iter().all(|&b| b == 0), || "SCAN_REQ_UMAC max_out_time/suspend_time are not zero".into());
    o.need(le32(p, 40) == 6, || "SCAN_REQ_UMAC scan_priority is not EXT_6".into());
    o.need(p[44] == 110 && p[45] == 0 && p[46] == 0 && p[47] == 0, || "SCAN_REQ_UMAC passive_dwell / num_of_fragments wrong".into());
    o.need(p[48] == 7, || format!("SCAN_REQ_UMAC channel flags {:#x}, want EBS|ACCURATE|CACHE_ADD", p[48]));
    let count = p[49] as usize;
    o.need(le16(p, 50) == 0, || "SCAN_REQ_UMAC channel.reserved set".into());
    let allowed = w.fw.rt.allowed();
    o.need(count <= n_scan, || format!("SCAN_REQ_UMAC names {count} channels, the array holds {n_scan}"));
    let mut chans = vec![];
    for i in 0..n_scan {
        let e = &p[52 + 8 * i..52 + 8 * i + 8];
        if i < count {
            o.need(le32(e, 0) == 0, || format!("channel {i}: flags {:#x} (no direct SSIDs)", le32(e, 0)));
            o.need(e[5] == 1 && e[6] == 0 && e[7] == 0, || format!("channel {i}: iter_count/iter_interval {:?}", &e[5..8]));
            o.need(allowed.contains(&e[4]), || format!("channel {} is not allowed by the regulatory domain {:?}", e[4], allowed));
            chans.push(e[4]);
        } else {
            o.need(e.iter().all(|&b| b == 0), || format!("channel slot {i} past the count is not zero"));
        }
    }
    let mut dedup = chans.clone();
    dedup.sort();
    dedup.dedup();
    o.need(dedup.len() == chans.len(), || "SCAN_REQ_UMAC lists a channel twice".into());
    let t = 52 + 8 * n_scan;
    o.need(p[t..t + 12] == [0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0], || format!("SCAN_REQ_UMAC schedule/delay {:02x?}", &p[t..t + 12]));
    // preq: mac_header{0,26} band0{26,3} band1{29,0} common{29,9}; frame is a probe request from the station
    let pr = t + 12;
    o.need(p[pr..pr + 16] == [0, 0, 26, 0, 26, 0, 3, 0, 29, 0, 0, 0, 29, 0, 9, 0], || format!("SCAN_REQ_UMAC probe segments {:02x?}", &p[pr..pr + 16]));
    let f = &p[pr + 16..pr + 16 + 38];
    let mac = w.fw.rt.mac;
    o.need(f[0] == 0x40 && f[1] == 0 && f[4..10] == [0xff; 6] && f[10..16] == mac && f[16..22] == [0xff; 6], || format!("SCAN_REQ_UMAC probe request header {:02x?}", &f[..24]));
    o.need(f[24] == 0 && f[25] == 0, || "SCAN_REQ_UMAC probe request has no wildcard SSID element".into());
    o.need(f[26..29] == [3, 1, 0], || "SCAN_REQ_UMAC probe request has no DS Parameter Set placeholder".into());
    o.need(f[29..38] == [221, 7, 0, 0x50, 0xf2, 8, 0, 0, 0], || "SCAN_REQ_UMAC probe request has no WFA TPC report element".into());
    let d = pr + 16 + 512;
    o.need(p[d..].iter().all(|&b| b == 0), || "SCAN_REQ_UMAC names direct SSIDs in a passive scan".into());
    w.fw.rt.scans.push(p.clone());
    w.fw.rt.running_uid = Some(uid);
    o.complete_uid = Some(uid);
    // ---- the air ----
    let mut t_us = 3_000u64;
    let aps = w.fw.rt.aps.clone();
    for (i, ch) in chans.iter().enumerate() {
        let here: Vec<&SimAp> = aps.iter().filter(|a| a.channel == *ch).collect();
        for a in here {
            o.posts.push((t_us, 0, 0xc1, 0x8000, rx_mpdu(a, false, 3)));
            if a.hidden {
                o.posts.push((t_us + 300, 0, 0xc1, 0x8000, rx_mpdu(a, true, 3)));
            }
            t_us += 600;
        }
        if i == 0 && w.fw.rt.junk {
            let a = SimAp::new("junk", 0x99, *ch, -30, ApSec::Open);
            o.posts.push((t_us, 0, 0xc1, 0x8000, rx_mpdu(&a, false, 1))); // CRC error
            o.posts.push((t_us + 300, 0, 0xc1, 0x8000, rx_data_frame(*ch)));
            o.posts.push((t_us + 600, 0, 0xc1, 0x8000, rx_truncated(*ch)));
            t_us += 1200;
        }
        t_us += 1_000;
    }
    if !w.fw.rt.never_complete {
        // struct iwl_umac_scan_complete { le32 uid; u8 last_schedule; u8 last_iter; u8 status; u8 ebs_status; le32 time_from_last_iter; le32 reserved; }
        let mut cpl = vec![];
        cpl.extend_from_slice(&uid.to_le_bytes());
        cpl.extend_from_slice(&[0, 1, w.fw.rt.complete_status, 0]);
        cpl.extend_from_slice(&(t_us as u32 / 1000).to_le_bytes());
        cpl.extend_from_slice(&0u32.to_le_bytes());
        o.posts.push((t_us + 500, 0, 0x0f, 0x8000, cpl));
    }
}

fn ie(id: u8, body: &[u8]) -> Vec<u8> {
    let mut v = vec![id, body.len() as u8];
    v.extend_from_slice(body);
    v
}

fn rsn(akms: &[u8], pairwise: &[u8]) -> Vec<u8> {
    let mut b = vec![1, 0, 0x00, 0x0f, 0xac, 4];
    b.extend_from_slice(&(pairwise.len() as u16).to_le_bytes());
    for p in pairwise {
        b.extend_from_slice(&[0x00, 0x0f, 0xac, *p]);
    }
    b.extend_from_slice(&(akms.len() as u16).to_le_bytes());
    for a in akms {
        b.extend_from_slice(&[0x00, 0x0f, 0xac, *a]);
    }
    b.extend_from_slice(&[0, 0]);
    ie(48, &b)
}

/// An 802.11 beacon (or, with `probe`, a probe response) as the access point sends it, IEEE 802.11-2020 9.3.3.3.
fn frame(a: &SimAp, probe: bool) -> Vec<u8> {
    let mut f = vec![0u8; 24];
    f[0] = if probe { 0x50 } else { 0x80 };
    f[4..10].fill(0xff);
    f[10..16].copy_from_slice(&a.bssid);
    f[16..22].copy_from_slice(&a.bssid);
    f.extend_from_slice(&[0u8; 8]);
    f.extend_from_slice(&100u16.to_le_bytes());
    let privacy = !matches!(a.sec, ApSec::Open);
    f.extend_from_slice(&(0x0401u16 | if privacy { 0x10 } else { 0 }).to_le_bytes());
    let name: &[u8] = if a.hidden && !probe { &[] } else { &a.ssid };
    f.extend(ie(0, name));
    f.extend(ie(1, &[0x82, 0x84, 0x8b, 0x96]));
    if a.ds {
        f.extend(ie(3, &[a.channel]));
    }
    match a.sec {
        ApSec::Open | ApSec::Wep => {}
        ApSec::Wpa2Psk => f.extend(rsn(&[2], &[4])),
        ApSec::Wpa2Sae => f.extend(rsn(&[8], &[4])),
        ApSec::Wpa2Tkip => f.extend(rsn(&[2], &[2])),
        ApSec::Wpa1 => f.extend(ie(221, &[0x00, 0x50, 0xf2, 1, 1, 0, 0x00, 0x50, 0xf2, 2, 1, 0, 0x00, 0x50, 0xf2, 2, 1, 0, 0x00, 0x50, 0xf2, 2])),
    }
    f
}

/// A REPLY_RX_MPDU_CMD payload: struct iwl_rx_mpdu_desc with the v1 tail (fw/api/rx.h) then the MPDU.
fn rx_payload(mpdu: &[u8], status: u32, rssi_dbm: i16, channel: u8, flags2: u8) -> Vec<u8> {
    let mut p = vec![0u8; 48];
    p[0..2].copy_from_slice(&(mpdu.len() as u16).to_le_bytes()); // mpdu_len
    p[3] = flags2; // mac_flags2: header length in 2-byte words
    p[12..16].copy_from_slice(&status.to_le_bytes()); // status
    p[32] = (-rssi_dbm) as u8; // energy_a
    p[33] = (-rssi_dbm) as u8 + 6; // energy_b is weaker; the host reports the max
    p[34] = channel;
    p.extend_from_slice(mpdu);
    p
}

fn rx_mpdu(a: &SimAp, probe: bool, status: u32) -> Vec<u8> {
    rx_payload(&frame(a, probe), status, a.rssi, a.channel, 12)
}

fn rx_data_frame(ch: u8) -> Vec<u8> {
    let mut f = vec![0u8; 40];
    f[0] = 0x88; // QoS data
    rx_payload(&f, 3, -50, ch, 13)
}

fn rx_truncated(ch: u8) -> Vec<u8> {
    let mut f = vec![0u8; 30];
    f[0] = 0x80; // a beacon cut off inside its fixed fields
    rx_payload(&f, 3, -50, ch, 12)
}
