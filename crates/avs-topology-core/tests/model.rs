// SPDX-License-Identifier: GPL-2.0-only
//! The per-object token tables (topology.c's `*_parsers[]`): each token lands in its field, and nothing else does.
mod common;
use avs_topology_core::error::TplgError;
use avs_topology_core::model::*;
use avs_topology_core::tokens::*;
use avs_topology_core::topology::Topology;
use avs_topology_core::vendor::{parse_tokens, Name};
use common::*;

fn name(s: &str) -> Name {
    Name::from_bytes(s.as_bytes()).unwrap()
}

#[test]
fn ssp_sprint_replaces_the_first_percent_d() {
    // avs_ssp_sprint (topology.c:376-396)
    let s = |f: &str, p, t| ssp_sprint(f.as_bytes(), p, t).unwrap().as_str().unwrap().to_string();
    assert_eq!(s("ssp%d Tx", 0, 0), "ssp0 Tx");
    assert_eq!(s("ssp%dp_fe", 1, 0), "ssp1p_fe");
    assert_eq!(s("ssp%d Rx", 1, 2), "ssp1:2 Rx", "a TDM slot is appended as :slot");
    assert_eq!(s("ssp%d", 5, 0), "ssp5");
    assert_eq!(s("%d", 3, 0), "3");
    assert_eq!(s("%d-end", 3, 1), "3:1-end");
    assert_eq!(s("ssp%d and %d", 2, 0), "ssp2 and %d", "only the first %d");
    assert_eq!(s("no format", 2, 3), "no format");
    assert_eq!(s("ssp%d", 10, 0), "ssp10");
    assert_eq!(s("ssp%d", 0, 12), "ssp0:12");
    assert_eq!(s("", 1, 0), "");
    assert_eq!(s("100%", 1, 0), "100%");
    assert_eq!(s("%x %d", 4, 0), "%x 4");
    assert_eq!(s("ssp%d", 4_000_000_000, 0), "ssp4000000000");
    // the result must fit the 44-byte field with its NUL
    let long = "a".repeat(40);
    assert_eq!(ssp_sprint(format!("{long}%d").as_bytes(), 1, 0).unwrap().as_bytes().len(), 41);
    assert_eq!(ssp_sprint(format!("{long}%d").as_bytes(), 12345, 0), Err(TplgError::NameTooLong));
    assert_eq!(ssp_sprint("a".repeat(44).as_bytes(), 0, 0), Err(TplgError::NameTooLong));
}

#[test]
fn audio_format_tokens() {
    let mut f = avs_ipc_core::modcfg::AudioFormat::default();
    let b = words(&[
        (AVS_TKN_AFMT_ID_U32, 99),
        (AVS_TKN_AFMT_SAMPLE_RATE_U32, 44100),
        (AVS_TKN_AFMT_BIT_DEPTH_U32, 24),
        (AVS_TKN_AFMT_CHANNEL_MAP_U32, 0x1234),
        (AVS_TKN_AFMT_CHANNEL_CFG_U32, 7),
        (AVS_TKN_AFMT_INTERLEAVING_U32, 1),
        (AVS_TKN_AFMT_NUM_CHANNELS_U32, 6),
        (AVS_TKN_AFMT_VALID_BIT_DEPTH_U32, 20),
        (AVS_TKN_AFMT_SAMPLE_TYPE_U32, 4),
    ]);
    parse_tokens(&mut f, &b).unwrap();
    assert_eq!((f.sampling_freq, f.bit_depth, f.channel_map, f.channel_config, f.interleaving, f.num_channels, f.valid_bit_depth, f.sample_type), (44100, 24, 0x1234, 7, 1, 6, 20, 4));
    // the three bit-fields are 8 bits wide
    for tok in [AVS_TKN_AFMT_NUM_CHANNELS_U32, AVS_TKN_AFMT_VALID_BIT_DEPTH_U32, AVS_TKN_AFMT_SAMPLE_TYPE_U32] {
        assert_eq!(parse_tokens(&mut f, &words(&[(tok, 256)])), Err(TplgError::ValueTooWide { token: tok, value: 256, bits: 8 }));
        assert!(parse_tokens(&mut f, &words(&[(tok, 255)])).is_ok());
    }
    // other tokens are ignored
    let before = f;
    parse_tokens(&mut f, &words(&[(AVS_TKN_MODCFG_BASE_CPC_U32, 5)])).unwrap();
    assert_eq!(f, before);
}

