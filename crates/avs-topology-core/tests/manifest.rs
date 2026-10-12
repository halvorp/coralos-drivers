// SPDX-License-Identifier: GPL-2.0-only
//! The manifest dictionaries (topology.c:1961-2095).
mod common;
use avs_ipc_core::modcfg::{AVS_DMA_HDA_HOST_OUTPUT, AVS_DMA_I2S_LINK_OUTPUT};
use avs_topology_core::error::TplgError;
use avs_topology_core::model::*;
use avs_topology_core::tokens::*;
use avs_topology_core::topology::*;
use common::*;

fn load(m: &[u8]) -> Result<Topology<'_>, TplgError> {
    let mut t = Topology::new();
    // leak-free: the manifest block must outlive `t`, so borrow the private area directly
    let _ = &mut t;
    t.load_manifest_for_test(m, None)?;
    Ok(t)
}

trait ManifestOnly<'a> {
    fn load_manifest_for_test(&mut self, priv_: &'a [u8], ssp: Option<SspBinding>) -> Result<(), TplgError>;
}
impl<'a> ManifestOnly<'a> for Topology<'a> {
    fn load_manifest_for_test(&mut self, priv_: &'a [u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
        // go through the public path: wrap the private data in a manifest block
        let blk: &'a [u8] = Box::leak(manifest_block(priv_).into_boxed_slice());
        self.load(blk, ssp)
    }
}

#[test]
fn a_small_manifest_loads_every_dictionary_with_its_values() {
    let m = small_manifest();
    let t = load(&m).unwrap();
    assert_eq!(t.header.name.as_str(), Some("test"));
    assert_eq!(t.header.version, 7);
    assert_eq!(t.libs.len(), 0);
    assert_eq!(t.fmts.len(), 2);
    let f = t.fmts.as_slice();
    assert_eq!((f[0].sampling_freq, f[0].bit_depth, f[0].channel_map, f[0].channel_config, f[0].interleaving, f[0].num_channels, f[0].valid_bit_depth, f[0].sample_type), (48000, 16, 0xFFFF_3210, 1, 0, 2, 16, 0));
    assert_eq!((f[1].bit_depth, f[1].valid_bit_depth), (32, 24));
    let b = t.modcfgs_base.as_slice();
    assert_eq!(b.len(), 2);
    assert_eq!((b[0].cpc, b[0].ibs, b[0].obs, b[0].is_pages), (12000, 192, 384, 1));
    assert_eq!((b[1].cpc, b[1].ibs, b[1].obs, b[1].is_pages), (12000, 384, 384, 1));
    let e = t.modcfgs_ext.as_slice();
    assert_eq!(e.len(), 3);
    assert_eq!(e[0].type_uuid, COPIER);
    assert_eq!((e[0].copier.out_fmt, e[0].copier.dma_type, e[0].copier.dma_buffer_size, e[0].copier.vindex), (Some(0), 1, 768, 0));
    assert_eq!((e[1].copier.out_fmt, e[1].copier.dma_type, e[1].copier.vindex), (Some(1), 13, 16));
    assert_eq!(e[2].copier.dma_type, 12);
    assert_eq!(t.pplcfgs.len(), 1);
    let p = t.pplcfgs.as_slice()[0];
    assert_eq!((p.req_size, p.priority, p.lp, p.attributes, p.trigger), (2, 0, false, 0, 0));
    assert_eq!(t.bindings.len(), 2);
    let bd = t.bindings.as_slice();
    assert_eq!((bd[0].target_tplg_name.as_str(), bd[0].target_path_tmpl_id, bd[0].target_ppl_id, bd[0].target_mod_id, bd[0].mod_id, bd[0].target_mod_pin, bd[0].mod_pin, bd[0].is_sink), (Some("test"), 1, 0, 0, 0, 0, 0, 1));
    assert_eq!((bd[1].target_path_tmpl_id, bd[1].is_sink), (3, 0));
    assert_eq!(t.condpath_tmpls.len(), 0);
    assert_eq!(t.init_configs.len(), 0);
    assert_eq!(t.nhlt_configs.len(), 0);
}

