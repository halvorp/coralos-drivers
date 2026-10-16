// SPDX-License-Identifier: GPL-2.0-only
//! A pure state machine for `iwl_run_init_mvm_ucode` (mvm/fw.c:701-823), non-unified image. The
//! caller performs each [`Action`] (start the firmware, send a host command, ask the rfkill state,
//! arm a timeout) and reports what happened as an [`Event`]; the machine never touches hardware.
//!
//! Sequence, all exact to Linux unless marked CoralOS:
//! 1. `iwl_trans_start_fw` (mvm/fw.c:316) then the ALIVE wait, 2000 ms (mvm/fw.c:328-329).
//! 2. `iwl_nvm_init` (mvm/nvm.c:306-398) unless the NVM is already loaded (mvm/fw.c:740).
//! 3. `iwl_mvm_is_radio_hw_killed` (mvm/fw.c:763): killed -> done, rfkill, no PHY activity.
//! 4. `TX_ANT_CONFIGURATION_CMD` (mvm/fw.c:772), `PHY_CONFIGURATION_CMD` (mvm/fw.c:776).
//! 5. Count `CALIB_RES_NOTIF_PHY_DB` entries until `INIT_COMPLETE_NOTIF`, 2000 ms (mvm/fw.c:788).
//!
//! The calibration wait is registered BEFORE the firmware is started (mvm/fw.c:717-722), so
//! notifications are accepted in every state after the start, as Linux's notif-wait would.

use crate::decode::{decode_calib_res_phy_db, decode_nvm_access_resp, DecodeError};
use crate::encode::PhyCfgV1;
use crate::ids::*;

/// `enum iwl_nvm_type` (iwl-config.h:68-70) — selects which sections `iwl_parse_nvm_sections` needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmType {
    /// `IWL_NVM` (iwl-config.h:68).
    Nvm,
    /// `IWL_NVM_EXT` (iwl-config.h:69).
    NvmExt,
    /// `IWL_NVM_SDP` (iwl-config.h:70).
    NvmSdp,
}

/// Everything the machine reads from the driver's configuration and the parsed firmware file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// `trans->mac_cfg->base->eeprom_size`. The caller's NVM scratch buffer must be this large
    /// (Linux `kmalloc`s it, nvm.c:319).
    pub eeprom_size: u32,
    /// `trans->mac_cfg->base->nvm_hw_section_num`.
    pub nvm_hw_section_num: u16,
    /// `trans->cfg->nvm_type`.
    pub nvm_type: NvmType,
    /// `mvm->nvm_data != NULL`: Linux skips `iwl_nvm_init` (mvm/fw.c:740).
    pub nvm_already_loaded: bool,
    /// PHY_SKU TLV value, `mvm->fw->phy_config` (iwl-drv.c:1023).
    pub fw_phy_config: u32,
    /// `iwl_mvm_get_valid_tx_ant` (mvm/mvm.h:1757).
    pub valid_tx_ant: u8,
    /// `iwl_mvm_get_valid_rx_ant`.
    pub valid_rx_ant: u8,
    /// `mac_cfg->extra_phy_cfg_flags` (mvm/fw.c:559-560).
    pub extra_phy_cfg_flags: u32,
    /// DEF_CALIB TLV for the INIT image: flow trigger (iwl-drv.c:532).
    pub flow_trigger: u32,
    /// DEF_CALIB TLV for the INIT image: event trigger (iwl-drv.c:534).
    pub event_trigger: u32,
    /// `iwl_fw_lookup_cmd_ver(PHY_CONFIGURATION_CMD) >= 3` (mvm/fw.c:567-570). CoralOS only encodes
    /// the v1 layout, so this refuses with a name rather than send a short command.
    pub phy_cfg_cmd_v3: bool,
}

/// Result of a synchronous host command WITHOUT `CMD_WANT_SKB`, as `iwl_mvm_send_cmd` reports it
/// (mvm/utils.c:33-49).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendStatus {
    Ok,
    /// `-ERFKILL`: silently turned into success (mvm/utils.c:47-48).
    Rfkill,
    /// `-EHOSTDOWN`: silently turned into success (mvm/utils.c:47-48).
    HostDown,
    /// Any other negative errno, carried opaque.
    Err(i32),
}

