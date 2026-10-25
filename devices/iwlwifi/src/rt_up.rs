// SPDX-License-Identifier: GPL-2.0-only
//! The RT image: stop the INIT image, restart the hardware, load the REGULAR image, wait for its ALIVE, hand the
//! firmware its paging image, and send the post-ALIVE command set until the firmware can scan. Ported from Linux
//! `mvm/fw.c:1542-1758` (`iwl_mvm_up`), `fw/paging.c` (`iwl_init_paging`), `mvm/sta.c` (`iwl_mvm_add_aux_sta`),
//! `mvm/nvm.c` (`iwl_mvm_init_mcc`, `iwl_mvm_update_mcc`) and `mvm/scan.c` (`iwl_mvm_config_scan`).
//!
//! Every stage prints one named marker; a failure prints `[IWL-RT FAIL stage=<s> …]` and ends the bring-up there.
//! What `iwl_mvm_up` does that this does not: see the crate doc of `iwlwifi-mvm-rt-core` (smart FIFO, LARI/PPAG/SAR,
//! LTR, RSS/RXQ, PHY and MAC contexts). The ordering and the conditions are Linux's for this firmware's capability
//! and API bits (tx_ant, phy_db + phy_cfg, bt, soc latency, dqa, aux station, temperature thresholds, power, mcc, scan).

use crate::driver::{Driver, DriverError};
use crate::host::Host;
use crate::transport::{Rx, TransportError};
use alloc::vec;
use alloc::vec::Vec;
use coralos_hal::DmaRegion;
use iwlwifi_fw_alive_core::alive::AliveV3;
use iwlwifi_fw_alive_core::dump::Image;
use iwlwifi_fw_image_core::sections::UcodeType;
use iwlwifi_fw_tlv_core::capa::*;
use iwlwifi_mvm_init_core::encode::{encode_phy_cfg_v1, encode_tx_ant_cfg, PhyCfgV1};
use iwlwifi_mvm_init_core::ids::{MVM_UCODE_ALIVE_TIMEOUT_MS, PHY_CFG_CMD_V1_LEN, TX_ANT_CFG_CMD_LEN};
use iwlwifi_mvm_init_core::machine::Event;
use iwlwifi_mvm_rt_core::decode::{add_sta_ok, decode_add_sta_status, decode_mcc_resp_v3, DecodeError};
use iwlwifi_mvm_rt_core::encode::*;
use iwlwifi_mvm_rt_core::ids::*;
use iwlwifi_mvm_rt_core::paging::{encode_fw_paging_cmd, plan, PagingError, SecInfo};
use iwlwifi_mvm_rt_core::scanreq::{encode_scan_config_v1, ScanConfig};
use iwlwifi_nvm_parse_core::NVM_CHANNEL_VALID;
use iwlwifi_pcie_nic_core::bus::NicIo;
use iwlwifi_pcie_nic_core::regs::HBUS_TARG_WRPTR;
use iwlwifi_pcie_ring_core::tx::Chunk;

/// Why an RT stage failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtWhy {
    Transport(TransportError),
    Encode(EncodeError),
    Decode(DecodeError),
    Paging(PagingError),
    /// The ALIVE wait ended as `what` (timeout / aborted / invalid).
    Alive(&'static str),
    /// A command replied with a status the stage refuses.
    Status(u32),
    /// The RT image is not up (a scan before `start_rt`).
    NotUp,
}

impl From<TransportError> for RtWhy {
    fn from(e: TransportError) -> Self {
        RtWhy::Transport(e)
    }
}
impl From<EncodeError> for RtWhy {
    fn from(e: EncodeError) -> Self {
        RtWhy::Encode(e)
    }
}

/// What the RT bring-up left behind.
#[derive(Debug, Clone, Default)]
pub struct RtState {
    pub up: bool,
    /// The channels a scan listens on: NVM channel order, 2.4 GHz then 5 GHz, valid per the MCC response.
    pub scan_channels: Vec<u8>,
    pub aux_sta_id: u8,
    /// The station interface's MAC context id (`mvmvif->id`), named in every scan's `scan_start_mac_id`.
    pub mac_id: u8,
    pub mcc: u16,
    /// Host commands the RT bring-up sent.
    pub cmds: u32,
    /// The paging DRAM blocks (CSS first); allocated once, reused by a later RT start.
    pub paging_blocks: Vec<DmaRegion>,
}

