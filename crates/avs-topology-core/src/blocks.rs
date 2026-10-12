// SPDX-License-Identifier: GPL-2.0-only
//! The ASoC topology container (soc-topology.c) as far as the AVS driver uses it: the 36-byte block header and its validation,
//! the load PASSES, and the element loaders for widgets, PCMs, the DAPM graph and kcontrols.
//!
//! A file is a sequence of blocks, each `snd_soc_tplg_hdr` + `payload_size` bytes. `snd_soc_tplg_component_load` walks the whole
//! file once per pass, loading only the blocks of that pass, so the manifest (pass 0) is loaded before any widget (pass 3).
use crate::error::TplgError;
use crate::grow::try_push;
use crate::model::{PathKind, PathTemplate, SspBinding};
use crate::topology::Topology;
use crate::vendor::*;

pub const SND_SOC_TPLG_MAGIC: u32 = 0x41536F43; // asoc.h:84
/// `SOC_TPLG_MAGIC_BIG_ENDIAN` (soc-topology.c:33).
pub const SOC_TPLG_MAGIC_BIG_ENDIAN: u32 = 0x436F5341; // soc-topology.c:33
pub const SND_SOC_TPLG_ABI_VERSION: u32 = 0x5; // asoc.h:90
pub const SND_SOC_TPLG_ABI_VERSION_MIN: u32 = 0x5; // asoc.h:91

pub const SND_SOC_TPLG_TYPE_MIXER: u32 = 1; // asoc.h:102
pub const SND_SOC_TPLG_TYPE_BYTES: u32 = 2; // asoc.h:103
pub const SND_SOC_TPLG_TYPE_ENUM: u32 = 3; // asoc.h:104
pub const SND_SOC_TPLG_TYPE_DAPM_GRAPH: u32 = 4; // asoc.h:105
pub const SND_SOC_TPLG_TYPE_DAPM_WIDGET: u32 = 5; // asoc.h:106
pub const SND_SOC_TPLG_TYPE_DAI_LINK: u32 = 6; // asoc.h:107
pub const SND_SOC_TPLG_TYPE_PCM: u32 = 7; // asoc.h:108
pub const SND_SOC_TPLG_TYPE_MANIFEST: u32 = 8; // asoc.h:109
pub const SND_SOC_TPLG_TYPE_CODEC_LINK: u32 = 9; // asoc.h:110
pub const SND_SOC_TPLG_TYPE_BACKEND_LINK: u32 = 10; // asoc.h:111
pub const SND_SOC_TPLG_TYPE_PDATA: u32 = 11; // asoc.h:112
pub const SND_SOC_TPLG_TYPE_DAI: u32 = 12; // asoc.h:113
pub const SND_SOC_TPLG_DAPM_SCHEDULER: u32 = 17; // asoc.h:74

/// `sizeof(struct snd_soc_tplg_hdr)` (asoc.h:190-200): nine dwords.
pub const HDR_SIZE: usize = 36; // asoc.h:190-200
/// `sizeof(struct snd_soc_tplg_manifest)` (asoc.h:370-380): 4 + 6*4 + 20*4 + the private size dword.
pub const MANIFEST_SIZE: usize = 112; // asoc.h:370-380
/// `sizeof(struct snd_soc_tplg_dapm_widget)` (asoc.h:480-500).
pub const WIDGET_SIZE: usize = 132; // asoc.h:480-500
/// `sizeof(struct snd_soc_tplg_dapm_graph_elem)` (asoc.h:458-462): three names.
pub const GRAPH_ELEM_SIZE: usize = 132; // asoc.h:458-462
/// `sizeof(struct snd_soc_tplg_pcm)` (asoc.h:513-528).
pub const PCM_SIZE: usize = 912; // asoc.h:513-528
/// `sizeof(struct snd_soc_tplg_ctl_hdr)` (asoc.h:286-293).
pub const CTL_HDR_SIZE: usize = 204; // asoc.h:286-293
/// `sizeof(struct snd_soc_tplg_mixer_control)` (asoc.h:392-402).
pub const MIXER_SIZE: usize = 360; // asoc.h:392-402
/// `sizeof(struct snd_soc_tplg_bytes_control)` (asoc.h:437-446).
pub const BYTES_SIZE: usize = 240; // asoc.h:437-446