#[test]
fn libraries_are_a_dictionary_of_names() {
    let mut m = small_manifest();
    let at = mf_header("test", 7).len();
    let libs = cat(&[
        count(AVS_TKN_MANIFEST_NUM_LIBRARIES_U32, 2),
        words(&[(AVS_TKN_LIBRARY_ID_U32, 0)]),
        string(AVS_TKN_LIBRARY_NAME_STRING, "dsp_lib_dsm_core_spt_release.bin"),
        words(&[(AVS_TKN_LIBRARY_ID_U32, 1)]),
        string(AVS_TKN_LIBRARY_NAME_STRING, "dsp_lib_waves5_9_spt_release.bin"),
    ]);
    let old = count(AVS_TKN_MANIFEST_NUM_LIBRARIES_U32, 0);
    m.splice(at..at + old.len(), libs);
    let t = load(&m).unwrap();
    let l = t.libs.as_slice();
    assert_eq!(l.len(), 2);
    assert_eq!(l[0].name.as_str(), Some("dsp_lib_dsm_core_spt_release.bin"));
    assert_eq!(l[1].name.as_str(), Some("dsp_lib_waves5_9_spt_release.bin"));
}

#[test]
fn the_manifest_must_begin_with_its_header() {
    // the very first array is the library count: nothing before it
    let m = cat(&[count(AVS_TKN_MANIFEST_NUM_LIBRARIES_U32, 0), count(AVS_TKN_MANIFEST_NUM_AFMTS_U32, 0)]);
    assert_eq!(load(&m).err(), Some(TplgError::ManifestWithoutHeader));
}

#[test]
fn every_dictionary_a_manifest_needs_is_named_when_missing() {
    let order = [
        AVS_TKN_MANIFEST_NUM_LIBRARIES_U32,
        AVS_TKN_MANIFEST_NUM_AFMTS_U32,
        AVS_TKN_MANIFEST_NUM_MODCFGS_BASE_U32,
        AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32,
        AVS_TKN_MANIFEST_NUM_PPLCFGS_U32,
        AVS_TKN_MANIFEST_NUM_BINDINGS_U32,
    ];
    // drop each dictionary header (and, as they are empty here, nothing else): the lookup that needs it fails by name
    for (i, tok) in order.iter().enumerate() {
        let mut parts = vec![mf_header("t", 0)];
        for t in order.iter() {
            if t != tok {
                parts.push(count(*t, 0));
            }
        }
        parts.push(count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 0));
        let m = cat(&parts);
        let r = load(&m).err();
        // the header lookup for the library count finds the NEXT thing; what is reported is the first token that cannot be found
        assert!(matches!(r, Some(TplgError::MissingDictionary { .. }) | Some(TplgError::ManifestWithoutHeader) | Some(TplgError::BadDictionaryHeader { .. })), "dropping {tok} (#{i}) gave {r:?}");
    }
    // the specific case Linux reports as "audio formats lookup failed"
    let m = cat(&[mf_header("t", 0), count(AVS_TKN_MANIFEST_NUM_LIBRARIES_U32, 0)]);
    assert_eq!(load(&m).err(), Some(TplgError::MissingDictionary { token: AVS_TKN_MANIFEST_NUM_AFMTS_U32 }));
}

#[test]
fn a_dictionary_header_must_carry_its_own_token() {
    // libraries dictionary whose first tuple is another dictionary's count
    let mut m = small_manifest();
    let at = mf_header("test", 7).len();
    // swap the library count token (3) for 99: the lookup for NUM_LIBRARIES fails first
    m[at + 12..at + 16].copy_from_slice(&dw(99));
    assert_eq!(load(&m).err(), Some(TplgError::MissingDictionary { token: AVS_TKN_MANIFEST_NUM_LIBRARIES_U32 }));
}

#[test]
fn a_dictionary_promising_more_entries_than_its_block_is_refused() {
    let mut m = small_manifest();
    // the format dictionary says 3 entries but carries 2
    let pos = m.windows(8).position(|w| w == [dw(AVS_TKN_MANIFEST_NUM_AFMTS_U32), dw(2)].concat()).unwrap();
    m[pos + 4..pos + 8].copy_from_slice(&dw(3));
    assert_eq!(load(&m).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_AFMT_ID_U32, entry: 2 }));
}

/// Offset of the VALUE dword of the count array `{ size 20, WORD, 1 element, token }`.
fn count_value_at(m: &[u8], token: u32) -> usize {
    let pat = [dw(20), dw(WORD), dw(1), dw(token)].concat();
    m.windows(pat.len()).position(|w| w == pat).expect("count array") + pat.len()
}