/// What the caller reports back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<'a> {
    /// `iwl_trans_start_fw` returned (`Err` carries its errno).
    FwStarted(Result<(), i32>),
    /// `iwl_alive_fn` accepted the ALIVE notification (`alive_data.valid`, mvm/fw.c:396).
    AliveOk,
    /// ALIVE arrived but `alive_data.valid` is false (mvm/fw.c:396-400).
    AliveInvalid,
    /// The armed wait expired (`-ETIMEDOUT`, fw/notif-wait.c:131-132).
    WaitTimedOut,
    /// The armed wait was aborted (`-EIO`, fw/notif-wait.c:127-128).
    WaitAborted,
    /// The `NVM_ACCESS_CMD` response payload (`pkt->data`).
    NvmReply(&'a [u8]),
    /// The `NVM_ACCESS_CMD` (a `CMD_WANT_SKB` command: nothing is swallowed, mvm/utils.c:40-41)
    /// failed to send; the errno is opaque.
    CmdFailed(i32),
    /// The answer to [`Action::CheckRfkill`]: `iwl_mvm_is_radio_hw_killed` (mvm/mvm.h:1324-1327).
    Rfkill(bool),
    /// The answer to a [`Action::SendTxAntCfg`] / [`Action::SendPhyCfg`].
    CmdSent(SendStatus),
    /// A firmware notification: command id and payload (`pkt->hdr.cmd`, `pkt->data`).
    Notification { cmd: u8, payload: &'a [u8] },
}

/// What the caller must do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// `iwl_trans_start_fw(INIT, run_in_rfkill = true)` (mvm/fw.c:290-291, :316).
    StartFw,
    /// Arm a wait for the ALIVE notification.
    WaitAlive { timeout_ms: u32 },
    /// Send `NVM_ACCESS_CMD` read (`encode_nvm_read`), WANT_SKB | SEND_IN_RFKILL (mvm/nvm.c:83-87).
    ReadNvmChunk { section: u16, offset: u16, length: u16 },
    /// Ask [`Event::Rfkill`].
    CheckRfkill,
    /// Send `TX_ANT_CONFIGURATION_CMD` with `encode_tx_ant_cfg`, flags 0.
    SendTxAntCfg { valid_tx_ant: u8 },
    /// Send `PHY_CONFIGURATION_CMD` with `encode_phy_cfg_v1`, flags 0.
    SendPhyCfg(PhyCfgV1),
    /// Arm the calibration wait (INIT_COMPLETE_NOTIF).
    WaitCalib { timeout_ms: u32 },
    /// Nothing new: keep waiting for whatever was last asked.
    Pending,
    /// Terminal: the flow ended without failure (see [`Machine::rfkill`]).
    Done,
    /// Terminal: the flow failed.
    Failed(Failure),
}

/// Why the flow stopped. Names the Linux errno or the CoralOS bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// `iwl_trans_start_fw` failed (mvm/fw.c:316-322).
    StartFw { errno: i32 },
    /// `-ETIMEDOUT` waiting for ALIVE (mvm/fw.c:328-344).
    AliveTimeout,
    /// `-EIO`, the ALIVE wait was aborted (fw/notif-wait.c:127-128).
    AliveAborted,
    /// `-EIO`, "Loaded ucode is not valid!" (mvm/fw.c:396-400).
    AliveInvalid,
    /// `-EINVAL`, `nvm_hw_section_num >= NVM_MAX_NUM_SECTIONS` (mvm/nvm.c:312-313).
    NvmHwSectionOutOfRange { section_num: u16 },
    /// An `NVM_ACCESS_CMD` send failed (mvm/nvm.c:93-95 returns it; mvm/nvm.c:200-205 propagates).
    NvmSendFailed { section: u16, offset: u16, errno: i32 },
    /// CoralOS bound: the response did not decode (Linux casts `pkt->data` unchecked, mvm/nvm.c:100).
    NvmRespMalformed { section: u16, offset: u16, error: DecodeError },
    /// `-EINVAL`, response offset differs from the request (mvm/nvm.c:129-134).
    NvmOffsetMismatch { requested: u16, got: u16 },
    /// CoralOS bound: the firmware returned more bytes than requested (Linux `memcpy`s them,
    /// mvm/nvm.c:136-138, and would overflow its buffer).
    NvmChunkOverrun { requested: u16, got: u16 },
    /// `-ENOBUFS`, "EEPROM size is too small for NVM" (mvm/nvm.c:193-197).
    NvmEepromTooSmall { size_read: u32, offset: u32, length: u16, eeprom_size: u32 },
    /// CoralOS bound: Linux's `u16 offset` (mvm/nvm.c:182) would wrap.
    NvmOffsetOverflow { offset: u32 },
    /// `iwl_parse_nvm_sections` returned NULL: `-ENODATA` (mvm/nvm.c:391-393).
    NvmSectionsMissing { what: MissingSections },
    /// `TX_ANT_CONFIGURATION_CMD` failed (mvm/fw.c:772-774).
    TxAntSendFailed { errno: i32 },
    /// `PHY_CONFIGURATION_CMD` failed (mvm/fw.c:776-781).
    PhyCfgSendFailed { errno: i32 },
    /// CoralOS bound: the firmware wants the v3 PHY_CONFIGURATION_CMD (mvm/fw.c:567-575).
    PhyCfgV3Unsupported,
    /// `-ETIMEDOUT`, calibrations did not complete and rfkill is not asserted (mvm/fw.c:788-798).
    CalibTimeout,
    /// `-EIO`, the calibration wait was aborted and rfkill is not asserted (mvm/fw.c:788-798).
    CalibAborted,
    /// A notification other than `CALIB_RES_NOTIF_PHY_DB` / `INIT_COMPLETE_NOTIF`: Linux
    /// `WARN_ON`s in `iwl_wait_phy_db_entry` (mvm/fw.c:253-255).
    UnexpectedNotification { cmd: u8 },
    /// The caller reported an event this state did not ask for.
    UnexpectedEvent { state: State },
}