/// The load passes (soc-topology.c:41-48), in order.
pub const PASS_MANIFEST: u32 = 0; // soc-topology.c:41
pub const PASS_VENDOR: u32 = 1; // soc-topology.c:42
pub const PASS_CONTROL: u32 = 2; // soc-topology.c:43
pub const PASS_WIDGET: u32 = 3; // soc-topology.c:44
pub const PASS_PCM_DAI: u32 = 4; // soc-topology.c:45
pub const PASS_GRAPH: u32 = 5; // soc-topology.c:46
pub const PASS_BE_DAI: u32 = 6; // soc-topology.c:47
pub const PASS_LINK: u32 = 7; // soc-topology.c:48

fn rd32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// A validated block header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    pub at: usize,
    pub abi: u32,
    pub version: u32,
    pub ty: u32,
    pub vendor_type: u32,
    pub payload_size: u32,
    pub index: u32,
    pub count: u32,
}

impl BlockHeader {
    /// Offset of the next block.
    pub fn next(&self) -> usize {
        self.at + HDR_SIZE + self.payload_size as usize
    }
}

/// `soc_tplg_valid_header` (soc-topology.c:1947-2001), plus a check Linux lacks: that the header itself and the whole payload lie
/// inside the file (Linux compares `offset + payload_size >= size`, which does not count the 36 header bytes).
pub fn valid_header(bytes: &[u8], at: usize) -> Result<BlockHeader, TplgError> {
    let have = bytes.len().saturating_sub(at);
    if have < HDR_SIZE {
        return Err(TplgError::Truncated { what: "topology block header", need: HDR_SIZE, have });
    }
    let h = &bytes[at..at + HDR_SIZE];
    let size = rd32(h, 16);
    if size as usize != HDR_SIZE {
        return Err(TplgError::BadHeaderSize { at, size }); // soc-topology.c:1948-1955
    }
    let magic = rd32(h, 0);
    let payload = rd32(h, 24);
    if (at as u64) + (HDR_SIZE as u64) + (payload as u64) > bytes.len() as u64 {
        return Err(TplgError::BlockPastEnd { at, payload, file_len: bytes.len() }); // soc-topology.c:1957-1963
    }
    if magic == SOC_TPLG_MAGIC_BIG_ENDIAN {
        return Err(TplgError::BigEndian { at }); // soc-topology.c:1966-1973
    }
    if magic != SND_SOC_TPLG_MAGIC {
        return Err(TplgError::BadMagic { at, magic }); // soc-topology.c:1975-1982
    }
    let abi = rd32(h, 4);
    if !(SND_SOC_TPLG_ABI_VERSION_MIN..=SND_SOC_TPLG_ABI_VERSION).contains(&abi) {
        return Err(TplgError::BadAbi { at, abi }); // soc-topology.c:1985-1992
    }
    if payload == 0 {
        return Err(TplgError::EmptyPayload { at }); // soc-topology.c:1994-1998
    }
    Ok(BlockHeader { at, abi, version: rd32(h, 8), ty: rd32(h, 12), vendor_type: rd32(h, 20), payload_size: payload, index: rd32(h, 28), count: rd32(h, 32) })
}

/// `soc_tplg_load_header`'s type -> pass map (soc-topology.c:2004-2058).
pub const fn pass_of(ty: u32) -> u32 {
    match ty {
        SND_SOC_TPLG_TYPE_MIXER | SND_SOC_TPLG_TYPE_ENUM | SND_SOC_TPLG_TYPE_BYTES => PASS_CONTROL,
        SND_SOC_TPLG_TYPE_DAPM_GRAPH => PASS_GRAPH,
        SND_SOC_TPLG_TYPE_DAPM_WIDGET => PASS_WIDGET,
        SND_SOC_TPLG_TYPE_PCM => PASS_PCM_DAI,
        SND_SOC_TPLG_TYPE_DAI => PASS_BE_DAI,
        SND_SOC_TPLG_TYPE_DAI_LINK | SND_SOC_TPLG_TYPE_BACKEND_LINK => PASS_LINK,
        SND_SOC_TPLG_TYPE_MANIFEST => PASS_MANIFEST,
        _ => PASS_VENDOR,
    }
}