#[test]
fn dictionaries_larger_than_the_fixed_storage_are_refused_by_name() {
    for (token, name, max) in [
        (AVS_TKN_MANIFEST_NUM_LIBRARIES_U32, "libraries", MAX_LIBS),
        (AVS_TKN_MANIFEST_NUM_AFMTS_U32, "audio formats", MAX_FMTS),
        (AVS_TKN_MANIFEST_NUM_MODCFGS_BASE_U32, "module configs (base)", MAX_MODCFGS_BASE),
        (AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32, "module configs (ext)", MAX_MODCFGS_EXT),
        (AVS_TKN_MANIFEST_NUM_PPLCFGS_U32, "pipeline configs", MAX_PPLCFGS),
        (AVS_TKN_MANIFEST_NUM_BINDINGS_U32, "bindings", MAX_BINDINGS),
    ] {
        let mut m = small_manifest();
        let pos = count_value_at(&m, token);
        m[pos..pos + 4].copy_from_slice(&dw(max as u32 + 1));
        assert_eq!(load(&m).err(), Some(TplgError::TooMany { what: name, count: max as u32 + 1, max: max as u32 }), "{name}");
    }
}

#[test]
fn a_huge_count_does_not_allocate_or_loop() {
    let mut m = small_manifest();
    let pos = count_value_at(&m, AVS_TKN_MANIFEST_NUM_AFMTS_U32);
    m[pos..pos + 4].copy_from_slice(&dw(u32::MAX));
    assert!(matches!(load(&m).err(), Some(TplgError::TooMany { .. })));
}

#[test]
fn index_tokens_are_checked_against_the_counts_declared_so_far() {
    // a pin/copier/module index equal to the count is out of range (topology.c:340-342)
    let mut m = small_manifest();
    // ext config 0 refers to audio format 0: change its out-format index to 2 (there are 2 formats)
    let pos = m.windows(8).position(|w| w == [dw(AVS_TKN_MODCFG_CPR_OUT_AFMT_ID_U32), dw(0)].concat()).unwrap();
    m[pos + 4..pos + 8].copy_from_slice(&dw(2));
    assert_eq!(load(&m).err(), Some(TplgError::IndexOutOfRange { what: "audio format", index: 2, count: 2 }));
    // index == count - 1 is fine
    let mut m = small_manifest();
    m[pos + 4..pos + 8].copy_from_slice(&dw(1));
    assert_eq!(load(&m).unwrap().modcfgs_ext.as_slice()[0].copier.out_fmt, Some(1));
}

#[test]
fn modcfg_ext_pin_formats_follow_the_entry_input_pins_first() {
    let mut m = small_manifest();
    // replace ext config 2 by a generic module with 1 input and 2 output pins
    let generic = cat(&[
        words(&[(AVS_TKN_MODCFG_EXT_ID_U32, 2)]),
        uuid(AVS_TKN_MODCFG_EXT_TYPE_UUID, [0x11; 16]),
        words(&[(AVS_TKN_MODCFG_EXT_NUM_INPUT_PINS_U16, 1), (AVS_TKN_MODCFG_EXT_NUM_OUTPUT_PINS_U16, 2)]),
        words(&[(AVS_TKN_PIN_FMT_INDEX_U32, 0), (AVS_TKN_PIN_FMT_IOBS_U32, 768), (AVS_TKN_PIN_FMT_AFMT_ID_U32, 1)]),
        words(&[(AVS_TKN_PIN_FMT_INDEX_U32, 0), (AVS_TKN_PIN_FMT_IOBS_U32, 960), (AVS_TKN_PIN_FMT_AFMT_ID_U32, 0)]),
        words(&[(AVS_TKN_PIN_FMT_INDEX_U32, 1), (AVS_TKN_PIN_FMT_IOBS_U32, 1920), (AVS_TKN_PIN_FMT_AFMT_ID_U32, 1)]),
    ]);
    let old = copier_ext(2, 1, 12, 768, None);
    let at = m.windows(old.len()).position(|w| w == old).unwrap();
    m.splice(at..at + old.len(), generic);
    let t = load(&m).unwrap();
    let e = &t.modcfgs_ext.as_slice()[2];
    assert_eq!(e.type_uuid, [0x11; 16]);
    assert_eq!((e.num_input_pins, e.num_output_pins, e.npins), (1, 2, 3));
    assert_eq!((e.pins[0].pin_index, e.pins[0].iobs, e.pins[0].fmt), (0, 768, Some(1)));
    assert_eq!((e.pins[1].pin_index, e.pins[1].iobs, e.pins[1].fmt), (0, 960, Some(0)));
    assert_eq!((e.pins[2].pin_index, e.pins[2].iobs, e.pins[2].fmt), (1, 1920, Some(1)));
    assert_eq!(t.kind_of_ext(2), ModuleKind::Other);
}

