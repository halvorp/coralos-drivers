// SPDX-License-Identifier: GPL-2.0-only
//! The driver: power the card, upload the INIT image, wait for ALIVE, run the MVM INIT sequence
//! (`iwl_run_init_mvm_ucode`: NVM, antennas, PHY configuration, calibration), and keep what it learned.
//! The sequence itself is the ported state machine in `iwlwifi-mvm-init-core`; this file performs each
//! [`Action`] the machine asks for against the transport and feeds the outcome back as an [`Event`].
//!
//! Ported from Linux `mvm/fw.c:286-330` (`iwl_mvm_load_ucode_wait_alive`), `:701-823`
//! (`iwl_run_init_mvm_ucode`), `mvm/nvm.c:306-398` (`iwl_nvm_init`), `mvm/ops.c` (`iwl_mvm_start_get_nvm`).
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH.
use crate::fwfile::{FwError, FwImage};
use crate::host::{say, Host};
use crate::transport::{Fault, Rx, Transport, TransportError};
use alloc::vec;
use alloc::vec::Vec;
use iwlwifi_fw_alive_core::alive::{parse_alive_v3, AliveError, AliveV3, ErrorTablePtrs, LEGACY_GROUP, UCODE_ALIVE_NTFY};
use iwlwifi_fw_alive_core::dump::{AliveTimeoutMarker, FwErrorMarker, HwErrorValueMarker, Image, ALIVE_TIMEOUT_PRPH_REGS};
use iwlwifi_fw_alive_core::error_table::{is_hw_error_value, lmac_table_base, LmacErrorTable, UmacErrorTable, LMAC_ERROR_TABLE_DWORDS, UMAC_ERROR_TABLE_DWORDS};
use iwlwifi_fw_alive_core::packet::RxPacket;
use iwlwifi_fw_image_core::cmdver::IWL_FW_CMD_VER_UNKNOWN;
use iwlwifi_fw_image_core::sections::UcodeType;
use iwlwifi_fw_tlv_core::params::phy_valid_ant;
use iwlwifi_mvm_init_core::encode::{encode_nvm_read, encode_phy_cfg_v1, encode_tx_ant_cfg};
use iwlwifi_mvm_init_core::ids::*;
use iwlwifi_mvm_init_core::machine::{Action, Config, Event, Failure, Machine, NvmType, SendStatus, State};
use iwlwifi_nvm_parse_core::{channels, mac_from_csr, parse_ext, NvmChannel, NvmData, NvmError, CSR_MAC_ADDR0_OTP, CSR_MAC_ADDR0_STRAP, CSR_MAC_ADDR1_OTP, CSR_MAC_ADDR1_STRAP};
use iwlwifi_pcie_nic_core::bus::{NicFault, NicIo};
use iwlwifi_pcie_nic_core::load::LoadReport;
use iwlwifi_pcie_ring_core::tx::Chunk;
use iwlwifi_pcie_trans_core::io::TransIo;

/// `trans->mac_cfg->base->eeprom_size` = `OTP_LOW_IMAGE_SIZE_32K`: 32 * 512 * sizeof(u16). // cfg/9000.c:28; iwl-config.h:316
pub const EEPROM_SIZE: u32 = 32 * 512 * 2;
/// `trans->mac_cfg->base->nvm_hw_section_num`. // cfg/9000.c:46
pub const NVM_HW_SECTION_NUM: u16 = 10; // cfg/9000.c:46
/// The RF-kill switch is in KILL: the INIT image ran as far as the check and stopped. // mvm/fw.c:761-766
pub const IWL_ALIVE_NOTIF_VER_FALLBACK: u8 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverError {
    Fw(FwError),
    Transport(TransportError),
    /// The MVM INIT state machine ended in a named failure.
    Init(Failure),
    /// ALIVE arrived with a layout this driver does not decode.
    Alive(AliveError),
    Nvm(NvmError),
    /// The firmware sent a status other than 0xCAFE.
    AliveStatus(u16),
    /// The NIC-access handshake timed out earlier and the card is latched dead (`STATUS_TRANS_DEAD`, trans.c:2339):
    /// `stage` is the first bring-up stage after which the latch was seen. CoralOS addition: Linux's callers see the
    /// -EIO of the access that failed; this driver reads 0x5a5a5a5a as a value, so the latch is checked by name.
    NicFault { stage: &'static str, gp_cntrl: u32 },
}

impl DriverError {
    pub fn name(&self) -> &'static str {
        match self {
            DriverError::Fw(e) => e.name(),
            DriverError::Transport(e) => e.name(),
            DriverError::Init(_) => "IWL-INIT",
            DriverError::Alive(_) | DriverError::AliveStatus(_) => "IWL-ALIVE",
            DriverError::Nvm(_) => "IWL-NVM",
            DriverError::NicFault { .. } => "IWL-NIC-FAULT",
        }
    }
}

