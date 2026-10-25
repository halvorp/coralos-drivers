// SPDX-License-Identifier: GPL-2.0-only
//! What the firmware does, as far as bring-up depends on it, written from the Linux structures it speaks
//! (`fw/api/alive.h`, `fw/api/nvm-reg.h`, `fw/api/commands.h`, `fw/api/cmdhdr.h`, `iwl-trans.h`): boot when a
//! complete image sits in SRAM and the loader was released, announce ALIVE through the RX queue, fetch
//! host commands through the TFD ring, answer each with a reply carrying its sequence number, and emit the
//! INIT image's calibration results and INIT_COMPLETE after PHY_CONFIGURATION_CMD.
use super::*;

pub const ALIVE_STATUS_OK: u16 = 0xCAFE; // alive.h:37
/// Where the scheduler's SRAM lives in the simulated card (the value the host reads from SCD_SRAM_BASE_ADDR).
pub const SCD_BASE_VALUE: u32 = 0x0040_3800;
pub const CPU1_CPU2_SEP: u32 = 0xFFFF_CCCC; // fw/file.h:580
pub const PAGING_SEP: u32 = 0xAAAA_BBBB; // fw/file.h:581

#[derive(Clone)]
pub struct SecModel {
    pub offset: u32,
    pub data: Vec<u8>,
}

#[derive(Clone)]
pub struct FwImageModel {
    pub secs: Vec<SecModel>,
}

