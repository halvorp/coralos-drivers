// SPDX-License-Identifier: GPL-2.0-only
//! The ASoC topology container (soc-topology.c:1927-2108): block headers, passes, widgets, PCMs, graph, controls.
mod common;
use avs_topology_core::blocks::*;
use avs_topology_core::error::TplgError;
use avs_topology_core::model::SspBinding;
use avs_topology_core::tokens::*;
use avs_topology_core::topology::Topology;
use common::*;

fn manifest() -> Vec<u8> {
    manifest_block(&small_manifest())
}

fn one_path_widget(name: &str) -> Vec<u8> {
    let p = cat(&[tmpl_hdr(0, name), path_hdr(0, 0, 1), pipeline_hdr(0, 0, 0), module(0, 0, 0, 0)]);
    widget(name, &p, &[])
}

#[test]
fn header_constants() {
    assert_eq!(SND_SOC_TPLG_MAGIC, 0x41536F43); // asoc.h:84
    assert_eq!(SOC_TPLG_MAGIC_BIG_ENDIAN, 0x436F5341); // soc-topology.c:33
    assert_eq!((SND_SOC_TPLG_ABI_VERSION, SND_SOC_TPLG_ABI_VERSION_MIN), (5, 5)); // asoc.h:90-91
    assert_eq!(HDR_SIZE, 36);
    assert_eq!((MANIFEST_SIZE, WIDGET_SIZE, GRAPH_ELEM_SIZE, PCM_SIZE, CTL_HDR_SIZE, MIXER_SIZE, BYTES_SIZE), (112, 132, 132, 912, 204, 360, 240));
    let types = [
        (SND_SOC_TPLG_TYPE_MIXER, 1),
        (SND_SOC_TPLG_TYPE_BYTES, 2),
        (SND_SOC_TPLG_TYPE_ENUM, 3),
        (SND_SOC_TPLG_TYPE_DAPM_GRAPH, 4),
        (SND_SOC_TPLG_TYPE_DAPM_WIDGET, 5),
        (SND_SOC_TPLG_TYPE_DAI_LINK, 6),
        (SND_SOC_TPLG_TYPE_PCM, 7),
        (SND_SOC_TPLG_TYPE_MANIFEST, 8),
        (SND_SOC_TPLG_TYPE_CODEC_LINK, 9),
        (SND_SOC_TPLG_TYPE_BACKEND_LINK, 10),
        (SND_SOC_TPLG_TYPE_PDATA, 11),
        (SND_SOC_TPLG_TYPE_DAI, 12),
    ];
    for (c, v) in types {
        assert_eq!(c, v);
    }
    assert_eq!(SND_SOC_TPLG_DAPM_SCHEDULER, 17);
    assert_eq!((PASS_MANIFEST, PASS_VENDOR, PASS_CONTROL, PASS_WIDGET, PASS_PCM_DAI, PASS_GRAPH, PASS_BE_DAI, PASS_LINK), (0, 1, 2, 3, 4, 5, 6, 7));
}

#[test]
fn the_pass_of_each_block_type() {
    // soc-topology.c:2004-2058
    let expect = [(1, 2), (2, 2), (3, 2), (4, 5), (5, 3), (6, 7), (7, 4), (8, 0), (9, 1), (10, 7), (11, 1), (12, 6), (13, 1), (1000, 1), (1001, 1)];
    for (ty, pass) in expect {
        assert_eq!(pass_of(ty), pass, "type {ty}");
    }
}

#[test]
fn a_valid_header_is_decoded() {
    let b = block_with(0x41536F43, 5, 7, 3, &[0u8; 12]);
    let h = valid_header(&b, 0).unwrap();
    assert_eq!((h.at, h.abi, h.ty, h.payload_size, h.count, h.index, h.version, h.vendor_type), (0, 5, 7, 12, 3, 0, 0, 0));
    assert_eq!(h.next(), 48);
    // fields of the header land where the declaration puts them
    let mut b = block(8, 9, &[1, 2, 3, 4]);
    b[8..12].copy_from_slice(&dw(0x11));
    b[20..24].copy_from_slice(&dw(0x22));
    b[28..32].copy_from_slice(&dw(0x33));
    let h = valid_header(&b, 0).unwrap();
    assert_eq!((h.version, h.vendor_type, h.index, h.count, h.ty, h.payload_size), (0x11, 0x22, 0x33, 9, 8, 4));
}

