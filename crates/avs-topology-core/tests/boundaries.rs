// SPDX-License-Identifier: GPL-2.0-only
//! Edges: exactly-at-capacity and exactly-one-over, exact-size inputs, and the exact error payloads. Found by mutating the crate
//! and reading what survived (PORT_CONVENTIONS.md section 4).
mod common;
use avs_ipc_core::modcfg::AVS_DMA_I2S_LINK_OUTPUT;
use avs_topology_core::blocks::*;
use avs_topology_core::error::TplgError;
use avs_topology_core::model::*;
use avs_topology_core::grow::{try_fill_default, try_push};
use avs_topology_core::tokens::*;
use avs_topology_core::topology::*;
use avs_topology_core::vendor::*;
use common::*;

fn leak(v: Vec<u8>) -> &'static [u8] {
    Box::leak(v.into_boxed_slice())
}

fn parse(v: Vec<u8>) -> Result<Topology<'static>, TplgError> {
    Topology::parse(leak(v), None)
}

/// A manifest with the given dictionary sizes; entries are minimal and valid.
fn sized(libs: u32, fmts: u32, bases: u32, exts: u32, ppls: u32, binds: u32) -> Vec<u8> {
    let mut v = vec![mf_header("t", 1), count(AVS_TKN_MANIFEST_NUM_LIBRARIES_U32, libs)];
    for i in 0..libs {
        v.push(words(&[(AVS_TKN_LIBRARY_ID_U32, i)]));
        v.push(string(AVS_TKN_LIBRARY_NAME_STRING, "l.bin"));
    }
    v.push(count(AVS_TKN_MANIFEST_NUM_AFMTS_U32, fmts));
    for i in 0..fmts {
        v.push(afmt(i, 48000, 16, 2, 16));
    }
    v.push(count(AVS_TKN_MANIFEST_NUM_MODCFGS_BASE_U32, bases));
    for i in 0..bases {
        v.push(base(i, 1, 2, 3, 4));
    }
    v.push(count(AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32, exts));
    for i in 0..exts {
        v.push(copier_ext(i, 0, 1, 768, None));
    }
    v.push(count(AVS_TKN_MANIFEST_NUM_PPLCFGS_U32, ppls));
    for i in 0..ppls {
        v.push(pplcfg(i, 2, 0, 0, 0, 0));
    }
    v.push(count(AVS_TKN_MANIFEST_NUM_BINDINGS_U32, binds));
    for i in 0..binds {
        v.push(binding(i, 0, 0, 0, 0, "t", 0, 0, 0));
    }
    v.push(count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 0));
    cat(&v)
}

fn load(m: Vec<u8>) -> Result<Topology<'static>, TplgError> {
    parse(manifest_block(&m))
}

#[test]
fn no_dictionary_has_a_cap() {
    let ok = load(sized(80, 320, 320, 320, 160, 320)).expect("ten times the old caps");
    assert_eq!((ok.libs.len(), ok.fmts.len(), ok.modcfgs_base.len(), ok.modcfgs_ext.len(), ok.pplcfgs.len(), ok.bindings.len()), (80, 320, 320, 320, 160, 320));
}

#[test]
fn a_count_uses_all_four_bytes() {
    let mut m = sized(0, 1, 0, 0, 0, 0);
    let pat = [dw(20), dw(WORD), dw(1), dw(AVS_TKN_MANIFEST_NUM_AFMTS_U32)].concat();
    let at = m.windows(pat.len()).position(|w| w == pat).unwrap() + pat.len();
    for bytes in [dw(0x0000_0100), dw(0x0001_0000), dw(0x0100_0000), dw(0x0A0B_0C0D)] {
        let mut x = m.clone();
        x[at..at + 4].copy_from_slice(&bytes);
        assert!(matches!(load(x).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_AFMT_ID_U32, .. })), "{bytes:?}");
    }
    m[at..at + 4].copy_from_slice(&dw(1));
    assert!(load(m).is_ok());
}

