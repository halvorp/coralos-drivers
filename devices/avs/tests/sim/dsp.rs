// SPDX-License-Identifier: GPL-2.0-only
//! The cAVS DSP (BAR4) as a register-level model: the core power/reset/stall handshake, the ROM that announces itself when the core is
//! released, the code loader's DMA (which "reads" the image the host moved into its buffer, in order, so a test can compare it byte for
//! byte), the HIPC mailbox and a firmware that answers the messages the headphone path sends - keeping pipeline state - and raises
//! FW_READY once the base firmware has been delivered.
#![allow(dead_code)]
use avs_fw_core::regs::*;
use avs_ipc_core::header::*;
use avs_ipc_core::modcfg::AVS_COPIER_MOD_UUID;
use avs_ipc_core::payload::*;
use avs_ipc_core::notify::{AVS_NOTIFY_EXCEPTION_CAUGHT, AVS_NOTIFY_FW_READY};
use avs_ipc_core::regs::{AVS_MAILBOX_SIZE, DOWNLINK_OFFSET, SKL_ADSP_HIPCIE_DONE, SKL_ADSP_HIPCI_BUSY, SKL_ADSP_HIPCT_BUSY, SKL_ADSP_REG_HIPCI, SKL_ADSP_REG_HIPCIE, SKL_ADSP_REG_HIPCT, UPLINK_OFFSET};

pub const LEN: usize = 0x10000;

#[derive(Clone)]
pub struct ModSpec {
    pub uuid: [u8; 16],
    pub module_id: u16,
    pub max_inst: u16,
}

#[derive(Clone)]
pub struct Firmware {
    pub version: (u16, u16, u16, u16),
    pub max_ppl: u32,
    pub max_libs: u32,
    pub ssp_ctrl_count: u32,
    pub modules: Vec<ModSpec>,
}

fn tlv(out: &mut Vec<u8>, ty: u32, value: &[u8]) {
    out.extend_from_slice(&ty.to_le_bytes());
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
}

impl Firmware {
    pub fn hw_payload(&self) -> Vec<u8> {
        let mut p = Vec::new();
        tlv(&mut p, AVS_HW_CFG_DSP_CORES, &2u32.to_le_bytes());
        let mut caps = Vec::new();
        caps.extend_from_slice(&1u32.to_le_bytes());
        caps.extend_from_slice(&self.ssp_ctrl_count.to_le_bytes());
        for i in 0..self.ssp_ctrl_count {
            caps.extend_from_slice(&(0x1000 * (i + 1)).to_le_bytes());
        }
        tlv(&mut p, AVS_HW_CFG_I2S_CAPS, &caps);
        p
    }
    pub fn fw_payload(&self) -> Vec<u8> {
        let mut p = Vec::new();
        let (a, b, c, d) = self.version;
        let mut v = Vec::new();
        for x in [a, b, c, d] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        tlv(&mut p, AVS_FW_CFG_FW_VERSION, &v);
        tlv(&mut p, AVS_FW_CFG_MAX_PPL_COUNT, &self.max_ppl.to_le_bytes());
        tlv(&mut p, AVS_FW_CFG_MAX_LIBS_COUNT, &self.max_libs.to_le_bytes());
        tlv(&mut p, AVS_FW_CFG_MODULES_COUNT, &(self.modules.len() as u32).to_le_bytes());
        p
    }
    pub fn mods_payload(&self) -> Vec<u8> {
        let mut p = (self.modules.len() as u32).to_le_bytes().to_vec();
        for m in &self.modules {
            let mut e = vec![0u8; MODULE_ENTRY_SIZE];
            e[0..2].copy_from_slice(&m.module_id.to_le_bytes());
            e[2..4].copy_from_slice(&1u16.to_le_bytes());
            e[12..28].copy_from_slice(&m.uuid);
            e[28..32].copy_from_slice(&0u32.to_le_bytes());
            e[76..78].copy_from_slice(&m.max_inst.to_le_bytes());
            p.extend_from_slice(&e);
        }
        p
    }
}

/// What the firmware does with one request (a test's hook decides).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Normal,
    /// Reply with this firmware status.
    Status(u32),
    /// Never reply.
    Silent,
    /// Raise an EXCEPTION_CAUGHT notification instead of replying.
    Exception,
}

/// A test's decision about one request.
pub type Hook = Box<dyn FnMut(&Req) -> Act>;

/// A request the firmware saw.
#[derive(Clone, Debug)]
pub struct Req {
    pub header: u64,
    pub target: u32,
    pub ty: u32,
    pub payload: Vec<u8>,
}