#[test]
fn every_header_refusal_by_name() {
    let good = block(8, 1, &[0u8; 8]);
    assert!(matches!(valid_header(&good[..35], 0), Err(TplgError::Truncated { .. })));
    let mut b = good.clone();
    b[16..20].copy_from_slice(&dw(40));
    assert_eq!(valid_header(&b, 0), Err(TplgError::BadHeaderSize { at: 0, size: 40 }));
    let mut b = good.clone();
    b[0..4].copy_from_slice(&dw(0x436F5341));
    assert_eq!(valid_header(&b, 0), Err(TplgError::BigEndian { at: 0 }));
    let mut b = good.clone();
    b[0..4].copy_from_slice(&dw(0x12345678));
    assert_eq!(valid_header(&b, 0), Err(TplgError::BadMagic { at: 0, magic: 0x12345678 }));
    for abi in [4, 6, 0, 0x500] {
        let mut b = good.clone();
        b[4..8].copy_from_slice(&dw(abi));
        assert_eq!(valid_header(&b, 0), Err(TplgError::BadAbi { at: 0, abi }));
    }
    let mut b = good.clone();
    b[24..28].copy_from_slice(&dw(0));
    assert_eq!(valid_header(&b, 0), Err(TplgError::EmptyPayload { at: 0 }));
    // the payload runs one byte past the file (Linux's check misses this: it does not count the 36 header bytes)
    let mut b = good.clone();
    b[24..28].copy_from_slice(&dw(9));
    assert_eq!(valid_header(&b, 0), Err(TplgError::BlockPastEnd { at: 0, payload: 9, file_len: 44 }));
    let mut b = good.clone();
    b[24..28].copy_from_slice(&dw(u32::MAX));
    assert!(matches!(valid_header(&b, 0), Err(TplgError::BlockPastEnd { .. })));
    // exactly fitting is fine
    assert!(valid_header(&good, 0).is_ok());
    // a second block cut short
    let two = cat(&[good.clone(), good[..30].to_vec()]);
    assert!(valid_header(&two, 44).is_err());
}

#[test]
fn error_names_are_stable() {
    assert_eq!(TplgError::BadMagic { at: 0, magic: 0 }.name(), "bad-magic");
    assert_eq!(TplgError::BigEndian { at: 0 }.name(), "big-endian");
    assert_eq!(TplgError::BlockPastEnd { at: 0, payload: 0, file_len: 0 }.name(), "block-past-end");
}

#[test]
fn the_manifest_loads_before_widgets_whatever_the_file_order() {
    // a widget block placed BEFORE the manifest still resolves its indices: pass 0 runs first
    let f = cat(&[block(5, 1, &one_path_widget("w")), manifest()]);
    let t = Topology::parse(&f, None).unwrap();
    assert_eq!(t.templates.len(), 1);
    assert_eq!(t.header.name.as_str(), Some("test"));
    // and a widget with no manifest at all cannot resolve a thing
    let f = block(5, 1, &one_path_widget("w"));
    assert!(matches!(Topology::parse(&f, None), Err(TplgError::IndexOutOfRange { .. })));
}

#[test]
fn two_manifests_are_refused() {
    let f = cat(&[manifest(), manifest()]);
    assert_eq!(Topology::parse(&f, None).err(), Some(TplgError::DuplicateBlock { ty: 8 }));
}

#[test]
fn a_manifest_of_the_wrong_size_or_cut_short() {
    let mut m = manifest();
    m[36..40].copy_from_slice(&dw(108));
    assert_eq!(Topology::parse(&m, None).err(), Some(TplgError::BadElementSize { what: "manifest", size: 108, want: 112 }));
    // private size beyond the block
    let mut m = manifest();
    m[36 + 108..36 + 112].copy_from_slice(&dw(0x10000));
    assert!(matches!(Topology::parse(&m, None).err(), Some(TplgError::Truncated { .. })));
    // payload shorter than the manifest structure
    let m = block(8, 1, &[0u8; 100]);
    assert!(matches!(Topology::parse(&m, None).err(), Some(TplgError::Truncated { .. })));
}

