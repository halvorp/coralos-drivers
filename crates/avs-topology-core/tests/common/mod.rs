// SPDX-License-Identifier: GPL-2.0-only
//! A tiny topology assembler for the tests: tuple arrays, blocks, widgets, controls.
#![allow(dead_code)]
use avs_topology_core::tokens::*;

pub const UUID: u32 = 0;
pub const STRING: u32 = 1;
pub const BOOL: u32 = 2;
pub const BYTE: u32 = 3;
pub const WORD: u32 = 4;
pub const SHORT: u32 = 5;

pub fn dw(x: u32) -> [u8; 4] {
    x.to_le_bytes()
}

/// An integer tuple array of type `ty` (WORD, SHORT, BYTE or BOOL).
pub fn ints(ty: u32, elems: &[(u32, u32)]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&dw(12 + 8 * elems.len() as u32));
    v.extend_from_slice(&dw(ty));
    v.extend_from_slice(&dw(elems.len() as u32));
    for (t, x) in elems {
        v.extend_from_slice(&dw(*t));
        v.extend_from_slice(&dw(*x));
    }
    v
}

pub fn words(elems: &[(u32, u32)]) -> Vec<u8> {
    ints(WORD, elems)
}

pub fn string(token: u32, s: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&dw(12 + 48));
    v.extend_from_slice(&dw(STRING));
    v.extend_from_slice(&dw(1));
    v.extend_from_slice(&dw(token));
    let mut f = [0u8; 44];
    f[..s.len()].copy_from_slice(s.as_bytes());
    v.extend_from_slice(&f);
    v
}

pub fn uuid(token: u32, u: [u8; 16]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&dw(12 + 20));
    v.extend_from_slice(&dw(UUID));
    v.extend_from_slice(&dw(1));
    v.extend_from_slice(&dw(token));
    v.extend_from_slice(&u);
    v
}

pub fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flatten().copied().collect()
}

/// A block: header + payload.
pub fn block(ty: u32, count: u32, payload: &[u8]) -> Vec<u8> {
    block_with(0x41536F43, 5, ty, count, payload)
}

pub fn block_with(magic: u32, abi: u32, ty: u32, count: u32, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    for x in [magic, abi, 0, ty, 36, 0, payload.len() as u32, 0, count] {
        v.extend_from_slice(&dw(x));
    }
    v.extend_from_slice(payload);
    v
}

pub fn manifest_block(priv_: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&dw(112));
    p.extend_from_slice(&[0u8; 24 + 80]);
    p.extend_from_slice(&dw(priv_.len() as u32));
    p.extend_from_slice(priv_);
    block(8, 1, &p)
}

pub fn name44(s: &str) -> [u8; 44] {
    let mut f = [0u8; 44];
    f[..s.len()].copy_from_slice(s.as_bytes());
    f
}

/// One DAPM widget element (132 bytes + private data + the given control elements).
pub fn widget(name: &str, priv_: &[u8], controls: &[Vec<u8>]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&dw(132));
    v.extend_from_slice(&dw(17)); // SND_SOC_TPLG_DAPM_SCHEDULER
    v.extend_from_slice(&name44(name));
    v.extend_from_slice(&name44(""));
    v.extend_from_slice(&[0u8; 24]); // reg, shift, mask, subseq, invert, ignore_suspend
    v.extend_from_slice(&[0u8; 4]); // event_flags, event_type
    v.extend_from_slice(&dw(controls.len() as u32));
    v.extend_from_slice(&dw(priv_.len() as u32));
    assert_eq!(v.len(), 132);
    v.extend_from_slice(priv_);
    for c in controls {
        v.extend_from_slice(c);
    }
    v
}

pub fn ctl_hdr(ty: u32, name: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&dw(204));
    v.extend_from_slice(&dw(ty));
    v.extend_from_slice(&name44(name));
    v.extend_from_slice(&dw(0x10000020)); // access
    v.extend_from_slice(&[0u8; 12 + 136]); // ops + tlv
    assert_eq!(v.len(), 204);
    v
}

pub fn mixer(name: &str, max: u32, priv_: &[u8]) -> Vec<u8> {
    let mut v = ctl_hdr(1, name);
    v.extend_from_slice(&dw(360));
    v.extend_from_slice(&dw(0)); // min
    v.extend_from_slice(&dw(max));
    v.extend_from_slice(&[0u8; 12 + 128]); // platform_max, invert, num_channels, channel[8]
    v.extend_from_slice(&dw(priv_.len() as u32));
    assert_eq!(v.len(), 360);
    v.extend_from_slice(priv_);
    v
}