/// Which sections `iwl_parse_nvm_sections` found absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingSections {
    /// `IWL_NVM`: SW or the HW section (mvm/nvm.c:227-231).
    SwOrHw,
    /// Family 8000+: SW or the regulatory section (mvm/nvm.c:240-244).
    SwOrRegulatory,
    /// Neither the HW section nor MAC_OVERRIDE (mvm/nvm.c:247-251).
    MacAddress,
    /// `IWL_NVM_EXT` without PHY_SKU (mvm/nvm.c:255-259).
    PhySku,
}

/// The machine's state, exposed for [`Failure::UnexpectedEvent`] and for the caller's logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Init,
    WaitFwStart,
    WaitAlive,
    NvmWaitReply,
    RfkillCheck,
    SendTxAnt,
    SendPhyCfg,
    WaitCalib,
    RfkillAfterWait,
    Done,
    Failed,
}

/// One chunk to copy into the caller's NVM scratch buffer at `offset` (`memcpy(data + offset, ...)`,
/// mvm/nvm.c:137). Borrowed from the event payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmChunk<'a> {
    pub section: u16,
    pub offset: u16,
    pub data: &'a [u8],
}

/// What one [`Machine::step`] produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step<'a> {
    pub action: Action,
    /// Bytes to place in the scratch buffer now.
    pub chunk: Option<NvmChunk<'a>>,
    /// `(section, length)`: the section is complete; copy `scratch[..length]` out now (Linux
    /// `kmemdup`s it before the next section overwrites the buffer, mvm/nvm.c:334).
    pub section_complete: Option<(u16, u32)>,
    /// The section answered an error status and is skipped (`-ENODATA` -> `continue`,
    /// mvm/nvm.c:325-329); discard any partial chunks.
    pub section_skipped: Option<u16>,
}