#[test]
fn a_dictionary_header_array_without_its_value_is_refused() {
    // an array of 16 bytes: the token is there (so the lookup finds it) but the count is not
    let mut m = sized(0, 1, 0, 0, 0, 0);
    let pat = [dw(20), dw(WORD), dw(1), dw(AVS_TKN_MANIFEST_NUM_AFMTS_U32)].concat();
    let at = m.windows(pat.len()).position(|w| w == pat).unwrap();
    m[at..at + 4].copy_from_slice(&dw(16)); // size 16: header + token only
    m.drain(at + 16..at + 20); // and drop the value so the arrays that follow stay aligned
    assert_eq!(load(m).err(), Some(TplgError::BadDictionaryHeader { want: AVS_TKN_MANIFEST_NUM_AFMTS_U32, got: 0 }));
}

#[test]
fn modcfg_ext_condpath_init_and_nhlt_dictionaries_that_promise_more_than_they_hold() {
    // ext: 2 promised, 1 present
    let mut m = sized(0, 1, 1, 1, 0, 0);
    let pat = [dw(20), dw(WORD), dw(1), dw(AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32)].concat();
    let at = m.windows(pat.len()).position(|w| w == pat).unwrap() + pat.len();
    m[at..at + 4].copy_from_slice(&dw(2));
    assert_eq!(load(m).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_MODCFG_EXT_ID_U32, entry: 1 }));
    // condpath: 1 promised, none present
    let mut m = sized(0, 1, 1, 1, 1, 1);
    let tail = count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 0);
    m.truncate(m.len() - tail.len());
    m.extend(count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 1));
    assert_eq!(load(m).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_CONDPATH_TMPL_ID_U32, entry: 0 }));
    // init configs: 2 promised, 1 present (Linux stops quietly; this port refuses)
    let mut m = sized(0, 1, 1, 1, 1, 1);
    m.extend(cat(&[count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 2), words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 1)]), words(&[(AVS_TKN_INIT_CONFIG_PARAM_U8, 1), (AVS_TKN_INIT_CONFIG_LENGTH_U32, 4)]), vec![0; 4]]));
    assert_eq!(load(m).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_INIT_CONFIG_ID_U32, entry: 1 }));
    // NHLT configs: 2 promised, 1 present
    let mut m = sized(0, 1, 1, 1, 1, 1);
    m.extend(cat(&[
        count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 0),
        count(AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32, 2),
        words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 0), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 4)]),
        vec![1; 4],
    ]));
    assert_eq!(load(m).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_NHLT_CONFIG_ID_U32, entry: 1 }));
}

#[test]
fn init_config_and_nhlt_entry_cuts_have_exact_errors() {
    let init = |entry: Vec<u8>| {
        let mut m = sized(0, 1, 1, 1, 1, 1);
        m.extend(cat(&[count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 1), entry]));
        load(m).err()
    };
    // only the first of the entry's two arrays: the second array's header cannot be read
    assert!(matches!(init(words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 1)])), Some(TplgError::Truncated { .. })));
    // the second array is cut mid-header
    let mut e = words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 1)]);
    e.extend_from_slice(&[0u8; 8]);
    assert!(matches!(init(e), Some(TplgError::Truncated { .. })));
    // both arrays present but the second claims more than is left
    let mut e = cat(&[words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 1)]), words(&[(AVS_TKN_INIT_CONFIG_LENGTH_U32, 0)])]);
    let n = e.len();
    e[20..24].copy_from_slice(&dw(200)); // second array's size
    assert_eq!(init(e[..n].to_vec()), Some(TplgError::ArrayPastBlock { at: 20, size: 220, block: 40 }));
    let nhlt = |entry: Vec<u8>| {
        let mut m = sized(0, 1, 1, 1, 1, 1);
        m.extend(cat(&[count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 0), count(AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32, 1), entry]));
        load(m).err()
    };
    // the entry array claims to be larger than the rest of the manifest
    let mut e = words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 0), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 0)]);
    e[0..4].copy_from_slice(&dw(500));
    assert_eq!(nhlt(e), Some(TplgError::ArrayPastBlock { at: 0, size: 500, block: 28 }));
    // an entry cut inside its header
    assert!(matches!(nhlt(vec![0; 8]), Some(TplgError::DictionaryTooShort { .. }) | Some(TplgError::Truncated { .. }) | Some(TplgError::ArrayTooSmall { .. })));
    // zero-length blobs fill the dictionary and the next entry starts right after
    let mut m = sized(0, 1, 1, 1, 1, 1);
    m.extend(cat(&[
        count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 0),
        count(AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32, 2),
        words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 0), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 0)]),
        words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 1), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 4)]),
        vec![7; 4],
    ]));
    let t = load(m).unwrap();
    assert_eq!((t.nhlt_configs.as_slice()[0].caps.len(), t.nhlt_configs.as_slice()[1].caps), (0, &[7u8; 4][..]));
}