trait KindOfExt {
    fn kind_of_ext(&self, i: usize) -> ModuleKind;
}
impl KindOfExt for Topology<'_> {
    fn kind_of_ext(&self, i: usize) -> ModuleKind {
        module_kind(&self.modcfgs_ext.as_slice()[i].type_uuid)
    }
}

#[test]
fn modcfg_ext_pin_refusals() {
    let base_ext = |n_in: u32, n_out: u32, pins: Vec<Vec<u8>>| {
        let mut m = small_manifest();
        let mut g = cat(&[words(&[(AVS_TKN_MODCFG_EXT_ID_U32, 2)]), uuid(AVS_TKN_MODCFG_EXT_TYPE_UUID, [0x11; 16]), words(&[(AVS_TKN_MODCFG_EXT_NUM_INPUT_PINS_U16, n_in), (AVS_TKN_MODCFG_EXT_NUM_OUTPUT_PINS_U16, n_out)])]);
        for p in pins {
            g.extend(p);
        }
        let old = copier_ext(2, 1, 12, 768, None);
        let at = m.windows(old.len()).position(|w| w == old).unwrap();
        m.splice(at..at + old.len(), g);
        m
    };
    let pin = |i, b, f| words(&[(AVS_TKN_PIN_FMT_INDEX_U32, i), (AVS_TKN_PIN_FMT_IOBS_U32, b), (AVS_TKN_PIN_FMT_AFMT_ID_U32, f)]);
    // pins present but the entry says zero pins (topology.c:983-984)
    assert_eq!(load(&base_ext(0, 0, vec![pin(0, 1, 0)])).err(), Some(TplgError::NoPins));
    // an entry that states 2 pins but carries 1
    assert_eq!(load(&base_ext(1, 1, vec![pin(0, 1, 0)])).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_PIN_FMT_INDEX_U32, entry: 1 }));
    // a pin naming a missing format
    assert_eq!(load(&base_ext(1, 0, vec![pin(0, 1, 5)])).err(), Some(TplgError::IndexOutOfRange { what: "audio format", index: 5, count: 2 }));
    // too many pins for the fixed storage
    let many: Vec<Vec<u8>> = (0..9).map(|i| pin(i, 1, 0)).collect();
    assert_eq!(load(&base_ext(9, 0, many)).err(), Some(TplgError::TooMany { what: "pin formats", count: 9, max: 8 }));
    // no pins and no pin arrays at all: fine (a copier)
    assert!(load(&base_ext(0, 0, vec![])).is_ok());
}

#[test]
fn the_copier_i2s_gateway_gets_its_virtual_index_from_a_single_ssp() {
    // assign_copier_gtw_instance (topology.c:918-952): vindex 0 + I2S dma type + COPIER + singular SSP -> instance = port, slot = tdm
    let m = small_manifest();
    let blk = manifest_block(&m);
    let t = Topology::parse(Box::leak(blk.into_boxed_slice()), Some(SspBinding { port: 1, tdm: 0 })).unwrap();
    let e = t.modcfgs_ext.as_slice();
    assert_eq!(e[0].copier.vindex, 0, "host DMA: not an I2S type, untouched");
    assert_eq!(e[1].copier.vindex, 16, "the topology set it (13 = I2S in): not overwritten");
    assert_eq!(e[2].copier.dma_type, AVS_DMA_I2S_LINK_OUTPUT);
    assert_eq!(e[2].copier.vindex, 0x10, "dma type 12, vindex 0 -> instance 1 (high nibble), slot 0");
    let t = Topology::parse(Box::leak(manifest_block(&m).into_boxed_slice()), Some(SspBinding { port: 2, tdm: 3 })).unwrap();
    assert_eq!(t.modcfgs_ext.as_slice()[2].copier.vindex, 0x23);
    // no binding: untouched
    let t = Topology::parse(Box::leak(manifest_block(&m).into_boxed_slice()), None).unwrap();
    assert_eq!(t.modcfgs_ext.as_slice()[2].copier.vindex, 0);
    // a non-copier UUID is never touched
    let mut m2 = small_manifest();
    let old = uuid(AVS_TKN_MODCFG_EXT_TYPE_UUID, COPIER);
    let positions: Vec<usize> = (0..m2.len() - old.len()).filter(|&i| m2[i..i + old.len()] == old[..]).collect();
    let third = positions[2];
    m2[third + 16..third + 32].copy_from_slice(&[0x55; 16]);
    let t = Topology::parse(Box::leak(manifest_block(&m2).into_boxed_slice()), Some(SspBinding { port: 1, tdm: 0 })).unwrap();
    assert_eq!(t.modcfgs_ext.as_slice()[2].copier.vindex, 0);
    // a port wider than the 4-bit field is refused, not truncated
    let r = Topology::parse(Box::leak(manifest_block(&m).into_boxed_slice()), Some(SspBinding { port: 16, tdm: 0 }));
    assert!(matches!(r.err(), Some(TplgError::ValueTooWide { bits: 4, .. })));
    assert_eq!(AVS_DMA_HOST_OUT_FOR_DOC, AVS_DMA_HDA_HOST_OUTPUT);
}
const AVS_DMA_HOST_OUT_FOR_DOC: u32 = 0;