#[test]
fn modcfg_base_tokens() {
    let mut b = ModCfgBase::default();
    parse_tokens(&mut b, &words(&[(AVS_TKN_MODCFG_BASE_ID_U32, 9), (AVS_TKN_MODCFG_BASE_CPC_U32, 1), (AVS_TKN_MODCFG_BASE_IBS_U32, 2), (AVS_TKN_MODCFG_BASE_OBS_U32, 3), (AVS_TKN_MODCFG_BASE_PAGES_U32, 4)])).unwrap();
    assert_eq!(b, ModCfgBase { cpc: 1, ibs: 2, obs: 3, is_pages: 4 });
}

#[test]
fn pplcfg_tokens_and_widths() {
    let mut p = PplCfg::default();
    let b = cat(&[words(&[(AVS_TKN_PPLCFG_ID_U32, 3), (AVS_TKN_PPLCFG_REQ_SIZE_U16, 0x234), (AVS_TKN_PPLCFG_PRIORITY_U8, 7), (AVS_TKN_PPLCFG_LOW_POWER_BOOL, 1), (AVS_TKN_PPLCFG_ATTRIBUTES_U16, 0xABC), (AVS_TKN_PPLCFG_TRIGGER_U32, 5)])]);
    parse_tokens(&mut p, &b).unwrap();
    assert_eq!(p, PplCfg { req_size: 0x234, priority: 7, lp: true, attributes: 0xABC, trigger: 5 });
    assert_eq!(parse_tokens(&mut p, &words(&[(AVS_TKN_PPLCFG_REQ_SIZE_U16, 0x10000)])), Err(TplgError::ValueTooWide { token: AVS_TKN_PPLCFG_REQ_SIZE_U16, value: 0x10000, bits: 16 }));
    assert_eq!(parse_tokens(&mut p, &words(&[(AVS_TKN_PPLCFG_ATTRIBUTES_U16, 0x10000)])), Err(TplgError::ValueTooWide { token: AVS_TKN_PPLCFG_ATTRIBUTES_U16, value: 0x10000, bits: 16 }));
    assert_eq!(parse_tokens(&mut p, &words(&[(AVS_TKN_PPLCFG_PRIORITY_U8, 256)])), Err(TplgError::ValueTooWide { token: AVS_TKN_PPLCFG_PRIORITY_U8, value: 256, bits: 8 }));
    parse_tokens(&mut p, &words(&[(AVS_TKN_PPLCFG_LOW_POWER_BOOL, 0)])).unwrap();
    assert!(!p.lp);
}

#[test]
fn library_tokens() {
    let mut l = Library::default();
    parse_tokens(&mut l, &cat(&[words(&[(AVS_TKN_LIBRARY_ID_U32, 1)]), string(AVS_TKN_LIBRARY_NAME_STRING, "lib.bin")])).unwrap();
    assert_eq!(l.name.as_str(), Some("lib.bin"));
    // a name delivered under another token is ignored
    let mut l = Library::default();
    parse_tokens(&mut l, &string(AVS_TKN_BINDING_TARGET_TPLG_NAME_STRING, "x")).unwrap();
    assert!(l.name.is_empty());
}

