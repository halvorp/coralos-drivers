// SPDX-License-Identifier: GPL-2.0-only
//! State-machine vectors for `iwl_run_init_mvm_ucode`.
//! Ported from Linux `mvm/fw.c:28-29,240-259,316-329,396-400,701-823`, `mvm/nvm.c:18-26,72-143,
//! 179-214,226-262,306-398`, `mvm/utils.c:33-49`, `fw/notif-wait.c:115-134`. Copyright Intel
//! Corporation. Expected values are written out by hand from those lines.

use iwlwifi_mvm_init_core::decode::DecodeError;
use iwlwifi_mvm_init_core::encode::PhyCfgV1;
use iwlwifi_mvm_init_core::machine::*;

const CALIB_RES: u8 = 0x6b; // fw/api/commands.h:319
const INIT_COMPLETE: u8 = 0x4; // fw/api/commands.h:80

fn cfg() -> Config {
    Config {
        eeprom_size: 0x4000,
        nvm_hw_section_num: 0,
        nvm_type: NvmType::NvmExt,
        nvm_already_loaded: false,
        fw_phy_config: 0x0033_0015,
        valid_tx_ant: 1,
        valid_rx_ant: 1,
        extra_phy_cfg_flags: 0,
        flow_trigger: 0xAAAA_0001,
        event_trigger: 0xBBBB_0002,
        phy_cfg_cmd_v3: false,
    }
}

/// An NVM_ACCESS_CMD response payload (nvm-reg.h:125-130), little-endian.
fn resp(offset: u16, length: u16, section: u16, status: u16, data: &[u8]) -> Vec<u8> {
    let mut p = vec![];
    for w in [offset, length, section, status] {
        p.extend_from_slice(&w.to_le_bytes());
    }
    p.extend_from_slice(data);
    p
}

fn leak(v: Vec<u8>) -> &'static [u8] {
    Box::leak(v.into_boxed_slice())
}

fn phydb(ty: u16, data: &[u8]) -> Vec<u8> {
    let mut p = vec![];
    p.extend_from_slice(&ty.to_le_bytes());
    p.extend_from_slice(&(data.len() as u16).to_le_bytes());
    p.extend_from_slice(data);
    p
}

/// What the "device" holds for a section: `Some(bytes)` = readable, `None` = error status 2.
type Dev = Vec<Option<Vec<u8>>>;

fn pattern(section: usize, len: usize) -> Vec<u8> {
    (0..len).map(|i| (i as u8) ^ (section as u8 * 17)).collect()
}

/// Serve one chunk the way the firmware does: up to `length` bytes; at/after the end of a section
/// whose size is a multiple of 2K the firmware answers NOT_VALID_ADDRESS (nvm.c:108-115).
fn serve(dev: &Dev, section: u16, offset: u16, length: u16) -> Vec<u8> {
    match &dev[section as usize] {
        None => resp(offset, 0, section, 2, &[]),
        Some(d) => {
            let off = offset as usize;
            if off >= d.len() && off != 0 {
                return resp(offset, 0, section, 1, &[]);
            }
            let end = d.len().min(off + length as usize);
            resp(offset, (end - off) as u16, section, 0, &d[off..end])
        }
    }
}

/// Drive a machine up to ALIVE success; returns the action after ALIVE.
fn to_alive(m: &mut Machine) -> Step<'static> {
    assert_eq!(m.start().action, Action::StartFw);
    assert_eq!(m.step(Event::FwStarted(Ok(()))).action, Action::WaitAlive { timeout_ms: 2000 });
    m.step(Event::AliveOk)
}

