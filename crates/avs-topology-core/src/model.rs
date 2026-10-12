// SPDX-License-Identifier: GPL-2.0-only
//! The typed topology: `struct avs_tplg` and everything it owns (topology.h:17-197), in fixed storage.
//!
//! Linux allocates each object and links them with pointers and lists. Here every collection is a growable `Vec` (no capacity: the
//! file decides, [`crate::grow`]), pointers are indices (`Option<u16>`: `None` is a NULL pointer; an index names a table the
//! file declared, so a file with more than 65535 of one kind is refused), and the list hierarchy (template -> path -> pipeline ->
//! module, plus pins, config ids and binding ids) is flattened into tables with `first`/`count` ranges that preserve list order. The tokens of each object are applied by its [`Sink`] impl, which is the port of that object's
//! `avs_tplg_token_parser[]` table.
use crate::error::TplgError;
use crate::tokens::*;
use crate::vendor::{Name, Sink};
use avs_ipc_core::modcfg::{AudioFormat, AVS_COEFF_CHANNELS_MAX};

/// An index into one of the topology's arrays; `None` is Linux's NULL pointer.
pub type Idx = Option<u16>;

fn byte(token: u32, v: u32) -> Result<u8, TplgError> {
    u8::try_from(v).map_err(|_| TplgError::ValueTooWide { token, value: v, bits: 8 })
}
fn short(token: u32, v: u32) -> Result<u16, TplgError> {
    u16::try_from(v).map_err(|_| TplgError::ValueTooWide { token, value: v, bits: 16 })
}

/// How many objects of each kind the manifest has declared so far: what `AVS_DEFINE_PTR_PARSER` checks an index against
/// (`idx >= acomp->tplg->num_##member`, topology.c:330-346).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub fmts: u32,
    pub modcfgs_base: u32,
    pub modcfgs_ext: u32,
    pub pplcfgs: u32,
    pub bindings: u32,
    pub nhlt_configs: u32,
}

fn index(what: &'static str, value: u32, count: u32) -> Result<Idx, TplgError> {
    // `Idx` is 16 bits wide: an index the width cannot carry names nothing (the dictionaries themselves are unbounded)
    if value >= count || value > u16::MAX as u32 {
        return Err(TplgError::IndexOutOfRange { what, index: value, count });
    }
    Ok(Some(value as u16))
}

// ---- manifest header ---------------------------------------------------------------------------------------------------------
/// `manifest_parsers` (topology.c:1946-1959).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub name: Name,
    pub version: u32,
}

pub struct HeaderSink<'a> {
    pub header: &'a mut Header,
    /// The machine's SSP binding, for `parse_link_formatted_string` (topology.c:398-422).
    pub ssp: Option<SspBinding>,
}

/// `avs_mach_ssp_port` / `avs_mach_ssp_tdm` of a machine whose `i2s_link_mask` names exactly one SSP and one TDM slot: the
/// condition under which Linux rewrites `%d` in names and assigns the copier's virtual index (topology.c:398-422, :918-952).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SspBinding {
    pub port: u32,
    pub tdm: u32,
}

/// `avs_ssp_sprint` (topology.c:376-396): replace the first `%d` of `fmt` by the SSP port, followed by `:tdm` when the TDM slot is
/// non-zero; a string without `%d` is copied. Refused where Linux truncates to 44 bytes.
pub fn ssp_sprint(fmt: &[u8], port: u32, tdm: u32) -> Result<Name, TplgError> {
    let Some(at) = fmt.windows(2).position(|w| w == b"%d") else {
        return Name::from_bytes(fmt);
    };
    let mut buf = [0u8; 128];
    let mut n = 0;
    let mut put = |bytes: &[u8]| -> Result<(), TplgError> {
        if n + bytes.len() > buf.len() {
            return Err(TplgError::NameTooLong);
        }
        buf[n..n + bytes.len()].copy_from_slice(bytes);
        n += bytes.len();
        Ok(())
    };
    put(&fmt[..at])?;
    let mut num = [0u8; 10];
    put(decimal(port, &mut num))?;
    if tdm != 0 {
        put(b":")?;
        put(decimal(tdm, &mut num))?;
    }
    put(&fmt[at + 2..])?;
    Name::from_bytes(&buf[..n])
}