/// The summary `start_rt` returns (everything is also in the markers).
#[derive(Debug, Clone)]
pub struct RtReport {
    pub alive: AliveV3,
    pub paging_blocks: u32,
    pub cmds: u32,
    pub mcc: u16,
    pub nvm_channels: usize,
    pub scan_channels: usize,
}

/// `MVM_UCODE_ALIVE_TIMEOUT`. // mvm/fw.c:28 (re-exported from the INIT flow's table)
const ALIVE_TIMEOUT_MS: u32 = MVM_UCODE_ALIVE_TIMEOUT_MS;

impl<H: Host> Driver<H> {
    /// `iwl_mvm_up` for the 9260. On success the firmware accepts `SCAN_REQ_UMAC`.
    pub fn start_rt(&mut self) -> Result<RtReport, DriverError> {
        let r = self.start_rt_inner();
        if r.is_err() && self.t.fault.is_some() {
            self.dump_fw_error(Image::Regular);
        }
        r
    }

    fn rt_fail(&mut self, stage: &'static str, why: RtWhy) -> DriverError {
        self.log(format_args!("[IWL-RT FAIL stage={stage} {why:?}]"));
        DriverError::Rt { stage, why }
    }

    /// Send one RT command and name the outcome. `stage` is the marker name.
    fn rt_cmd(&mut self, stage: &'static str, group: u8, cmd: u8, chunks: &[Chunk<'_>]) -> Result<Rx, DriverError> {
        match self.t.send_cmd(group, cmd, 0, chunks) {
            Ok(rx) => {
                self.rt.cmds += 1;
                self.nic_fault_check(stage)?;
                Ok(rx)
            }
            Err(e) => {
                let _ = self.nic_fault_check(stage);
                Err(self.rt_fail(stage, RtWhy::Transport(e)))
            }
        }
    }

    /// A command whose reply carries nothing the host checks (Linux: `iwl_mvm_send_cmd_pdu` without `CMD_WANT_SKB`).
    fn rt_simple(&mut self, stage: &'static str, group: u8, cmd: u8, payload: &[u8]) -> Result<(), DriverError> {
        self.rt_cmd(stage, group, cmd, &[Chunk { data: payload, nocopy: false }])?;
        self.log(format_args!("[IWL-RT-CMD {stage} ok]"));
        Ok(())
    }

    fn start_rt_inner(&mut self) -> Result<RtReport, DriverError> {
        let blocks = core::mem::take(&mut self.rt.paging_blocks);
        self.rt = RtState { paging_blocks: blocks, ..RtState::default() };
        // iwl_mvm_stop_device after the INIT image, then iwl_mvm_up -> iwl_trans_start_hw. // mvm/fw.c:1550
        let rep = self.t.stop_device();
        self.t.inbox.clear();
        self.log(format_args!("[IWL-RT-STOP master_stopped={} retake={}]", rep.master_stopped as u8, if rep.retake.is_ok() { "ok" } else { "FAIL" }));
        self.nic_fault_check("rt-stop")?;
        let kill = match self.t.start_hw() {
            Ok(k) => k,
            Err(e) => return Err(self.rt_fail("start_hw", RtWhy::Transport(e))),
        };
        self.nic_fault_check("rt-start_hw")?;
        self.log(format_args!("[IWL-RT-HW-READY rfkill={}]", if kill { "ON" } else { "off" }));

        // iwl_mvm_load_rt_fw -> iwl_mvm_load_ucode_wait_alive(REGULAR): run_in_rfkill is false. // mvm/fw.c:286-330
        let phy = self.fw.info.phy_config.unwrap_or(0);
        let started = {
            let secs = self.fw.sections(UcodeType::Regular)?;
            self.t.start_fw(&secs, phy, false)
        };
        match started {
            Ok(rep) => self.log(format_args!("[IWL-FW-LOADED rt cpu1={} cpu2={} chunks={} bytes={}]", rep.cpu1_sections, rep.cpu2_sections, rep.chunks, rep.bytes)),
            Err(e) => {
                self.log(format_args!("[IWL-FW-LOADED FAIL rt {} {:?}]", e.name(), e));
                let _ = self.nic_fault_check("rt-start_fw");
                return Err(self.rt_fail("start_fw", RtWhy::Transport(e)));
            }
        }
        self.nic_fault_check("rt-start_fw")?;
        match self.wait_alive(Image::Regular, ALIVE_TIMEOUT_MS) {
            Event::AliveOk => {}
            Event::WaitTimedOut => return Err(self.rt_fail("alive", RtWhy::Alive("timeout"))),
            Event::WaitAborted => return Err(self.rt_fail("alive", RtWhy::Alive("aborted"))),
            _ => return Err(self.rt_fail("alive", RtWhy::Alive("invalid"))),
        }
        self.nic_fault_check("rt-alive")?;
        let alive = self.alive.ok_or_else(|| self.rt_fail("alive", RtWhy::Alive("missing")))?;

        // iwl_init_paging(REGULAR): the CPU2 paging image, once, after ALIVE. // mvm/fw.c (iwl_mvm_load_rt_fw); fw/paging.c:271-296
        let paging_blocks = self.rt_paging()?;
        self.nic_fault_check("rt-paging")?;

        self.rt_post_alive()?;
        self.nic_fault_check("rt-up")?;
        self.rt.up = true;
        let nvm_channels = self.channels.len();
        let (cmds, mcc, scan_n, aux) = (self.rt.cmds, self.rt.mcc, self.rt.scan_channels.len(), self.rt.aux_sta_id);
        self.log(format_args!("[IWL-RT-UP cmds={cmds} mcc={mcc:04x} nvm_channels={nvm_channels} scan_channels={scan_n} aux_sta={aux}]"));
        Ok(RtReport { alive, paging_blocks, cmds: self.rt.cmds, mcc: self.rt.mcc, nvm_channels, scan_channels: self.rt.scan_channels.len() })
    }

    /// `iwl_alloc_fw_paging_mem`, `iwl_fill_paging_mem`, `iwl_send_paging_cmd`. // fw/paging.c:34-296
    fn rt_paging(&mut self) -> Result<u32, DriverError> {
        let size = self.fw.info.paging_mem_size;
        if size == 0 {
            self.log(format_args!("[IWL-PAGING none]"));
            return Ok(0);
        }
        let infos: Vec<SecInfo> = self.fw.sections(UcodeType::Regular)?.iter().map(|s| SecInfo { offset: s.offset, len: s.data.len() }).collect();
        let layout = match plan(&infos, size) {
            Ok(l) => l,
            Err(e) => return Err(self.rt_fail("paging", RtWhy::Paging(e))),
        };
        // One DRAM block per paging block plus the CSS block: 4 KiB for the CSS, 32 KiB for the rest. The memory is
        // never freed (CoralOS has no DMA free); a second RT start finds the blocks already there. // fw/paging.c:41-43
        if self.rt.paging_blocks.is_empty() {
            for blk in 0..layout.dram_blocks() {
                match self.t.alloc("fw paging block", layout.block_size(blk), 4096) {
                    Ok(r) => self.rt.paging_blocks.push(r),
                    Err(e) => return Err(self.rt_fail("paging", RtWhy::Transport(e))),
                }
            }
        }
        let regions: Vec<DmaRegion> = self.rt.paging_blocks.clone();
        {
            let secs = self.fw.sections(UcodeType::Regular)?;
            let host = self.t.host();
            host.dma_write(&regions[0], 0, secs[layout.css_sec].data);
            for blk in 1..=layout.num_blk {
                let (off, len) = layout.copy_range(blk);
                host.dma_write(&regions[blk as usize], 0, &secs[layout.data_sec].data[off..off + len]);
            }
        }
        let phys: Vec<u64> = regions.iter().map(|r| r.phys).collect();
        let mut cmd = vec![0u8; FW_PAGING_CMD_LEN];
        if let Err(e) = encode_fw_paging_cmd(&layout, &phys, &mut cmd) {
            return Err(self.rt_fail("paging", RtWhy::Encode(e)));
        }
        self.log(format_args!("[IWL-PAGING blocks={} last_pages={} size={size}]", layout.num_blk, layout.pages_in_last));
        self.rt_cmd("paging", LONG_GROUP, FW_PAGING_BLOCK_CMD, &[Chunk { data: &cmd, nocopy: false }])?;
        self.log(format_args!("[IWL-RT-CMD paging ok]"));
        Ok(layout.num_blk)
    }

    /// `iwl_mvm_up` after the paging, in its order. // mvm/fw.c:1581-1714
    fn rt_post_alive(&mut self) -> Result<(), DriverError> {
        let capa = |me: &Self, b: u32| me.fw.info.capa.has(b);
        let mut b = [0u8; 64];

        // iwl_send_tx_ant_cfg. // mvm/fw.c:1581
        let n = encode_tx_ant_cfg(self.valid_tx_ant, &mut b[..TX_ANT_CFG_CMD_LEN]).expect("4 bytes fit");
        self.rt_simple("tx_ant", LEGACY_GROUP, iwlwifi_mvm_init_core::ids::TX_ANT_CONFIGURATION_CMD, &b[..n])?;

        // iwl_send_phy_db_data then iwl_send_phy_cfg_cmd (the image is not unified). // mvm/fw.c:1585-1593
        let mut sent = 0u32;
        let mut bytes = 0usize;
        let mut idx = 0usize;
        let total = self.phydb.send_order().count();
        while idx < total {
            let (ty, len) = {
                let (ty, d) = self.phydb.send_order().nth(idx).expect("idx < total");
                (ty, d.len())
            };
            let hdr = encode_phy_db_hdr(ty, len as u16);
            let r = {
                let (_, data) = self.phydb.send_order().nth(idx).expect("idx < total");
                self.t.send_cmd(LEGACY_GROUP, PHY_DB_CMD, 0, &[Chunk { data: &hdr, nocopy: false }, Chunk { data, nocopy: true }])
            };
            match r {
                Ok(_) => {
                    self.rt.cmds += 1;
                    sent += 1;
                    bytes += len;
                }
                Err(e) => {
                    let _ = self.nic_fault_check("phy_db");
                    return Err(self.rt_fail("phy_db", RtWhy::Transport(e)));
                }
            }
            idx += 1;
        }
        self.nic_fault_check("phy_db")?;
        self.log(format_args!("[IWL-RT-CMD phy_db ok sections={sent} bytes={bytes}]"));

        let cfg = PhyCfgV1::new(self.fw.info.phy_config.unwrap_or(0), self.valid_tx_ant, self.valid_rx_ant, 0, self.fw.regular_calib.flow_trigger, self.fw.regular_calib.event_trigger);
        let n = encode_phy_cfg_v1(&cfg, &mut b[..PHY_CFG_CMD_V1_LEN]).expect("12 bytes fit");
        self.rt_simple("phy_cfg", LEGACY_GROUP, iwlwifi_mvm_init_core::ids::PHY_CONFIGURATION_CMD, &b[..n])?;

        // iwl_mvm_send_bt_init_conf. // mvm/fw.c:1595
        let n = encode_bt_config(capa(self, CAPA_BT_MPLUT_SUPPORT), &mut b).map_err(|e| self.rt_fail("bt", e.into()))?;
        self.rt_simple("bt", LEGACY_GROUP, BT_CONFIG, &b[..n])?;

        // iwl_set_soc_latency when the firmware has SOC_LATENCY_SUPPORT. // mvm/fw.c:1599-1604
        if capa(self, CAPA_SOC_LATENCY_SUPPORT) {
            let n = encode_soc_config(&mut b).map_err(|e| self.rt_fail("soc", e.into()))?;
            self.rt_simple("soc", SYSTEM_GROUP, SOC_CONFIGURATION_CMD, &b[..n])?;
        } else {
            self.log(format_args!("[IWL-RT-CMD soc skipped: no SOC_LATENCY_SUPPORT]"));
        }

        // iwl_mvm_send_dqa_cmd. // mvm/fw.c:1633-1637
        if capa(self, CAPA_DQA_SUPPORT) {
            let n = encode_dqa_enable(&mut b).map_err(|e| self.rt_fail("dqa", e.into()))?;
            self.rt_simple("dqa", DATA_PATH_GROUP, DQA_ENABLE_CMD, &b[..n])?;
        }

        // iwl_mvm_add_aux_sta: the station API is the old one (ADD_STA v10 < 12), so the aux station is added here.
        // The queue is enabled first. // mvm/fw.c:1645-1653; mvm/sta.c:1693-1741, :2243-2265, :1006-1020
        self.rt_aux_sta()?;

        // iwl_mvm_send_temp_report_ths_cmd when the firmware does CT-kill (iwl_mvm_is_tt_in_fw). // mvm/fw.c:1665-1671
        if capa(self, CAPA_CT_KILL_BY_FW) && capa(self, CAPA_TEMP_THS_REPORT_SUPPORT) {
            let n = encode_temp_report_ths(&mut b).map_err(|e| self.rt_fail("temp_ths", e.into()))?;
            self.rt_simple("temp_ths", PHY_OPS_GROUP, TEMP_REPORTING_THRESHOLDS_CMD, &b[..n])?;
        }

        // iwl_mvm_power_update_device. // mvm/fw.c:1694
        let n = encode_device_power(&mut b).map_err(|e| self.rt_fail("power", e.into()))?;
        self.rt_simple("power", LEGACY_GROUP, POWER_TABLE_CMD, &b[..n])?;

        // iwl_mvm_init_mcc: LAR on (TLV and NVM agree). // mvm/fw.c:1702-1706; mvm/nvm.c (iwl_mvm_init_mcc)
        let lar = capa(self, CAPA_LAR_SUPPORT) && self.nvm.is_some_and(|n| n.lar_enabled);
        if lar {
            self.rt_mcc()?;
        } else {
            self.log(format_args!("[IWL-RT-CMD mcc skipped: LAR off]"));
            self.rt.scan_channels = self.channels.iter().filter(|c| c.flags & NVM_CHANNEL_VALID != 0).map(|c| c.number as u8).collect();
        }

        // iwl_mvm_config_scan. // mvm/fw.c:1708-1714
        if capa(self, CAPA_UMAC_SCAN) {
            self.rt_scan_config()?;
        }
        // iwl_mvm_up is done; what mac80211 does when the station interface comes up follows it.
        self.rt_mac_ctxt()?;
        Ok(())
    }

    /// `iwl_mvm_mac_add_interface` for a (non-P2P) station: `iwl_mvm_mac_ctxt_init` hands out MAC id 0 and TSF A (id 0 is
    /// reserved for the managed vif on the non-MLD API), `iwl_mvm_mac_ctxt_add` sends `MAC_CONTEXT_CMD` with
    /// `FW_CTXT_ACTION_ADD`. The scan then names that id in `scan_start_mac_id` (`iwl_mvm_scan_umac`). // mvm/mac80211.c:1687-1760;
    /// mvm/mac-ctxt.c (iwl_mvm_mac_ctxt_init, iwl_mvm_mac_ctxt_add); mvm/scan.c:2330
    ///
    /// NOT sent, deliberately: a PHY context and a binding. Linux adds a PHY context from `assign_vif_chanctx` and binds the
    /// MAC to it there; an unassociated station scanning has no channel context (`iwl_mvm_scan_umac` carries no PHY id), and
    /// `iwl_mvm_up` of this tree adds none. `iwl_mvm_power_update_mac` and `iwl_mvm_disable_beacon_filter`, the two commands
    /// `add_interface` sends after the add, are configuration of an interface that is not associated and are not sent either.
    fn rt_mac_ctxt(&mut self) -> Result<(), DriverError> {
        let id = 0u8; // iwl_mvm_mac_ctxt_init: first free id, 0 allowed for the managed vif
        let mac = self.mac.unwrap_or([0; 6]);
        let mut b = [0u8; MAC_CTX_CMD_LEN];
        let n = match encode_mac_ctxt_sta_add(id, mac, &mut b) {
            Ok(n) => n,
            Err(e) => return Err(self.rt_fail("mac_ctxt", e.into())),
        };
        match self.rt_cmd("mac_ctxt", LEGACY_GROUP, MAC_CONTEXT_CMD, &[Chunk { data: &b[..n], nocopy: false }]) {
            Ok(_) => {}
            Err(e) => {
                self.log(format_args!("[IWL-MAC-CTXT FAIL id={id}]"));
                return Err(e);
            }
        }
        self.rt.mac_id = id;
        self.log(format_args!("[IWL-MAC-CTXT OK id={id} type=sta tsf=A assoc=0]"));
        Ok(())
    }

    /// `iwl_mvm_add_aux_sta(MAC_INDEX_AUX)` on a DQA firmware. // mvm/sta.c:2243-2265
    fn rt_aux_sta(&mut self) -> Result<(), DriverError> {
        let (queue, sta_id) = (IWL_MVM_DQA_AUX_QUEUE, AUX_STA_ID);
        // iwl_mvm_enable_aux_snif_queue -> iwl_mvm_enable_txq: iwl_trans_txq_enable_cfg(cfg = NULL) moves the write pointer
        // (mq_rx_supported: no SCD 0x40 bug), then SCD_QUEUE_CFG enables the queue in the firmware. // mvm/sta.c:973-1000;
        // pcie/gen1_2/tx.c:1147-1230
        let ssn = 0u16;
        self.t.nic.write_direct32(HBUS_TARG_WRPTR, (ssn as u32 & 0xff) | ((queue as u32) << 8));
        self.nic_fault_check("aux_queue")?;
        let mut b = [0u8; ADD_STA_CMD_LEN];
        let n = encode_scd_queue_cfg_enable(queue, sta_id, IWL_MVM_TX_FIFO_MCAST, ssn, &mut b).map_err(|e| self.rt_fail("aux_queue", e.into()))?;
        self.rt_simple("aux_queue", LEGACY_GROUP, SCD_QUEUE_CFG, &b[..n])?;
        // iwl_mvm_add_int_sta_common: ADD_STA with CMD_WANT_SKB-like status. // mvm/sta.c:1693-1741
        let n = encode_add_sta_aux(sta_id, queue, &mut b).map_err(|e| self.rt_fail("aux_sta", e.into()))?;
        let rx = self.rt_cmd("aux_sta", LEGACY_GROUP, ADD_STA, &[Chunk { data: &b[..n], nocopy: false }])?;
        let status = decode_add_sta_status(&rx.payload).map_err(|e| self.rt_fail("aux_sta", RtWhy::Decode(e)))?;
        if !add_sta_ok(status) {
            return Err(self.rt_fail("aux_sta", RtWhy::Status(status))); // "Add internal station failed, status=0x%x"
        }
        self.rt.aux_sta_id = sta_id;
        self.log(format_args!("[IWL-RT-CMD aux_sta ok sta={sta_id} queue={queue}]"));
        Ok(())
    }

    /// `iwl_mvm_get_current_regdomain` -> `iwl_mvm_update_mcc("ZZ", MCC_SOURCE_GET_CURRENT)`: the firmware's current
    /// regulatory domain and which NVM channels it allows. // mvm/nvm.c (iwl_mvm_init_mcc, iwl_mvm_update_mcc)
    fn rt_mcc(&mut self) -> Result<(), DriverError> {
        let mut b = [0u8; MCC_UPDATE_CMD_LEN];
        let n = encode_mcc_update(*b"ZZ", MCC_SOURCE_GET_CURRENT, &mut b).map_err(|e| self.rt_fail("mcc", e.into()))?;
        let rx = self.rt_cmd("mcc", LEGACY_GROUP, MCC_UPDATE_CMD, &[Chunk { data: &b[..n], nocopy: false }])?;
        let r = decode_mcc_resp_v3(&rx.payload).map_err(|e| self.rt_fail("mcc", RtWhy::Decode(e)))?;
        let mut scan: Vec<u8> = Vec::new();
        let mut valid = 0usize;
        for (i, c) in self.channels.iter().enumerate() {
            if r.channel_flags(i).is_some_and(|f| f & NVM_CHANNEL_VALID as u16 != 0) {
                valid += 1;
                scan.push(c.number as u8);
            }
        }
        let max = self.fw.info.n_scan_channels as usize;
        if scan.len() > max {
            scan.truncate(max); // WARN_ON(num_channels > n_scan_channels), mvm/scan.c:1168-1171
        }
        self.rt.mcc = r.mcc;
        self.rt.scan_channels = scan;
        self.log(format_args!(
            "[IWL-RT-CMD mcc ok status={:#x} mcc={}{} n_channels={} valid={} source={:#x}]",
            r.status,
            (r.mcc >> 8) as u8 as char,
            (r.mcc & 0xff) as u8 as char,
            r.n_channels,
            valid,
            r.source_id
        ));
        Ok(())
    }

    /// `iwl_mvm_config_scan` -> `iwl_mvm_legacy_config_scan` (v1 layout). // mvm/scan.c:1156-1246
    fn rt_scan_config(&mut self) -> Result<(), DriverError> {
        let n_scan = self.fw.info.n_scan_channels as usize;
        let mac = self.mac.unwrap_or([0; 6]);
        let cfg = ScanConfig { tx_chains: self.valid_tx_ant as u32, rx_chains: self.valid_rx_ant as u32, mac_addr: mac, bcast_sta_id: self.rt.aux_sta_id, channels: &self.rt.scan_channels, n_scan_channels: n_scan };
        let mut buf = vec![0u8; SCAN_CONFIG_V1_HDR_LEN + n_scan];
        let n = match encode_scan_config_v1(&cfg, &mut buf) {
            Ok(n) => n,
            Err(e) => return Err(self.rt_fail("scan_cfg", e.into())),
        };
        self.rt_cmd("scan_cfg", LONG_GROUP, SCAN_CFG_CMD, &[Chunk { data: &buf[..n], nocopy: true }])?;
        self.log(format_args!("[IWL-RT-CMD scan_cfg ok channels={} n_scan_channels={n_scan}]", self.rt.scan_channels.len()));
        Ok(())
    }
}