/// Run the NVM walk against `dev`; returns the (section, offset) request log, scratch copies per
/// section, and the final step.
fn walk<'a>(
    m: &mut Machine,
    dev: &'a Dev,
    first: Step<'static>,
    log: &mut Vec<(u16, u16)>,
    out: &mut Vec<(u16, Vec<u8>)>,
) -> Action {
    // Owned replies, kept alive for the chunk borrows.
    let mut scratch = vec![0u8; 0x4000];
    let mut act = first.action;
    let mut last = first;
    loop {
        match act {
            Action::ReadNvmChunk { section, offset, length } => {
                assert_eq!(length, 2048);
                log.push((section, offset));
                let payload = serve(dev, section, offset, length);
                let leaked: &'static [u8] = Box::leak(payload.into_boxed_slice());
                last = m.step(Event::NvmReply(leaked));
                if let Some(c) = last.chunk {
                    scratch[c.offset as usize..c.offset as usize + c.data.len()].copy_from_slice(c.data);
                }
                if let Some((s, l)) = last.section_complete {
                    out.push((s, scratch[..l as usize].to_vec()));
                }
                act = last.action;
            }
            _ => {
                let _ = last;
                return act;
            }
        }
    }
}

fn happy_dev() -> Dev {
    // 0:100  1:5000 (3 chunks)  2:ENODATA  3:40  4:ENODATA  5:2048 (exact multiple)  6..10 ENODATA
    // 11:16  12:8
    let mut d: Dev = vec![None; 13];
    d[0] = Some(pattern(0, 100));
    d[1] = Some(pattern(1, 5000));
    d[3] = Some(pattern(3, 40));
    d[5] = Some(pattern(5, 2048));
    d[11] = Some(pattern(11, 16));
    d[12] = Some(pattern(12, 8));
    d
}

#[test]
fn happy_path_all_sections_then_phy_then_calibration() {
    let mut m = Machine::new(cfg());
    let first = to_alive(&mut m);
    let dev = happy_dev();
    let (mut log, mut got) = (vec![], vec![]);
    let act = walk(&mut m, &dev, first, &mut log, &mut got);

    // Every section 0..13 is asked for, in order, multi-chunk at increasing offsets, and the
    // 2048-byte section is asked for a second chunk (answered NOT_VALID_ADDRESS).
    assert_eq!(
        log,
        vec![
            (0, 0), (1, 0), (1, 2048), (1, 4096), (2, 0), (3, 0), (4, 0), (5, 0), (5, 2048),
            (6, 0), (7, 0), (8, 0), (9, 0), (10, 0), (11, 0), (12, 0),
        ]
    );
    let want: Vec<(u16, Vec<u8>)> = vec![
        (0, pattern(0, 100)),
        (1, pattern(1, 5000)),
        (3, pattern(3, 40)),
        (5, pattern(5, 2048)),
        (11, pattern(11, 16)),
        (12, pattern(12, 8)),
    ];
    assert_eq!(got, want);
    assert_eq!(
        *m.nvm_sections(),
        [Some(100), Some(5000), None, Some(40), None, Some(2048), None, None, None, None, None, Some(16), Some(8)]
    );
    assert!(!m.otp_blank());

    // rfkill check, then TX_ANT, then PHY_CFG, then the calibration wait.
    assert_eq!(act, Action::CheckRfkill);
    assert!(!m.rfkill_safe_init_done());
    assert_eq!(m.step(Event::Rfkill(false)).action, Action::SendTxAntCfg { valid_tx_ant: 1 });
    assert!(m.rfkill_safe_init_done());
    let phy = m.step(Event::CmdSent(SendStatus::Ok)).action;
    assert_eq!(
        phy,
        Action::SendPhyCfg(PhyCfgV1 { phy_cfg: 0x0011_0015, flow_trigger: 0xAAAA_0001, event_trigger: 0xBBBB_0002 })
    );
    assert_eq!(m.step(Event::CmdSent(SendStatus::Ok)).action, Action::WaitCalib { timeout_ms: 2000 });
    let e = phydb(1, &[1, 2, 3]);
    let n = phydb(2, &[4]);
    let p = phydb(4, &[9, 0, 5]);
    for pl in [&e, &n, &p] {
        assert_eq!(m.step(Event::Notification { cmd: CALIB_RES, payload: pl }).action, Action::Pending);
    }
    // a malformed entry: Linux WARNs and keeps waiting (fw.c:258)
    assert_eq!(m.step(Event::Notification { cmd: CALIB_RES, payload: &[1, 0, 9, 0] }).action, Action::Pending);
    assert_eq!(m.step(Event::Notification { cmd: INIT_COMPLETE, payload: &[] }).action, Action::Done);
    assert_eq!((m.phy_db_entries(), m.phy_db_rejected()), (3, 1));
    assert!(!m.rfkill());
    assert!(!m.rfkill_safe_init_done());
    assert_eq!(m.state(), State::Done);
    assert_eq!(m.failure(), None);
}

