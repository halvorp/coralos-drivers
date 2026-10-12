// SPDX-License-Identifier: GPL-2.0-only
//! The manifest block: the header (name, version) and the dictionaries `avs_manifest` parses in a fixed order (topology.c:1961-2123):
//! libraries, audio formats, module configs (base, ext), pipeline configs, bindings, conditional-path templates, initial configs
//! and NHLT configs. Each dictionary is a header tuple stating its entry count, then that many entries; where one ends is where
//! the NEXT dictionary's header token is found.
//!
//! Differences from Linux, each a refusal where Linux reads past its block: a dictionary promising more entries than its block
//! holds (Linux's init-config and NHLT loops stop quietly at the end of the block), a raw-data section (init config, NHLT blob)
//! longer than what is left, and, in the NHLT dictionary, the remaining-size
//! bookkeeping (Linux forgets to subtract the offset of the NHLT header, topology.c:2118-2121).
use crate::error::TplgError;
use crate::model::*;
use crate::tokens::*;
use crate::topology::{InitConfig, NhltConfig, Topology};
use crate::vendor::*;
use avs_ipc_core::modcfg::{AudioFormat, AVS_COPIER_MOD_UUID, AVS_DMA_I2S_LINK_INPUT, AVS_DMA_I2S_LINK_OUTPUT};

fn rd32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

pub(crate) fn tail(b: &[u8], n: u32) -> Result<&[u8], TplgError> {
    b.get(n as usize..).ok_or(TplgError::Truncated { what: "vendor block", need: n as usize, have: b.len() })
}

pub(crate) fn sub(a: u32, b: u32) -> Result<u32, TplgError> {
    a.checked_sub(b).ok_or(TplgError::Truncated { what: "vendor block", need: b as usize, have: a as usize })
}

/// `parse_dictionary_header` (topology.c:440-462): the first array's first tuple must be the entry-count token. Returns the count
/// and the size of the header array.
fn dict_header(b: &[u8], block: u32, token: u32) -> Result<(u32, u32), TplgError> {
    let blk = b.get(..block as usize).ok_or(TplgError::Truncated { what: "dictionary", need: block as usize, have: b.len() })?;
    let (size, _ty, n) = array_header(blk, 0)?;
    if size > block {
        return Err(TplgError::ArrayPastBlock { at: 0, size, block });
    }
    if n < 1 || (size as usize) < ARRAY_HEADER_SIZE + VALUE_ELEM_SIZE {
        return Err(TplgError::BadDictionaryHeader { want: token, got: 0 });
    }
    let got = rd32(blk, ARRAY_HEADER_SIZE);
    if got != token {
        return Err(TplgError::BadDictionaryHeader { want: token, got });
    }
    Ok((rd32(blk, ARRAY_HEADER_SIZE + 4), size))
}

/// `parse_dictionary_entries` (topology.c:464-496): `count` entries, each ending where the next `entry_token` array begins (the last
/// ends with the block). `f` receives the entry index and exactly its bytes.
pub(crate) fn for_entries<F>(b: &[u8], mut block: u32, count: u32, entry_token: u32, mut f: F) -> Result<(), TplgError>
where
    F: FnMut(u32, &[u8]) -> Result<(), TplgError>,
{
    let mut b = b;
    for i in 0..count {
        if block == 0 {
            return Err(TplgError::DictionaryTooShort { token: entry_token, entry: i });
        }
        let esize = entry_size(b, block, entry_token)?;
        f(i, &b[..esize as usize])?;
        block = sub(block, esize)?;
        b = tail(b, esize)?;
    }
    Ok(())
}

fn cap(what: &'static str, n: u32, max: usize) -> Result<(), TplgError> {
    if n as usize > max {
        return Err(TplgError::TooMany { what, count: n, max: max as u32 });
    }
    Ok(())
}

/// `assign_copier_gtw_instance` (topology.c:918-952).
fn assign_copier_gtw_instance(cfg: &mut ModCfgExt, ssp: Option<SspBinding>) -> Result<(), TplgError> {
    if cfg.type_uuid != AVS_COPIER_MOD_UUID {
        return Ok(());
    }
    // "Only I2S boards assign port instance in ->i2s_link_mask."
    if cfg.copier.dma_type != AVS_DMA_I2S_LINK_OUTPUT && cfg.copier.dma_type != AVS_DMA_I2S_LINK_INPUT {
        return Ok(());
    }
    // "If topology sets value don't overwrite it"
    if cfg.copier.vindex != 0 {
        return Ok(());
    }
    let Some(s) = ssp else { return Ok(()) };
    if s.port > 15 || s.tdm > 15 {
        return Err(TplgError::ValueTooWide { token: AVS_TKN_MODCFG_CPR_VINDEX_U8, value: s.port.max(s.tdm), bits: 4 });
    }
    // union avs_virtual_index.i2s: time_slot:4 then instance:4
    cfg.copier.vindex = (s.tdm as u8) | ((s.port as u8) << 4);
    Ok(())
}