#[test]
fn unsupported_block_types_are_refused_but_dai_and_links_are_skipped() {
    // no vendor_load callback (soc-topology.c:233-244)
    for ty in [9, 11, 13, 1000] {
        let f = cat(&[manifest(), block(ty, 1, &[0u8; 4])]);
        assert_eq!(Topology::parse(&f, None).err(), Some(TplgError::UnsupportedBlockType { ty }), "type {ty}");
    }
    for ty in [6, 10, 12] {
        let f = cat(&[manifest(), block(ty, 1, &[0u8; 4])]);
        assert!(Topology::parse(&f, None).is_ok(), "type {ty}");
    }
}

#[test]
fn widgets_with_private_data_become_templates_in_file_order_and_others_are_counted() {
    let w = cat(&[one_path_widget("first"), widget("plain", &[], &[]), one_path_widget("second")]);
    let f = cat(&[manifest(), block(5, 3, &w)]);
    let t = Topology::parse(&f, None).unwrap();
    assert_eq!(t.widgets_seen, 3);
    assert_eq!(t.templates.len(), 2);
    assert_eq!(t.templates.as_slice()[0].widget_name.as_str(), Some("first"));
    assert_eq!(t.templates.as_slice()[1].widget_name.as_str(), Some("second"));
    assert!(t.template_by_widget(b"second").is_some());
    assert!(t.template_by_widget(b"plain").is_none());
    assert!(t.template_by_widget(b"nope").is_none());
}

#[test]
fn widget_refusals() {
    let ok = one_path_widget("w");
    // wrong structure size
    let mut w = ok.clone();
    w[0..4].copy_from_slice(&dw(128));
    let f = cat(&[manifest(), block(5, 1, &w)]);
    assert_eq!(Topology::parse(&f, None).err(), Some(TplgError::BadElementSize { what: "widget", size: 128, want: 132 }));
    // private size beyond the block
    let mut w = ok.clone();
    w[128..132].copy_from_slice(&dw(0x1000));
    let f = cat(&[manifest(), block(5, 1, &w)]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { what: "widget private data", .. })));
    // block shorter than a widget
    let f = cat(&[manifest(), block(5, 1, &ok[..100])]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { what: "widget", .. })));
    // unterminated name
    let mut w = ok.clone();
    for x in &mut w[8..52] {
        *x = b'A';
    }
    let f = cat(&[manifest(), block(5, 1, &w)]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::StringNotTerminated { .. })));
    // the widget count promises more widgets than the block holds
    let f = cat(&[manifest(), block(5, 2, &ok)]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { .. })));
}

#[test]
fn widget_kcontrols_are_loaded_after_the_widget_and_tied_to_it() {
    let id = ints(WORD, &[(AVS_TKN_KCONTROL_ID_U32, 7)]);
    let ctl = mixer("Volume", 100, &id);
    let w = cat(&[widget("plain", &[], &[]), widget("with-ctl", &[], &[ctl.clone(), bytes_ctl("DSM data", 1 << 20, &ints(WORD, &[(AVS_TKN_KCONTROL_ID_U32, 9)]))])]);
    let f = cat(&[manifest(), block(5, 2, &w)]);
    let t = Topology::parse(&f, None).unwrap();
    assert_eq!(t.controls.len(), 2);
    let c = t.controls.as_slice();
    assert_eq!((c[0].kind, c[0].name.as_str(), c[0].id, c[0].max, c[0].widget), (ControlKind::Mixer, Some("Volume"), 7, 100, Some(1)));
    assert_eq!((c[1].kind, c[1].name.as_str(), c[1].id, c[1].max, c[1].widget), (ControlKind::Bytes, Some("DSM data"), 9, 1 << 20, Some(1)));
    assert_eq!(c[0].access, 0x10000020);
    // an enum kcontrol on a widget is refused (`avs_control_load`: only mixers)
    let mut e = ctl.clone();
    e[4..8].copy_from_slice(&dw(3));
    let f = cat(&[manifest(), block(5, 1, &widget("w", &[], &[e]))]);
    assert_eq!(Topology::parse(&f, None).err(), Some(TplgError::WidgetControlType { ty: 3 }));
}