#[test]
fn nvm_not_valid_address_at_nonzero_offset_ends_the_section_normally() {
    let mut m = Machine::new(cfg());
    let first = to_alive(&mut m);
    let mut dev: Dev = vec![None; 13];
    dev[0] = Some(pattern(0, 2048));
    dev[1] = Some(vec![1]);
    dev[3] = Some(vec![3]);
    dev[12] = Some(vec![12]);
    let (mut log, mut got) = (vec![], vec![]);
    let act = walk(&mut m, &dev, first, &mut log, &mut got);
    assert_eq!(act, Action::CheckRfkill);
    assert_eq!(&log[..2], &[(0, 0), (0, 2048)]);
    assert_eq!(m.nvm_sections()[0], Some(2048));
}

#[test]
fn nvm_error_status_skips_the_section_not_a_failure() {
    // status 1 (NOT_VALID_ADDRESS) at OFFSET 0 is -ENODATA, not the "ends normally" case.
    let mut m = Machine::new(cfg());
    let first = to_alive(&mut m);
    assert_eq!(first.action, Action::ReadNvmChunk { section: 0, offset: 0, length: 2048 });
    let r = resp(0, 0, 0, 1, &[]);
    let s = m.step(Event::NvmReply(&r));
    assert_eq!(s.section_skipped, Some(0));
    assert_eq!(s.action, Action::ReadNvmChunk { section: 1, offset: 0, length: 2048 });
    assert_eq!(m.nvm_sections()[0], None);
    // any other nonzero status at offset 0
    let r = resp(0, 0, 1, 2, &[]);
    let s = m.step(Event::NvmReply(&r));
    assert_eq!(s.section_skipped, Some(1));
    assert_eq!(s.action, Action::ReadNvmChunk { section: 2, offset: 0, length: 2048 });
}

#[test]
fn nvm_error_status_mid_section_also_skips_it() {
    // nvm.c:105-126: only NOT_VALID_ADDRESS at offset != 0 is benign; status 2 at offset 2048 is
    // -ENODATA and iwl_nvm_init discards the whole section (nvm.c:327-330).
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    let data = pattern(0, 2048);
    let r = resp(0, 2048, 0, 0, &data);
    let s = m.step(Event::NvmReply(&r));
    assert_eq!(s.action, Action::ReadNvmChunk { section: 0, offset: 2048, length: 2048 });
    assert_eq!(s.chunk.unwrap().data.len(), 2048);
    let r = resp(2048, 0, 0, 2, &[]);
    let s = m.step(Event::NvmReply(&r));
    assert_eq!(s.section_skipped, Some(0));
    assert_eq!(s.section_complete, None);
    assert_eq!(m.nvm_sections()[0], None);
}

#[test]
fn nvm_offset_mismatch_is_a_named_failure() {
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    let r = resp(4, 10, 0, 0, &[0; 10]); // asked for offset 0, answered 4 (nvm.c:129-134)
    let s = m.step(Event::NvmReply(&r));
    assert_eq!(s.action, Action::Failed(Failure::NvmOffsetMismatch { requested: 0, got: 4 }));
    assert_eq!(s.chunk, None);
}

#[test]
fn nvm_offset_mismatch_on_a_later_chunk() {
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    let d = pattern(0, 2048);
    m.step(Event::NvmReply(leak(resp(0, 2048, 0, 0, &d))));
    let r = resp(0, 5, 0, 0, &[0; 5]); // second chunk (offset 2048) answered with offset 0
    assert_eq!(
        m.step(Event::NvmReply(&r)).action,
        Action::Failed(Failure::NvmOffsetMismatch { requested: 2048, got: 0 })
    );
}