#[test]
fn binding_tokens_and_ssp_naming() {
    let mut b = Binding::default();
    let arrays = cat(&[
        words(&[(AVS_TKN_BINDING_ID_U32, 1), (AVS_TKN_BINDING_TARGET_PATH_TMPL_ID_U32, 2), (AVS_TKN_BINDING_TARGET_PPL_ID_U32, 3), (AVS_TKN_BINDING_TARGET_MOD_ID_U32, 4), (AVS_TKN_BINDING_MOD_ID_U32, 5)]),
        string(AVS_TKN_BINDING_TARGET_TPLG_NAME_STRING, "avs_ssp%d"),
        ints(BYTE, &[(AVS_TKN_BINDING_TARGET_MOD_PIN_U8, 6), (AVS_TKN_BINDING_MOD_PIN_U8, 7), (AVS_TKN_BINDING_IS_SINK_U8, 1)]),
    ]);
    parse_tokens(&mut BindingSink { binding: &mut b, ssp: None }, &arrays).unwrap();
    assert_eq!((b.target_path_tmpl_id, b.target_ppl_id, b.target_mod_id, b.mod_id, b.target_mod_pin, b.mod_pin, b.is_sink), (2, 3, 4, 5, 6, 7, 1));
    assert_eq!(b.target_tplg_name.as_str(), Some("avs_ssp%d"));
    let mut b = Binding::default();
    parse_tokens(&mut BindingSink { binding: &mut b, ssp: Some(SspBinding { port: 1, tdm: 0 }) }, &arrays).unwrap();
    assert_eq!(b.target_tplg_name.as_str(), Some("avs_ssp1"));
    assert_eq!(parse_tokens(&mut BindingSink { binding: &mut b, ssp: None }, &ints(BYTE, &[(AVS_TKN_BINDING_IS_SINK_U8, 300)])), Err(TplgError::ValueTooWide { token: AVS_TKN_BINDING_IS_SINK_U8, value: 300, bits: 8 }));
}

#[test]
fn init_and_nhlt_config_header_tokens() {
    let mut i = InitConfigHdr::default();
    parse_tokens(&mut i, &cat(&[words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 4)]), ints(BYTE, &[(AVS_TKN_INIT_CONFIG_PARAM_U8, 5)]), words(&[(AVS_TKN_INIT_CONFIG_LENGTH_U32, 6)])])).unwrap();
    assert_eq!(i, InitConfigHdr { id: 4, param: 5, length: 6 });
    let mut n = NhltConfigHdr::default();
    assert_eq!(n.size, None);
    parse_tokens(&mut n, &words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 8), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 0)])).unwrap();
    assert_eq!((n.id, n.size), (8, Some(0)));
}

#[test]
fn id_list_and_control_id_sinks_take_only_their_token() {
    let mut x = 0;
    parse_tokens(&mut IdListSink { token: AVS_TKN_MOD_INIT_CONFIG_ID_U32, out: &mut x }, &words(&[(AVS_TKN_PPL_BINDING_ID_U32, 5)])).unwrap();
    assert_eq!(x, 0);
    parse_tokens(&mut IdListSink { token: AVS_TKN_MOD_INIT_CONFIG_ID_U32, out: &mut x }, &words(&[(AVS_TKN_MOD_INIT_CONFIG_ID_U32, 5)])).unwrap();
    assert_eq!(x, 5);
    let mut id = 0;
    parse_tokens(&mut ControlIdSink { id: &mut id }, &words(&[(AVS_TKN_MOD_KCONTROL_ID_U32, 3)])).unwrap();
    assert_eq!(id, 0, "the module's kcontrol token is not the control's id token");
    parse_tokens(&mut ControlIdSink { id: &mut id }, &words(&[(AVS_TKN_KCONTROL_ID_U32, 3)])).unwrap();
    assert_eq!(id, 3);
}