/// A DAPM graph element after the machine's SSP naming (`avs_route_load`, topology.c:1799-1837).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Route {
    pub sink: Name,
    pub control: Name,
    pub source: Name,
}

/// The PCM fields the AVS driver's DAI/PCM handling uses (`struct snd_soc_tplg_pcm`, asoc.h:513-528).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pcm {
    pub pcm_name: Name,
    pub dai_name: Name,
    pub pcm_id: u32,
    pub dai_id: u32,
    pub playback: bool,
    pub capture: bool,
    pub compress: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Mixer,
    /// BYTES controls are what the Pixelbook Go's recovery topologies use for the speaker-protection data ("DSM data", "WAVES
    /// data"). Mainline `avs_control_load` refuses them (`-EINVAL`, topology.c:2168-2169); the recovery image's driver evidently
    /// does not, so they are parsed here and left for the speaker-protection work to interpret.
    Bytes,
}

/// A kcontrol: its name, its `AVS_TKN_KCONTROL_ID_U32`, and where it hangs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub kind: ControlKind,
    pub name: Name,
    pub access: u32,
    /// `ctl_data->id`: what a module's `AVS_TKN_MOD_KCONTROL_ID_U32` refers to.
    pub id: u32,
    pub max: u32,
    /// The widget (in file order) the control belongs to, or `None` for a stand-alone control block.
    pub widget: Option<u32>,
}

impl Default for Control {
    fn default() -> Self {
        Self { kind: ControlKind::Mixer, name: Name::default(), access: 0, id: 0, max: 0, widget: None }
    }
}

fn name_at(b: &[u8], at: usize) -> Result<Name, TplgError> {
    let mut f = [0u8; NAME_LEN];
    f.copy_from_slice(&b[at..at + NAME_LEN]);
    Name::from_field(&f, 0)
}

fn rename(ssp: Option<SspBinding>, n: Name) -> Result<Name, TplgError> {
    match ssp {
        Some(s) => crate::model::ssp_sprint(n.as_bytes(), s.port, s.tdm),
        None => Ok(n),
    }
}

/// Load `bytes` into `t`: the whole file, pass by pass.
pub fn load<'a>(t: &mut Topology<'a>, bytes: &'a [u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
    for pass in PASS_MANIFEST..=PASS_LINK {
        let mut at = 0usize;
        while at < bytes.len() {
            let h = valid_header(bytes, at)?;
            if pass_of(h.ty) == pass {
                let body = &bytes[at + HDR_SIZE..h.next()];
                load_block(t, &h, body, ssp)?;
            }
            at = h.next();
        }
    }
    Ok(())
}

fn load_block<'a>(t: &mut Topology<'a>, h: &BlockHeader, body: &'a [u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
    match h.ty {
        SND_SOC_TPLG_TYPE_MANIFEST => load_manifest_block(t, body, ssp),
        SND_SOC_TPLG_TYPE_MIXER | SND_SOC_TPLG_TYPE_BYTES | SND_SOC_TPLG_TYPE_ENUM => load_controls(t, h, body, None),
        SND_SOC_TPLG_TYPE_DAPM_WIDGET => load_widgets(t, h, body, ssp),
        SND_SOC_TPLG_TYPE_PCM => load_pcms(t, h, body),
        SND_SOC_TPLG_TYPE_DAPM_GRAPH => load_graph(t, h, body, ssp),
        // physical link / DAI configuration: validated, nothing in the AVS path graph (avs_dai_load, avs_link_load only set flags)
        SND_SOC_TPLG_TYPE_DAI | SND_SOC_TPLG_TYPE_DAI_LINK | SND_SOC_TPLG_TYPE_BACKEND_LINK => Ok(()),
        ty => Err(TplgError::UnsupportedBlockType { ty }),
    }
}