#[test]
fn stand_alone_control_blocks() {
    let m = cat(&[mixer("A", 10, &ints(WORD, &[(AVS_TKN_KCONTROL_ID_U32, 1)])), mixer("B", 20, &[])]);
    let b = bytes_ctl("DSM data", 4096, &ints(WORD, &[(AVS_TKN_KCONTROL_ID_U32, 1)]));
    let f = cat(&[manifest(), block(1, 2, &m), block(2, 1, &b)]);
    let t = Topology::parse(&f, None).unwrap();
    let c = t.controls.as_slice();
    assert_eq!(c.len(), 3);
    assert_eq!((c[0].name.as_str(), c[0].id, c[0].widget), (Some("A"), 1, None));
    assert_eq!((c[1].name.as_str(), c[1].id, c[1].max), (Some("B"), 0, 20), "no private data: id stays 0");
    assert_eq!((c[2].kind, c[2].name.as_str(), c[2].max), (ControlKind::Bytes, Some("DSM data"), 4096));
}

#[test]
fn control_refusals() {
    let ok = mixer("A", 10, &[]);
    // an enum control block: AVS refuses
    let mut e = ok.clone();
    e[4..8].copy_from_slice(&dw(3));
    let f = cat(&[manifest(), block(3, 1, &e)]);
    assert_eq!(Topology::parse(&f, None).err(), Some(TplgError::UnsupportedControlType { ty: 3 }));
    // wrong header size (soc-topology.c:985)
    let mut b = ok.clone();
    b[0..4].copy_from_slice(&dw(200));
    let f = cat(&[manifest(), block(1, 1, &b)]);
    assert_eq!(Topology::parse(&f, None).err(), Some(TplgError::BadElementSize { what: "control header", size: 200, want: 204 }));
    // wrong structure size
    let mut b = ok.clone();
    b[204..208].copy_from_slice(&dw(300));
    let f = cat(&[manifest(), block(1, 1, &b)]);
    assert_eq!(Topology::parse(&f, None).err(), Some(TplgError::BadElementSize { what: "control", size: 300, want: 360 }));
    // private data beyond the block
    let mut b = ok.clone();
    b[356..360].copy_from_slice(&dw(100));
    let f = cat(&[manifest(), block(1, 1, &b)]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { what: "control private data", .. })));
    // block shorter than the structure
    let f = cat(&[manifest(), block(1, 1, &ok[..300])]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { what: "control", .. })));
    let f = cat(&[manifest(), block(1, 1, &ok[..100])]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { .. })));
}

#[test]
fn pcms_carry_names_ids_and_directions() {
    let p = cat(&[pcm("Headset", "Headset-dai", 3, 1, 0, &[]), pcm("Mic", "Mic-dai", 4, 0, 1, &[1, 2, 3, 4])]);
    let f = cat(&[manifest(), block(7, 2, &p)]);
    let t = Topology::parse(&f, None).unwrap();
    let s = t.pcms.as_slice();
    assert_eq!(s.len(), 2);
    assert_eq!((s[0].pcm_name.as_str(), s[0].dai_name.as_str(), s[0].pcm_id, s[0].dai_id, s[0].playback, s[0].capture, s[0].compress), (Some("Headset"), Some("Headset-dai"), 3, 13, true, false, false));
    assert_eq!((s[1].pcm_name.as_str(), s[1].pcm_id, s[1].playback, s[1].capture), (Some("Mic"), 4, false, true));
    // wrong size / cut short
    let mut bad = pcm("x", "y", 1, 1, 0, &[]);
    bad[0..4].copy_from_slice(&dw(900));
    let f = cat(&[manifest(), block(7, 1, &bad)]);
    assert_eq!(Topology::parse(&f, None).err(), Some(TplgError::BadElementSize { what: "pcm", size: 900, want: 912 }));
    let f = cat(&[manifest(), block(7, 1, &bad[..500])]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { what: "pcm", .. })));
    let mut big = pcm("x", "y", 1, 1, 0, &[]);
    big[908..912].copy_from_slice(&dw(77));
    let f = cat(&[manifest(), block(7, 1, &big)]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { what: "pcm private data", .. })));
}