impl FwImageModel {
    /// The loadable sections as the secure loader splits them: CPU1 up to the first separator, CPU2 from there to the
    /// next separator or the end. Paging sections (after the PAGING separator) are not loaded by this path.
    pub fn loadable(&self) -> Vec<&SecModel> {
        let mut out = vec![];
        let mut seps = 0;
        for s in &self.secs {
            if s.offset == CPU1_CPU2_SEP {
                seps += 1;
                if seps >= 2 {
                    break;
                }
                continue;
            }
            if s.offset == PAGING_SEP {
                break;
            }
            out.push(s);
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwState {
    Off,
    Init,
    Regular,
    /// Released and loaded but no image matched: the card sits silent.
    Dead,
}

pub struct Fw {
    pub init: Option<FwImageModel>,
    pub regular: Option<FwImageModel>,
    pub cpu_released: bool,
    pub scd_base_reg: u32,
    pub state: FwState,
    pub nvm: Vec<Option<Vec<u8>>>,
    pub boot_delay_us: u64,
    // fault injection
    pub suppress_alive: bool,
    pub alive_status: u16,
    pub swallow_cmd: Option<u8>,
    pub sw_error_on_cmd: Option<u8>,
    pub skip_init_complete: bool,
    // what the host sent
    pub tx_ant_seen: Vec<u32>,
    pub phy_cfg_seen: Vec<[u32; 3]>,
    pub boots: Vec<FwState>,
    pub notif_counter: u16,
    pub calib_types: Vec<u16>,
    pub extra: Option<Box<dyn FnMut(&mut World, &SeenCmd) -> Option<Vec<Reply>>>>,
}

/// One packet the firmware sends back.
#[derive(Clone, Debug)]
pub struct Reply {
    pub group: u8,
    pub cmd: u8,
    pub seq: u16,
    pub payload: Vec<u8>,
    pub delay_us: u64,
}

impl Fw {
    pub fn new() -> Fw {
        Fw {
            init: None,
            regular: None,
            cpu_released: false,
            scd_base_reg: 0,
            state: FwState::Off,
            nvm: super::nvmdata::NvmSpec::atlas().sections(),
            boot_delay_us: 20_000,
            suppress_alive: false,
            alive_status: ALIVE_STATUS_OK,
            swallow_cmd: None,
            sw_error_on_cmd: None,
            skip_init_complete: false,
            tx_ant_seen: vec![],
            phy_cfg_seen: vec![],
            boots: vec![],
            notif_counter: 0,
            calib_types: vec![1, 2, 4, 5],
            extra: None,
        }
    }

    pub fn reset(&mut self) {
        self.cpu_released = false;
        self.scd_base_reg = 0;
        self.state = FwState::Off;
    }
}

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

/// FH_UCODE_LOAD_STATUS was written. 0xFFFFFFFF with the loader released starts the boot.
pub fn load_status_written(w: &mut World, val: u32) {
    if val == 0xFFFF_FFFF {
        if !w.fw.cpu_released {
            w.violation("FH_UCODE_LOAD_STATUS = 0xFFFFFFFF but RELEASE_CPU_RESET was never written".into());
            return;
        }
        let d = w.fw.boot_delay_us;
        w.schedule(d, Ev::Boot);
    }
}

fn image_matches(w: &World, img: &FwImageModel) -> bool {
    img.loadable().iter().all(|s| {
        let mut got = vec![0u8; s.data.len()];
        w.sram_read(s.offset, &mut got);
        got == s.data
    })
}

/// The loader finished: find which image is in SRAM and run it.
pub fn boot(w: &mut World) {
    let which = if w.fw.init.as_ref().is_some_and(|i| image_matches(w, i)) {
        FwState::Init
    } else if w.fw.regular.as_ref().is_some_and(|i| image_matches(w, i)) {
        FwState::Regular
    } else {
        FwState::Dead
    };
    w.fw.state = which;
    w.fw.boots.push(which);
    w.log.push(format!("[sim] firmware boot: {which:?}"));
    if which == FwState::Dead || w.fw.suppress_alive {
        return;
    }
    w.fw.scd_base_reg = SCD_BASE_VALUE;
    let mut p = Vec::new();
    p.extend_from_slice(&w.fw.alive_status.to_le_bytes()); // status
    p.extend_from_slice(&0u16.to_le_bytes()); // flags
    // lmac: ucode_major, ucode_minor, ver_subtype, ver_type, mac, opt, timestamp, then 8 debug pointers (alive.h:41-61)
    p.extend_from_slice(&46u32.to_le_bytes());
    p.extend_from_slice(&0x7e3e_4b69u32.to_le_bytes());
    p.extend_from_slice(&[0, 0, 0, 0]);
    p.extend_from_slice(&0u32.to_le_bytes());
    for ptr in [0x0080_0100u32, 0x0080_0200, 0x0080_0300, 0, 0x0080_0400, SCD_BASE_VALUE, 0, 0] {
        p.extend_from_slice(&ptr.to_le_bytes());
    }
    // umac: major, minor, error_info_addr, dbg_print_buff_addr (alive.h:63-72)
    p.extend_from_slice(&46u32.to_le_bytes());
    p.extend_from_slice(&0x7e3e_4b69u32.to_le_bytes());
    p.extend_from_slice(&0x0088_00c4u32.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(p.len(), 68);
    post(w, 0, 0x01, 0x8000, &p);
    w.raise(regs::INT_ALIVE);
}

/// Put one packet into the next posted receive buffer and tell the host.
pub fn post(w: &mut World, group: u8, cmd: u8, seq: u16, payload: &[u8]) {
    post_many(w, &[(group, cmd, seq, payload.to_vec())]);
}

/// Several packets in ONE receive buffer, 64-byte aligned, the invalid marker after the last.
pub fn post_many(w: &mut World, pkts: &[(u8, u8, u16, Vec<u8>)]) {
    let cfg = w.prph.get(&regs::P_RFH_DMA_CFG).copied().unwrap_or(0);
    let active = w.prph.get(&regs::P_RFH_RXQ_ACTIVE).copied().unwrap_or(0);
    if cfg & 0x8000_0000 == 0 || active & 1 == 0 {
        w.violation("firmware has a packet for the host but the RFH receive queue is not enabled".into());
        return;
    }
    if w.rx_free_ridx == w.rx_free_widx {
        w.rx_overruns += 1;
        w.log.push("[sim] RX overrun: no posted buffer".into());
        return;
    }
    let p64 = |w: &World, a: u32| (w.prph.get(&a).copied().unwrap_or(0) as u64) | (w.prph.get(&(a + 4)).copied().unwrap_or(0) as u64) << 32;
    let bd_base = p64(w, regs::P_FRBDCB_BA);
    let used_base = p64(w, regs::P_URBDCB_BA);
    let stts_base = p64(w, regs::P_URBD_STTS);
    let mut e = [0u8; 8];
    if !w.dma_read(bd_base + w.rx_free_ridx as u64 * 8, &mut e) {
        w.violation(format!("RBD table at {bd_base:#x} is not host memory"));
        return;
    }
    let ent = u64::from_le_bytes(e);
    let (page, vid) = (ent & !0xfff, (ent & 0xfff) as u32);
    let mut off = 0usize;
    for (group, cmd, seq, payload) in pkts {
        let len = 4 + payload.len(); // cmd header + payload
        let mut b = Vec::with_capacity(len + 4);
        b.extend_from_slice(&(len as u32).to_le_bytes()); // len_n_flags: frame size, rxq 0
        b.extend_from_slice(&[*cmd, *group]);
        b.extend_from_slice(&seq.to_le_bytes());
        b.extend_from_slice(payload);
        if !w.dma_write(page + off as u64, &b) {
            w.violation(format!("RB at {page:#x} is not host memory"));
            return;
        }
        off += (b.len() + 63) / 64 * 64;
        assert!(off <= 4096, "test packets overflow one receive buffer");
    }
    if off + 4 <= 4096 {
        w.dma_write(page + off as u64, &0x5555_0000u32.to_le_bytes()); // the end-of-frames marker
    }
    w.dma_write(used_base + w.rx_used_widx as u64 * 4, &vid.to_le_bytes());
    w.rx_used_widx = (w.rx_used_widx + 1) & 511;
    w.rx_free_ridx = (w.rx_free_ridx + 1) & 511;
    let mut stts = [0u8; 12];
    stts[0..2].copy_from_slice(&w.rx_used_widx.to_le_bytes());
    w.dma_write(stts_base, &stts);
    w.fh_int |= 1 << 16;
    w.raise(regs::INT_FH_RX);
}

/// HBUS_TARG_WRPTR for queue 0: fetch every TFD from the last seen write pointer up to `wr`.
pub fn doorbell(w: &mut World, wr: u16) {
    if w.fw.state == FwState::Off || w.fw.state == FwState::Dead {
        w.violation("host command doorbell while no firmware runs".into());
    }
    let base = (w.fh.get(&regs::cbbc_queue(0)).copied().unwrap_or(0) as u64) << 8; // FH_MEM_CBBC_QUEUE(0) holds ring >> 8
    let mut i = w.cmd_seen_wptr;
    while i != wr {
        let mut tfd = [0u8; 128];
        if !w.dma_read(base + i as u64 * 128, &mut tfd) {
            w.violation(format!("TFD {i} at {:#x}: not host memory", base + i as u64 * 128));
            return;
        }
        let n = (tfd[3] & 0x1f) as usize;
        let mut bytes = Vec::new();
        let mut tbs = Vec::new();
        for k in 0..n {
            let o = 4 + 6 * k;
            let lo = u32::from_le_bytes([tfd[o], tfd[o + 1], tfd[o + 2], tfd[o + 3]]) as u64;
            let hl = le16(&tfd, o + 4);
            let addr = lo | ((hl & 0xf) as u64) << 32;
            let len = hl >> 4;
            let mut buf = vec![0u8; len as usize];
            if !w.dma_read(addr, &mut buf) {
                w.violation(format!("TB {k} of TFD {i}: {len} bytes at {addr:#x} is not host memory"));
                return;
            }
            bytes.extend_from_slice(&buf);
            tbs.push((addr, len));
        }
        if tbs.first().is_some_and(|t| t.1 > 20) {
            w.violation(format!("TB0 of TFD {i} is {} bytes (> IWL_FIRST_TB_SIZE 20)", tbs[0].1));
        }
        if bytes.len() < 4 {
            w.violation(format!("TFD {i} carries {} bytes", bytes.len()));
            w.cmd_seen_wptr = (i + 1) & 255;
            i = (i + 1) & 255;
            continue;
        }
        let (cmd, group, seq) = (bytes[0], bytes[1], le16(&bytes, 2));
        let payload = if group != 0 && bytes.len() >= 8 {
            let l = le16(&bytes, 4) as usize;
            if bytes.len() != 8 + l {
                w.violation(format!("wide command {group}.{cmd:#x}: header says {l} payload bytes, TFD carries {}", bytes.len() - 8));
            }
            bytes[8..].to_vec()
        } else {
            bytes[4..].to_vec()
        };
        if seq & 0xff != i || (seq >> 8) & 0x1f != 0 {
            w.violation(format!("command {group}.{cmd:#x} sequence {seq:#x} does not name TFD {i} on queue 0"));
        }
        let seen = SeenCmd { group, cmd, seq, payload, tbs };
        w.cmds.push(seen.clone());
        w.cmd_seen_wptr = (i + 1) & 255;
        i = (i + 1) & 255;
        dispatch(w, seen);
    }
}

fn nvm_reply(w: &World, payload: &[u8]) -> Vec<u8> {
    // struct iwl_nvm_access_cmd { u8 op_code; u8 target; le16 type; le16 offset; le16 length; } (nvm-reg.h:60-70)
    let (ty, offset, length) = (le16(payload, 2) as usize, le16(payload, 4) as usize, le16(payload, 6) as usize);
    // struct iwl_nvm_access_resp { le16 offset; le16 length; le16 type; le16 status; u8 data[]; } (nvm-reg.h:125-130)
    let mut r = Vec::new();
    match w.fw.nvm.get(ty).and_then(|s| s.as_ref()) {
        None => {
            for v in [offset as u16, 0, ty as u16, 2] {
                r.extend_from_slice(&v.to_le_bytes());
            }
        }
        Some(sec) if offset > sec.len() || (offset == sec.len() && offset != 0) => {
            for v in [offset as u16, 0, ty as u16, 1] {
                r.extend_from_slice(&v.to_le_bytes()); // NOT_VALID_ADDRESS
            }
        }
        Some(sec) => {
            let n = length.min(sec.len() - offset);
            for v in [offset as u16, n as u16, ty as u16, 0] {
                r.extend_from_slice(&v.to_le_bytes());
            }
            r.extend_from_slice(&sec[offset..offset + n]);
        }
    }
    r
}

fn dispatch(w: &mut World, c: SeenCmd) {
    if Some(c.cmd) == w.fw.sw_error_on_cmd && c.group == 0 {
        w.log.push(format!("[sim] firmware asserts on command {:#x}", c.cmd));
        // struct iwl_error_event_table (fw/dump.c:23-71) at the pointer the ALIVE gave: valid=1, error_id=SYSASSERT (0x35)
        let mut t = vec![0u32; 38];
        t[0] = 1;
        t[1] = 0x35;
        t[20] = 0x0040_beef; // log_pc
        t[23] = c.cmd as u32; // hcmd
        let bytes: Vec<u8> = t.iter().flat_map(|v| v.to_le_bytes()).collect();
        w.sram_write(0x0080_0100, &bytes);
        w.fw.state = FwState::Dead;
        w.raise(regs::INT_SW_ERR);
        return;
    }
    if Some(c.cmd) == w.fw.swallow_cmd && c.group == 0 {
        return; // never answered
    }
    if let Some(mut f) = w.fw.extra.take() {
        let r = f(w, &c);
        w.fw.extra = Some(f);
        if let Some(rs) = r {
            for rp in rs {
                w.schedule(rp.delay_us, Ev::Post { group: rp.group, cmd: rp.cmd, seq: rp.seq, payload: rp.payload });
            }
            return;
        }
    }
    let reply = |payload: Vec<u8>| Reply { group: c.group, cmd: c.cmd, seq: c.seq, payload, delay_us: 80 };
    match (c.group, c.cmd) {
        (0, 0x88) => {
            let p = nvm_reply(w, &c.payload);
            let rp = reply(p);
            w.schedule(rp.delay_us, Ev::Post { group: rp.group, cmd: rp.cmd, seq: rp.seq, payload: rp.payload });
        }
        (0, 0x98) => {
            if c.payload.len() == 4 {
                w.fw.tx_ant_seen.push(u32::from_le_bytes([c.payload[0], c.payload[1], c.payload[2], c.payload[3]]));
            } else {
                w.violation(format!("TX_ANT_CONFIGURATION_CMD payload is {} bytes, not 4", c.payload.len()));
            }
            let rp = reply(vec![]);
            w.schedule(rp.delay_us, Ev::Post { group: rp.group, cmd: rp.cmd, seq: rp.seq, payload: rp.payload });
        }
        (0, 0x6a) => {
            if c.payload.len() == 12 {
                let g = |i: usize| u32::from_le_bytes([c.payload[i], c.payload[i + 1], c.payload[i + 2], c.payload[i + 3]]);
                w.fw.phy_cfg_seen.push([g(0), g(4), g(8)]);
            } else {
                w.violation(format!("PHY_CONFIGURATION_CMD payload is {} bytes, not the v1 12", c.payload.len()));
            }
            let rp = reply(vec![]);
            w.schedule(rp.delay_us, Ev::Post { group: rp.group, cmd: rp.cmd, seq: rp.seq, payload: rp.payload });
            if w.fw.state == FwState::Init {
                let mut t = 2_000;
                for ty in w.fw.calib_types.clone() {
                    // struct iwl_calib_res_notif_phy_db { le16 type; le16 length; u8 data[]; } (cmdhdr.h:130-134)
                    let data = vec![ty as u8; 16 * ty as usize];
                    let mut p = vec![];
                    p.extend_from_slice(&ty.to_le_bytes());
                    p.extend_from_slice(&(data.len() as u16).to_le_bytes());
                    p.extend_from_slice(&data);
                    w.fw.notif_counter += 1;
                    let seq = 0x8000 | w.fw.notif_counter;
                    w.schedule(t, Ev::Post { group: 0, cmd: 0x6b, seq, payload: p });
                    t += 500;
                }
                if !w.fw.skip_init_complete {
                    w.fw.notif_counter += 1;
                    let seq = 0x8000 | w.fw.notif_counter;
                    w.schedule(t + 1_000, Ev::Post { group: 0, cmd: 0x04, seq, payload: vec![] });
                }
            }
        }
        _ => {
            let rp = reply(vec![]);
            w.schedule(rp.delay_us, Ev::Post { group: rp.group, cmd: rp.cmd, seq: rp.seq, payload: rp.payload });
        }
    }
}