impl From<TransportError> for DriverError {
    fn from(e: TransportError) -> Self {
        DriverError::Transport(e)
    }
}
impl From<FwError> for DriverError {
    fn from(e: FwError) -> Self {
        DriverError::Fw(e)
    }
}

/// What the INIT run learned about the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    pub hw_rev: u32,
    pub alive: AliveV3,
    pub nvm: NvmData,
    pub mac: [u8; 6],
    pub phy_db_entries: u32,
    pub nvm_bytes_read: u32,
    pub rfkill: bool,
    pub fw_chunks: usize,
    pub fw_bytes: usize,
}

pub struct Driver<H: Host> {
    pub t: Transport<H>,
    pub fw: FwImage,
    pub alive: Option<AliveV3>,
    pub error_tables: Option<ErrorTablePtrs>,
    pub nvm: Option<NvmData>,
    pub mac: Option<[u8; 6]>,
    pub channels: Vec<NvmChannel>,
    nvm_sections: Vec<Option<Vec<u8>>>,
}

/// The bring-up stage an [`Action`] belongs to, for `[IWL-NIC-FAULT stage=…]`.
fn stage_of(a: &Action) -> &'static str {
    match a {
        Action::StartFw => "start_fw",
        Action::WaitAlive { .. } => "alive",
        Action::ReadNvmChunk { .. } => "nvm-read",
        Action::CheckRfkill => "rfkill-check",
        Action::SendTxAntCfg { .. } => "tx-ant-cfg",
        Action::SendPhyCfg(_) => "phy-cfg",
        Action::WaitCalib { .. } | Action::Pending => "calib",
        Action::Done | Action::Failed(_) => "end",
    }
}

fn status_of(r: Result<Rx, TransportError>) -> SendStatus {
    match r {
        Ok(_) => SendStatus::Ok,
        Err(TransportError::RfKill) => SendStatus::Rfkill, // mvm/utils.c:33-49
        Err(e) => SendStatus::Err(e.errno()),
    }
}

impl<H: Host> Driver<H> {
    /// Verify and parse the firmware against its baked digest, and take the card. Nothing touches the
    /// hardware yet.
    pub fn new(host: H, firmware: Vec<u8>, digest: &[u8; 32]) -> Result<Driver<H>, DriverError> {
        let fw = FwImage::load(firmware, digest)?;
        let mut nvm_sections = Vec::new();
        nvm_sections.resize_with(NVM_MAX_NUM_SECTIONS as usize, || None);
        Ok(Driver { t: Transport::new(host), fw, alive: None, error_tables: None, nvm: None, mac: None, channels: Vec::new(), nvm_sections })
    }

    fn log(&mut self, args: core::fmt::Arguments<'_>) {
        say(self.t.host(), args);
    }