pub fn bytes_ctl(name: &str, max: u32, priv_: &[u8]) -> Vec<u8> {
    let mut v = ctl_hdr(2, name);
    v.extend_from_slice(&dw(240)); // size
    v.extend_from_slice(&dw(max));
    v.extend_from_slice(&[0u8; 12]); // mask, base, num_regs
    v.extend_from_slice(&[0u8; 12]); // ext_ops
    v.extend_from_slice(&dw(priv_.len() as u32));
    assert_eq!(v.len(), 240);
    v.extend_from_slice(priv_);
    v
}

pub fn pcm(pcm_name: &str, dai_name: &str, pcm_id: u32, playback: u32, capture: u32, priv_: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&dw(912));
    v.extend_from_slice(&name44(pcm_name));
    v.extend_from_slice(&name44(dai_name));
    v.extend_from_slice(&dw(pcm_id));
    v.extend_from_slice(&dw(pcm_id + 10)); // dai_id
    v.extend_from_slice(&dw(playback));
    v.extend_from_slice(&dw(capture));
    v.extend_from_slice(&dw(0)); // compress
    v.resize(912 - 4, 0);
    v.extend_from_slice(&dw(priv_.len() as u32));
    v.extend_from_slice(priv_);
    v
}

pub fn graph(routes: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut v = Vec::new();
    for (sink, ctl, src) in routes {
        v.extend_from_slice(&name44(sink));
        v.extend_from_slice(&name44(ctl));
        v.extend_from_slice(&name44(src));
    }
    v
}

// ---- AVS manifest pieces -------------------------------------------------------------------------------------------------------

pub fn mf_header(name: &str, version: u32) -> Vec<u8> {
    cat(&[string(AVS_TKN_MANIFEST_NAME_STRING, name), words(&[(AVS_TKN_MANIFEST_VERSION_U32, version)])])
}

pub fn count(token: u32, n: u32) -> Vec<u8> {
    words(&[(token, n)])
}

pub fn afmt(id: u32, rate: u32, depth: u32, ch: u32, valid: u32) -> Vec<u8> {
    words(&[
        (AVS_TKN_AFMT_ID_U32, id),
        (AVS_TKN_AFMT_SAMPLE_RATE_U32, rate),
        (AVS_TKN_AFMT_BIT_DEPTH_U32, depth),
        (AVS_TKN_AFMT_CHANNEL_MAP_U32, 0xFFFF_3210),
        (AVS_TKN_AFMT_CHANNEL_CFG_U32, 1),
        (AVS_TKN_AFMT_INTERLEAVING_U32, 0),
        (AVS_TKN_AFMT_NUM_CHANNELS_U32, ch),
        (AVS_TKN_AFMT_VALID_BIT_DEPTH_U32, valid),
        (AVS_TKN_AFMT_SAMPLE_TYPE_U32, 0),
    ])
}

pub fn base(id: u32, cpc: u32, ibs: u32, obs: u32, pages: u32) -> Vec<u8> {
    words(&[(AVS_TKN_MODCFG_BASE_ID_U32, id), (AVS_TKN_MODCFG_BASE_CPC_U32, cpc), (AVS_TKN_MODCFG_BASE_IBS_U32, ibs), (AVS_TKN_MODCFG_BASE_OBS_U32, obs), (AVS_TKN_MODCFG_BASE_PAGES_U32, pages)])
}

pub const COPIER: [u8; 16] = [0x83, 0x0c, 0xa0, 0x9b, 0x12, 0xca, 0x83, 0x4a, 0x94, 0x3c, 0x1f, 0xa2, 0xe8, 0x2f, 0x9d, 0xda];

pub fn copier_ext(id: u32, out_fmt: u32, dma_type: u32, dma_buf: u32, vindex: Option<u32>) -> Vec<u8> {
    let mut v = cat(&[words(&[(AVS_TKN_MODCFG_EXT_ID_U32, id), (AVS_TKN_MODCFG_CPR_OUT_AFMT_ID_U32, out_fmt), (AVS_TKN_MODCFG_CPR_FEATURE_MASK_U32, 0), (AVS_TKN_MODCFG_CPR_DMA_TYPE_U32, dma_type), (AVS_TKN_MODCFG_CPR_DMABUFF_SIZE_U32, dma_buf)]), uuid(AVS_TKN_MODCFG_EXT_TYPE_UUID, COPIER)]);
    if let Some(x) = vindex {
        v.extend(ints(BYTE, &[(AVS_TKN_MODCFG_CPR_VINDEX_U8, x)]));
    }
    v
}