fn decimal(mut v: u32, out: &mut [u8; 10]) -> &[u8] {
    let mut i = out.len();
    loop {
        i -= 1;
        out[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    &out[i..]
}

fn formatted(ssp: Option<SspBinding>, v: &Name) -> Result<Name, TplgError> {
    match ssp {
        Some(s) => ssp_sprint(v.as_bytes(), s.port, s.tdm),
        None => Ok(*v),
    }
}

impl Sink for HeaderSink<'_> {
    fn int(&mut self, token: u32, value: u32) -> Result<(), TplgError> {
        if token == AVS_TKN_MANIFEST_VERSION_U32 {
            self.header.version = value;
        }
        Ok(())
    }
    fn string(&mut self, token: u32, value: &Name) -> Result<(), TplgError> {
        if token == AVS_TKN_MANIFEST_NAME_STRING {
            self.header.name = formatted(self.ssp, value)?;
        }
        Ok(())
    }
}

// ---- library -----------------------------------------------------------------------------------------------------------------
/// `struct avs_tplg_library` (topology.h:48-50).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Library {
    pub name: Name,
}
impl Sink for Library {
    fn string(&mut self, token: u32, value: &Name) -> Result<(), TplgError> {
        if token == AVS_TKN_LIBRARY_NAME_STRING {
            self.name = *value;
        }
        Ok(())
    }
}

// ---- audio format ------------------------------------------------------------------------------------------------------------
impl Sink for AudioFormat {
    fn int(&mut self, token: u32, value: u32) -> Result<(), TplgError> {
        match token {
            AVS_TKN_AFMT_SAMPLE_RATE_U32 => self.sampling_freq = value,
            AVS_TKN_AFMT_BIT_DEPTH_U32 => self.bit_depth = value,
            AVS_TKN_AFMT_CHANNEL_MAP_U32 => self.channel_map = value,
            AVS_TKN_AFMT_CHANNEL_CFG_U32 => self.channel_config = value,
            AVS_TKN_AFMT_INTERLEAVING_U32 => self.interleaving = value,
            // the three bit-fields of the last dword (topology.c:355-374): 8 bits each
            AVS_TKN_AFMT_NUM_CHANNELS_U32 => self.num_channels = byte(token, value)?,
            AVS_TKN_AFMT_VALID_BIT_DEPTH_U32 => self.valid_bit_depth = byte(token, value)?,
            AVS_TKN_AFMT_SAMPLE_TYPE_U32 => self.sample_type = byte(token, value)?,
            _ => {}
        }
        Ok(())
    }
}

// ---- module config base ------------------------------------------------------------------------------------------------------
/// `struct avs_tplg_modcfg_base` (topology.h:53-58).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModCfgBase {
    pub cpc: u32,
    pub ibs: u32,
    pub obs: u32,
    pub is_pages: u32,
}
impl Sink for ModCfgBase {
    fn int(&mut self, token: u32, value: u32) -> Result<(), TplgError> {
        match token {
            AVS_TKN_MODCFG_BASE_CPC_U32 => self.cpc = value,
            AVS_TKN_MODCFG_BASE_IBS_U32 => self.ibs = value,
            AVS_TKN_MODCFG_BASE_OBS_U32 => self.obs = value,
            AVS_TKN_MODCFG_BASE_PAGES_U32 => self.is_pages = value,
            _ => {}
        }
        Ok(())
    }
}

// ---- module config ext -------------------------------------------------------------------------------------------------------
/// `struct avs_tplg_pin_format` (topology.h:60-64).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PinFormat {
    pub pin_index: u32,
    pub iobs: u32,
    pub fmt: Idx,
}