#[test]
fn nvm_status_is_checked_before_offset() {
    // nvm.c:105 precedes :129: an error status with a wrong offset is still a skip.
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    let s = m.step(Event::NvmReply(leak(resp(77, 0, 0, 2, &[]))));
    assert_eq!(s.section_skipped, Some(0));
}

#[test]
fn nvm_chunk_longer_than_requested_is_refused_coralos_bound() {
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    let data = vec![0u8; 2049];
    let s = m.step(Event::NvmReply(leak(resp(0, 2049, 0, 0, &data))));
    assert_eq!(s.action, Action::Failed(Failure::NvmChunkOverrun { requested: 2048, got: 2049 }));
}

#[test]
fn nvm_malformed_reply_is_a_named_failure() {
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    let s = m.step(Event::NvmReply(&[1, 2, 3]));
    assert_eq!(
        s.action,
        Action::Failed(Failure::NvmRespMalformed {
            section: 0,
            offset: 0,
            error: DecodeError::ShortHeader { need: 8, have: 3 }
        })
    );
}

#[test]
fn nvm_send_failure_is_a_named_failure() {
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    assert_eq!(
        m.step(Event::CmdFailed(-110)).action,
        Action::Failed(Failure::NvmSendFailed { section: 0, offset: 0, errno: -110 })
    );
}

#[test]
fn nvm_eeprom_size_overflow_is_enobufs() {
    // size_read + offset + length > eeprom_size (nvm.c:193-197), checked before EACH request.
    let mut c = cfg();
    c.eeprom_size = 4095;
    let mut m = Machine::new(c);
    let first = to_alive(&mut m); // 0 + 0 + 2048 <= 4095
    assert_eq!(first.action, Action::ReadNvmChunk { section: 0, offset: 0, length: 2048 });
    let d = pattern(0, 2048);
    let s = m.step(Event::NvmReply(leak(resp(0, 2048, 0, 0, &d)))); // next: 0 + 2048 + 2048 = 4096 > 4095
    assert_eq!(
        s.action,
        Action::Failed(Failure::NvmEepromTooSmall { size_read: 0, offset: 2048, length: 2048, eeprom_size: 4095 })
    );
}

#[test]
fn nvm_eeprom_size_exact_fit_is_allowed() {
    let mut c = cfg();
    c.eeprom_size = 4096; // 0 + 2048 + 2048 == 4096 is NOT ">" (nvm.c:193)
    let mut m = Machine::new(c);
    to_alive(&mut m);
    let d = pattern(0, 2048);
    let s = m.step(Event::NvmReply(leak(resp(0, 2048, 0, 0, &d))));
    assert_eq!(s.action, Action::ReadNvmChunk { section: 0, offset: 2048, length: 2048 });
    let s = m.step(Event::NvmReply(leak(resp(2048, 2048, 0, 0, &d))));
    // third request: 0 + 4096 + 2048 > 4096
    assert_eq!(
        s.action,
        Action::Failed(Failure::NvmEepromTooSmall { size_read: 0, offset: 4096, length: 2048, eeprom_size: 4096 })
    );
}

#[test]
fn nvm_size_read_accumulates_across_sections() {
    // After section 0 (100 bytes) the next section's check is 100 + 0 + 2048 (nvm.c:333, :193).
    let mut c = cfg();
    c.eeprom_size = 2147;
    let mut m = Machine::new(c);
    to_alive(&mut m);
    let s = m.step(Event::NvmReply(leak(resp(0, 100, 0, 0, &[0; 100]))));
    assert_eq!(
        s.action,
        Action::Failed(Failure::NvmEepromTooSmall { size_read: 100, offset: 0, length: 2048, eeprom_size: 2147 })
    );
    let mut c = cfg();
    c.eeprom_size = 2148;
    let mut m = Machine::new(c);
    to_alive(&mut m);
    let s = m.step(Event::NvmReply(leak(resp(0, 100, 0, 0, &[0; 100]))));
    assert_eq!(s.action, Action::ReadNvmChunk { section: 1, offset: 0, length: 2048 });
}

