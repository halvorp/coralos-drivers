// SPDX-License-Identifier: GPL-2.0-only
//! Scan (M3): one passive, all-channel `SCAN_REQ_UMAC` (v8) on the RT image, the `REPLY_RX_MPDU_CMD` notifications it
//! produces turned into [`Event::ScanResult`]s, and the `SCAN_COMPLETE_UMAC` that ends it. Ported from Linux
//! `mvm/scan.c` (`_iwl_mvm_single_scan_start`, `iwl_mvm_scan_umac`, `iwl_mvm_rx_umac_scan_complete_notif`,
//! `iwl_mvm_umac_scan_abort`) and `mvm/rxmq.c` (`iwl_mvm_rx_mpdu_mq`).
//!
//! Passive, because the 9260's regulatory state after the MCC update is read from the firmware but a probe request
//! needs the aux station's TX path (not built). A passive listen of 110 ms per channel hears every network that beacons
//! at the usual 102 ms interval.

use crate::driver::{Driver, DriverError};
use crate::event::{Bss, Event, Security, Ssid};
use crate::host::Host;
use crate::rt_up::RtWhy;
use crate::transport::TransportError;
use alloc::vec;
use alloc::vec::Vec;
use iwlwifi_mvm_rt_core::decode::{decode_rx_mpdu_v1, decode_scan_complete, decode_scan_iter_complete};
use iwlwifi_mvm_rt_core::frame::{heard_from_rx, Heard, Security as FrameSecurity, Skip};
use iwlwifi_mvm_rt_core::ids::*;
use iwlwifi_mvm_rt_core::scanreq::{encode_scan_req_umac_v8, scan_req_umac_len, ScanRequest};
use iwlwifi_pcie_ring_core::tx::Chunk;

/// `SCAN_TIMEOUT`: how long Linux waits for the scan to finish before it aborts it. // mvm/scan.c:48 (20 * HZ)
pub const SCAN_TIMEOUT_MS: u32 = 20_000;
/// The scan table bound. A busy site hears dozens, never hundreds; beyond this the weakest is dropped and counted.
pub const MAX_BSS: usize = 128;

/// What one scan did, besides its events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub rx_packets: u32,
    pub heard: u32,
    pub bad_crc: u32,
    pub not_beacon: u32,
    pub malformed: u32,
    pub undecodable: u32,
    pub dropped_weakest: u32,
    pub elapsed_ms: u64,
    /// `iwl_umac_scan_complete.status`; 0 when the scan never completed.
    pub status: u8,
    pub ebs_status: u8,
    pub iter_notifs: u32,
}

/// The result of [`Driver::scan`].
#[derive(Debug, Clone)]
pub struct ScanOutcome {
    /// One `ScanResult` per distinct BSSID (strongest first), then one `ScanDone`.
    pub events: Vec<Event>,
    pub complete: bool,
    pub stats: ScanStats,
}

fn event_security(s: FrameSecurity) -> Security {
    match s {
        FrameSecurity::Open => Security::Open,
        FrameSecurity::Wpa2Psk => Security::Wpa2Psk,
        FrameSecurity::Unsupported => Security::Unsupported,
    }
}

impl<H: Host> Driver<H> {
    /// Run one passive scan and return its events. `scan_id` is the manager's id, echoed in every event.
    pub fn scan(&mut self, scan_id: u32) -> Result<ScanOutcome, DriverError> {
        let r = self.scan_inner(scan_id);
        if r.is_err() && self.t.fault.is_some() {
            self.dump_fw_error(iwlwifi_fw_alive_core::dump::Image::Regular);
        }
        r
    }