pub struct PinSink<'a> {
    pub pin: &'a mut PinFormat,
    pub counts: Counts,
}
impl Sink for PinSink<'_> {
    fn int(&mut self, token: u32, value: u32) -> Result<(), TplgError> {
        match token {
            AVS_TKN_PIN_FMT_INDEX_U32 => self.pin.pin_index = value,
            AVS_TKN_PIN_FMT_IOBS_U32 => self.pin.iobs = value,
            AVS_TKN_PIN_FMT_AFMT_ID_U32 => self.pin.fmt = index("audio format", value, self.counts.fmts)?,
            _ => {}
        }
        Ok(())
    }
}

/// The copier members of `struct avs_tplg_modcfg_ext` (topology.h:73-80).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CopierExt {
    pub out_fmt: Idx,
    /// Optional override of the NHLT format selection.
    pub blob_fmt: Idx,
    pub feature_mask: u32,
    /// `union avs_virtual_index`: for I2S, `time_slot` in bits 0..3 and `instance` in bits 4..7.
    pub vindex: u8,
    pub dma_type: u32,
    pub dma_buffer_size: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WhmExt {
    pub ref_fmt: Idx,
    pub out_fmt: Idx,
    pub wake_tick_period: u32,
    pub vindex: u8,
    pub dma_type: u32,
    pub dma_buffer_size: u32,
    pub blob_fmt: Idx,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdownMixExt {
    pub out_channel_config: u32,
    pub coefficients_select: u32,
    pub coefficients: [i32; AVS_COEFF_CHANNELS_MAX],
    pub channel_map: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AsrcExt {
    pub out_freq: u32,
    pub mode: u8,
    pub disable_jitter_buffer: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AecExt {
    pub ref_fmt: Idx,
    pub out_fmt: Idx,
    pub cpc_lp_mode: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MuxExt {
    pub ref_fmt: Idx,
    pub out_fmt: Idx,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PeakvolExt {
    pub target_volume: u32,
    pub curve_type: u32,
    pub curve_duration: u32,
}

/// `struct avs_tplg_modcfg_ext` (topology.h:66-125). Linux overlays the per-module members in a union; the module's `type`
/// UUID says which member means anything. Here each member has its own storage, so a token for another module type is kept
/// rather than aliased.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModCfgExt {
    pub type_uuid: [u8; 16],
    pub num_input_pins: u16,
    pub num_output_pins: u16,
    /// This config's pins are `Topology::pins[first_pin .. first_pin + npins]`, input pins first.
    pub first_pin: u32,
    pub npins: u32,
    pub copier: CopierExt,
    pub whm: WhmExt,
    pub updown_mix: UpdownMixExt,
    pub src_out_freq: u32,
    pub asrc: AsrcExt,
    pub wov_cpc_lp_mode: u32,
    pub aec: AecExt,
    pub mux: MuxExt,
    pub micsel_out_fmt: Idx,
    pub peakvol: PeakvolExt,
}

pub struct ModCfgExtSink<'a> {
    pub cfg: &'a mut ModCfgExt,
    pub counts: Counts,
}

impl Sink for ModCfgExtSink<'_> {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        let f = self.counts.fmts;
        let c = &mut *self.cfg;
        match token {
            AVS_TKN_MODCFG_CPR_OUT_AFMT_ID_U32 => c.copier.out_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_CPR_FEATURE_MASK_U32 => c.copier.feature_mask = v,
            AVS_TKN_MODCFG_CPR_VINDEX_U8 => c.copier.vindex = byte(token, v)?,
            AVS_TKN_MODCFG_CPR_DMA_TYPE_U32 => c.copier.dma_type = v,
            AVS_TKN_MODCFG_CPR_DMABUFF_SIZE_U32 => c.copier.dma_buffer_size = v,
            AVS_TKN_MODCFG_CPR_BLOB_FMT_ID_U32 => c.copier.blob_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_MICSEL_OUT_AFMT_ID_U32 => c.micsel_out_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_INTELWOV_CPC_LP_MODE_U32 => c.wov_cpc_lp_mode = v,
            AVS_TKN_MODCFG_SRC_OUT_FREQ_U32 => c.src_out_freq = v,
            AVS_TKN_MODCFG_MUX_REF_AFMT_ID_U32 => c.mux.ref_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_MUX_OUT_AFMT_ID_U32 => c.mux.out_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_AEC_REF_AFMT_ID_U32 => c.aec.ref_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_AEC_OUT_AFMT_ID_U32 => c.aec.out_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_AEC_CPC_LP_MODE_U32 => c.aec.cpc_lp_mode = v,
            AVS_TKN_MODCFG_ASRC_OUT_FREQ_U32 => c.asrc.out_freq = v,
            AVS_TKN_MODCFG_ASRC_MODE_U8 => c.asrc.mode = byte(token, v)?,
            AVS_TKN_MODCFG_ASRC_DISABLE_JITTER_U8 => c.asrc.disable_jitter_buffer = byte(token, v)?,
            AVS_TKN_MODCFG_UPDOWN_MIX_OUT_CHAN_CFG_U32 => c.updown_mix.out_channel_config = v,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_SELECT_U32 => c.updown_mix.coefficients_select = v,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_0_S32 => c.updown_mix.coefficients[0] = v as i32,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_1_S32 => c.updown_mix.coefficients[1] = v as i32,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_2_S32 => c.updown_mix.coefficients[2] = v as i32,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_3_S32 => c.updown_mix.coefficients[3] = v as i32,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_4_S32 => c.updown_mix.coefficients[4] = v as i32,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_5_S32 => c.updown_mix.coefficients[5] = v as i32,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_6_S32 => c.updown_mix.coefficients[6] = v as i32,
            AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_7_S32 => c.updown_mix.coefficients[7] = v as i32,
            AVS_TKN_MODCFG_UPDOWN_MIX_CHAN_MAP_U32 => c.updown_mix.channel_map = v,
            AVS_TKN_MODCFG_EXT_NUM_INPUT_PINS_U16 => c.num_input_pins = short(token, v)?,
            AVS_TKN_MODCFG_EXT_NUM_OUTPUT_PINS_U16 => c.num_output_pins = short(token, v)?,
            AVS_TKN_MODCFG_WHM_REF_AFMT_ID_U32 => c.whm.ref_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_WHM_OUT_AFMT_ID_U32 => c.whm.out_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_WHM_WAKE_TICK_PERIOD_U32 => c.whm.wake_tick_period = v,
            AVS_TKN_MODCFG_WHM_VINDEX_U8 => c.whm.vindex = byte(token, v)?,
            AVS_TKN_MODCFG_WHM_DMA_TYPE_U32 => c.whm.dma_type = v,
            AVS_TKN_MODCFG_WHM_DMABUFF_SIZE_U32 => c.whm.dma_buffer_size = v,
            AVS_TKN_MODCFG_WHM_BLOB_AFMT_ID_U32 => c.whm.blob_fmt = index("audio format", v, f)?,
            AVS_TKN_MODCFG_PEAKVOL_VOLUME_U32 => c.peakvol.target_volume = v,
            AVS_TKN_MODCFG_PEAKVOL_CURVE_TYPE_U32 => c.peakvol.curve_type = v,
            AVS_TKN_MODCFG_PEAKVOL_CURVE_DURATION_U32 => c.peakvol.curve_duration = v,
            _ => {}
        }
        Ok(())
    }
    fn uuid(&mut self, token: u32, value: &[u8; 16]) -> Result<(), TplgError> {
        if token == AVS_TKN_MODCFG_EXT_TYPE_UUID {
            self.cfg.type_uuid = *value;
        }
        Ok(())
    }
}

// ---- pipeline config ---------------------------------------------------------------------------------------------------------
/// `struct avs_tplg_pplcfg` (topology.h:131-138); `trigger` is `enum avs_tplg_trigger` (`AVS_TPLG_TRIGGER_AUTO` = 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PplCfg {
    pub req_size: u16,
    pub priority: u8,
    pub lp: bool,
    pub attributes: u16,
    pub trigger: u32,
}
impl Sink for PplCfg {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        match token {
            AVS_TKN_PPLCFG_REQ_SIZE_U16 => self.req_size = short(token, v)?,
            AVS_TKN_PPLCFG_PRIORITY_U8 => self.priority = byte(token, v)?,
            AVS_TKN_PPLCFG_LOW_POWER_BOOL => self.lp = v != 0,
            AVS_TKN_PPLCFG_ATTRIBUTES_U16 => self.attributes = short(token, v)?,
            AVS_TKN_PPLCFG_TRIGGER_U32 => self.trigger = v,
            _ => {}
        }
        Ok(())
    }
}