impl<'a> Topology<'a> {
    /// `avs_tplg_parse_modcfg_ext` (topology.c:954-1002).
    fn parse_modcfg_ext(&mut self, i: usize, e: &[u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
        let counts = self.counts;
        let total = e.len() as u32;
        // "See where pin block starts."
        let esize = entry_size(e, total, AVS_TKN_PIN_FMT_INDEX_U32)?;
        let cfg = self.modcfgs_ext.get_mut(i).ok_or(TplgError::IndexOutOfRange { what: "module config ext", index: i as u32, count: 0 })?;
        parse_tokens(&mut ModCfgExtSink { cfg, counts }, &e[..esize as usize])?;
        assign_copier_gtw_instance(cfg, ssp)?;
        let rest = sub(total, esize)?;
        // "Parse trailing in/out pin formats if any."
        if rest != 0 {
            let npins = cfg.num_input_pins as u32 + cfg.num_output_pins as u32;
            if npins == 0 {
                return Err(TplgError::NoPins);
            }
            cap("pin formats", npins, MAX_PINS)?;
            let pins = &mut cfg.pins;
            for_entries(tail(e, esize)?, rest, npins, AVS_TKN_PIN_FMT_INDEX_U32, |k, pe| {
                parse_tokens(&mut PinSink { pin: &mut pins[k as usize], counts }, pe)
            })?;
            cfg.npins = npins as u8;
        }
        Ok(())
    }

    /// `avs_manifest` (topology.c:1961-2123) over the manifest's private data.
    pub(crate) fn load_manifest(&mut self, priv_: &'a [u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
        let mut tuples = priv_;
        let mut remaining = priv_.len() as u32;
        let find = |t: &[u8], rem: u32, token: u32| -> Result<u32, TplgError> { lookup(t, rem, token)?.ok_or(TplgError::MissingDictionary { token }) };

        // "Manifest MUST begin with a header."
        let off = find(tuples, remaining, AVS_TKN_MANIFEST_NUM_LIBRARIES_U32)?;
        if off == 0 {
            return Err(TplgError::ManifestWithoutHeader);
        }
        parse_tokens(&mut HeaderSink { header: &mut self.header, ssp }, &tuples[..off as usize])?;
        remaining = sub(remaining, off)?;
        tuples = tail(tuples, off)?;

        // libraries
        let off = find(tuples, remaining, AVS_TKN_MANIFEST_NUM_AFMTS_U32)?;
        let (n, hs) = dict_header(tuples, off, AVS_TKN_MANIFEST_NUM_LIBRARIES_U32)?;
        cap("libraries", n, MAX_LIBS)?;
        self.libs.reserve_default("libraries", n)?;
        let libs = &mut self.libs;
        for_entries(tail(tuples, hs)?, sub(off, hs)?, n, AVS_TKN_LIBRARY_ID_U32, |i, e| parse_tokens(libs.get_mut(i as usize).ok_or(TplgError::Truncated { what: "library", need: 0, have: 0 })?, e))?;
        remaining = sub(remaining, off)?;
        tuples = tail(tuples, off)?;

        // audio formats
        let off = find(tuples, remaining, AVS_TKN_MANIFEST_NUM_MODCFGS_BASE_U32)?;
        let (n, hs) = dict_header(tuples, off, AVS_TKN_MANIFEST_NUM_AFMTS_U32)?;
        cap("audio formats", n, MAX_FMTS)?;
        self.fmts.reserve_default("audio formats", n)?;
        self.counts.fmts = n;
        let fmts = &mut self.fmts;
        for_entries(tail(tuples, hs)?, sub(off, hs)?, n, AVS_TKN_AFMT_ID_U32, |i, e| {
            let f: &mut AudioFormat = fmts.get_mut(i as usize).ok_or(TplgError::Truncated { what: "audio format", need: 0, have: 0 })?;
            parse_tokens(f, e)
        })?;
        remaining = sub(remaining, off)?;
        tuples = tail(tuples, off)?;

        // module configs, base
        let off = find(tuples, remaining, AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32)?;
        let (n, hs) = dict_header(tuples, off, AVS_TKN_MANIFEST_NUM_MODCFGS_BASE_U32)?;
        cap("module configs (base)", n, MAX_MODCFGS_BASE)?;
        self.modcfgs_base.reserve_default("module configs (base)", n)?;
        self.counts.modcfgs_base = n;
        let base = &mut self.modcfgs_base;
        for_entries(tail(tuples, hs)?, sub(off, hs)?, n, AVS_TKN_MODCFG_BASE_ID_U32, |i, e| parse_tokens(base.get_mut(i as usize).ok_or(TplgError::Truncated { what: "modcfg base", need: 0, have: 0 })?, e))?;
        remaining = sub(remaining, off)?;
        tuples = tail(tuples, off)?;

        // module configs, ext
        let off = find(tuples, remaining, AVS_TKN_MANIFEST_NUM_PPLCFGS_U32)?;
        let (n, hs) = dict_header(tuples, off, AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32)?;
        cap("module configs (ext)", n, MAX_MODCFGS_EXT)?;
        self.modcfgs_ext.reserve_default("module configs (ext)", n)?;
        self.counts.modcfgs_ext = n;
        let mut block = sub(off, hs)?;
        let mut cur = tail(tuples, hs)?;
        for i in 0..n {
            if block == 0 {
                return Err(TplgError::DictionaryTooShort { token: AVS_TKN_MODCFG_EXT_ID_U32, entry: i });
            }
            let esize = entry_size(cur, block, AVS_TKN_MODCFG_EXT_ID_U32)?;
            self.parse_modcfg_ext(i as usize, &cur[..esize as usize], ssp)?;
            block = sub(block, esize)?;
            cur = tail(cur, esize)?;
        }
        remaining = sub(remaining, off)?;
        tuples = tail(tuples, off)?;

        // pipeline configs
        let off = find(tuples, remaining, AVS_TKN_MANIFEST_NUM_BINDINGS_U32)?;
        let (n, hs) = dict_header(tuples, off, AVS_TKN_MANIFEST_NUM_PPLCFGS_U32)?;
        cap("pipeline configs", n, MAX_PPLCFGS)?;
        self.pplcfgs.reserve_default("pipeline configs", n)?;
        self.counts.pplcfgs = n;
        let ppl = &mut self.pplcfgs;
        for_entries(tail(tuples, hs)?, sub(off, hs)?, n, AVS_TKN_PPLCFG_ID_U32, |i, e| parse_tokens(ppl.get_mut(i as usize).ok_or(TplgError::Truncated { what: "pplcfg", need: 0, have: 0 })?, e))?;
        remaining = sub(remaining, off)?;
        tuples = tail(tuples, off)?;

        // bindings
        let off = find(tuples, remaining, AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32)?;
        let (n, hs) = dict_header(tuples, off, AVS_TKN_MANIFEST_NUM_BINDINGS_U32)?;
        cap("bindings", n, MAX_BINDINGS)?;
        self.bindings.reserve_default("bindings", n)?;
        self.counts.bindings = n;
        let bindings = &mut self.bindings;
        for_entries(tail(tuples, hs)?, sub(off, hs)?, n, AVS_TKN_BINDING_ID_U32, |i, e| {
            let b = bindings.get_mut(i as usize).ok_or(TplgError::Truncated { what: "binding", need: 0, have: 0 })?;
            parse_tokens(&mut BindingSink { binding: b, ssp }, e)
        })?;
        remaining = sub(remaining, off)?;
        tuples = tail(tuples, off)?;

        // conditional-path templates; "has_init_config" decides where this dictionary ends (topology.c:2081-2094)
        let init_off = lookup(tuples, remaining, AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32)?;
        let cond_block = init_off.unwrap_or(remaining);
        self.load_condpaths(tuples, cond_block)?;
        let Some(init_off) = init_off else { return Ok(()) };
        remaining = sub(remaining, init_off)?;
        tuples = tail(tuples, init_off)?;

        // initial configs
        let consumed = self.load_init_configs(tuples, remaining)?;
        remaining = sub(remaining, consumed)?;
        tuples = tail(tuples, consumed)?;

        // NHLT configs (optional)
        let Some(nhlt_off) = lookup(tuples, remaining, AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32)? else { return Ok(()) };
        remaining = sub(remaining, nhlt_off)?;
        tuples = tail(tuples, nhlt_off)?;
        self.load_nhlt_configs(tuples, remaining)
    }

    /// `avs_tplg_parse_condpath_templates` (topology.c:1604-1652).
    fn load_condpaths(&mut self, tuples: &'a [u8], block: u32) -> Result<(), TplgError> {
        let (n, hs) = dict_header(tuples, block, AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32)?;
        cap("conditional path templates", n, MAX_CONDPATH_TMPLS)?;
        self.condpath_tmpls.reserve_default("conditional path templates", n)?;
        let mut left = sub(block, hs)?;
        let mut cur = tail(tuples, hs)?;
        for i in 0..n {
            if left == 0 {
                return Err(TplgError::DictionaryTooShort { token: AVS_TKN_CONDPATH_TMPL_ID_U32, entry: i });
            }
            let esize = entry_size(cur, left, AVS_TKN_CONDPATH_TMPL_ID_U32)?;
            let mut t = PathTemplate::default();
            self.parse_path_template(&cur[..esize as usize], &mut t, PathKind::Conditional)?;
            if let Some(slot) = self.condpath_tmpls.get_mut(i as usize) {
                *slot = t;
            }
            left = sub(left, esize)?;
            cur = tail(cur, esize)?;
        }
        Ok(())
    }

    /// `avs_tplg_parse_initial_configs` (topology.c:1675-1733): each entry is TWO tuple arrays followed by `length` raw bytes.
    /// Returns how many bytes of the manifest it consumed.
    fn load_init_configs(&mut self, tuples: &'a [u8], block: u32) -> Result<u32, TplgError> {
        let (n, hs) = dict_header(tuples, block, AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32)?;
        cap("initial configs", n, MAX_INIT_CONFIGS)?;
        self.init_configs.reserve_default("initial configs", n)?;
        let mut offset = hs;
        let mut left = sub(block, hs)?;
        let mut cur: &'a [u8] = tail(tuples, hs)?;
        let mut i = 0;
        while i < n {
            if left == 0 {
                // Linux's loop is `i < n && block_size > 0`: it stops quietly and leaves the rest zeroed
                return Err(TplgError::DictionaryTooShort { token: AVS_TKN_INIT_CONFIG_ID_U32, entry: i });
            }
            // "Usually to get section length we search for first token of next group of data, but in this case we can't as tuples
            // are followed by raw data."
            let (s1, _, _) = array_header(cur.get(..left as usize).ok_or(TplgError::Truncated { what: "init config", need: left as usize, have: cur.len() })?, offset as usize)?;
            let rest = tail(cur, s1)?;
            let (s2, _, _) = array_header(rest.get(..sub(left, s1)? as usize).ok_or(TplgError::Truncated { what: "init config", need: s1 as usize, have: cur.len() })?, (offset + s1) as usize)?;
            let esize = s1 + s2;
            if esize > left {
                return Err(TplgError::ArrayPastBlock { at: offset as usize, size: esize, block: left });
            }
            let mut hdr = InitConfigHdr::default();
            // one entry of two arrays: parse both
            parse_tokens(&mut hdr, &cur[..esize as usize])?;
            offset += esize;
            left -= esize;
            // raw data section
            if hdr.length > left {
                return Err(TplgError::RawDataPastBlock { token: AVS_TKN_INIT_CONFIG_LENGTH_U32, need: hdr.length, have: left });
            }
            let data = &cur[esize as usize..(esize + hdr.length) as usize];
            offset += hdr.length;
            left -= hdr.length;
            cur = tail(cur, esize + hdr.length)?;
            if let Some(slot) = self.init_configs.get_mut(i as usize) {
                *slot = InitConfig { id: hdr.id, param: hdr.param, data };
            }
            i += 1;
        }
        Ok(offset)
    }

    /// `avs_tplg_parse_nhlt_configs` (topology.c:1750-1797): each entry is ONE tuple array (id, size) followed by `size` bytes of
    /// capabilities.
    fn load_nhlt_configs(&mut self, tuples: &'a [u8], block: u32) -> Result<(), TplgError> {
        let (n, hs) = dict_header(tuples, block, AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32)?;
        cap("NHLT configs", n, MAX_NHLT_CONFIGS)?;
        self.nhlt_configs.reserve_default("NHLT configs", n)?;
        self.counts.nhlt_configs = n;
        let mut left = sub(block, hs)?;
        let mut cur: &'a [u8] = tail(tuples, hs)?;
        let mut i = 0;
        while i < n {
            if left == 0 {
                // Linux's loop is `i < n && block_size > 0`: it stops quietly and leaves the rest zeroed
                return Err(TplgError::DictionaryTooShort { token: AVS_TKN_NHLT_CONFIG_ID_U32, entry: i });
            }
            let (esize, _, _) = array_header(cur.get(..left as usize).ok_or(TplgError::Truncated { what: "NHLT config", need: left as usize, have: cur.len() })?, 0)?;
            if esize > left {
                return Err(TplgError::ArrayPastBlock { at: 0, size: esize, block: left });
            }
            let mut hdr = NhltConfigHdr::default();
            parse_tokens(&mut hdr, &cur[..esize as usize])?;
            // "With tuples parsed, the blob shall be allocated."
            let size = hdr.size.ok_or(TplgError::NhltConfigWithoutSize)?;
            let after = left - esize;
            if size > after {
                return Err(TplgError::RawDataPastBlock { token: AVS_TKN_NHLT_CONFIG_SIZE_U32, need: size, have: after });
            }
            let caps = &cur[esize as usize..(esize + size) as usize];
            left = after - size;
            cur = tail(cur, esize + size)?;
            if let Some(slot) = self.nhlt_configs.get_mut(i as usize) {
                *slot = NhltConfig { id: hdr.id, caps };
            }
            i += 1;
        }
        Ok(())
    }
}