pub struct Dsp {
    pub mem: Vec<u8>,
    pub fw: Firmware,
    /// The length of the base firmware body the host must deliver before the ROM reports it entered.
    pub basefw_len: usize,
    /// Knobs.
    pub rom_never_ready: bool,
    pub fw_never_ready: bool,
    pub hook: Option<Hook>,
    /// Everything seen.
    pub reqs: Vec<Req>,
    pub ppl_state: Vec<(u8, u32)>,
    pub libs_loaded: u32,
    // HIPC
    busy: bool,
    done: bool,
    rsp: u64,
    // code loader
    cl_spib_last: u32,
    cl_written: u64,
    cl_consumed: u64,
    pub cl_ioc: bool,
    /// The bytes the code loader read, one entry per transfer (between RUN going up and coming down).
    pub transfers: Vec<Vec<u8>>,
    cl_run: bool,
    pub fw_ready_sent: bool,
    pub notifications: Vec<u64>,
}

impl Dsp {
    pub fn new(fw: Firmware, basefw_len: usize) -> Self {
        let mut d = Self {
            mem: vec![0; LEN],
            fw,
            basefw_len,
            rom_never_ready: false,
            fw_never_ready: false,
            hook: None,
            reqs: vec![],
            ppl_state: vec![],
            libs_loaded: 0,
            busy: false,
            done: false,
            rsp: 0,
            cl_spib_last: 0,
            cl_written: 0,
            cl_consumed: 0,
            cl_ioc: false,
            transfers: vec![],
            cl_run: false,
            fw_ready_sent: false,
            notifications: vec![],
        };
        d.w32(AVS_ADSP_REG_ADSPCS, 0x0000_0101); // reset and stall asserted, core off
        d
    }