// ---- binding -----------------------------------------------------------------------------------------------------------------
/// `struct avs_tplg_binding` (topology.h:140-149).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Binding {
    pub target_tplg_name: Name,
    pub target_path_tmpl_id: u32,
    pub target_ppl_id: u32,
    pub target_mod_id: u32,
    pub target_mod_pin: u8,
    pub mod_id: u32,
    pub mod_pin: u8,
    pub is_sink: u8,
}

pub struct BindingSink<'a> {
    pub binding: &'a mut Binding,
    pub ssp: Option<SspBinding>,
}
impl Sink for BindingSink<'_> {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        let b = &mut *self.binding;
        match token {
            AVS_TKN_BINDING_TARGET_PATH_TMPL_ID_U32 => b.target_path_tmpl_id = v,
            AVS_TKN_BINDING_TARGET_PPL_ID_U32 => b.target_ppl_id = v,
            AVS_TKN_BINDING_TARGET_MOD_ID_U32 => b.target_mod_id = v,
            AVS_TKN_BINDING_TARGET_MOD_PIN_U8 => b.target_mod_pin = byte(token, v)?,
            AVS_TKN_BINDING_MOD_ID_U32 => b.mod_id = v,
            AVS_TKN_BINDING_MOD_PIN_U8 => b.mod_pin = byte(token, v)?,
            AVS_TKN_BINDING_IS_SINK_U8 => b.is_sink = byte(token, v)?,
            _ => {}
        }
        Ok(())
    }
    fn string(&mut self, token: u32, value: &Name) -> Result<(), TplgError> {
        if token == AVS_TKN_BINDING_TARGET_TPLG_NAME_STRING {
            self.binding.target_tplg_name = formatted(self.ssp, value)?;
        }
        Ok(())
    }
}