    fn scan_inner(&mut self, scan_id: u32) -> Result<ScanOutcome, DriverError> {
        if !self.rt.up {
            self.log(format_args!("[IWL-SCAN FAIL id={scan_id} the RT image is not up]"));
            return Err(DriverError::Rt { stage: "scan", why: RtWhy::NotUp });
        }
        let n_scan = self.fw.info.n_scan_channels as usize;
        let uid = 0u32; // iwl_mvm_scan_uid_by_status(mvm, 0): the first free slot, and one scan runs at a time
        let use_ebs = self.fw.info.flags & IWL_UCODE_TLV_FLAGS_EBS_SUPPORT != 0;
        let sa = self.mac.unwrap_or([0; 6]);
        let req = ScanRequest { uid, channels: &self.rt.scan_channels, n_scan_channels: n_scan, sa, scan_start_mac_id: self.rt.mac_id, use_ebs };
        let mut buf = vec![0u8; scan_req_umac_len(n_scan)];
        let len = match encode_scan_req_umac_v8(&req, &mut buf) {
            Ok(n) => n,
            Err(e) => {
                self.log(format_args!("[IWL-SCAN FAIL id={scan_id} encode {e:?}]"));
                return Err(DriverError::Rt { stage: "scan", why: RtWhy::Encode(e) });
            }
        };
        self.t.inbox.clear(); // stale notifications from before this scan
        let t0 = self.t.host().now_ms();
        self.log(format_args!("[IWL-SCAN-REQ id={scan_id} uid={uid} passive channels={} len={len} ebs={}]", self.rt.scan_channels.len(), use_ebs as u8));
        if let Err(e) = self.t.send_cmd(LONG_GROUP, SCAN_REQ_UMAC, 0, &[Chunk { data: &buf[..len], nocopy: true }]) {
            let _ = self.nic_fault_check("scan");
            self.log(format_args!("[IWL-SCAN FAIL id={scan_id} send {} {e:?}]", e.name()));
            return Err(DriverError::Rt { stage: "scan", why: RtWhy::Transport(e) });
        }
        self.rt.cmds += 1;

        let mut stats = ScanStats::default();
        let mut table: Vec<Heard> = Vec::new();
        let mut complete = false;
        let deadline = t0 + SCAN_TIMEOUT_MS as u64;
        loop {
            let now = self.t.host().now_ms();
            let left = deadline.saturating_sub(now) as u32;
            if left == 0 {
                break;
            }
            let rx = match self.t.wait_notif(left, |r| (r.group == LEGACY_GROUP || r.group == LONG_GROUP) && matches!(r.cmd, REPLY_RX_MPDU_CMD | SCAN_ITERATION_COMPLETE_UMAC | SCAN_COMPLETE_UMAC)) {
                Ok(rx) => rx,
                Err(TransportError::Timeout { .. }) => break,
                Err(e) => {
                    let _ = self.nic_fault_check("scan");
                    self.log(format_args!("[IWL-SCAN FAIL id={scan_id} wait {} {e:?}]", e.name()));
                    return Err(DriverError::Rt { stage: "scan", why: RtWhy::Transport(e) });
                }
            };
            match rx.cmd {
                REPLY_RX_MPDU_CMD => {
                    stats.rx_packets += 1;
                    match decode_rx_mpdu_v1(&rx.payload) {
                        Err(_) => stats.undecodable += 1,
                        Ok(m) => match heard_from_rx(&m) {
                            Ok(h) => {
                                stats.heard += 1;
                                merge(&mut table, h, &mut stats);
                            }
                            Err(Skip::BadCrc) => stats.bad_crc += 1,
                            Err(Skip::NotBeaconOrProbeResp) => stats.not_beacon += 1,
                            Err(_) => stats.malformed += 1,
                        },
                    }
                }
                SCAN_ITERATION_COMPLETE_UMAC => {
                    stats.iter_notifs += 1;
                    if let Ok(i) = decode_scan_iter_complete(&rx.payload) {
                        self.log(format_args!("[IWL-SCAN-ITER uid={} channels={} status={} last_channel={}]", i.uid, i.scanned_channels, i.status, i.last_channel));
                    }
                }
                _ => match decode_scan_complete(&rx.payload) {
                    Ok(c) if c.uid == uid => {
                        stats.status = c.status;
                        stats.ebs_status = c.ebs_status;
                        complete = c.status == IWL_SCAN_OFFLOAD_COMPLETED;
                        break;
                    }
                    Ok(c) => self.log(format_args!("[IWL-SCAN-NOTE completion for uid {} ignored (ours is {uid})]", c.uid)),
                    Err(e) => self.log(format_args!("[IWL-SCAN-NOTE undecodable completion {e:?}]")),
                },
            }
        }
        stats.elapsed_ms = self.t.host().now_ms().saturating_sub(t0);
        if stats.status == 0 {
            // iwl_mvm_scan_stop_wait: ask the firmware to stop it. struct iwl_umac_scan_abort { le32 uid; le32 flags; }
            // // mvm/scan.c:3290-3350; fw/api/scan.h:1141-1144
            let mut ab = [0u8; 8];
            ab[0..4].copy_from_slice(&uid.to_le_bytes());
            let _ = self.t.send_cmd(LONG_GROUP, SCAN_ABORT_UMAC, 0, &[Chunk { data: &ab, nocopy: false }]);
            self.log(format_args!("[IWL-SCAN FAIL id={scan_id} no SCAN_COMPLETE_UMAC within {}ms; aborted, {} BSS so far]", SCAN_TIMEOUT_MS, table.len()));
        }
        self.nic_fault_check("scan")?;
        table.sort_by(|a, b| b.rssi.cmp(&a.rssi));
        let mut events = Vec::with_capacity(table.len() + 1);
        for h in &table {
            let ssid = Ssid::new(&h.ssid[..h.ssid_len as usize]).unwrap_or(Ssid::EMPTY);
            events.push(Event::ScanResult { scan_id, bss: Bss { ssid, bssid: h.bssid, rssi: h.rssi, channel: h.channel, security: event_security(h.security) } });
        }
        events.push(Event::ScanDone { scan_id, complete });
        self.log(format_args!(
            "[IWL-SCAN-DONE id={scan_id} complete={} status={} ebs={} bss={} frames={} heard={} bad_crc={} not_beacon={} malformed={} undecodable={} ms={}]",
            complete as u8,
            stats.status,
            stats.ebs_status,
            table.len(),
            stats.rx_packets,
            stats.heard,
            stats.bad_crc,
            stats.not_beacon,
            stats.malformed,
            stats.undecodable,
            stats.elapsed_ms
        ));
        Ok(ScanOutcome { events, complete, stats })
    }