/// `soc_tplg_manifest_load` (soc-topology.c:1927-1945).
fn load_manifest_block<'a>(t: &mut Topology<'a>, body: &'a [u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
    if t.have_manifest {
        return Err(TplgError::DuplicateBlock { ty: SND_SOC_TPLG_TYPE_MANIFEST });
    }
    if body.len() < MANIFEST_SIZE {
        return Err(TplgError::Truncated { what: "manifest", need: MANIFEST_SIZE, have: body.len() });
    }
    // "check ABI version by size"
    let size = rd32(body, 0);
    if size as usize != MANIFEST_SIZE {
        return Err(TplgError::BadElementSize { what: "manifest", size, want: MANIFEST_SIZE as u32 });
    }
    let priv_size = rd32(body, MANIFEST_SIZE - 4) as usize;
    let end = MANIFEST_SIZE.checked_add(priv_size).filter(|&e| e <= body.len()).ok_or(TplgError::Truncated { what: "manifest private data", need: MANIFEST_SIZE.saturating_add(priv_size), have: body.len() })?;
    t.have_manifest = true;
    t.load_manifest(&body[MANIFEST_SIZE..end], ssp)
}

/// One control element at `body[p..]`; returns where the next element starts. `widget` is the owning widget, if any.
fn load_control_at(t: &mut Topology<'_>, body: &[u8], p: usize, widget: Option<u32>) -> Result<usize, TplgError> {
    if body.len() < p + CTL_HDR_SIZE + 4 {
        return Err(TplgError::Truncated { what: "control", need: p + CTL_HDR_SIZE + 4, have: body.len() });
    }
    let hsize = rd32(body, p);
    if hsize as usize != CTL_HDR_SIZE {
        return Err(TplgError::BadElementSize { what: "control header", size: hsize, want: CTL_HDR_SIZE as u32 }); // soc-topology.c:985-988
    }
    let ty = rd32(body, p + 4);
    let (kind, fixed) = match ty {
        SND_SOC_TPLG_TYPE_MIXER => (ControlKind::Mixer, MIXER_SIZE),
        SND_SOC_TPLG_TYPE_BYTES => (ControlKind::Bytes, BYTES_SIZE),
        // `avs_control_load`: "default: return -EINVAL" (topology.c:2168-2169)
        _ => return Err(if widget.is_some() { TplgError::WidgetControlType { ty } } else { TplgError::UnsupportedControlType { ty } }),
    };
    if body.len() < p + fixed {
        return Err(TplgError::Truncated { what: "control", need: p + fixed, have: body.len() });
    }
    let size = rd32(body, p + CTL_HDR_SIZE);
    if size as usize != fixed {
        return Err(TplgError::BadElementSize { what: "control", size, want: fixed as u32 });
    }
    let priv_size = rd32(body, p + fixed - 4) as usize;
    let end = (p + fixed).checked_add(priv_size).filter(|&e| e <= body.len()).ok_or(TplgError::Truncated { what: "control private data", need: (p + fixed).saturating_add(priv_size), have: body.len() })?;
    let mut c = Control { kind, name: name_at(body, p + 8)?, access: rd32(body, p + 52), widget, ..Control::default() };
    // mixer: size, min, max (offset 8); bytes: size, max (offset 4)
    c.max = rd32(body, p + CTL_HDR_SIZE + if kind == ControlKind::Mixer { 8 } else { 4 });
    if priv_size > 0 {
        parse_tokens(&mut crate::model::ControlIdSink { id: &mut c.id }, &body[p + fixed..end])?;
    }
    try_push(&mut t.controls, "controls", c)?;
    Ok(end)
}

/// `soc_tplg_kcontrol_elems_load` (soc-topology.c:973-1013).
fn load_controls(t: &mut Topology<'_>, h: &BlockHeader, body: &[u8], widget: Option<u32>) -> Result<(), TplgError> {
    let mut p = 0usize;
    for _ in 0..h.count {
        p = load_control_at(t, body, p, widget)?;
    }
    Ok(())
}