// ---- init config and NHLT config ---------------------------------------------------------------------------------------------
/// `struct avs_tplg_init_config` (topology.h:165-170): `data` is the raw section that follows the tuples (`length` bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InitConfigHdr {
    pub id: u32,
    pub param: u8,
    pub length: u32,
}
impl Sink for InitConfigHdr {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        match token {
            AVS_TKN_INIT_CONFIG_ID_U32 => self.id = v,
            AVS_TKN_INIT_CONFIG_PARAM_U8 => self.param = byte(token, v)?,
            AVS_TKN_INIT_CONFIG_LENGTH_U32 => self.length = v,
            _ => {}
        }
        Ok(())
    }
}

/// `struct avs_tplg_nhlt_config` (topology.h:172-175): `size` is the `capabilities_size` of the blob that follows the tuples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NhltConfigHdr {
    pub id: u32,
    pub size: Option<u32>,
}
impl Sink for NhltConfigHdr {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        match token {
            AVS_TKN_NHLT_CONFIG_ID_U32 => self.id = v,
            AVS_TKN_NHLT_CONFIG_SIZE_U32 => self.size = Some(v),
            _ => {}
        }
        Ok(())
    }
}

// ---- module, pipeline, path, template ----------------------------------------------------------------------------------------
/// `struct avs_tplg_module` (topology.h:194-209).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Module {
    pub id: u32,
    pub cfg_base: Idx,
    pub in_fmt: Idx,
    pub core_id: u8,
    pub domain: u8,
    pub cfg_ext: Idx,
    pub ctl_id: u32,
    pub num_config_ids: u32,
    /// This module's init-config ids are `Topology::config_ids[first_config_id .. first_config_id + nconfig_ids]`.
    pub first_config_id: u32,
    pub nconfig_ids: u32,
    pub nhlt_config: Idx,
}