#[test]
fn nvm_hw_section_number_must_be_below_13() {
    let mut c = cfg();
    c.nvm_hw_section_num = 13;
    let mut m = Machine::new(c);
    assert_eq!(
        to_alive(&mut m).action,
        Action::Failed(Failure::NvmHwSectionOutOfRange { section_num: 13 })
    );
    let mut c = cfg();
    c.nvm_hw_section_num = 12;
    let mut m = Machine::new(c);
    assert_eq!(to_alive(&mut m).action, Action::ReadNvmChunk { section: 0, offset: 0, length: 2048 });
}

#[test]
fn nvm_already_loaded_skips_straight_to_the_rfkill_check() {
    let mut c = cfg();
    c.nvm_already_loaded = true;
    c.nvm_hw_section_num = 99; // not even looked at (the check lives in iwl_nvm_init)
    let mut m = Machine::new(c);
    assert_eq!(to_alive(&mut m).action, Action::CheckRfkill);
}

/// Run a walk where `present` sections hold one byte and the rest answer ENODATA; return the step
/// after section 12.
fn walk_present(c: Config, present: &[usize]) -> Action {
    let mut m = Machine::new(c);
    let first = to_alive(&mut m);
    let mut dev: Dev = vec![None; 13];
    for &p in present {
        dev[p] = Some(vec![p as u8]);
    }
    let (mut log, mut got) = (vec![], vec![]);
    walk(&mut m, &dev, first, &mut log, &mut got)
}

#[test]
fn required_sections_by_nvm_type() {
    let mut c = cfg();
    c.nvm_type = NvmType::Nvm;
    // IWL_NVM: SW (1) and the HW section (0), nothing else (nvm.c:227-231)
    assert_eq!(walk_present(c, &[1, 0]), Action::CheckRfkill);
    assert_eq!(
        walk_present(c, &[0]),
        Action::Failed(Failure::NvmSectionsMissing { what: MissingSections::SwOrHw })
    );
    assert_eq!(
        walk_present(c, &[1]),
        Action::Failed(Failure::NvmSectionsMissing { what: MissingSections::SwOrHw })
    );

    // IWL_NVM_EXT: SW, REGULATORY (3), HW or MAC_OVERRIDE (11), PHY_SKU (12)
    let mut c = cfg();
    c.nvm_type = NvmType::NvmExt;
    assert_eq!(walk_present(c, &[1, 3, 0, 12]), Action::CheckRfkill);
    assert_eq!(walk_present(c, &[1, 3, 11, 12]), Action::CheckRfkill); // MAC_OVERRIDE instead of HW
    assert_eq!(
        walk_present(c, &[1, 0, 12]),
        Action::Failed(Failure::NvmSectionsMissing { what: MissingSections::SwOrRegulatory })
    );
    assert_eq!(
        walk_present(c, &[3, 0, 12]),
        Action::Failed(Failure::NvmSectionsMissing { what: MissingSections::SwOrRegulatory })
    );
    assert_eq!(
        walk_present(c, &[1, 3, 12]),
        Action::Failed(Failure::NvmSectionsMissing { what: MissingSections::MacAddress })
    );
    assert_eq!(
        walk_present(c, &[1, 3, 0]),
        Action::Failed(Failure::NvmSectionsMissing { what: MissingSections::PhySku })
    );

    // IWL_NVM_SDP: REGULATORY_SDP (8) instead of 3, and PHY_SKU is NOT required (nvm.c:255)
    let mut c = cfg();
    c.nvm_type = NvmType::NvmSdp;
    assert_eq!(walk_present(c, &[1, 8, 0]), Action::CheckRfkill);
    assert_eq!(
        walk_present(c, &[1, 3, 0]),
        Action::Failed(Failure::NvmSectionsMissing { what: MissingSections::SwOrRegulatory })
    );
}