    /// The INIT image's `Config` for the machine. // mvm/fw.c:534-577, :701-823
    fn init_config(&self) -> Config {
        let phy = self.fw.info.phy_config.unwrap_or(0);
        let (tx, rx) = phy_valid_ant(phy);
        let cv = self.fw.cmd_versions();
        let phy_cfg_ver = cv.cmd_ver(0, PHY_CONFIGURATION_CMD, IWL_FW_CMD_VER_UNKNOWN); // mvm/fw.c:566-567
        Config {
            eeprom_size: EEPROM_SIZE,
            nvm_hw_section_num: NVM_HW_SECTION_NUM,
            nvm_type: NvmType::NvmExt, // cfg/rf-jf.c:79
            nvm_already_loaded: false,
            fw_phy_config: phy,
            valid_tx_ant: tx,
            valid_rx_ant: rx,
            extra_phy_cfg_flags: 0, // iwl9000_mac_cfg sets none, cfg/9000.c:70-74
            flow_trigger: self.fw.init_calib.flow_trigger,
            event_trigger: self.fw.init_calib.event_trigger,
            phy_cfg_cmd_v3: phy_cfg_ver >= 3,
        }
    }

    /// `iwl_run_init_mvm_ucode` with `iwl_trans_start_hw` before it: the whole of M1 and M2. On a failure
    /// that left the card faulted, the firmware's error table is read and logged (D1 of the readiness plan:
    /// `[IWL-FW-ERROR …]`, the single most useful line when a firmware asserts).
    pub fn bring_up(&mut self) -> Result<InitReport, DriverError> {
        let r = self.bring_up_inner();
        if r.is_err() && self.t.fault.is_some() {
            self.dump_fw_error(Image::Init);
        }
        r
    }

    /// If the NIC-access handshake has failed, say so by name and fail `stage`: a latched fault must never hide
    /// behind a green marker. `[IWL-NIC-FAULT stage=<stage> gp_cntrl=0x........]`
    pub fn nic_fault_check(&mut self, stage: &'static str) -> Result<(), DriverError> {
        match self.t.nic.fault() {
            None => Ok(()),
            Some(NicFault::AccessTimeout { gp_cntrl }) => {
                self.log(format_args!("[IWL-NIC-FAULT stage={stage} gp_cntrl={gp_cntrl:#010x}]"));
                Err(DriverError::NicFault { stage, gp_cntrl })
            }
        }
    }

    /// `iwl_fwrt_dump_lmac_error_log` and `_umac_`: read the error tables the ALIVE pointed at and log them.
    /// // fw/dump.c:146-231
    pub fn dump_fw_error(&mut self, image: Image) {
        let Some(p) = self.error_tables else {
            self.log(format_args!("[IWL-FW-ERROR img={image:?} no ALIVE yet: no error-table pointer]"));
            return;
        };
        let init = image == Image::Init;
        let base = match lmac_table_base(p.lmac, init, self.fw.info.init_errlog_ptr, self.fw.info.inst_errlog_ptr) {
            Ok(b) => b,
            Err(e) => {
                self.log(format_args!("[IWL-FW-ERROR img={image:?} {e:?}]"));
                return;
            }
        };
        let mut l = [0u32; LMAC_ERROR_TABLE_DWORDS];
        if self.t.nic.read_mem(base, &mut l).is_err() {
            self.log(format_args!("[IWL-FW-ERROR img={image:?} SRAM unreadable at {base:#x}]"));
            return;
        }
        if is_hw_error_value(l[0]) {
            self.log(format_args!("{}", HwErrorValueMarker { image, base, value: l[0] }));
            return;
        }
        let lmac = LmacErrorTable(l);
        let umac = p.umac.and_then(|a| {
            let mut u = [0u32; UMAC_ERROR_TABLE_DWORDS];
            self.t.nic.read_mem(a, &mut u).ok().map(|_| UmacErrorTable(u))
        });
        self.log(format_args!("{}", FwErrorMarker { image, lmac: &lmac, umac: umac.as_ref() }));
    }