/// `soc_tplg_dapm_widget_elems_load` + `avs_widget_load` (soc-topology.c:1258-1300, topology.c:1839-1894).
fn load_widgets<'a>(t: &mut Topology<'a>, h: &BlockHeader, body: &'a [u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
    let mut p = 0usize;
    for _ in 0..h.count {
        if body.len() < p + WIDGET_SIZE {
            return Err(TplgError::Truncated { what: "widget", need: p + WIDGET_SIZE, have: body.len() });
        }
        let size = rd32(body, p);
        if size as usize != WIDGET_SIZE {
            return Err(TplgError::BadElementSize { what: "widget", size, want: WIDGET_SIZE as u32 });
        }
        let name = name_at(body, p + 8)?;
        let _sname = name_at(body, p + 52)?;
        let num_kcontrols = rd32(body, p + 124);
        let priv_size = rd32(body, p + WIDGET_SIZE - 4) as usize;
        let priv_end = (p + WIDGET_SIZE).checked_add(priv_size).filter(|&e| e <= body.len()).ok_or(TplgError::Truncated { what: "widget private data", need: (p + WIDGET_SIZE).saturating_add(priv_size), have: body.len() })?;
        let widget_index = t.widgets_seen;
        t.widgets_seen += 1;
        // "if (!le32_to_cpu(dw->priv.size)) return 0;" - a widget without private data has no path template
        if priv_size > 0 {
            let mut tmpl = PathTemplate { widget_name: rename(ssp, name)?, ..PathTemplate::default() };
            t.parse_path_template(&body[p + WIDGET_SIZE..priv_end], &mut tmpl, PathKind::Plain)?;
            try_push(&mut t.templates, "path templates", tmpl)?;
        }
        p = priv_end;
        for _ in 0..num_kcontrols {
            p = load_control_at(t, body, p, Some(widget_index))?;
        }
    }
    Ok(())
}

/// `soc_tplg_pcm_elems_load` (soc-topology.c:1560-1595).
fn load_pcms(t: &mut Topology<'_>, h: &BlockHeader, body: &[u8]) -> Result<(), TplgError> {
    let mut p = 0usize;
    for _ in 0..h.count {
        if body.len() < p + PCM_SIZE {
            return Err(TplgError::Truncated { what: "pcm", need: p + PCM_SIZE, have: body.len() });
        }
        let size = rd32(body, p);
        if size as usize != PCM_SIZE {
            return Err(TplgError::BadElementSize { what: "pcm", size, want: PCM_SIZE as u32 });
        }
        let priv_size = rd32(body, p + PCM_SIZE - 4) as usize;
        let end = (p + PCM_SIZE).checked_add(priv_size).filter(|&e| e <= body.len()).ok_or(TplgError::Truncated { what: "pcm private data", need: (p + PCM_SIZE).saturating_add(priv_size), have: body.len() })?;
        try_push(&mut t.pcms, "pcms",
            Pcm {
                pcm_name: name_at(body, p + 4)?,
                dai_name: name_at(body, p + 48)?,
                pcm_id: rd32(body, p + 92),
                dai_id: rd32(body, p + 96),
                playback: rd32(body, p + 100) != 0,
                capture: rd32(body, p + 104) != 0,
                compress: rd32(body, p + 108) != 0,
            },
        )?;
        p = end;
    }
    Ok(())
}

/// `soc_tplg_dapm_graph_elems_load` (soc-topology.c:1026-1095) + `avs_route_load` (topology.c:1799-1837).
fn load_graph(t: &mut Topology<'_>, h: &BlockHeader, body: &[u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
    // `soc_tplg_check_elem_count`: the elements must fit in the chunk
    let need = (h.count as usize).saturating_mul(GRAPH_ELEM_SIZE);
    if need > body.len() {
        return Err(TplgError::Truncated { what: "graph elements", need, have: body.len() });
    }
    for i in 0..h.count as usize {
        let p = i * GRAPH_ELEM_SIZE;
        let sink = name_at(body, p)?;
        let control = name_at(body, p + 44)?;
        let source = name_at(body, p + 88)?;
        let r = Route {
            sink: rename(ssp, sink)?,
            control: if control.is_empty() { control } else { rename(ssp, control)? },
            source: rename(ssp, source)?,
        };
        try_push(&mut t.routes, "routes", r)?;
    }
    Ok(())
}