#[test]
fn ssp_binding_edges_for_the_virtual_index() {
    let m = leak(manifest_block(&small_manifest()));
    let vindex = |port, tdm| Topology::parse(m, Some(SspBinding { port, tdm })).map(|t| t.modcfgs_ext.as_slice()[2].copier.vindex);
    assert_eq!(vindex(15, 0), Ok(0xF0));
    assert_eq!(vindex(0, 15), Ok(0x0F));
    assert_eq!(vindex(15, 15), Ok(0xFF));
    assert!(matches!(vindex(16, 0), Err(TplgError::ValueTooWide { bits: 4, value: 16, .. })));
    assert!(matches!(vindex(0, 16), Err(TplgError::ValueTooWide { bits: 4, value: 16, .. })));
    assert!(matches!(vindex(20, 30), Err(TplgError::ValueTooWide { bits: 4, value: 30, .. })));
    assert_eq!(AVS_DMA_I2S_LINK_OUTPUT, 12);
}

// ---- blocks ----------------------------------------------------------------------------------------------------------------------

#[test]
fn header_edges() {
    // exactly 36 bytes: a header whose payload is not there
    let b = block(8, 1, &[0u8; 4]);
    assert_eq!(valid_header(&b[..36], 0), Err(TplgError::BlockPastEnd { at: 0, payload: 4, file_len: 36 }));
    assert!(matches!(valid_header(&b[..35], 0), Err(TplgError::Truncated { need: 36, have: 35, .. })));
    // a manifest block of exactly 112 bytes has an empty private area: the first dictionary is missing
    let f = block(8, 1, &{
        let mut p = dw(112).to_vec();
        p.extend_from_slice(&[0u8; 108]);
        p
    });
    assert_eq!(parse(f).err(), Some(TplgError::MissingDictionary { token: AVS_TKN_MANIFEST_NUM_LIBRARIES_U32 }));
}

#[test]
fn control_truncation_reports_what_it_needed() {
    let c = mixer("A", 10, &[]);
    for n in [0usize, 100, 207] {
        let f = cat(&[manifest_block(&small_manifest()), block(1, 1, &c[..n.max(1)])]);
        let n = n.max(1);
        assert_eq!(parse(f).err(), Some(TplgError::Truncated { what: "control", need: 208, have: n }), "{n} bytes");
    }
    for n in [208usize, 300, 359] {
        let f = cat(&[manifest_block(&small_manifest()), block(1, 1, &c[..n])]);
        assert_eq!(parse(f).err(), Some(TplgError::Truncated { what: "control", need: 360, have: n }), "{n} bytes");
    }
    // exactly the structure, no private data
    let f = cat(&[manifest_block(&small_manifest()), block(1, 1, &c)]);
    assert!(parse(f).is_ok());
}

#[test]
fn tiny_private_areas_are_refused_not_skipped() {
    // a private area of 1..11 bytes cannot hold even one tuple array header
    for n in [1usize, 4, 11] {
        let f = cat(&[manifest_block(&small_manifest()), block(1, 1, &mixer("A", 1, &vec![0u8; n]))]);
        assert!(parse(f).is_err(), "control priv {n}");
        let f = cat(&[manifest_block(&small_manifest()), block(5, 1, &widget("w", &vec![0u8; n], &[]))]);
        assert!(parse(f).is_err(), "widget priv {n}");
    }
}