impl<'a> Step<'a> {
    fn act(action: Action) -> Self {
        Step { action, chunk: None, section_complete: None, section_skipped: None }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaitErr {
    Timeout,
    Aborted,
}

/// The `iwl_run_init_mvm_ucode` state machine.
pub struct Machine {
    cfg: Config,
    state: State,
    section: u16,
    offset: u32,
    size_read: u32,
    requested: u16,
    sections: [Option<u32>; NVM_MAX_NUM_SECTIONS as usize],
    phy_db_entries: u32,
    phy_db_rejected: u32,
    init_complete: bool,
    rfkill_safe_init_done: bool,
    rfkill: bool,
    otp_blank: bool,
    wait_err: Option<WaitErr>,
    failure: Option<Failure>,
}

impl Machine {
    pub fn new(cfg: Config) -> Self {
        Machine {
            cfg,
            state: State::Init,
            section: 0,
            offset: 0,
            size_read: 0,
            requested: 0,
            sections: [None; NVM_MAX_NUM_SECTIONS as usize],
            phy_db_entries: 0,
            phy_db_rejected: 0,
            init_complete: false,
            rfkill_safe_init_done: false,
            rfkill: false,
            otp_blank: false,
            wait_err: None,
            failure: None,
        }
    }

    /// The first action (`StartFw`). Call once.
    pub fn start(&mut self) -> Step<'static> {
        self.state = State::WaitFwStart;
        Step::act(Action::StartFw)
    }

    /// Replace the antenna masks the TX_ANT and PHY commands will carry. Linux computes them AFTER
    /// `iwl_nvm_init` (mvm/fw.c:772, :776): `iwl_mvm_get_valid_tx_ant` is the firmware's mask ANDed with
    /// the NVM's when the NVM names one (mvm/mvm.h:1757-1765), and the NVM is not known when the flow
    /// starts. Call it when the machine asks for `CheckRfkill` after the NVM walk, before answering.
    pub fn set_valid_antennas(&mut self, tx: u8, rx: u8) {
        self.cfg.valid_tx_ant = tx;
        self.cfg.valid_rx_ant = rx;
    }

    pub fn state(&self) -> State {
        self.state
    }
    pub fn failure(&self) -> Option<Failure> {
        self.failure
    }
    /// True if the flow ended at the rfkill check or because rfkill rose during calibration.
    pub fn rfkill(&self) -> bool {
        self.rfkill
    }
    /// `CALIB_RES_NOTIF_PHY_DB` entries `iwl_phy_db_set_section` accepted.
    pub fn phy_db_entries(&self) -> u32 {
        self.phy_db_entries
    }
    /// Entries `iwl_phy_db_set_section` refused: Linux `WARN_ON`s and keeps waiting (mvm/fw.c:258).
    pub fn phy_db_rejected(&self) -> u32 {
        self.phy_db_rejected
    }
    /// `mvm->rfkill_safe_init_done`: true only between the rfkill check and the end (mvm/fw.c:769,
    /// :805).
    pub fn rfkill_safe_init_done(&self) -> bool {
        self.rfkill_safe_init_done
    }
    /// Length of each NVM section read, indexed by section type; `None` = not read / skipped.
    pub fn nvm_sections(&self) -> &[Option<u32>; NVM_MAX_NUM_SECTIONS as usize] {
        &self.sections
    }
    /// "OTP is blank" (`size_read == 0`, mvm/nvm.c:377-378).
    pub fn otp_blank(&self) -> bool {
        self.otp_blank
    }

    fn fail<'a>(&mut self, f: Failure) -> Step<'a> {
        self.state = State::Failed;
        self.failure = Some(f);
        self.rfkill_safe_init_done = false; // mvm/fw.c:805
        Step::act(Action::Failed(f))
    }

    fn finish<'a>(&mut self, rfkill: bool) -> Step<'a> {
        self.state = State::Done;
        self.rfkill = rfkill;
        self.rfkill_safe_init_done = false; // mvm/fw.c:805
        Step::act(Action::Done)
    }

    fn unexpected<'a>(&mut self) -> Step<'a> {
        let state = self.state;
        self.fail(Failure::UnexpectedEvent { state })
    }

    /// Feed one event, get the next step.
    pub fn step<'a>(&mut self, ev: Event<'a>) -> Step<'a> {
        match self.state {
            State::Done => return Step::act(Action::Done),
            State::Failed => {
                let f = self.failure.unwrap_or(Failure::UnexpectedEvent { state: State::Failed });
                return Step::act(Action::Failed(f));
            }
            State::Init => return self.unexpected(),
            _ => {}
        }
        if let Event::Notification { cmd, payload } = ev {
            return self.on_notification(cmd, payload);
        }
        match (self.state, ev) {
            (State::WaitFwStart, Event::FwStarted(Ok(()))) => {
                self.state = State::WaitAlive;
                Step::act(Action::WaitAlive { timeout_ms: MVM_UCODE_ALIVE_TIMEOUT_MS })
            }
            (State::WaitFwStart, Event::FwStarted(Err(errno))) => self.fail(Failure::StartFw { errno }),
            (State::WaitAlive, Event::AliveOk) => self.after_alive(),
            (State::WaitAlive, Event::AliveInvalid) => self.fail(Failure::AliveInvalid),
            (State::WaitAlive, Event::WaitTimedOut) => self.fail(Failure::AliveTimeout),
            (State::WaitAlive, Event::WaitAborted) => self.fail(Failure::AliveAborted),
            (State::NvmWaitReply, Event::CmdFailed(errno)) => {
                let (section, offset) = (self.section, self.offset as u16);
                self.fail(Failure::NvmSendFailed { section, offset, errno })
            }
            (State::NvmWaitReply, Event::NvmReply(p)) => self.on_nvm_reply(p),
            (State::RfkillCheck, Event::Rfkill(true)) => self.finish(true),
            (State::RfkillCheck, Event::Rfkill(false)) => {
                self.rfkill_safe_init_done = true; // mvm/fw.c:769
                self.state = State::SendTxAnt;
                Step::act(Action::SendTxAntCfg { valid_tx_ant: self.cfg.valid_tx_ant })
            }
            (State::SendTxAnt, Event::CmdSent(SendStatus::Err(errno))) => {
                self.fail(Failure::TxAntSendFailed { errno })
            }
            (State::SendTxAnt, Event::CmdSent(_)) => self.send_phy_cfg(),
            (State::SendPhyCfg, Event::CmdSent(SendStatus::Err(errno))) => {
                self.fail(Failure::PhyCfgSendFailed { errno })
            }
            (State::SendPhyCfg, Event::CmdSent(_)) => self.after_phy_cfg(),
            (State::WaitCalib, Event::WaitTimedOut) => self.calib_wait_failed(WaitErr::Timeout),
            (State::WaitCalib, Event::WaitAborted) => self.calib_wait_failed(WaitErr::Aborted),
            (State::RfkillAfterWait, Event::Rfkill(true)) => self.finish(true), // mvm/fw.c:792-794
            (State::RfkillAfterWait, Event::Rfkill(false)) => {
                let f = match self.wait_err {
                    Some(WaitErr::Aborted) => Failure::CalibAborted,
                    _ => Failure::CalibTimeout,
                };
                self.fail(f)
            }
            _ => self.unexpected(),
        }
    }

    fn on_notification<'a>(&mut self, cmd: u8, payload: &'a [u8]) -> Step<'a> {
        if self.state == State::WaitFwStart {
            return self.unexpected(); // nothing runs before iwl_trans_start_fw returns
        }
        if cmd == CALIB_RES_NOTIF_PHY_DB {
            // mvm/fw.c:258: WARN_ON(iwl_phy_db_set_section()) and keep waiting (`return false`).
            match decode_calib_res_phy_db(payload) {
                Ok(_) => self.phy_db_entries += 1,
                Err(_) => self.phy_db_rejected += 1,
            }
        } else if cmd == INIT_COMPLETE_NOTIF {
            self.init_complete = true; // mvm/fw.c:254-255 `return true`
            if self.state == State::WaitCalib {
                return self.finish(false);
            }
        } else {
            return self.fail(Failure::UnexpectedNotification { cmd });
        }
        Step::act(Action::Pending)
    }

    fn after_alive<'a>(&mut self) -> Step<'a> {
        // The < IWL_DEVICE_FAMILY_8000 BT init (mvm/fw.c:733-737) is not taken on 9000.
        if self.cfg.nvm_already_loaded {
            return self.check_rfkill(); // mvm/fw.c:740
        }
        if self.cfg.nvm_hw_section_num >= NVM_MAX_NUM_SECTIONS {
            let section_num = self.cfg.nvm_hw_section_num;
            return self.fail(Failure::NvmHwSectionOutOfRange { section_num }); // nvm.c:312-313
        }
        self.section = 0;
        self.size_read = 0;
        self.request_chunk(0)
    }

    fn check_rfkill<'a>(&mut self) -> Step<'a> {
        self.state = State::RfkillCheck;
        Step::act(Action::CheckRfkill)
    }

    /// Top of the `while (ret == length)` loop (mvm/nvm.c:191-197).
    fn request_chunk<'a>(&mut self, offset: u32) -> Step<'a> {
        let length = IWL_NVM_DEFAULT_CHUNK_SIZE;
        self.offset = offset;
        let need = self.size_read as u64 + offset as u64 + length as u64;
        if need > self.cfg.eeprom_size as u64 {
            let (size_read, eeprom_size) = (self.size_read, self.cfg.eeprom_size);
            return self.fail(Failure::NvmEepromTooSmall { size_read, offset, length, eeprom_size });
        }
        if offset > u16::MAX as u32 {
            return self.fail(Failure::NvmOffsetOverflow { offset });
        }
        self.requested = length;
        self.state = State::NvmWaitReply;
        Step::act(Action::ReadNvmChunk { section: self.section, offset: offset as u16, length })
    }

    fn on_nvm_reply<'a>(&mut self, payload: &'a [u8]) -> Step<'a> {
        let (section, offset, length) = (self.section, self.offset as u16, self.requested);
        let resp = match decode_nvm_access_resp(payload) {
            Ok(r) => r,
            Err(error) => return self.fail(Failure::NvmRespMalformed { section, offset, error }),
        };
        let mut chunk = None;
        let n: u16;
        if resp.status != READ_NVM_CHUNK_SUCCEED {
            // mvm/nvm.c:105-127
            if offset != 0 && resp.status == READ_NVM_CHUNK_NOT_VALID_ADDRESS {
                n = 0; // the section ended on a 2K boundary: not an error
            } else {
                // -ENODATA: iwl_nvm_init skips the section (mvm/nvm.c:325-329).
                let mut s = self.next_section();
                s.section_skipped = Some(section);
                return s;
            }
        } else if resp.offset != offset {
            return self.fail(Failure::NvmOffsetMismatch { requested: offset, got: resp.offset });
        } else if resp.length > length {
            return self.fail(Failure::NvmChunkOverrun { requested: length, got: resp.length });
        } else {
            n = resp.length;
            chunk = Some(NvmChunk { section, offset, data: resp.data });
        }
        let next_offset = self.offset + n as u32;
        if n == length {
            let mut s = self.request_chunk(next_offset);
            s.chunk = chunk;
            return s;
        }
        // Section exhausted: `return offset` (mvm/nvm.c:213); iwl_nvm_init records it.
        self.sections[section as usize] = Some(next_offset);
        self.size_read += next_offset;
        let mut s = self.next_section();
        s.chunk = chunk;
        if self.state != State::Failed {
            s.section_complete = Some((section, next_offset));
        }
        s
    }

    /// Advance the `for (section...)` loop (mvm/nvm.c:323), or finish the NVM read.
    fn next_section<'a>(&mut self) -> Step<'a> {
        self.section += 1;
        if self.section < NVM_MAX_NUM_SECTIONS {
            return self.request_chunk(0);
        }
        self.otp_blank = self.size_read == 0; // mvm/nvm.c:377-378
        if let Some(what) = self.missing_sections() {
            return self.fail(Failure::NvmSectionsMissing { what }); // mvm/nvm.c:391-393
        }
        self.check_rfkill()
    }

    /// `iwl_parse_nvm_sections`'s presence checks (mvm/nvm.c:226-262). `.data != NULL` is "the
    /// section was read" — even a zero-length read (kmemdup of 0 bytes is non-NULL).
    fn missing_sections(&self) -> Option<MissingSections> {
        let has = |s: u16| self.sections[s as usize].is_some();
        let hw = has(self.cfg.nvm_hw_section_num);
        if self.cfg.nvm_type == NvmType::Nvm {
            if !has(NVM_SECTION_TYPE_SW) || !hw {
                return Some(MissingSections::SwOrHw);
            }
            return None;
        }
        let reg = if self.cfg.nvm_type == NvmType::NvmSdp {
            NVM_SECTION_TYPE_REGULATORY_SDP
        } else {
            NVM_SECTION_TYPE_REGULATORY
        };
        if !has(NVM_SECTION_TYPE_SW) || !has(reg) {
            return Some(MissingSections::SwOrRegulatory);
        }
        if !hw && !has(NVM_SECTION_TYPE_MAC_OVERRIDE) {
            return Some(MissingSections::MacAddress);
        }
        if self.cfg.nvm_type == NvmType::NvmExt && !has(NVM_SECTION_TYPE_PHY_SKU) {
            return Some(MissingSections::PhySku);
        }
        None
    }

    fn send_phy_cfg<'a>(&mut self) -> Step<'a> {
        if self.cfg.phy_cfg_cmd_v3 {
            return self.fail(Failure::PhyCfgV3Unsupported);
        }
        let c = &self.cfg;
        let cmd = PhyCfgV1::new(
            c.fw_phy_config,
            c.valid_tx_ant,
            c.valid_rx_ant,
            c.extra_phy_cfg_flags,
            c.flow_trigger,
            c.event_trigger,
        );
        self.state = State::SendPhyCfg;
        Step::act(Action::SendPhyCfg(cmd))
    }

    fn after_phy_cfg<'a>(&mut self) -> Step<'a> {
        if self.init_complete {
            // The wait entry already triggered: iwl_wait_notification returns 0 at once.
            return self.finish(false);
        }
        self.state = State::WaitCalib;
        Step::act(Action::WaitCalib { timeout_ms: MVM_UCODE_CALIB_TIMEOUT_MS })
    }

    fn calib_wait_failed<'a>(&mut self, e: WaitErr) -> Step<'a> {
        self.wait_err = Some(e);
        self.state = State::RfkillAfterWait;
        Step::act(Action::CheckRfkill) // mvm/fw.c:792
    }
}