pub struct ModuleSink<'a> {
    pub module: &'a mut Module,
    pub counts: Counts,
}
impl Sink for ModuleSink<'_> {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        let m = &mut *self.module;
        match token {
            AVS_TKN_MOD_ID_U32 => m.id = v,
            AVS_TKN_MOD_MODCFG_BASE_ID_U32 => m.cfg_base = index("module config base", v, self.counts.modcfgs_base)?,
            AVS_TKN_MOD_IN_AFMT_ID_U32 => m.in_fmt = index("audio format", v, self.counts.fmts)?,
            AVS_TKN_MOD_CORE_ID_U8 => m.core_id = byte(token, v)?,
            AVS_TKN_MOD_PROC_DOMAIN_U8 => m.domain = byte(token, v)?,
            AVS_TKN_MOD_MODCFG_EXT_ID_U32 => m.cfg_ext = index("module config ext", v, self.counts.modcfgs_ext)?,
            // `avs_parse_byte_token` into a u32 field (topology.c:1193, :1199): only the low byte is written; larger values refused
            AVS_TKN_MOD_KCONTROL_ID_U32 => m.ctl_id = byte(token, v)? as u32,
            AVS_TKN_MOD_INIT_CONFIG_NUM_IDS_U32 => m.num_config_ids = byte(token, v)? as u32,
            AVS_TKN_MOD_NHLT_CONFIG_ID_U32 => m.nhlt_config = index("nhlt config", v, self.counts.nhlt_configs)?,
            _ => {}
        }
        Ok(())
    }
}

/// `struct avs_tplg_pipeline` (topology.h:178-191): modules and bindings by range into the pools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pipeline {
    pub id: u32,
    pub cfg: Idx,
    /// `num_bindings` as the pipeline header states it.
    pub num_bindings: u32,
    /// This pipeline's binding ids are `Topology::pipeline_bindings[first_binding .. first_binding + nbindings]`.
    pub first_binding: u32,
    pub nbindings: u32,
    pub first_module: u32,
    pub nmodules: u32,
}

pub struct PipelineSink<'a> {
    pub ppl: &'a mut Pipeline,
    pub counts: Counts,
}
impl Sink for PipelineSink<'_> {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        match token {
            AVS_TKN_PPL_ID_U32 => self.ppl.id = v,
            AVS_TKN_PPL_PPLCFG_ID_U32 => self.ppl.cfg = index("pipeline config", v, self.counts.pplcfgs)?,
            AVS_TKN_PPL_NUM_BINDING_IDS_U32 => self.ppl.num_bindings = v,
            _ => {}
        }
        Ok(())
    }
}

/// `struct avs_tplg_path` (topology.h:150-163).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Path {
    pub id: u32,
    pub fe_fmt: Idx,
    pub be_fmt: Idx,
    pub source_path_id: u32,
    pub sink_path_id: u32,
    pub first_pipeline: u32,
    pub npipelines: u32,
}

/// Which token table a path is parsed with: `path_parsers` (widget templates) or `condpath_parsers` (conditional paths).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    Plain,
    Conditional,
}