#[test]
fn a_widget_of_exactly_132_bytes_is_a_widget_without_a_template() {
    let w = widget("bare", &[], &[]);
    assert_eq!(w.len(), 132);
    let f = cat(&[manifest_block(&small_manifest()), block(5, 1, &w)]);
    let t = parse(f).unwrap();
    assert_eq!((t.widgets_seen, t.templates.len()), (1, 0));
    // cut one byte short
    let f = cat(&[manifest_block(&small_manifest()), block(5, 1, &w[..131])]);
    assert_eq!(parse(f).err(), Some(TplgError::Truncated { what: "widget", need: 132, have: 131 }));
}

#[test]
fn every_widget_string_is_checked_for_its_nul() {
    let mut w = widget("x", &[], &[]);
    for b in &mut w[52..96] {
        *b = b'A';
    }
    let f = cat(&[manifest_block(&small_manifest()), block(5, 1, &w)]);
    assert_eq!(parse(f).err(), Some(TplgError::StringNotTerminated { token: 0 }));
    let mut w = widget("x", &[], &[]);
    for b in &mut w[8..52] {
        *b = b'A';
    }
    let f = cat(&[manifest_block(&small_manifest()), block(5, 1, &w)]);
    assert_eq!(parse(f).err(), Some(TplgError::StringNotTerminated { token: 0 }));
}

#[test]
fn the_pcm_compress_flag_is_read_from_its_own_field() {
    let mut p = pcm("c", "d", 1, 0, 0, &[]);
    p[108..112].copy_from_slice(&dw(1));
    let f = cat(&[manifest_block(&small_manifest()), block(7, 1, &p)]);
    let t = parse(f).unwrap();
    let s = t.pcms.as_slice()[0];
    assert_eq!((s.compress, s.playback, s.capture), (true, false, false));
    // neighbours do not leak into it
    let mut p = pcm("c", "d", 1, 1, 1, &[]);
    p[108..112].copy_from_slice(&dw(0));
    let f = cat(&[manifest_block(&small_manifest()), block(7, 1, &p)]);
    assert!(!parse(f).unwrap().pcms.as_slice()[0].compress);
}

// ---- path -------------------------------------------------------------------------------------------------------------------------

#[test]
fn config_ids_and_pipeline_bindings_have_no_cap() {
    let ids = |n: u32| {
        let mut parts = vec![words(&[(AVS_TKN_MOD_ID_U32, 0), (AVS_TKN_MOD_INIT_CONFIG_NUM_IDS_U32, n)])];
        for i in 0..n {
            parts.push(words(&[(AVS_TKN_MOD_INIT_CONFIG_ID_U32, 100 + i)]));
        }
        let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0), cat(&parts)]);
        parse(cat(&[manifest_block(&small_manifest()), block(5, 1, &widget("w", &p, &[]))]))
    };
    for n in [8u32, 9, 255] {
        let t = ids(n).unwrap();
        let m = t.modules.as_slice()[0];
        assert_eq!(m.nconfig_ids, n);
        assert_eq!(t.config_ids_of(&m), (100..100 + n).collect::<Vec<_>>().as_slice());
    }
    // the count token is a byte (`AVS_TKN_MOD_INIT_CONFIG_NUM_IDS_U32` is parsed by `avs_parse_byte_token`): 256 is refused
    assert!(matches!(ids(256).err(), Some(TplgError::ValueTooWide { bits: 8, .. })));
    let binds = |n: u32| {
        let mut parts = vec![tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, n), module(0, 0, 0, 0)];
        for _ in 0..n {
            parts.push(binding_ref(1));
        }
        parse(cat(&[manifest_block(&small_manifest()), block(5, 1, &widget("w", &cat(&parts), &[]))]))
    };
    for n in [8u32, 9, 1000] {
        let t = binds(n).unwrap();
        assert_eq!(t.pipelines.as_slice()[0].nbindings, n);
        assert_eq!(t.binding_ids_of(&t.pipelines.as_slice()[0]).len(), n as usize);
    }
}

#[test]
fn a_pipeline_after_an_unmatched_module_section_starts_cleanly() {
    // the module walk ends at the next pipeline, not before it and not after it
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(1, 0, 0), module(0, 0, 0, 0), module(1, 0, 0, 0), pipeline_hdr(2, 0, 0), module(0, 1, 1, 1)]);
    let f = cat(&[manifest_block(&small_manifest()), block(5, 1, &widget("w", &p, &[]))]);
    let t = parse(f).unwrap();
    let tm = t.templates.as_slice()[0];
    let ppls = t.pipelines_of(&t.paths_of(&tm)[0]);
    assert_eq!((ppls[0].id, t.modules_of(&ppls[0]).len(), ppls[1].id, t.modules_of(&ppls[1]).len()), (1, 2, 2, 1));
}