#[test]
fn nvm_hw_section_number_selects_the_hw_section() {
    let mut c = cfg();
    c.nvm_type = NvmType::NvmExt;
    c.nvm_hw_section_num = 2;
    assert_eq!(walk_present(c, &[1, 3, 2, 12]), Action::CheckRfkill);
    assert_eq!(
        walk_present(c, &[1, 3, 0, 12]), // section 0 present but HW is 2
        Action::Failed(Failure::NvmSectionsMissing { what: MissingSections::MacAddress })
    );
}

#[test]
fn zero_length_sections_count_as_read_and_a_blank_otp_is_flagged() {
    // A first chunk answering 0 bytes returns 0 (not -ENODATA): the section is "read", kmemdup(0).
    let mut m = Machine::new(cfg());
    let mut s = to_alive(&mut m);
    for sec in 0..13u16 {
        assert_eq!(s.action, Action::ReadNvmChunk { section: sec, offset: 0, length: 2048 });
        s = m.step(Event::NvmReply(leak(resp(0, 0, sec, 0, &[]))));
        if sec < 12 {
            assert_eq!(s.section_complete, Some((sec, 0)));
        }
    }
    assert_eq!(s.action, Action::CheckRfkill);
    assert!(m.otp_blank()); // size_read == 0 (nvm.c:377-378)
    assert_eq!(*m.nvm_sections(), [Some(0); 13]);
}

#[test]
fn rfkill_short_circuits_before_any_phy_activity() {
    let mut m = Machine::new(cfg());
    let first = to_alive(&mut m);
    let dev = happy_dev();
    let (mut log, mut got) = (vec![], vec![]);
    assert_eq!(walk(&mut m, &dev, first, &mut log, &mut got), Action::CheckRfkill);
    let s = m.step(Event::Rfkill(true));
    assert_eq!(s.action, Action::Done); // Linux: goto remove_notif with ret 0 (fw.c:763-766)
    assert!(m.rfkill());
    assert!(!m.rfkill_safe_init_done());
    assert_eq!(m.failure(), None);
    assert_eq!(m.phy_db_entries(), 0);
    // nothing further is ever asked
    assert_eq!(m.step(Event::CmdSent(SendStatus::Ok)).action, Action::Done);
}

fn to_phy(m: &mut Machine) {
    let mut c = cfg();
    c.nvm_already_loaded = true;
    *m = Machine::new(c);
    assert_eq!(to_alive(m).action, Action::CheckRfkill);
    assert_eq!(m.step(Event::Rfkill(false)).action, Action::SendTxAntCfg { valid_tx_ant: 1 });
}

#[test]
fn tx_ant_and_phy_cfg_send_errors_are_named() {
    let mut m = Machine::new(cfg());
    to_phy(&mut m);
    assert_eq!(
        m.step(Event::CmdSent(SendStatus::Err(-5))).action,
        Action::Failed(Failure::TxAntSendFailed { errno: -5 })
    );
    assert!(!m.rfkill_safe_init_done());

    to_phy(&mut m);
    m.step(Event::CmdSent(SendStatus::Ok));
    assert_eq!(
        m.step(Event::CmdSent(SendStatus::Err(-12))).action,
        Action::Failed(Failure::PhyCfgSendFailed { errno: -12 })
    );
}

#[test]
fn rfkill_and_hostdown_send_errors_are_swallowed_like_iwl_mvm_send_cmd() {
    // mvm/utils.c:47-48 (the commands carry no CMD_WANT_SKB)
    for st in [SendStatus::Rfkill, SendStatus::HostDown] {
        let mut m = Machine::new(cfg());
        to_phy(&mut m);
        assert!(matches!(m.step(Event::CmdSent(st)).action, Action::SendPhyCfg(_)));
        assert_eq!(m.step(Event::CmdSent(st)).action, Action::WaitCalib { timeout_ms: 2000 });
    }
}