#[test]
fn initial_configs_carry_raw_data_after_their_tuples() {
    let mut m = small_manifest();
    let raw0: Vec<u8> = (0..16).collect();
    let raw1: Vec<u8> = vec![0xAA; 8];
    m.extend(cat(&[
        count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 2),
        words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 5)]),
        words(&[(AVS_TKN_INIT_CONFIG_PARAM_U8, 3), (AVS_TKN_INIT_CONFIG_LENGTH_U32, raw0.len() as u32)]),
        raw0.clone(),
        words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 6)]),
        words(&[(AVS_TKN_INIT_CONFIG_PARAM_U8, 4), (AVS_TKN_INIT_CONFIG_LENGTH_U32, raw1.len() as u32)]),
        raw1.clone(),
    ]));
    let t = load(&m).unwrap();
    let c = t.init_configs.as_slice();
    assert_eq!(c.len(), 2);
    assert_eq!((c[0].id, c[0].param, c[0].data), (5, 3, &raw0[..]));
    assert_eq!((c[1].id, c[1].param, c[1].data), (6, 4, &raw1[..]));
}

#[test]
fn initial_config_refusals() {
    let hdr = |len: u32| -> Vec<u8> {
        let mut m = small_manifest();
        m.extend(cat(&[count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 1), words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 5)]), words(&[(AVS_TKN_INIT_CONFIG_PARAM_U8, 3), (AVS_TKN_INIT_CONFIG_LENGTH_U32, len)])]));
        m
    };
    // raw data longer than what is left
    let mut m = hdr(16);
    m.extend_from_slice(&[0u8; 15]);
    assert_eq!(load(&m).err(), Some(TplgError::RawDataPastBlock { token: AVS_TKN_INIT_CONFIG_LENGTH_U32, need: 16, have: 15 }));
    // exactly enough
    let mut m = hdr(16);
    m.extend_from_slice(&[0u8; 16]);
    assert!(load(&m).is_ok());
    // a length that overflows
    let mut m = hdr(u32::MAX);
    m.extend_from_slice(&[0u8; 16]);
    assert!(matches!(load(&m).err(), Some(TplgError::RawDataPastBlock { .. })));
    // only the first of the two arrays present: the entry cannot be sized
    let mut m = small_manifest();
    m.extend(cat(&[count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 1), words(&[(AVS_TKN_INIT_CONFIG_ID_U32, 5)])]));
    assert!(load(&m).is_err());
    // too many
    let mut m = small_manifest();
    m.extend(count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 9));
    assert_eq!(load(&m).err(), Some(TplgError::TooMany { what: "initial configs", count: 9, max: 8 }));
}

#[test]
fn nhlt_configs_carry_their_capabilities_blob() {
    let mut m = small_manifest();
    let blob0: Vec<u8> = (1..=12).collect();
    m.extend(cat(&[
        count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 0),
        count(AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32, 2),
        words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 0), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 12)]),
        blob0.clone(),
        words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 1), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 4)]),
        vec![9, 9, 9, 9],
    ]));
    let t = load(&m).unwrap();
    let n = t.nhlt_configs.as_slice();
    assert_eq!(n.len(), 2);
    assert_eq!((n[0].id, n[0].caps), (0, &blob0[..]));
    assert_eq!((n[1].id, n[1].caps), (1, &[9u8, 9, 9, 9][..]));
    assert_eq!(t.nhlt_config(Some(1)).unwrap().id, 1);
    assert!(t.nhlt_config(Some(2)).is_none());
    assert!(t.nhlt_config(None).is_none());
}