pub fn pplcfg(id: u32, req: u32, prio: u32, lp: u32, attrs: u32, trigger: u32) -> Vec<u8> {
    cat(&[words(&[(AVS_TKN_PPLCFG_ID_U32, id), (AVS_TKN_PPLCFG_TRIGGER_U32, trigger)]), words(&[(AVS_TKN_PPLCFG_REQ_SIZE_U16, req)]), ints(BYTE, &[(AVS_TKN_PPLCFG_PRIORITY_U8, prio)]), ints(BOOL, &[(AVS_TKN_PPLCFG_LOW_POWER_BOOL, lp)]), words(&[(AVS_TKN_PPLCFG_ATTRIBUTES_U16, attrs)])])
}

pub fn binding(id: u32, tmpl: u32, ppl: u32, tgt_mod: u32, my_mod: u32, tname: &str, tpin: u32, mpin: u32, sink: u32) -> Vec<u8> {
    cat(&[
        words(&[(AVS_TKN_BINDING_ID_U32, id), (AVS_TKN_BINDING_TARGET_PATH_TMPL_ID_U32, tmpl), (AVS_TKN_BINDING_TARGET_PPL_ID_U32, ppl), (AVS_TKN_BINDING_TARGET_MOD_ID_U32, tgt_mod), (AVS_TKN_BINDING_MOD_ID_U32, my_mod)]),
        string(AVS_TKN_BINDING_TARGET_TPLG_NAME_STRING, tname),
        ints(BYTE, &[(AVS_TKN_BINDING_TARGET_MOD_PIN_U8, tpin), (AVS_TKN_BINDING_MOD_PIN_U8, mpin), (AVS_TKN_BINDING_IS_SINK_U8, sink)]),
    ])
}

/// A module section of a pipeline.
pub fn module(id: u32, base: u32, in_fmt: u32, ext: u32) -> Vec<u8> {
    words(&[(AVS_TKN_MOD_ID_U32, id), (AVS_TKN_MOD_MODCFG_BASE_ID_U32, base), (AVS_TKN_MOD_IN_AFMT_ID_U32, in_fmt), (AVS_TKN_MOD_MODCFG_EXT_ID_U32, ext)])
}

pub fn pipeline_hdr(id: u32, cfg: u32, nbind: u32) -> Vec<u8> {
    words(&[(AVS_TKN_PPL_ID_U32, id), (AVS_TKN_PPL_PPLCFG_ID_U32, cfg), (AVS_TKN_PPL_NUM_BINDING_IDS_U32, nbind)])
}

pub fn binding_ref(id: u32) -> Vec<u8> {
    words(&[(AVS_TKN_PPL_BINDING_ID_U32, id)])
}

pub fn path_hdr(id: u32, fe: u32, be: u32) -> Vec<u8> {
    words(&[(AVS_TKN_PATH_ID_U32, id), (AVS_TKN_PATH_FE_FMT_ID_U32, fe), (AVS_TKN_PATH_BE_FMT_ID_U32, be)])
}

pub fn tmpl_hdr(id: u32, name: &str) -> Vec<u8> {
    cat(&[words(&[(AVS_TKN_PATH_TMPL_ID_U32, id)]), string(1802, name)])
}

/// The manifest private data of a small, valid topology: 2 formats, 2 base cfgs, 3 ext cfgs, 1 pplcfg, 2 bindings, 0 libs.
pub fn small_manifest() -> Vec<u8> {
    cat(&[
        mf_header("test", 7),
        count(AVS_TKN_MANIFEST_NUM_LIBRARIES_U32, 0),
        count(AVS_TKN_MANIFEST_NUM_AFMTS_U32, 2),
        afmt(0, 48000, 16, 2, 16),
        afmt(1, 48000, 32, 2, 24),
        count(AVS_TKN_MANIFEST_NUM_MODCFGS_BASE_U32, 2),
        base(0, 12000, 192, 384, 1),
        base(1, 12000, 384, 384, 1),
        count(AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32, 3),
        copier_ext(0, 0, 1, 768, None),
        copier_ext(1, 1, 13, 768, Some(16)),
        copier_ext(2, 1, 12, 768, None),
        count(AVS_TKN_MANIFEST_NUM_PPLCFGS_U32, 1),
        pplcfg(0, 2, 0, 0, 0, 0),
        count(AVS_TKN_MANIFEST_NUM_BINDINGS_U32, 2),
        binding(0, 1, 0, 0, 0, "test", 0, 0, 1),
        binding(1, 3, 0, 0, 0, "test", 0, 0, 0),
        count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 0),
    ])
}