#[test]
fn phy_cfg_v3_is_refused_by_name() {
    let mut c = cfg();
    c.nvm_already_loaded = true;
    c.phy_cfg_cmd_v3 = true;
    let mut m = Machine::new(c);
    to_alive(&mut m);
    m.step(Event::Rfkill(false));
    assert_eq!(m.step(Event::CmdSent(SendStatus::Ok)).action, Action::Failed(Failure::PhyCfgV3Unsupported));
}

#[test]
fn tx_ant_value_and_extra_phy_flags_flow_through() {
    let mut c = cfg();
    c.nvm_already_loaded = true;
    c.valid_tx_ant = 3;
    c.valid_rx_ant = 3;
    c.extra_phy_cfg_flags = 0x8000_0000;
    let mut m = Machine::new(c);
    to_alive(&mut m);
    assert_eq!(m.step(Event::Rfkill(false)).action, Action::SendTxAntCfg { valid_tx_ant: 3 });
    assert_eq!(
        m.step(Event::CmdSent(SendStatus::Ok)).action,
        Action::SendPhyCfg(PhyCfgV1 { phy_cfg: 0x8033_0015, flow_trigger: 0xAAAA_0001, event_trigger: 0xBBBB_0002 })
    );
}

#[test]
fn alive_failures_are_named() {
    let mut m = Machine::new(cfg());
    m.start();
    assert_eq!(m.step(Event::FwStarted(Err(-110))).action, Action::Failed(Failure::StartFw { errno: -110 }));
    for (ev, f) in [
        (Event::WaitTimedOut, Failure::AliveTimeout),
        (Event::WaitAborted, Failure::AliveAborted),
        (Event::AliveInvalid, Failure::AliveInvalid),
    ] {
        let mut m = Machine::new(cfg());
        m.start();
        m.step(Event::FwStarted(Ok(())));
        assert_eq!(m.step(ev).action, Action::Failed(f));
        assert_eq!(m.failure(), Some(f));
        // terminal: further events repeat the failure
        assert_eq!(m.step(Event::AliveOk).action, Action::Failed(f));
    }
}

#[test]
fn calibration_timeout_without_rfkill_fails() {
    let mut m = Machine::new(cfg());
    to_phy(&mut m);
    m.step(Event::CmdSent(SendStatus::Ok));
    m.step(Event::CmdSent(SendStatus::Ok));
    assert_eq!(m.step(Event::WaitTimedOut).action, Action::CheckRfkill); // fw.c:792
    assert_eq!(m.step(Event::Rfkill(false)).action, Action::Failed(Failure::CalibTimeout));
    assert!(!m.rfkill_safe_init_done());
}

#[test]
fn calibration_abort_without_rfkill_fails_differently() {
    let mut m = Machine::new(cfg());
    to_phy(&mut m);
    m.step(Event::CmdSent(SendStatus::Ok));
    m.step(Event::CmdSent(SendStatus::Ok));
    assert_eq!(m.step(Event::WaitAborted).action, Action::CheckRfkill);
    assert_eq!(m.step(Event::Rfkill(false)).action, Action::Failed(Failure::CalibAborted));
}

#[test]
fn rfkill_raised_during_calibration_is_success_with_rfkill() {
    // fw.c:792-794: "RFKILL while calibrating." -> ret = 0
    for ev in [Event::WaitTimedOut, Event::WaitAborted] {
        let mut m = Machine::new(cfg());
        to_phy(&mut m);
        m.step(Event::CmdSent(SendStatus::Ok));
        m.step(Event::CmdSent(SendStatus::Ok));
        m.step(ev);
        assert_eq!(m.step(Event::Rfkill(true)).action, Action::Done);
        assert!(m.rfkill());
        assert_eq!(m.failure(), None);
    }
}