#[test]
fn path_and_template_tokens_depend_on_the_kind() {
    // plain
    let mut p = Path::default();
    let tok = words(&[(AVS_TKN_PATH_ID_U32, 1), (AVS_TKN_PATH_FE_FMT_ID_U32, 1), (AVS_TKN_PATH_BE_FMT_ID_U32, 0), (AVS_TKN_CONDPATH_SOURCE_PATH_ID_U32, 8), (AVS_TKN_CONDPATH_SINK_PATH_ID_U32, 9)]);
    let counts = Counts { fmts: 2, ..Counts::default() };
    parse_tokens(&mut PathSink { path: &mut p, kind: PathKind::Plain, counts }, &tok).unwrap();
    assert_eq!((p.id, p.fe_fmt, p.be_fmt, p.source_path_id, p.sink_path_id), (1, Some(1), Some(0), 0, 0));
    // conditional: the FE/BE format tokens are not part of its table
    let mut p = Path::default();
    parse_tokens(&mut PathSink { path: &mut p, kind: PathKind::Conditional, counts }, &tok).unwrap();
    assert_eq!((p.id, p.fe_fmt, p.be_fmt, p.source_path_id, p.sink_path_id), (1, None, None, 8, 9));
    // templates
    let mut t = PathTemplate::default();
    let toks = cat(&[
        words(&[(AVS_TKN_PATH_TMPL_ID_U32, 6), (AVS_TKN_CONDPATH_TMPL_SOURCE_PATH_TMPL_ID_U32, 7), (AVS_TKN_CONDPATH_TMPL_SINK_PATH_TMPL_ID_U32, 8)]),
        string(AVS_TKN_CONDPATH_TMPL_SOURCE_TPLG_NAME_STRING, "src"),
        string(AVS_TKN_CONDPATH_TMPL_SINK_TPLG_NAME_STRING, "snk"),
    ]);
    parse_tokens(&mut TemplateSink { tmpl: &mut t, kind: PathKind::Plain }, &toks).unwrap();
    assert_eq!((t.id, t.source.id, t.sink.id, t.source.tplg_name.is_empty(), t.sink.tplg_name.is_empty()), (6, 0, 0, true, true));
    let mut t = PathTemplate::default();
    parse_tokens(&mut TemplateSink { tmpl: &mut t, kind: PathKind::Conditional }, &toks).unwrap();
    assert_eq!((t.id, t.source.id, t.sink.id, t.source.tplg_name.as_str(), t.sink.tplg_name.as_str()), (6, 7, 8, Some("src"), Some("snk")));
}

#[test]
fn pipeline_sink_tokens() {
    let mut p = Pipeline::default();
    parse_tokens(&mut PipelineSink { ppl: &mut p, counts: Counts { pplcfgs: 2, ..Counts::default() } }, &words(&[(AVS_TKN_PPL_ID_U32, 4), (AVS_TKN_PPL_PPLCFG_ID_U32, 1), (AVS_TKN_PPL_NUM_BINDING_IDS_U32, 3)])).unwrap();
    assert_eq!((p.id, p.cfg, p.num_bindings), (4, Some(1), 3));
}