    pub fn r8(&self, off: u32) -> u8 {
        self.mem[off as usize]
    }
    pub fn r32(&self, off: u32) -> u32 {
        u32::from_le_bytes(self.mem[off as usize..off as usize + 4].try_into().unwrap())
    }
    pub fn w32(&mut self, off: u32, v: u32) {
        self.mem[off as usize..off as usize + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn adspic(&self) -> u32 {
        self.r32(AVS_ADSP_REG_ADSPIC)
    }
    fn hipcctl(&self) -> u32 {
        self.r32(SKL_ADSP_REG_HIPCCTL)
    }

    /// The `ADSPIS` register: the IPC cause (a response waiting or an ack, enabled in `HIPCCTL`) and the code loader's.
    pub fn adspis(&self) -> u32 {
        let ctl = self.hipcctl();
        let ipc = (self.busy && ctl & AVS_ADSP_HIPCCTL_BUSY != 0) || (self.done && ctl & AVS_ADSP_HIPCCTL_DONE != 0);
        let mut v = 0;
        if ipc && self.adspic() & AVS_ADSP_ADSPIC_IPC != 0 {
            v |= AVS_ADSP_ADSPIS_IPC;
        }
        if self.cl_ioc && self.adspic() & AVS_ADSP_ADSPIC_CLDMA != 0 {
            v |= AVS_ADSP_ADSPIS_CLDMA;
        }
        v
    }

    pub fn irq(&self) -> bool {
        self.adspis() != 0
    }

    pub fn read(&mut self, off: u32, width: u32) -> u32 {
        let v = match off {
            o if o == AVS_ADSP_REG_ADSPIS && width == 4 => return self.adspis(),
            o if o == SKL_ADSP_REG_HIPCT && width == 4 => return (self.rsp as u32 & 0x7fff_ffff) | if self.busy { SKL_ADSP_HIPCT_BUSY } else { 0 },
            o if o == 0x44 && width == 4 => return (self.rsp >> 32) as u32,
            o if o == SKL_ADSP_REG_HIPCIE && width == 4 => return self.r32(SKL_ADSP_REG_HIPCIE) & !SKL_ADSP_HIPCIE_DONE | if self.done { SKL_ADSP_HIPCIE_DONE } else { 0 },
            _ => 0,
        };
        let _ = v;
        let o = off as usize;
        match width {
            1 => self.mem[o] as u32,
            2 => u16::from_le_bytes([self.mem[o], self.mem[o + 1]]) as u32,
            _ => self.r32(off),
        }
    }

    /// A write; `read_mem` reads DMA memory (the code loader's buffer).
    pub fn write(&mut self, off: u32, width: u32, v: u32, read_mem: &dyn Fn(u64, usize) -> Vec<u8>) -> Vec<u8> {
        let cl = |reg: u32| cl_sd(reg);
        match (off, width) {
            (o, 4) if o == AVS_ADSP_REG_ADSPCS => {
                let mut v = v;
                // CPA follows SPA (core power-up acknowledged)
                v = (v & !(1 << 24)) | (((v >> 16) & 1) << 24);
                self.w32(o, v);
                if v & (1 << 16) != 0 && v & 1 == 0 && v & (1 << 8) == 0 && !self.rom_never_ready && self.r32(AVS_FW_REG_STATUS) == 0 {
                    self.w32(AVS_FW_REG_STATUS, AVS_ROM_INIT_DONE);
                }
                if v & (1 << 16) == 0 {
                    // core powered off: the ROM state is gone
                    self.w32(AVS_FW_REG_STATUS, 0);
                    self.fw_ready_sent = false;
                    self.cl_written = 0;
                    self.cl_consumed = 0;
                    self.cl_spib_last = 0;
                    self.busy = false;
                    self.done = false;
                }
                vec![]
            }
            (o, 4) if o == SKL_ADSP_REG_HIPCT => {
                if v & SKL_ADSP_HIPCT_BUSY != 0 {
                    self.busy = false;
                }
                vec![]
            }
            (o, 4) if o == SKL_ADSP_REG_HIPCIE => {
                if v & SKL_ADSP_HIPCIE_DONE != 0 {
                    self.done = false;
                }
                self.w32(o, v & !SKL_ADSP_HIPCIE_DONE);
                vec![]
            }
            (o, 4) if o == SKL_ADSP_REG_HIPCI => {
                self.w32(o, v);
                if v & SKL_ADSP_HIPCI_BUSY != 0 {
                    let ext = self.r32(SKL_ADSP_REG_HIPCIE) as u64;
                    let header = (ext << 32) | (v & !SKL_ADSP_HIPCI_BUSY) as u64;
                    self.request(header);
                }
                vec![]
            }
            (o, 1) if o == cl(AZX_REG_SD_CTL) => {
                let was = self.cl_run;
                self.mem[o as usize] = v as u8;
                self.cl_run = v & SD_CTL_DMA_START != 0;
                if self.cl_run && !was {
                    self.transfers.push(vec![]);
                    self.cl_consume(read_mem);
                }
                vec![]
            }
            (o, 4) if o == cl(AZX_REG_SD_CTL) => {
                self.w32(o, v);
                vec![]
            }
            (o, 4) if o == cl(AZX_REG_CL_SD_SPIB) => {
                let size = self.r32(cl(AZX_REG_SD_CBL)).max(1);
                let last = self.cl_spib_last;
                let delta = if v > last { v - last } else if v == last { size } else { v + size - last };
                self.cl_spib_last = v;
                self.w32(o, v);
                self.cl_written += delta as u64;
                self.cl_ioc = false;
                if self.cl_run {
                    self.cl_consume(read_mem);
                }
                vec![]
            }
            _ => {
                let o = off as usize;
                match width {
                    1 => self.mem[o] = v as u8,
                    2 => self.mem[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes()),
                    _ => self.w32(off, v),
                }
                vec![]
            }
        }
    }

    /// The code loader reads what the host wrote since the last read.
    fn cl_consume(&mut self, read_mem: &dyn Fn(u64, usize) -> Vec<u8>) {
        let avail = self.cl_written - self.cl_consumed;
        if avail == 0 {
            return;
        }
        let size = self.r32(cl_sd(AZX_REG_SD_CBL)) as u64;
        let base = self.r32(cl_sd(AZX_REG_SD_BDLPL)) as u64 | (self.r32(cl_sd(AZX_REG_SD_BDLPU)) as u64) << 32;
        // the ring's base: the first BDL entry's address
        let entry = read_mem(base & !0x7f, 16);
        let ring = u32::from_le_bytes(entry[0..4].try_into().unwrap()) as u64 | (u32::from_le_bytes(entry[4..8].try_into().unwrap()) as u64) << 32;
        let mut n = avail;
        let mut pos = self.cl_consumed % size;
        while n > 0 {
            let chunk = n.min(size - pos);
            let data = read_mem(ring + pos, chunk as usize);
            if let Some(t) = self.transfers.last_mut() {
                t.extend_from_slice(&data);
            }
            pos = (pos + chunk) % size;
            n -= chunk;
        }
        self.cl_consumed = self.cl_written;
        self.cl_ioc = true;
        self.mem[cl_sd(AZX_REG_SD_STS) as usize] |= SD_INT_COMPLETE as u8;
        // the base firmware, delivered whole: the ROM reports it entered and the firmware announces itself
        let total: usize = self.transfers.first().map_or(0, |t| t.len());
        if self.r32(AVS_FW_REG_STATUS) == AVS_ROM_INIT_DONE && total >= self.basefw_len && !self.rom_never_ready {
            self.w32(AVS_FW_REG_STATUS, SKL_ROM_BASEFW_ENTERED);
            if !self.fw_never_ready && !self.fw_ready_sent {
                self.fw_ready_sent = true;
                self.notify(notification(AVS_NOTIFY_FW_READY).unwrap());
            }
        }
    }

    /// Put a notification in the response registers.
    pub fn notify(&mut self, header: u64) {
        self.rsp = header;
        self.busy = true;
        self.notifications.push(header);
    }

    fn reply_header(req: u64, status: u32) -> u64 {
        let mut h = 0u64;
        MSG_TYPE.put(&mut h, MSG_TYPE.get(req) as u64).unwrap();
        MSG_TARGET.put(&mut h, MSG_TARGET.get(req) as u64).unwrap();
        MSG_DIRECTION.put(&mut h, AVS_MSG_REPLY as u64).unwrap();
        REPLY_STATUS.put(&mut h, status as u64).unwrap();
        h
    }

    fn request(&mut self, header: u64) {
        let target = MSG_TARGET.get(header);
        let ty = MSG_TYPE.get(header);
        // the payload is whatever sits in the downlink window; a test needs it only for messages that carry one
        let payload = self.mem[DOWNLINK_OFFSET as usize..DOWNLINK_OFFSET as usize + 256].to_vec();
        let req = Req { header, target, ty, payload };
        let act = self.hook.as_mut().map_or(Act::Normal, |h| h(&req));
        self.reqs.push(req);
        // the DSP takes the request: BUSY clears, DONE (ack) rises
        self.done = true;
        match act {
            Act::Silent => return,
            Act::Exception => {
                let mut n = notification(AVS_NOTIFY_EXCEPTION_CAUGHT).unwrap();
                NOTIFY_COREDUMP_CORE_ID.put(&mut n, 0).unwrap();
                self.notify(n);
                return;
            }
            Act::Status(s) => {
                self.rsp = Self::reply_header(header, s);
                self.busy = true;
                return;
            }
            Act::Normal => {}
        }
        let mut reply = Self::reply_header(header, 0);
        let mut body: Vec<u8> = vec![];
        if target == AVS_MOD_MSG {
            if ty == AVS_MOD_LARGE_CONFIG_GET {
                body = match LARGE_PARAM_ID.get(header) {
                    7 => self.fw.fw_payload(),
                    8 => self.fw.hw_payload(),
                    9 => self.fw.mods_payload(),
                    _ => vec![],
                };
                REPLY_LARGE_DATA_OFF_SIZE.put(&mut reply, body.len() as u64).unwrap();
                REPLY_LARGE_FINAL_BLOCK.put(&mut reply, 1).unwrap();
            }
        } else {
            match ty {
                AVS_GLB_SET_PIPELINE_STATE => {
                    let id = SET_PPL_ID.get(header) as u8;
                    let st = SET_PPL_STATE.get(header);
                    self.ppl_state.retain(|p| p.0 != id);
                    self.ppl_state.push((id, st));
                }
                AVS_GLB_GET_PIPELINE_STATE => {
                    let id = GET_PPL_ID.get(header) as u8;
                    let st = self.ppl_state.iter().find(|p| p.0 == id).map_or(AVS_PPL_STATE_RESET, |p| p.1);
                    REPLY_PPL_STATE.put(&mut reply, st as u64).unwrap();
                }
                AVS_GLB_LOAD_LIBRARY => self.libs_loaded += 1,
                _ => {}
            }
        }
        let n = body.len().min(AVS_MAILBOX_SIZE);
        self.mem[UPLINK_OFFSET as usize..UPLINK_OFFSET as usize + n].copy_from_slice(&body[..n]);
        self.rsp = reply;
        self.busy = true;
    }
}

pub fn atlas_firmware(module_uuids: &[[u8; 16]]) -> Firmware {
    let mut modules = vec![ModSpec { uuid: AVS_COPIER_MOD_UUID, module_id: 1, max_inst: 32 }];
    for u in module_uuids {
        if !modules.iter().any(|m| &m.uuid == u) {
            let id = modules.len() as u16 + 1;
            modules.push(ModSpec { uuid: *u, module_id: id, max_inst: 8 });
        }
    }
    Firmware { version: (9, 21, 0, 4899), max_ppl: 8, max_libs: 6, ssp_ctrl_count: 3, modules }
}