    fn bring_up_inner(&mut self) -> Result<InitReport, DriverError> {
        let kill = self.t.start_hw()?;
        self.nic_fault_check("start_hw")?;
        let (hw_rev, name) = (self.t.hw_rev, if self.t.hw_rev & 0xF == 1 { 'B' } else { 'C' });
        self.log(format_args!("[IWL-HW-READY hw_rev={hw_rev:#010x} step={name} rfkill={}]", if kill { "ON" } else { "off" }));
        let cfg = self.init_config();
        let mut m = Machine::new(cfg);
        let mut scratch = vec![0u8; EEPROM_SIZE as usize];
        let mut loaded = LoadReport::default();
        let mut calib_deadline: Option<u64> = None;
        let mut reply: Vec<u8>;
        let mut step = m.start();
        loop {
            // Apply the step's NVM bookkeeping before acting on it. // mvm/nvm.c:131-137, :331-345
            if let Some(c) = step.chunk {
                scratch[c.offset as usize..c.offset as usize + c.data.len()].copy_from_slice(c.data);
            }
            if let Some((section, len)) = step.section_complete {
                self.nvm_sections[section as usize] = Some(scratch[..len as usize].to_vec());
            }
            if let Some(section) = step.section_skipped {
                self.nvm_sections[section as usize] = None;
            }
            let action = step.action;
            let ev = match action {
                Action::StartFw => {
                    let secs = self.fw.sections(UcodeType::Init)?;
                    let phy = self.fw.info.phy_config.unwrap_or(0);
                    // The firmware file is borrowed through `secs` while the transport (a different field) works.
                    let r = self.t.start_fw(&secs, phy, true);
                    match r {
                        Ok(rep) => {
                            self.log(format_args!(
                                "[IWL-FW-LOADED init cpu1={} cpu2={} chunks={} bytes={}]",
                                rep.cpu1_sections, rep.cpu2_sections, rep.chunks, rep.bytes
                            ));
                            loaded = rep;
                            Event::FwStarted(Ok(()))
                        }
                        Err(e) => {
                            self.log(format_args!("[IWL-FW-LOADED FAIL {} {:?}]", e.name(), e));
                            Event::FwStarted(Err(e.errno()))
                        }
                    }
                }
                Action::WaitAlive { timeout_ms } => self.wait_alive(timeout_ms),
                Action::ReadNvmChunk { section, offset, length } => {
                    let mut p = [0u8; NVM_ACCESS_CMD_LEN];
                    encode_nvm_read(section, offset, length, &mut p).expect("8 bytes fit");
                    match self.t.send_cmd_in_rfkill(0, NVM_ACCESS_CMD, 0, &[Chunk { data: &p, nocopy: false }]) {
                        Ok(rx) => {
                            reply = rx.payload;
                            Event::NvmReply(&reply[..])
                        }
                        Err(e) => Event::CmdFailed(e.errno()),
                    }
                }
                Action::CheckRfkill => {
                    if self.nvm.is_none() && m.state() == State::RfkillCheck {
                        self.parse_nvm(&mut m)?;
                    }
                    Event::Rfkill(self.t.check_hw_rf_kill())
                }
                Action::SendTxAntCfg { valid_tx_ant } => {
                    let mut p = [0u8; TX_ANT_CFG_CMD_LEN];
                    encode_tx_ant_cfg(valid_tx_ant, &mut p).expect("4 bytes fit");
                    Event::CmdSent(status_of(self.t.send_cmd(0, TX_ANT_CONFIGURATION_CMD, 0, &[Chunk { data: &p, nocopy: false }])))
                }
                Action::SendPhyCfg(c) => {
                    let mut p = [0u8; PHY_CFG_CMD_V1_LEN];
                    encode_phy_cfg_v1(&c, &mut p).expect("12 bytes fit");
                    Event::CmdSent(status_of(self.t.send_cmd(0, PHY_CONFIGURATION_CMD, 0, &[Chunk { data: &p, nocopy: false }])))
                }
                Action::WaitCalib { .. } | Action::Pending => {
                    // ONE budget for the whole wait, as `iwl_wait_notification`'s single timeout; `Pending` is
                    // the machine absorbing a PHY_DB entry and asking for the next. // mvm/fw.c:790
                    let now = self.t.host().now_ms();
                    let budget = match action {
                        Action::WaitCalib { timeout_ms } => timeout_ms,
                        _ => MVM_UCODE_CALIB_TIMEOUT_MS,
                    };
                    let deadline = *calib_deadline.get_or_insert(now + budget as u64);
                    match self.wait_calib(deadline.saturating_sub(now) as u32) {
                        Ok(rx) => {
                            reply = rx.payload;
                            Event::Notification { cmd: rx.cmd, payload: &reply[..] }
                        }
                        Err(TransportError::Timeout { .. }) => Event::WaitTimedOut,
                        Err(_) => Event::WaitAborted,
                    }
                }
                Action::Done => break,
                Action::Failed(f) => {
                    self.log(format_args!("[IWL-INIT FAIL {f:?}]"));
                    return Err(DriverError::Init(f));
                }
            };
            self.nic_fault_check(stage_of(&action))?;
            step = m.step(ev);
        }
        self.nic_fault_check("init-complete")?;
        let nvm = self.nvm.ok_or(DriverError::Init(Failure::UnexpectedEvent { state: m.state() }))?;
        let mac = self.mac.unwrap_or([0; 6]);
        let alive = self.alive.ok_or(DriverError::AliveStatus(0))?;
        let nvm_bytes_read: u32 = m.nvm_sections().iter().flatten().sum();
        let phydb = m.phy_db_entries();
        self.log(format_args!("[IWL-INIT-COMPLETE phydb={phydb} nvm_bytes={nvm_bytes_read} rfkill={}]", if m.rfkill() { "ON" } else { "off" }));
        Ok(InitReport { hw_rev, alive, nvm, mac, phy_db_entries: phydb, nvm_bytes_read, rfkill: m.rfkill(), fw_chunks: loaded.chunks, fw_bytes: loaded.bytes })
    }