#[test]
fn graph_routes_keep_their_three_names() {
    let g = graph(&[("Headset-capture", "", "ssp1c_fe"), ("ssp1p_be", "mixer", "ssp1p_fe")]);
    let f = cat(&[manifest(), block(4, 2, &g)]);
    let t = Topology::parse(&f, None).unwrap();
    let r = t.routes.as_slice();
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].sink.as_str(), r[0].control.as_str(), r[0].source.as_str()), (Some("Headset-capture"), Some(""), Some("ssp1c_fe")));
    assert_eq!((r[1].sink.as_str(), r[1].control.as_str(), r[1].source.as_str()), (Some("ssp1p_be"), Some("mixer"), Some("ssp1p_fe")));
    // more elements than the block holds (soc_tplg_check_elem_count)
    let f = cat(&[manifest(), block(4, 3, &g)]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { what: "graph elements", .. })));
    // an unterminated name is an invalid route
    let mut bad = g.clone();
    for x in &mut bad[0..44] {
        *x = b'Z';
    }
    let f = cat(&[manifest(), block(4, 2, &bad)]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::StringNotTerminated { .. })));
}

#[test]
fn a_single_ssp_renames_widgets_routes_and_the_manifest_name() {
    // avs_widget_load / avs_route_load / parse_link_formatted_string: `%d` -> port (and `:tdm`)
    let p = cat(&[tmpl_hdr(0, "ssp%dp_fe"), path_hdr(0, 0, 1), pipeline_hdr(0, 0, 0), module(0, 0, 0, 0)]);
    let g = graph(&[("ssp%d Tx", "", "ssp%dp_be"), ("Headset-capture", "", "ssp%dc_fe")]);
    let mf = {
        let mut m = small_manifest();
        // replace the header's name by a formatted one
        m.splice(..mf_header("test", 7).len(), mf_header("avs_ssp%d", 7));
        m
    };
    let f = cat(&[manifest_block(&mf), block(5, 1, &widget("ssp%dp_fe", &p, &[])), block(4, 2, &g)]);
    let t = Topology::parse(&f, Some(SspBinding { port: 1, tdm: 0 })).unwrap();
    assert_eq!(t.header.name.as_str(), Some("avs_ssp1"));
    assert_eq!(t.templates.as_slice()[0].widget_name.as_str(), Some("ssp1p_fe"));
    assert_eq!(t.routes.as_slice()[0].sink.as_str(), Some("ssp1 Tx"));
    assert_eq!(t.routes.as_slice()[0].source.as_str(), Some("ssp1p_be"));
    assert_eq!(t.routes.as_slice()[1].source.as_str(), Some("ssp1c_fe"));
    assert_eq!(t.routes.as_slice()[1].sink.as_str(), Some("Headset-capture"));
    // with a TDM slot
    let t = Topology::parse(&f, Some(SspBinding { port: 2, tdm: 3 })).unwrap();
    assert_eq!(t.header.name.as_str(), Some("avs_ssp2:3"));
    assert_eq!(t.routes.as_slice()[0].sink.as_str(), Some("ssp2:3 Tx"));
    // and without a binding the text is left alone
    let t = Topology::parse(&f, None).unwrap();
    assert_eq!(t.header.name.as_str(), Some("avs_ssp%d"));
    assert_eq!(t.routes.as_slice()[0].sink.as_str(), Some("ssp%d Tx"));
}

#[test]
fn walking_a_file_is_bounded_by_its_blocks() {
    // 40000 empty-ish blocks would be 40000 iterations; each iteration consumes at least 37 bytes, so a file always terminates
    let f = cat(&[manifest(), block(12, 1, &[0u8; 1])]);
    assert!(Topology::parse(&f, None).is_ok());
    // trailing garbage shorter than a header is a named truncation, not an infinite loop or a panic
    let mut f = manifest();
    f.extend_from_slice(&[0u8; 20]);
    assert!(matches!(Topology::parse(&f, None).err(), Some(TplgError::Truncated { what: "topology block header", .. })));
}