/// A manifest whose one ext config sets every token of `modcfg_ext_parsers`, each to a distinct value.
#[test]
fn every_module_config_ext_token_lands_in_its_own_field() {
    let toks = cat(&[
        words(&[(AVS_TKN_MODCFG_EXT_ID_U32, 0)]),
        uuid(AVS_TKN_MODCFG_EXT_TYPE_UUID, [0x42; 16]),
        words(&[
            (AVS_TKN_MODCFG_CPR_OUT_AFMT_ID_U32, 1),
            (AVS_TKN_MODCFG_CPR_FEATURE_MASK_U32, 0x11),
            (AVS_TKN_MODCFG_CPR_DMA_TYPE_U32, 0x12),
            (AVS_TKN_MODCFG_CPR_DMABUFF_SIZE_U32, 0x13),
            (AVS_TKN_MODCFG_CPR_BLOB_FMT_ID_U32, 0),
            (AVS_TKN_MODCFG_MICSEL_OUT_AFMT_ID_U32, 1),
            (AVS_TKN_MODCFG_INTELWOV_CPC_LP_MODE_U32, 0x14),
            (AVS_TKN_MODCFG_SRC_OUT_FREQ_U32, 0x15),
            (AVS_TKN_MODCFG_MUX_REF_AFMT_ID_U32, 0),
            (AVS_TKN_MODCFG_MUX_OUT_AFMT_ID_U32, 1),
            (AVS_TKN_MODCFG_AEC_REF_AFMT_ID_U32, 1),
            (AVS_TKN_MODCFG_AEC_OUT_AFMT_ID_U32, 0),
            (AVS_TKN_MODCFG_AEC_CPC_LP_MODE_U32, 0x16),
            (AVS_TKN_MODCFG_ASRC_OUT_FREQ_U32, 0x17),
            (AVS_TKN_MODCFG_UPDOWN_MIX_OUT_CHAN_CFG_U32, 0x18),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_SELECT_U32, 0x19),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_0_S32, 100),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_1_S32, 101),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_2_S32, 102),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_3_S32, 103),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_4_S32, 104),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_5_S32, 105),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_6_S32, 106),
            (AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_7_S32, 0xFFFF_FF9C), // -100
            (AVS_TKN_MODCFG_UPDOWN_MIX_CHAN_MAP_U32, 0x1A),
            (AVS_TKN_MODCFG_WHM_REF_AFMT_ID_U32, 1),
            (AVS_TKN_MODCFG_WHM_OUT_AFMT_ID_U32, 0),
            (AVS_TKN_MODCFG_WHM_WAKE_TICK_PERIOD_U32, 0x1B),
            (AVS_TKN_MODCFG_WHM_DMA_TYPE_U32, 0x1C),
            (AVS_TKN_MODCFG_WHM_DMABUFF_SIZE_U32, 0x1D),
            (AVS_TKN_MODCFG_WHM_BLOB_AFMT_ID_U32, 1),
            (AVS_TKN_MODCFG_PEAKVOL_VOLUME_U32, 0x1E),
            (AVS_TKN_MODCFG_PEAKVOL_CURVE_TYPE_U32, 0x1F),
            (AVS_TKN_MODCFG_PEAKVOL_CURVE_DURATION_U32, 0x20),
        ]),
        ints(SHORT, &[(AVS_TKN_MODCFG_EXT_NUM_INPUT_PINS_U16, 0), (AVS_TKN_MODCFG_EXT_NUM_OUTPUT_PINS_U16, 0)]),
        ints(BYTE, &[(AVS_TKN_MODCFG_CPR_VINDEX_U8, 0x21), (AVS_TKN_MODCFG_ASRC_MODE_U8, 0x22), (AVS_TKN_MODCFG_ASRC_DISABLE_JITTER_U8, 0x23), (AVS_TKN_MODCFG_WHM_VINDEX_U8, 0x24)]),
        // reserved in tokens.h: parsed by nobody
        words(&[(AVS_TKN_MODCFG_PEAKVOL_CHANNEL_ID_U32, 0x99)]),
    ]);
    let mut cfg = ModCfgExt::default();
    parse_tokens(&mut ModCfgExtSink { cfg: &mut cfg, counts: Counts { fmts: 2, ..Counts::default() } }, &toks).unwrap();
    assert_eq!(cfg.type_uuid, [0x42; 16]);
    assert_eq!(cfg.copier, CopierExt { out_fmt: Some(1), blob_fmt: Some(0), feature_mask: 0x11, vindex: 0x21, dma_type: 0x12, dma_buffer_size: 0x13 });
    assert_eq!(cfg.micsel_out_fmt, Some(1));
    assert_eq!(cfg.wov_cpc_lp_mode, 0x14);
    assert_eq!(cfg.src_out_freq, 0x15);
    assert_eq!(cfg.mux, MuxExt { ref_fmt: Some(0), out_fmt: Some(1) });
    assert_eq!(cfg.aec, AecExt { ref_fmt: Some(1), out_fmt: Some(0), cpc_lp_mode: 0x16 });
    assert_eq!(cfg.asrc, AsrcExt { out_freq: 0x17, mode: 0x22, disable_jitter_buffer: 0x23 });
    assert_eq!(cfg.updown_mix, UpdownMixExt { out_channel_config: 0x18, coefficients_select: 0x19, coefficients: [100, 101, 102, 103, 104, 105, 106, -100], channel_map: 0x1A });
    assert_eq!(cfg.whm, WhmExt { ref_fmt: Some(1), out_fmt: Some(0), wake_tick_period: 0x1B, vindex: 0x24, dma_type: 0x1C, dma_buffer_size: 0x1D, blob_fmt: Some(1) });
    assert_eq!(cfg.peakvol, PeakvolExt { target_volume: 0x1E, curve_type: 0x1F, curve_duration: 0x20 });
    assert_eq!((cfg.num_input_pins, cfg.num_output_pins, cfg.npins), (0, 0, 0));
    // width limits
    for (tok, bits) in [(AVS_TKN_MODCFG_CPR_VINDEX_U8, 8), (AVS_TKN_MODCFG_ASRC_MODE_U8, 8), (AVS_TKN_MODCFG_ASRC_DISABLE_JITTER_U8, 8), (AVS_TKN_MODCFG_WHM_VINDEX_U8, 8), (AVS_TKN_MODCFG_EXT_NUM_INPUT_PINS_U16, 16), (AVS_TKN_MODCFG_EXT_NUM_OUTPUT_PINS_U16, 16)] {
        let max = if bits == 8 { 256 } else { 65536 };
        assert_eq!(parse_tokens(&mut ModCfgExtSink { cfg: &mut cfg, counts: Counts::default() }, &words(&[(tok, max)])), Err(TplgError::ValueTooWide { token: tok, value: max, bits }), "token {tok}");
    }
    // format indices are checked against the formats declared
    for tok in [AVS_TKN_MODCFG_CPR_OUT_AFMT_ID_U32, AVS_TKN_MODCFG_CPR_BLOB_FMT_ID_U32, AVS_TKN_MODCFG_MICSEL_OUT_AFMT_ID_U32, AVS_TKN_MODCFG_MUX_REF_AFMT_ID_U32, AVS_TKN_MODCFG_MUX_OUT_AFMT_ID_U32, AVS_TKN_MODCFG_AEC_REF_AFMT_ID_U32, AVS_TKN_MODCFG_AEC_OUT_AFMT_ID_U32, AVS_TKN_MODCFG_WHM_REF_AFMT_ID_U32, AVS_TKN_MODCFG_WHM_OUT_AFMT_ID_U32, AVS_TKN_MODCFG_WHM_BLOB_AFMT_ID_U32] {
        assert_eq!(parse_tokens(&mut ModCfgExtSink { cfg: &mut cfg, counts: Counts { fmts: 2, ..Counts::default() } }, &words(&[(tok, 2)])), Err(TplgError::IndexOutOfRange { what: "audio format", index: 2, count: 2 }), "token {tok}");
    }
}

#[test]
fn a_real_manifest_goes_through_the_same_tables() {
    // end to end through Topology: the copier's out format index resolves to a format
    let f: &'static [u8] = Box::leak(manifest_block(&small_manifest()).into_boxed_slice());
    let t = Topology::parse(f, None).unwrap();
    let ext = t.modcfg_ext(Some(1)).unwrap();
    assert_eq!(t.fmt(ext.copier.out_fmt).unwrap().bit_depth, 32);
    assert_eq!(t.modcfg_base(Some(1)).unwrap().ibs, 384);
    assert_eq!(t.pplcfg(Some(0)).unwrap().req_size, 2);
    let _ = name("x");
}