pub struct PathSink<'a> {
    pub path: &'a mut Path,
    pub kind: PathKind,
    pub counts: Counts,
}
impl Sink for PathSink<'_> {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        // AVS_TKN_PATH_ID_U32 and AVS_TKN_CONDPATH_ID_U32 are the same number (1901)
        match (self.kind, token) {
            (_, AVS_TKN_PATH_ID_U32) => self.path.id = v,
            (PathKind::Plain, AVS_TKN_PATH_FE_FMT_ID_U32) => self.path.fe_fmt = index("audio format", v, self.counts.fmts)?,
            (PathKind::Plain, AVS_TKN_PATH_BE_FMT_ID_U32) => self.path.be_fmt = index("audio format", v, self.counts.fmts)?,
            (PathKind::Conditional, AVS_TKN_CONDPATH_SOURCE_PATH_ID_U32) => self.path.source_path_id = v,
            (PathKind::Conditional, AVS_TKN_CONDPATH_SINK_PATH_ID_U32) => self.path.sink_path_id = v,
            _ => {}
        }
        Ok(())
    }
}

/// `struct avs_tplg_path_template_id` (topology.h:127-130).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PathTemplateId {
    pub id: u32,
    pub tplg_name: Name,
}

/// `struct avs_tplg_path_template` (topology.h:132-148). A widget's template carries the widget's name; a conditional one carries
/// its source and sink.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PathTemplate {
    pub id: u32,
    pub widget_name: Name,
    pub source: PathTemplateId,
    pub sink: PathTemplateId,
    pub first_path: u32,
    pub npaths: u32,
}

pub struct TemplateSink<'a> {
    pub tmpl: &'a mut PathTemplate,
    pub kind: PathKind,
}
impl Sink for TemplateSink<'_> {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        // AVS_TKN_PATH_TMPL_ID_U32 and AVS_TKN_CONDPATH_TMPL_ID_U32 are the same number (1801)
        match (self.kind, token) {
            (_, AVS_TKN_PATH_TMPL_ID_U32) => self.tmpl.id = v,
            (PathKind::Conditional, AVS_TKN_CONDPATH_TMPL_SOURCE_PATH_TMPL_ID_U32) => self.tmpl.source.id = v,
            (PathKind::Conditional, AVS_TKN_CONDPATH_TMPL_SINK_PATH_TMPL_ID_U32) => self.tmpl.sink.id = v,
            _ => {}
        }
        Ok(())
    }
    fn string(&mut self, token: u32, value: &Name) -> Result<(), TplgError> {
        match (self.kind, token) {
            (PathKind::Conditional, AVS_TKN_CONDPATH_TMPL_SOURCE_TPLG_NAME_STRING) => self.tmpl.source.tplg_name = *value,
            (PathKind::Conditional, AVS_TKN_CONDPATH_TMPL_SINK_TPLG_NAME_STRING) => self.tmpl.sink.tplg_name = *value,
            _ => {}
        }
        Ok(())
    }
}

/// A module's trailing `AVS_TKN_MOD_INIT_CONFIG_ID_U32` ids and a pipeline's trailing `AVS_TKN_PPL_BINDING_ID_U32` ids: entries
/// that are one index each (`init_config_parsers`, `bindings_parsers`).
pub struct IdListSink<'a> {
    pub token: u32,
    pub out: &'a mut u32,
}
impl Sink for IdListSink<'_> {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        if token == self.token {
            *self.out = v;
        }
        Ok(())
    }
}

/// Whether Linux's `kcontrol` ids and similar `u8` tokens are all valid: used by the control parser.
pub struct ControlIdSink<'a> {
    pub id: &'a mut u32,
}
impl Sink for ControlIdSink<'_> {
    fn int(&mut self, token: u32, v: u32) -> Result<(), TplgError> {
        if token == AVS_TKN_KCONTROL_ID_U32 {
            *self.id = v;
        }
        Ok(())
    }
}
