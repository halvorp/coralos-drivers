// SPDX-License-Identifier: GPL-2.0-only
//! A scripted cAVS system for driving the orchestrator: hardware steps, the IPC channel with a firmware that keeps pipeline state, the
//! BDL page and the codec question. Everything the orchestrator asks for is logged IN ORDER, so a test asserts what happened and what
//! never did.
#![allow(dead_code)]
use avs_boot_core::boot::*;
use avs_boot_core::config::*;

/// The Atlas board row (ATLAS-PERIPHERALS-FROM-RECOVERY.md section 3.2): the headset codec on SSP1, the speaker amplifiers on SSP0; the
/// topology's playback widgets (da7219-tplg.bin).
pub const HEADPHONE_SSP_INSTANCE: u32 = 1;
pub const SPEAKER_SSP: [u32; 1] = [0];
pub const ATLAS_FE_WIDGET: &[u8] = b"ssp1p_fe";
pub const ATLAS_BE_WIDGET: &[u8] = b"ssp1p_be";
use avs_boot_core::error::BootFailure;
use avs_boot_core::running::Running;
use avs_fw_core::bdl::{ClDmaLayout, Extent};
use avs_fw_core::effect::{Event as FwEvent, Input as FwInput, Op, Step};
use avs_ipc_core::header::*;
use avs_ipc_core::modcfg::*;
use avs_ipc_core::payload::*;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::Topology;
use hda_ext_core::bdl::plan_bdl;
use hda_ext_core::stream::Streams;

pub const DIR: &str = "/home/peter/Projects/coral_handoffs/atlas/recovery/configs/lib/firmware/intel/avs";

pub fn tplg_bytes() -> Option<Vec<u8>> {
    let d = std::env::var("CORAL_AVS_FW_DIR").unwrap_or_else(|_| DIR.to_string());
    let p = std::path::Path::new(&d).join("da7219-tplg.bin");
    if !p.exists() {
        eprintln!("da7219-tplg.bin: SKIPPED (set CORAL_AVS_FW_DIR=<.../intel/avs>)");
        return None;
    }
    Some(std::fs::read(p).unwrap())
}

/// The upstream speaker topology (thesofproject/avs-topology-xml@221e4a6 i2s/max98373-tplg.xml, built by avsdk + alsatplg), or None.
pub fn speaker_tplg_bytes() -> Option<Vec<u8>> {
    let p = std::env::var("CORAL_AVS_SPK_TPLG").unwrap_or_else(|_| "/home/peter/Projects/coral_handoffs/atlas/recovery/max98373-tplg.upstream-221e4a6.bin".to_string());
    if !std::path::Path::new(&p).exists() {
        eprintln!("max98373-tplg.upstream-221e4a6.bin: SKIPPED (set CORAL_AVS_SPK_TPLG)");
        return None;
    }
    Some(std::fs::read(p).unwrap())
}

pub const SPEAKER_FE_WIDGET: &[u8] = b"ssp0p_fe";
pub const SPEAKER_BE_WIDGET: &[u8] = b"ssp0p_be";

// ---- a hand-built NHLT shaped like coreboot's atlas variant (see avs-topology-core/tests/nhlt.rs) -----------------------------------
fn dw(x: u32) -> [u8; 4] {
    x.to_le_bytes()
}

pub struct NFmt {
    pub ch: u16,
    pub rate: u32,
    pub container: u16,
    pub valid: u16,
    pub mask: u32,
    pub caps: Vec<u8>,
}