    /// Serve one neutral request. `Scan` runs a scan; the events come back in order.
    pub fn handle(&mut self, req: crate::event::Request) -> Result<Vec<Event>, DriverError> {
        match req {
            crate::event::Request::Scan { scan_id } => Ok(self.scan(scan_id)?.events),
        }
    }
}

/// One entry per BSSID: the strongest signal wins, a non-empty SSID replaces an empty one (a beacon of a hidden
/// network carries a zero-length SSID, its probe response the name).
fn merge(table: &mut Vec<Heard>, h: Heard, stats: &mut ScanStats) {
    if let Some(e) = table.iter_mut().find(|e| e.bssid == h.bssid) {
        let (ssid_len, ssid) = if e.ssid_len == 0 && h.ssid_len != 0 { (h.ssid_len, h.ssid) } else { (e.ssid_len, e.ssid) };
        if h.rssi > e.rssi {
            *e = h;
        }
        e.ssid_len = ssid_len;
        e.ssid = ssid;
        return;
    }
    if table.len() >= MAX_BSS {
        // Replace the weakest if the newcomer is stronger; either way one is dropped and counted.
        stats.dropped_weakest += 1;
        if let Some((i, w)) = table.iter().enumerate().min_by_key(|(_, e)| e.rssi) {
            if w.rssi < h.rssi {
                table[i] = h;
            }
        }
        return;
    }
    table.push(h);
}