// ---- pool -------------------------------------------------------------------------------------------------------------------------

#[test]
fn growth_helpers() {
    let mut v: Vec<u32> = Vec::new();
    assert_eq!(try_push(&mut v, "x", 10), Ok(0));
    assert_eq!(try_push(&mut v, "x", 11), Ok(1));
    assert_eq!(v, [10, 11]);
    try_fill_default(&mut v, "x", 4).unwrap();
    assert_eq!(v, [0, 0, 0, 0]);
    try_fill_default(&mut v, "x", 0).unwrap();
    assert!(v.is_empty());
}

// ---- vendor -----------------------------------------------------------------------------------------------------------------------

#[test]
fn name_edges() {
    assert!(!Name::from_bytes(b"x").unwrap().is_empty());
    let mut a = [0u8; 44];
    a[..2].copy_from_slice(b"ab");
    let mut b = a;
    b[10] = b'z';
    b[43] = b'q'; // garbage after the NUL is not part of the name
    assert_eq!(Name::from_field(&a, 1).unwrap(), Name::from_field(&b, 1).unwrap());
}

#[test]
fn walkers_on_exactly_sized_blocks() {
    // a 12-byte array with no elements, in a block of exactly 12 bytes
    let mut b = Vec::new();
    b.extend_from_slice(&dw(12));
    b.extend_from_slice(&dw(WORD));
    b.extend_from_slice(&dw(0));
    assert_eq!(lookup(&b, 12, 5), Ok(None));
    // a 16-byte array with its token and no value: matched on the token
    let mut t = Vec::new();
    t.extend_from_slice(&dw(16));
    t.extend_from_slice(&dw(WORD));
    t.extend_from_slice(&dw(1));
    t.extend_from_slice(&dw(77));
    assert_eq!(lookup(&t, 16, 77), Ok(Some(0)));
    assert_eq!(lookup(&t, 16, 78), Ok(None));
    // an array that CLAIMS an element but is only 12 bytes long has no first token, even if the next bytes look like one
    let mut u = Vec::new();
    u.extend_from_slice(&dw(12));
    u.extend_from_slice(&dw(WORD));
    u.extend_from_slice(&dw(1));
    u.extend_from_slice(&dw(77));
    assert_eq!(lookup(&u, 12, 77), Ok(None));
    // a 16-byte array whose buffer holds exactly 16 bytes
    assert_eq!(lookup(&t[..16], 16, 77), Ok(Some(0)));
    // lookup_next on a tiny first array names where it was
    let tiny = [vec![0u8; 4], vec![0u8; 20]].concat();
    assert_eq!(lookup_next(&tiny, 24, 5), Err(TplgError::ArrayTooSmall { at: 0, size: 0 }));
    // an element count of exactly what fits is accepted by the token walk
    let exact = words(&[(1, 1), (2, 2)]);
    struct Count(usize);
    impl Sink for Count {
        fn int(&mut self, _: u32, _: u32) -> Result<(), TplgError> {
            self.0 += 1;
            Ok(())
        }
    }
    let mut c = Count(0);
    parse_tokens(&mut c, &exact).unwrap();
    assert_eq!(c.0, 2);
}

#[test]
fn an_initial_config_with_no_raw_data_may_end_the_manifest() {
    let mut m = sized(0, 1, 1, 1, 1, 1);
    m.extend(cat(&[count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 1), words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 3)]), words(&[(AVS_TKN_INIT_CONFIG_PARAM_U8, 2), (AVS_TKN_INIT_CONFIG_LENGTH_U32, 0)])]));
    let t = load(m).unwrap();
    assert_eq!((t.init_configs.as_slice()[0].id, t.init_configs.as_slice()[0].param, t.init_configs.as_slice()[0].data.len()), (3, 2, 0));
}