fn wave(f: &NFmt) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&0xFFFEu16.to_le_bytes());
    v.extend_from_slice(&f.ch.to_le_bytes());
    v.extend_from_slice(&f.rate.to_le_bytes());
    v.extend_from_slice(&(f.rate * f.ch as u32 * (f.container as u32 / 8)).to_le_bytes());
    v.extend_from_slice(&(f.ch * (f.container / 8)).to_le_bytes());
    v.extend_from_slice(&f.container.to_le_bytes());
    v.extend_from_slice(&22u16.to_le_bytes());
    v.extend_from_slice(&f.valid.to_le_bytes());
    v.extend_from_slice(&f.mask.to_le_bytes());
    v.extend_from_slice(&[0x01, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    v.extend_from_slice(&dw(f.caps.len() as u32));
    v.extend_from_slice(&f.caps);
    v
}

#[allow(clippy::too_many_arguments)]
fn endpoint(link: u8, instance: u8, device_id: u16, dev_type: u8, dir: u8, vbus: u8, dev_caps: &[u8], fmts: &[NFmt], trailer: &[u8]) -> Vec<u8> {
    let mut body = vec![link, instance];
    body.extend_from_slice(&0x8086u16.to_le_bytes());
    body.extend_from_slice(&device_id.to_le_bytes());
    body.extend_from_slice(&1u16.to_le_bytes());
    body.extend_from_slice(&0x0000_0123u32.to_le_bytes());
    body.push(dev_type);
    body.push(dir);
    body.push(vbus);
    body.extend_from_slice(&dw(dev_caps.len() as u32));
    body.extend_from_slice(dev_caps);
    body.push(fmts.len() as u8);
    for f in fmts {
        body.extend(wave(f));
    }
    body.extend_from_slice(trailer);
    let mut v = dw(4 + body.len() as u32).to_vec();
    v.extend(body);
    v
}

pub fn blob(n: usize, seed: u8) -> Vec<u8> {
    (0..n).map(|i| seed.wrapping_add(i as u8)).collect()
}

/// The DA7219 render blob on SSP1 and the MAX98373 render blob on SSP0 have DIFFERENT seeds, so a test can prove which one a message carried.
pub const DA7219_RENDER_SEED: u8 = 0x10;
pub const MAX98373_SEED: u8 = 0x40;

pub fn nhlt_bytes() -> Vec<u8> {
    use avs_topology_core::nhlt::*;
    let tdm = [0u8, 0];
    let da = |seed| NFmt { ch: 2, rate: 48000, container: 32, valid: 24, mask: 3, caps: blob(88, seed) };
    // coreboot's max98373 render format (soc/intel/skylake/nhlt/max98373.c:6-16): 2 channels, 48 kHz, 16 of 16 bits
    let max = NFmt { ch: 2, rate: 48000, container: 16, valid: 16, mask: 3, caps: blob(88, MAX98373_SEED) };
    let eps = [
        endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_RENDER, 1, &tdm, &[da(DA7219_RENDER_SEED)], &[0u8]),
        endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_CAPTURE, 1, &tdm, &[da(0x20)], &[0u8]),
        endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_RENDER, 0, &tdm, &[max], &[0u8]),
    ];
    table(&eps)
}

fn table(eps: &[Vec<u8>]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"NHLT");
    v.extend_from_slice(&[0u8; 4]);
    v.push(1);
    v.push(0);
    v.extend_from_slice(b"GOOGLE");
    v.extend_from_slice(b"ATLASMAX");
    v.extend_from_slice(&dw(0));
    v.extend_from_slice(b"CORL");
    v.extend_from_slice(&dw(1));
    v.push(eps.len() as u8);
    for e in eps {
        v.extend_from_slice(e);
    }
    let n = v.len() as u32;
    v[4..8].copy_from_slice(&dw(n));
    let sum = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v[9] = 0u8.wrapping_sub(sum);
    v
}

// ---- the firmware's tables --------------------------------------------------------------------------------------------------------
fn tlv(out: &mut Vec<u8>, ty: u32, value: &[u8]) {
    out.extend_from_slice(&ty.to_le_bytes());
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
}

#[derive(Clone)]
pub struct ModSpec {
    pub uuid: [u8; 16],
    pub module_id: u16,
    pub loaded: bool,
    pub max_inst: u16,
}

#[derive(Clone)]
pub struct Fw {
    pub version: (u16, u16, u16, u16),
    pub max_ppl: u32,
    pub max_libs: u32,
    pub ssp_ctrl_count: u32,
    pub modules: Vec<ModSpec>,
}

impl Fw {
    pub fn atlas(topo: &Topology<'_>) -> Self {
        let mut modules = vec![ModSpec { uuid: AVS_COPIER_MOD_UUID, module_id: 1, loaded: true, max_inst: 32 }];
        // every module type the topology uses must be in the firmware
        for e in topo.modcfgs_ext.iter() {
            if !modules.iter().any(|m| m.uuid == e.type_uuid) {
                let id = modules.len() as u16 + 1;
                modules.push(ModSpec { uuid: e.type_uuid, module_id: id, loaded: true, max_inst: 8 });
            }
        }
        Self { version: (9, 21, 0, 4899), max_ppl: 8, max_libs: 6, ssp_ctrl_count: 3, modules }
    }

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
            e[2..4].copy_from_slice(&(if m.loaded { 1u16 } else { 0 }).to_le_bytes());
            e[12..28].copy_from_slice(&m.uuid);
            // builtin (0) when loaded, loadable (1) when not
            e[28..32].copy_from_slice(&(if m.loaded { 0u32 } else { 1 }).to_le_bytes());
            e[76..78].copy_from_slice(&m.max_inst.to_le_bytes());
            p.extend_from_slice(&e);
        }
        p
    }
}