    /// Wait for the ALIVE notification, decode it, and start the TX scheduler. // mvm/fw.c:286-330
    fn wait_alive(&mut self, timeout_ms: u32) -> Event<'static> {
        let rx = match self.t.wait_notif(timeout_ms, |r| r.group == LEGACY_GROUP && r.cmd == UCODE_ALIVE_NTFY && r.is_notification()) {
            Ok(rx) => rx,
            Err(TransportError::Timeout { .. }) => {
                let csr_int = self.t.nic.csr_read(iwlwifi_pcie_trans_core::regs::CSR_INT);
                self.log(format_args!("[IWL-ALIVE FAIL timeout={timeout_ms}ms csr_int={csr_int:#010x}]"));
                // iwl_mvm_load_ucode_wait_alive's timeout dump (mvm/fw.c:305-312, fw/dump.c:233-262)
                let mut values = [0u32; 8];
                for (v, (_, reg)) in values.iter_mut().zip(ALIVE_TIMEOUT_PRPH_REGS) {
                    *v = self.t.nic.prph_read(reg);
                }
                self.log(format_args!("{}", AliveTimeoutMarker { image: Image::Init, values }));
                return Event::WaitTimedOut;
            }
            Err(e) => {
                self.log(format_args!("[IWL-ALIVE FAIL {} {:?}]", e.name(), e));
                return Event::WaitAborted;
            }
        };
        let notif_ver = self.fw.cmd_versions().notif_ver(LEGACY_GROUP, UCODE_ALIVE_NTFY, IWL_ALIVE_NOTIF_VER_FALLBACK); // mvm/fw.c:112
        let pkt = RxPacket { len_n_flags: 0, cmd: rx.cmd, group_id: rx.group, sequence: rx.sequence, payload: &rx.payload };
        let a = match parse_alive_v3(notif_ver, &pkt) {
            Ok(a) => a,
            Err(e) => {
                self.log(format_args!("[IWL-ALIVE FAIL layout {e:?}]"));
                return Event::AliveInvalid;
            }
        };
        self.error_tables = Some(a.error_tables());
        self.alive = Some(a);
        if !a.is_ok() {
            self.log(format_args!("[IWL-ALIVE FAIL status={:#06x}]", a.status));
            return Event::AliveInvalid; // mvm/fw.c:220-229
        }
        self.log(format_args!(
            "[IWL-ALIVE init status={:#06x} fw={}.{:x} lmac_err={:#x} umac_err={:#x}]",
            a.status,
            a.lmac.ucode_major,
            a.lmac.ucode_minor,
            a.lmac.dbg_ptrs.error_event_table_ptr,
            a.umac.error_info_addr
        ));
        match self.t.fw_alive() {
            Ok(base) => {
                if base != a.lmac.dbg_ptrs.scd_base_ptr {
                    // "SCD addresses mismatch" is a WARN in Linux (mvm/fw.c:317-321); the register wins.
                    self.log(format_args!("[IWL-SCD-BASE mismatch reg={base:#x} alive={:#x}]", a.lmac.dbg_ptrs.scd_base_ptr));
                }
                Event::AliveOk
            }
            Err(e) => {
                self.log(format_args!("[IWL-ALIVE FAIL fw_alive {} {:?}]", e.name(), e));
                Event::AliveInvalid
            }
        }
    }

    /// One PHY_DB or INIT_COMPLETE notification. // mvm/fw.c:240-259
    fn wait_calib(&mut self, timeout_ms: u32) -> Result<Rx, TransportError> {
        self.t.wait_notif(timeout_ms, |r| r.group == LEGACY_GROUP && r.is_notification() && (r.cmd == CALIB_RES_NOTIF_PHY_DB || r.cmd == INIT_COMPLETE_NOTIF))
    }

    /// `iwl_parse_nvm_sections` for the sections just read, then the antenna masks. // mvm/nvm.c:215-290
    fn parse_nvm(&mut self, m: &mut Machine) -> Result<(), DriverError> {
        let sec = |me: &Self, s: u16| me.nvm_sections[s as usize].clone().unwrap_or_default();
        let (sw, reg, phy) = (sec(self, NVM_SECTION_TYPE_SW), sec(self, NVM_SECTION_TYPE_REGULATORY), sec(self, NVM_SECTION_TYPE_PHY_SKU));
        let data = parse_ext(&sw, &reg, &phy).map_err(DriverError::Nvm)?;
        let strap0 = self.t.nic.csr_read(CSR_MAC_ADDR0_STRAP);
        let strap1 = self.t.nic.csr_read(CSR_MAC_ADDR1_STRAP);
        let otp0 = self.t.nic.csr_read(CSR_MAC_ADDR0_OTP);
        let otp1 = self.t.nic.csr_read(CSR_MAC_ADDR1_OTP);
        let mac = mac_from_csr(strap0, strap1, otp0, otp1).map_err(DriverError::Nvm)?;
        let (fw_tx, fw_rx) = phy_valid_ant(self.fw.info.phy_config.unwrap_or(0));
        // iwl_mvm_get_valid_tx_ant / _rx_ant: the firmware's mask ANDed with the NVM's when it names one.
        // // mvm/mvm.h:1757-1765
        let tx = if data.radio.valid_tx_ant != 0 { fw_tx & data.radio.valid_tx_ant } else { fw_tx };
        let rx = if data.radio.valid_rx_ant != 0 { fw_rx & data.radio.valid_rx_ant } else { fw_rx };
        m.set_valid_antennas(tx, rx);
        self.channels = channels(&reg, data.band_52ghz(), data.lar_enabled).collect();
        self.log(format_args!(
            "[IWL-NVM ver={:#x} sku={:#x} tx_ant={tx:#x} rx_ant={rx:#x} lar={} mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} channels={}]",
            data.nvm_version,
            data.sku,
            data.lar_enabled as u8,
            mac[0],
            mac[1],
            mac[2],
            mac[3],
            mac[4],
            mac[5],
            self.channels.len()
        ));
        self.nvm = Some(data);
        self.mac = Some(mac);
        Ok(())
    }

    /// The fault the card reported, if any (for the log line and the wifi-manager `E_FAULT` code).
    pub fn fault(&self) -> Option<Fault> {
        self.t.fault
    }
}