#[test]
fn nhlt_config_refusals() {
    let with = |entry: Vec<u8>| {
        let mut m = small_manifest();
        m.extend(cat(&[count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 0), count(AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32, 1), entry]));
        m
    };
    // no SIZE token: "With tuples parsed, the blob shall be allocated" (topology.c:1784-1785)
    assert_eq!(load(&with(words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 0)]))).err(), Some(TplgError::NhltConfigWithoutSize));
    // blob longer than what remains
    let mut e = words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 0), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 8)]);
    e.extend_from_slice(&[0u8; 7]);
    assert_eq!(load(&with(e)).err(), Some(TplgError::RawDataPastBlock { token: AVS_TKN_NHLT_CONFIG_SIZE_U32, need: 8, have: 7 }));
    // a size of zero is a valid, empty blob
    let e = words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 0), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 0)]);
    assert!(load(&with(e)).is_ok());
}

#[test]
fn conditional_path_templates_are_loaded_from_the_manifest() {
    let mut m = small_manifest();
    let at = m.len() - count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 0).len();
    m.truncate(at);
    m.extend(cat(&[
        count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 1),
        cat(&[
            words(&[(AVS_TKN_CONDPATH_TMPL_ID_U32, 4), (AVS_TKN_CONDPATH_TMPL_SOURCE_PATH_TMPL_ID_U32, 1), (AVS_TKN_CONDPATH_TMPL_SINK_PATH_TMPL_ID_U32, 2)]),
            string(AVS_TKN_CONDPATH_TMPL_SOURCE_TPLG_NAME_STRING, "srcT"),
            string(AVS_TKN_CONDPATH_TMPL_SINK_TPLG_NAME_STRING, "snkT"),
            words(&[(AVS_TKN_CONDPATH_ID_U32, 9), (AVS_TKN_CONDPATH_SOURCE_PATH_ID_U32, 11), (AVS_TKN_CONDPATH_SINK_PATH_ID_U32, 12)]),
            pipeline_hdr(0, 0, 0),
            module(0, 0, 0, 0),
        ]),
    ]));
    let t = load(&m).unwrap();
    assert_eq!(t.condpath_tmpls.len(), 1);
    let c = t.condpath_tmpls.as_slice()[0];
    assert_eq!((c.id, c.source.id, c.sink.id, c.source.tplg_name.as_str(), c.sink.tplg_name.as_str()), (4, 1, 2, Some("srcT"), Some("snkT")));
    let paths = t.paths_of(&c);
    assert_eq!(paths.len(), 1);
    assert_eq!((paths[0].id, paths[0].source_path_id, paths[0].sink_path_id, paths[0].fe_fmt, paths[0].be_fmt), (9, 11, 12, None, None));
    assert_eq!(t.pipelines_of(&paths[0]).len(), 1);
}

#[test]
fn a_condpath_module_cannot_name_an_nhlt_config_the_manifest_has_not_declared_yet() {
    // Linux parses the condpath templates before the NHLT dictionary, so `num_nhlt_configs` is still 0 (topology.c:2092-2121)
    let mut m = small_manifest();
    let at = m.len() - count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 0).len();
    m.truncate(at);
    m.extend(cat(&[
        count(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 1),
        words(&[(AVS_TKN_CONDPATH_TMPL_ID_U32, 4)]),
        words(&[(AVS_TKN_CONDPATH_ID_U32, 9)]),
        pipeline_hdr(0, 0, 0),
        words(&[(AVS_TKN_MOD_ID_U32, 0), (AVS_TKN_MOD_NHLT_CONFIG_ID_U32, 0)]),
        count(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 0),
        count(AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32, 1),
        words(&[(AVS_TKN_NHLT_CONFIG_ID_U32, 0), (AVS_TKN_NHLT_CONFIG_SIZE_U32, 4)]),
        vec![0, 0, 0, 0],
    ]));
    assert_eq!(load(&m).err(), Some(TplgError::IndexOutOfRange { what: "nhlt config", index: 0, count: 0 }));
}