// ---- the system -----------------------------------------------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum T {
    Fw(&'static str),
    Ipc { name: &'static str, header: u64, payload: Vec<u8> },
    Bdl(usize),
    Codec,
}

pub struct Sys {
    pub fw: Fw,
    pub trace: Vec<T>,
    /// Answer this hardware step with that input (every time it is asked).
    pub fw_override: Vec<(&'static str, FwInput)>,
    /// Refuse the n-th (0-based) message of that name with this firmware status.
    pub ipc_status: Vec<(&'static str, usize, u32)>,
    /// Fail the n-th message of that name at the IPC layer.
    pub ipc_error: Vec<(&'static str, usize, avs_ipc_core::error::IpcError)>,
    /// After a pipeline-state change to `from`, report this state instead on the next read.
    pub state_lie: Vec<(u8, u32, u32)>,
    pub codec_safe: bool,
    pub counts: Vec<(&'static str, usize)>,
    pub ppl_state: Vec<(u8, u32)>,
}

impl Sys {
    pub fn new(fw: Fw) -> Self {
        Self { fw, trace: vec![], fw_override: vec![], ipc_status: vec![], ipc_error: vec![], state_lie: vec![], codec_safe: true, counts: vec![], ppl_state: vec![] }
    }

    fn bump(&mut self, name: &'static str) -> usize {
        if let Some(c) = self.counts.iter_mut().find(|c| c.0 == name) {
            c.1 += 1;
            c.1 - 1
        } else {
            self.counts.push((name, 1));
            0
        }
    }

    pub fn fw_answer(&mut self, s: &Step) -> FwInput {
        self.trace.push(T::Fw(s.label));
        if let Some((_, a)) = self.fw_override.iter().find(|o| o.0 == s.label) {
            return *a;
        }
        match s.op {
            Op::Update { .. } | Op::Write { .. } | Op::DelayUs(_) | Op::CopyToDma { .. } | Op::IpcSend(_) => FwInput::Done,
            Op::Read { .. } => FwInput::Value(0),
            Op::Poll(p) => FwInput::Value(p.want),
            Op::Wait { event: FwEvent::CldmaIoc, .. } => FwInput::IocStatus(0x04),
            Op::Wait { .. } => FwInput::EventFired,
            Op::IpcAwait { .. } => FwInput::IpcStatus(0),
        }
    }

    /// Returns (header, payload) of the reply, or an IPC-layer error.
    pub fn ipc(&mut self, name: &'static str, header: u64, payload: &[u8]) -> Result<(u64, Vec<u8>), avs_ipc_core::error::IpcError> {
        // the three base-firmware queries share one wire message; name them by their parameter id (messages.h:355-362)
        let name = if name == "large config get" {
            match LARGE_PARAM_ID.get(header) {
                7 => "get fw config",
                8 => "get hw config",
                9 => "get modules info",
                _ => name,
            }
        } else {
            name
        };
        self.trace.push(T::Ipc { name, header, payload: payload.to_vec() });
        let n = self.bump(name);
        if let Some(e) = self.ipc_error.iter().find(|e| e.0 == name && e.1 == n) {
            return Err(e.2);
        }
        if let Some(s) = self.ipc_status.iter().find(|e| e.0 == name && e.1 == n) {
            return Ok((s.2 as u64, vec![]));
        }
        let mut reply_header = 0u64;
        let mut body = vec![];
        match name {
            "get hw config" => body = self.fw.hw_payload(),
            "get fw config" => body = self.fw.fw_payload(),
            "get modules info" => body = self.fw.mods_payload(),
            "set pipeline state" => {
                let id = SET_PPL_ID.get(header) as u8;
                let st = SET_PPL_STATE.get(header);
                self.ppl_state.retain(|p| p.0 != id);
                self.ppl_state.push((id, st));
            }
            "get pipeline state" => {
                let id = GET_PPL_ID.get(header) as u8;
                let mut st = self.ppl_state.iter().find(|p| p.0 == id).map(|p| p.1).unwrap_or(AVS_PPL_STATE_RESET);
                if let Some(l) = self.state_lie.iter().find(|l| l.0 == id && l.1 == st) {
                    st = l.2;
                }
                REPLY_PPL_STATE.put(&mut reply_header, st as u64).unwrap();
            }
            _ => {}
        }
        Ok((reply_header, body))
    }
}

pub fn drive(boot: &mut Boot<'_>, sys: &mut Sys) -> Result<Running, BootFailure> {
    drive_with(boot, sys, Event::Begin)
}

pub fn drive_with(boot: &mut Boot<'_>, sys: &mut Sys, first: Event<'static>) -> Result<Running, BootFailure> {
    let mut ev = first;
    for _ in 0..5000 {
        match boot.next(ev) {
            Action::Fw(step) => ev = Event::Fw(sys.fw_answer(&step)),
            Action::Ipc(m) => match sys.ipc(m.name, m.header, &m.payload) {
                Ok((h, body)) => {
                    // the reply payload must outlive the event; leak it for the test
                    let body: &'static [u8] = Box::leak(body.into_boxed_slice());
                    ev = Event::IpcReply { header: h, payload: body };
                }
                Err(e) => ev = Event::IpcError(e),
            },
            Action::WriteBdl(b) => {
                sys.trace.push(T::Bdl(b.len()));
                ev = Event::BdlWritten;
            }
            Action::AskCodecSafe => {
                sys.trace.push(T::Codec);
                ev = Event::CodecSafe(sys.codec_safe);
            }
            Action::Finished(r) => return Ok(r),
            Action::Failed(f) => return Err(f),
        }
    }
    panic!("the orchestrator did not stop within 5000 actions");
}

// ---- the configuration ------------------------------------------------------------------------------------------------------------
pub const S16_48K: StreamParams = StreamParams { rate: 48000, channels: 2, container_bits: 16, valid_bits: 16 };

pub fn layout() -> ClDmaLayout {
    ClDmaLayout::plan(&[Extent { addr: 0x1000_0000, len: 128 * 1024 }], 0x2000, 128 * 1024, 1).unwrap()
}

pub fn host_setup() -> HostSetup {
    let bdl = plan_bdl(&[Extent { addr: 0x2000_0000, len: 4 * 4096 }], 4 * 4096, 4096, true).unwrap();
    HostSetup { streams: Streams { capture: 7, playback: 7 }, playback_stream: 0, pp: 0x800, spb: Some(0x700), bdl, bdl_addr: 0x3000_0000, posbuf: true, spib: Some(0), format_bits: 16 }
}

pub fn cfg<'a>(topo: &'a Topology<'a>, nhlt: &'a Nhlt<'a>) -> BootConfig<'a> {
    BootConfig {
        topology: topo,
        nhlt,
        layout: layout(),
        basefw_len: 40_000,
        libs: vec![],
        playback: PlaybackSel { fe_widget: ATLAS_FE_WIDGET, be_widget: ATLAS_BE_WIDGET, fe: S16_48K, be: S16_48K, ssp_instance: HEADPHONE_SSP_INSTANCE, speaker_ssp: &SPEAKER_SSP, speaker_ceiling: None },
        host: host_setup(),
        min_fw: avs_fw_core::regs::SKL_MIN_FW_VERSION,
    }
}

pub fn names(sys: &Sys) -> Vec<&'static str> {
    sys.trace
        .iter()
        .filter_map(|t| match t {
            T::Ipc { name, .. } => Some(*name),
            _ => None,
        })
        .collect()
}

pub fn pos(sys: &Sys, f: impl Fn(&T) -> bool) -> Option<usize> {
    sys.trace.iter().position(f)
}

pub fn fw_pos(sys: &Sys, label: &str) -> Option<usize> {
    sys.trace.iter().position(|t| *t == T::Fw(Box::leak(label.to_string().into_boxed_str())))
}

/// The IPC messages of a given name, in order.
pub fn msgs<'s>(sys: &'s Sys, name: &str) -> Vec<(u64, &'s [u8])> {
    sys.trace
        .iter()
        .filter_map(|t| match t {
            T::Ipc { name: n, header, payload } if *n == name => Some((*header, payload.as_slice())),
            _ => None,
        })
        .collect()
}
