// SPDX-License-Identifier: GPL-2.0-only
//! Path templates: the private data of a DAPM widget (`avs_tplg_path_template_create`, topology.c:1580-1602) and of a conditional
//! path template (`avs_tplg_parse_condpath_templates`). A template holds one or more PATH variants; a path holds PIPELINES; a
//! pipeline holds MODULES followed by optional BINDING ids. Each level ends where the next sibling's id token begins.
use crate::error::TplgError;
use crate::manifest::{for_entries, sub, tail};
use crate::model::*;
use crate::tokens::*;
use crate::topology::Topology;
use crate::vendor::*;

impl<'a> Topology<'a> {
    /// `avs_tplg_module_create` (topology.c:1221-1274) over exactly the module's bytes.
    fn create_module(&mut self, e: &[u8]) -> Result<(), TplgError> {
        let counts = self.counts;
        let total = e.len() as u32;
        // "See where config id block starts."
        let esize = entry_size(e, total, AVS_TKN_MOD_INIT_CONFIG_ID_U32)?;
        let mut m = Module::default();
        parse_tokens(&mut ModuleSink { module: &mut m, counts }, &e[..esize as usize])?;
        let rest = sub(total, esize)?;
        // "Parse trailing config ids if any."
        if rest != 0 {
            let n = m.num_config_ids;
            if n == 0 {
                return Err(TplgError::ConfigIdsWithoutCount);
            }
            if n as usize > MAX_CONFIG_IDS {
                return Err(TplgError::TooMany { what: "module config ids", count: n, max: MAX_CONFIG_IDS as u32 });
            }
            let ids = &mut m.config_ids;
            for_entries(tail(e, esize)?, rest, n, AVS_TKN_MOD_INIT_CONFIG_ID_U32, |i, ie| {
                let mut id = 0u32;
                parse_tokens(&mut IdListSink { token: AVS_TKN_MOD_INIT_CONFIG_ID_U32, out: &mut id }, ie)?;
                ids[i as usize] = id;
                Ok(())
            })?;
            m.nconfig_ids = n as u8;
        }
        self.modules.push("modules", m)?;
        Ok(())
    }

    /// `avs_tplg_pipeline_create` (topology.c:1306-1390) over the pipeline's bytes.
    fn create_pipeline(&mut self, e: &[u8]) -> Result<(), TplgError> {
        let counts = self.counts;
        let mut block = e.len() as u32;
        let mut cur = e;
        let mut ppl = Pipeline { first_module: self.modules.len() as u16, ..Pipeline::default() };
        // "Pipeline header MUST be followed by at least one module."
        let off = match lookup(cur, block, AVS_TKN_MOD_ID_U32)? {
            Some(o) if o != 0 => o,
            _ => return Err(TplgError::PipelineWithoutModule),
        };
        parse_tokens(&mut PipelineSink { ppl: &mut ppl, counts }, &cur[..off as usize])?;
        block = sub(block, off)?;
        cur = tail(cur, off)?;
        // "Optionally, binding sections follow module ones."
        let mut modblk = match lookup_next(cur, block, AVS_TKN_PPL_BINDING_ID_U32)? {
            Some(o) => o,
            None => {
                // "Does header information match actual block layout?"
                if ppl.num_bindings != 0 {
                    return Err(TplgError::BindingCountMismatch { states: ppl.num_bindings, found: 0 });
                }
                block
            }
        };
        if ppl.num_bindings as usize > MAX_PPL_BINDINGS {
            return Err(TplgError::TooMany { what: "pipeline bindings", count: ppl.num_bindings, max: MAX_PPL_BINDINGS as u32 });
        }
        block = sub(block, modblk)?;
        loop {
            let esize = entry_size(cur, modblk, AVS_TKN_MOD_ID_U32)?;
            self.create_module(&cur[..esize as usize])?;
            ppl.nmodules += 1;
            modblk = sub(modblk, esize)?;
            cur = tail(cur, esize)?;
            if modblk == 0 {
                break;
            }
        }
        // "What's left is optional range of bindings."
        let bcount = counts.bindings;
        let ids = &mut ppl.bindings;
        for_entries(cur, block, ppl.num_bindings, AVS_TKN_PPL_BINDING_ID_U32, |i, be| {
            let mut id = 0u32;
            parse_tokens(&mut IdListSink { token: AVS_TKN_PPL_BINDING_ID_U32, out: &mut id }, be)?;
            if id >= bcount {
                return Err(TplgError::IndexOutOfRange { what: "binding", index: id, count: bcount });
            }
            ids[i as usize] = id as u16;
            Ok(())
        })?;
        ppl.nbindings = ppl.num_bindings as u8;
        self.pipelines.push("pipelines", ppl)?;
        Ok(())
    }

    /// `avs_tplg_path_create` (topology.c:1434-1490) over the path's bytes.
    fn create_path(&mut self, e: &[u8], kind: PathKind) -> Result<(), TplgError> {
        let counts = self.counts;
        let mut block = e.len() as u32;
        let mut cur = e;
        let mut path = Path { first_pipeline: self.pipelines.len() as u16, ..Path::default() };
        // "Path header MAY be followed by one or more pipelines."
        // (Linux's `!offset` -> -EINVAL here cannot happen: a path entry begins at a PATH_ID array, never a PPL_ID one.)
        let off = lookup(cur, block, AVS_TKN_PPL_ID_U32)?.unwrap_or(block);
        parse_tokens(&mut PathSink { path: &mut path, kind, counts }, &cur[..off as usize])?;
        block = sub(block, off)?;
        cur = tail(cur, off)?;
        while block > 0 {
            let esize = entry_size(cur, block, AVS_TKN_PPL_ID_U32)?;
            self.create_pipeline(&cur[..esize as usize])?;
            path.npipelines += 1;
            block = sub(block, esize)?;
            cur = tail(cur, esize)?;
        }
        self.paths.push("paths", path)?;
        Ok(())
    }

    /// `parse_path_template` (topology.c:1534-1578): the template header, then one or more path variants.
    pub(crate) fn parse_path_template(&mut self, e: &[u8], tmpl: &mut PathTemplate, kind: PathKind) -> Result<(), TplgError> {
        let mut block = e.len() as u32;
        let mut cur = e;
        tmpl.first_path = self.paths.len() as u16;
        // "Path template header MUST be followed by at least one path variant."
        let off = lookup(cur, block, AVS_TKN_PATH_ID_U32)?.ok_or(TplgError::TemplateWithoutPath)?;
        parse_tokens(&mut TemplateSink { tmpl, kind }, &cur[..off as usize])?;
        block = sub(block, off)?;
        cur = tail(cur, off)?;
        loop {
            let esize = entry_size(cur, block, AVS_TKN_PATH_ID_U32)?;
            self.create_path(&cur[..esize as usize], kind)?;
            tmpl.npaths += 1;
            block = sub(block, esize)?;
            cur = tail(cur, esize)?;
            if block == 0 {
                break;
            }
        }
        Ok(())
    }
}