#[test]
fn init_complete_seen_early_completes_the_wait_at_once() {
    // The calibration wait is registered before the firmware starts (fw.c:717-722); a notification
    // that already triggered it makes iwl_wait_notification return 0 immediately.
    let mut m = Machine::new(cfg());
    to_phy(&mut m);
    assert_eq!(
        m.step(Event::Notification { cmd: INIT_COMPLETE, payload: &[] }).action,
        Action::Pending
    );
    m.step(Event::CmdSent(SendStatus::Ok));
    assert_eq!(m.step(Event::CmdSent(SendStatus::Ok)).action, Action::Done);
}

#[test]
fn phy_db_entries_during_nvm_are_counted_too() {
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    let p = phydb(1, &[1]);
    assert_eq!(m.step(Event::Notification { cmd: CALIB_RES, payload: &p }).action, Action::Pending);
    assert_eq!(m.phy_db_entries(), 1);
    // the pending NVM request is still outstanding and still answered normally
    let s = m.step(Event::NvmReply(leak(resp(0, 0, 0, 2, &[]))));
    assert_eq!(s.action, Action::ReadNvmChunk { section: 1, offset: 0, length: 2048 });
}

#[test]
fn unexpected_notification_is_a_named_failure() {
    // iwl_wait_phy_db_entry WARNs on anything but CALIB_RES / INIT_COMPLETE (fw.c:253-255).
    for cmd in [0x01u8, 0x6a, 0x88, 0x00, 0xff] {
        let mut m = Machine::new(cfg());
        to_phy(&mut m);
        m.step(Event::CmdSent(SendStatus::Ok));
        m.step(Event::CmdSent(SendStatus::Ok));
        assert_eq!(
            m.step(Event::Notification { cmd, payload: &[] }).action,
            Action::Failed(Failure::UnexpectedNotification { cmd })
        );
    }
}

#[test]
fn unexpected_events_are_named_with_the_state() {
    let mut m = Machine::new(cfg());
    assert_eq!(
        m.step(Event::AliveOk).action,
        Action::Failed(Failure::UnexpectedEvent { state: State::Init })
    );
    let mut m = Machine::new(cfg());
    m.start();
    assert_eq!(
        m.step(Event::Notification { cmd: CALIB_RES, payload: &[] }).action,
        Action::Failed(Failure::UnexpectedEvent { state: State::WaitFwStart })
    );
    let mut m = Machine::new(cfg());
    to_phy(&mut m);
    m.step(Event::CmdSent(SendStatus::Ok));
    m.step(Event::CmdSent(SendStatus::Ok));
    assert_eq!(
        m.step(Event::AliveOk).action,
        Action::Failed(Failure::UnexpectedEvent { state: State::WaitCalib })
    );
    let mut m = Machine::new(cfg());
    to_alive(&mut m);
    assert_eq!(
        m.step(Event::Rfkill(false)).action,
        Action::Failed(Failure::UnexpectedEvent { state: State::NvmWaitReply })
    );
}

#[test]
fn set_valid_antennas_after_the_nvm_walk_changes_what_both_commands_carry() {
    // mvm/fw.c:772 and :776 read iwl_mvm_get_valid_tx_ant() / iwl_mvm_get_phy_config() AFTER iwl_nvm_init
    // (mvm/mvm.h:1757-1800): the firmware's mask ANDed with the NVM's. cfg() starts at 1/1; the NVM said 3/2.
    let mut c = cfg();
    c.nvm_already_loaded = true;
    c.fw_phy_config = 0x0033_0015; // tx chain 3, rx chain 3 in the firmware's word
    let mut m = Machine::new(c);
    assert_eq!(to_alive(&mut m).action, Action::CheckRfkill);
    m.set_valid_antennas(3, 2);
    assert_eq!(m.step(Event::Rfkill(false)).action, Action::SendTxAntCfg { valid_tx_ant: 3 });
    match m.step(Event::CmdSent(SendStatus::Ok)).action {
        Action::SendPhyCfg(p) => {
            // phy_cfg = fw_phy_config & (~(TX|RX chain) | tx << 16 | rx << 20), mvm/mvm.h:1789-1800
            assert_eq!(p.phy_cfg, 0x0023_0015);
        }
        a => panic!("{a:?}"),
    }
}
