// SPDX-License-Identifier: GPL-2.0-only
//! The module configuration blobs `INIT_INSTANCE` carries and the runtime parameters `LARGE_CONFIG_SET` sets
//! (messages.h:615-1018): audio format, the module config headers, the copier and its gateway node id, volume and mute, the
//! sample-rate converter, the mixers, and the module-type UUIDs. Every struct is `__packed` and little-endian on the wire;
//! sizes are the `static_assert`s Linux makes.
//!
//! Which module, with which format, on which gateway is decided by the topology and `path.c` - that is the next wave. These
//! are only the encoders, each refusing a destination too small for it and a payload past the mailbox.
use crate::error::IpcError;
use crate::regs::AVS_MAILBOX_SIZE;

// ---- audio format ------------------------------------------------------------------------------------------------------------
pub const AUDIO_FORMAT_SIZE: usize = 24; // messages.h:717

/// `enum avs_channel_index` (messages.h:661-670).
pub const AVS_CHANNEL_LEFT: u32 = 0; // messages.h:662
pub const AVS_CHANNEL_RIGHT: u32 = 1; // messages.h:663
pub const AVS_CHANNEL_CENTER: u32 = 2; // messages.h:664
pub const AVS_CHANNEL_LEFT_SURROUND: u32 = 3; // messages.h:665
pub const AVS_CHANNEL_RIGHT_SURROUND: u32 = 4; // messages.h:667
pub const AVS_CHANNEL_LFE: u32 = 7; // messages.h:668
pub const AVS_CHANNEL_INVALID: u32 = 0xF; // messages.h:669

/// `enum avs_channel_config` (messages.h:672-687).
pub const AVS_CHANNEL_CONFIG_MONO: u32 = 0; // messages.h:673
pub const AVS_CHANNEL_CONFIG_STEREO: u32 = 1; // messages.h:674
pub const AVS_CHANNEL_CONFIG_2_1: u32 = 2; // messages.h:675
pub const AVS_CHANNEL_CONFIG_3_0: u32 = 3; // messages.h:676
pub const AVS_CHANNEL_CONFIG_3_1: u32 = 4; // messages.h:677
pub const AVS_CHANNEL_CONFIG_QUATRO: u32 = 5; // messages.h:678
pub const AVS_CHANNEL_CONFIG_4_0: u32 = 6; // messages.h:679
pub const AVS_CHANNEL_CONFIG_5_0: u32 = 7; // messages.h:680
pub const AVS_CHANNEL_CONFIG_5_1: u32 = 8; // messages.h:681
pub const AVS_CHANNEL_CONFIG_DUAL_MONO: u32 = 9; // messages.h:682
pub const AVS_CHANNEL_CONFIG_I2S_DUAL_STEREO_0: u32 = 10; // messages.h:683
pub const AVS_CHANNEL_CONFIG_I2S_DUAL_STEREO_1: u32 = 11; // messages.h:684
pub const AVS_CHANNEL_CONFIG_7_1: u32 = 12; // messages.h:685

pub const AVS_INTERLEAVING_PER_CHANNEL: u32 = 0; // messages.h:690
pub const AVS_INTERLEAVING_PER_SAMPLE: u32 = 1; // messages.h:691
/// `enum avs_sample_type` (messages.h:694-700).
pub const AVS_SAMPLE_TYPE_INT_MSB: u32 = 0; // messages.h:695
pub const AVS_SAMPLE_TYPE_INT_LSB: u32 = 1; // messages.h:696
pub const AVS_SAMPLE_TYPE_INT_SIGNED: u32 = 2; // messages.h:697
pub const AVS_SAMPLE_TYPE_INT_UNSIGNED: u32 = 3; // messages.h:698
pub const AVS_SAMPLE_TYPE_FLOAT: u32 = 4; // messages.h:699

/// `struct avs_audio_format` (messages.h:704-717): five dwords then `num_channels:8, valid_bit_depth:8, sample_type:8, reserved:8`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioFormat {
    pub sampling_freq: u32,
    pub bit_depth: u32,
    pub channel_map: u32,
    pub channel_config: u32,
    pub interleaving: u32,
    pub num_channels: u8,
    pub valid_bit_depth: u8,
    pub sample_type: u8,
}

impl AudioFormat {
    pub fn to_bytes(&self) -> [u8; AUDIO_FORMAT_SIZE] {
        let mut b = [0u8; AUDIO_FORMAT_SIZE];
        b[0..4].copy_from_slice(&self.sampling_freq.to_le_bytes());
        b[4..8].copy_from_slice(&self.bit_depth.to_le_bytes());
        b[8..12].copy_from_slice(&self.channel_map.to_le_bytes());
        b[12..16].copy_from_slice(&self.channel_config.to_le_bytes());
        b[16..20].copy_from_slice(&self.interleaving.to_le_bytes());
        b[20] = self.num_channels;
        b[21] = self.valid_bit_depth;
        b[22] = self.sample_type;
        b
    }
}

fn put<'a>(out: &'a mut [u8], at: &mut usize, bytes: &[u8]) -> Result<(), IpcError> {
    if *at + bytes.len() > out.len() {
        return Err(IpcError::PayloadTooBig { len: *at + bytes.len(), max: out.len() });
    }
    out[*at..*at + bytes.len()].copy_from_slice(bytes);
    *at += bytes.len();
    Ok(())
}

fn finish(at: usize) -> Result<usize, IpcError> {
    if at > AVS_MAILBOX_SIZE {
        return Err(IpcError::PayloadTooBig { len: at, max: AVS_MAILBOX_SIZE });
    }
    Ok(at)
}

// ---- module config headers -----------------------------------------------------------------------------------------------------
pub const MODCFG_BASE_SIZE: usize = 40; // messages.h:726
pub const PIN_FORMAT_SIZE: usize = 32; // messages.h:733
pub const MODCFG_EXT_SIZE: usize = 56; // messages.h:743

/// `struct avs_modcfg_base` (messages.h:719-726): cycles per chunk, input/output buffer sizes, pages, input format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModCfgBase {
    pub cpc: u32,
    pub ibs: u32,
    pub obs: u32,
    pub is_pages: u32,
    pub audio_fmt: AudioFormat,
}

impl ModCfgBase {
    pub fn to_bytes(&self) -> [u8; MODCFG_BASE_SIZE] {
        let mut b = [0u8; MODCFG_BASE_SIZE];
        b[0..4].copy_from_slice(&self.cpc.to_le_bytes());
        b[4..8].copy_from_slice(&self.ibs.to_le_bytes());
        b[8..12].copy_from_slice(&self.obs.to_le_bytes());
        b[12..16].copy_from_slice(&self.is_pages.to_le_bytes());
        b[16..].copy_from_slice(&self.audio_fmt.to_bytes());
        b
    }
}

/// `struct avs_pin_format` (messages.h:728-733).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PinFormat {
    pub pin_index: u32,
    pub iobs: u32,
    pub audio_fmt: AudioFormat,
}

impl PinFormat {
    pub fn to_bytes(&self) -> [u8; PIN_FORMAT_SIZE] {
        let mut b = [0u8; PIN_FORMAT_SIZE];
        b[0..4].copy_from_slice(&self.pin_index.to_le_bytes());
        b[4..8].copy_from_slice(&self.iobs.to_le_bytes());
        b[8..].copy_from_slice(&self.audio_fmt.to_bytes());
        b
    }
}

/// `struct avs_modcfg_ext` (messages.h:735-743) followed by its pin formats, "input pin formats followed by output ones".
/// Returns the bytes written.
pub fn encode_modcfg_ext(base: &ModCfgBase, inputs: &[PinFormat], outputs: &[PinFormat], out: &mut [u8]) -> Result<usize, IpcError> {
    let mut at = 0;
    put(out, &mut at, &base.to_bytes())?;
    put(out, &mut at, &(inputs.len() as u16).to_le_bytes())?;
    put(out, &mut at, &(outputs.len() as u16).to_le_bytes())?;
    put(out, &mut at, &[0u8; 12])?; // reserved
    for p in inputs.iter().chain(outputs.iter()) {
        put(out, &mut at, &p.to_bytes())?;
    }
    finish(at)
}

// ---- gateway node ids --------------------------------------------------------------------------------------------------------
/// `enum avs_dma_type` (messages.h:745-753).
pub const AVS_DMA_HDA_HOST_OUTPUT: u32 = 0; // messages.h:746
pub const AVS_DMA_HDA_HOST_INPUT: u32 = 1; // messages.h:747
pub const AVS_DMA_HDA_LINK_OUTPUT: u32 = 8; // messages.h:748
pub const AVS_DMA_HDA_LINK_INPUT: u32 = 9; // messages.h:749
pub const AVS_DMA_DMIC_LINK_INPUT: u32 = 11; // messages.h:750
pub const AVS_DMA_I2S_LINK_OUTPUT: u32 = 12; // messages.h:751
pub const AVS_DMA_I2S_LINK_INPUT: u32 = 13; // messages.h:752

/// `INVALID_NODE_ID` (messages.h:780-781): all ones.
pub const INVALID_NODE_ID: u32 = u32::MAX; // messages.h:780
/// `INVALID_PIPELINE_ID` (messages.h:779).
pub const INVALID_PIPELINE_ID: u32 = 0xFF; // messages.h:779

/// `union avs_virtual_index` for an I2S gateway: `time_slot:4` then `instance:4` (messages.h:755-767).
pub const fn i2s_vindex(instance: u8, time_slot: u8) -> u8 {
    (time_slot & 0xF) | ((instance & 0xF) << 4) // messages.h:757-758
}

/// `avs_init_node_id` (path.c:243-272). `vindex` is the topology's statically assigned virtual index; `dma_id` the host or link
/// stream. A DMA type this port does not know is `INVALID_NODE_ID`, exactly as Linux.
pub const fn node_id(dma_type: u32, vindex: u8, dma_id: u32) -> u32 {
    let v = match dma_type {
        AVS_DMA_DMIC_LINK_INPUT | AVS_DMA_I2S_LINK_OUTPUT | AVS_DMA_I2S_LINK_INPUT => vindex as u32, // path.c:251-254
        AVS_DMA_HDA_HOST_OUTPUT | AVS_DMA_HDA_HOST_INPUT => dma_id,                                  // path.c:258-260
        AVS_DMA_HDA_LINK_OUTPUT | AVS_DMA_HDA_LINK_INPUT => (vindex as u32) | dma_id,                // path.c:263-265
        _ => return INVALID_NODE_ID,                                                                 // path.c:268-269
    };
    // union avs_connector_node_id: vindex:8, dma_type:5 (messages.h:769-777)
    (v & 0xFF) | ((dma_type & 0x1F) << 8)
}

// ---- copier ------------------------------------------------------------------------------------------------------------------
pub const COPIER_GTW_HEADER_SIZE: usize = 12; // messages.h:828 minus the 4-byte config union
pub const COPIER_CFG_FIXED_SIZE: usize = 68; // messages.h:836 (84) minus the 16-byte gateway config: offsetof(gtw_cfg.config) (path.c:417)

/// `avs_copier_create` + `avs_fill_gtw_config` (path.c:381-446) for a gateway that carries its blob: the copier base, the output
/// format, the feature mask, then the gateway header (node id, DMA buffer size, config length in DWORDS) and the blob (the NHLT
/// endpoint's `capabilities`). The blob must be a whole number of dwords, as Linux's `gtw_size / sizeof(u32)` silently assumes.
pub fn encode_copier_cfg(
    base: &ModCfgBase,
    out_fmt: &AudioFormat,
    feature_mask: u32,
    node_id: u32,
    dma_buffer_size: u32,
    gtw_blob: &[u8],
    out: &mut [u8],
) -> Result<usize, IpcError> {
    if gtw_blob.len() % 4 != 0 {
        return Err(IpcError::Truncated { what: "gateway config blob (not a whole number of dwords)", need: (gtw_blob.len() + 3) / 4 * 4, have: gtw_blob.len() });
    }
    let mut at = 0;
    put(out, &mut at, &base.to_bytes())?;
    put(out, &mut at, &out_fmt.to_bytes())?;
    put(out, &mut at, &feature_mask.to_le_bytes())?;
    put(out, &mut at, &node_id.to_le_bytes())?;
    put(out, &mut at, &dma_buffer_size.to_le_bytes())?;
    put(out, &mut at, &((gtw_blob.len() / 4) as u32).to_le_bytes())?;
    put(out, &mut at, gtw_blob)?;
    finish(at)
}

// ---- volume and mute ---------------------------------------------------------------------------------------------------------
pub const VOLUME_CFG_SIZE: usize = 24; // messages.h:845
pub const MUTE_CFG_SIZE: usize = 24; // messages.h:854
pub const AVS_PEAKVOL_VOLUME: u8 = 0; // messages.h:957
pub const AVS_PEAKVOL_MUTE: u8 = 3; // messages.h:958
pub const AVS_VENDOR_CONFIG: u8 = 0xFF; // messages.h:938
pub const AVS_AUDIO_CURVE_NONE: u32 = 0; // messages.h:962
pub const AVS_AUDIO_CURVE_WINDOWS_FADE: u32 = 1; // messages.h:963
/// `AVS_ALL_CHANNELS_MASK` (messages.h:703): the channel id that addresses every channel.
pub const AVS_ALL_CHANNELS_MASK: u32 = u32::MAX; // messages.h:703

/// `struct avs_volume_cfg` (messages.h:838-845): `target_volume` is the module's linear gain (Q1.31 in the firmware's
/// convention; the table that maps a dB step to it is the topology's, not Linux's code).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VolumeCfg {
    pub channel_id: u32,
    pub target_volume: u32,
    pub curve_type: u32,
    pub curve_duration: u64,
}

impl VolumeCfg {
    pub fn to_bytes(&self) -> [u8; VOLUME_CFG_SIZE] {
        let mut b = [0u8; VOLUME_CFG_SIZE];
        b[0..4].copy_from_slice(&self.channel_id.to_le_bytes());
        b[4..8].copy_from_slice(&self.target_volume.to_le_bytes());
        b[8..12].copy_from_slice(&self.curve_type.to_le_bytes());
        // b[12..16] reserved "alignment"
        b[16..24].copy_from_slice(&self.curve_duration.to_le_bytes());
        b
    }
}

/// `struct avs_mute_cfg` (messages.h:847-854).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MuteCfg {
    pub channel_id: u32,
    pub mute: bool,
    pub curve_type: u32,
    pub curve_duration: u64,
}

impl MuteCfg {
    pub fn to_bytes(&self) -> [u8; MUTE_CFG_SIZE] {
        let mut b = [0u8; MUTE_CFG_SIZE];
        b[0..4].copy_from_slice(&self.channel_id.to_le_bytes());
        b[4..8].copy_from_slice(&(self.mute as u32).to_le_bytes());
        b[8..12].copy_from_slice(&self.curve_type.to_le_bytes());
        b[16..24].copy_from_slice(&self.curve_duration.to_le_bytes());
        b
    }
}

/// The TLV list `avs_ipc_peakvol_set_volumes` / `_set_mutes` send as AVS_VENDOR_CONFIG (messages.c:709-741, :772-805): one TLV of
/// type `tlv_type` and length 24 per entry. Refused where Linux returns -EINVAL: more than fits the mailbox.
pub fn encode_peakvol_tlvs(tlv_type: u8, entries: &[[u8; 24]], out: &mut [u8]) -> Result<usize, IpcError> {
    let size = entries.len() * (8 + 24);
    if size > AVS_MAILBOX_SIZE {
        return Err(IpcError::PayloadTooBig { len: size, max: AVS_MAILBOX_SIZE });
    }
    let mut at = 0;
    for e in entries {
        put(out, &mut at, &(tlv_type as u32).to_le_bytes())?;
        put(out, &mut at, &24u32.to_le_bytes())?;
        put(out, &mut at, e)?;
    }
    finish(at)
}

/// `struct avs_peakvol_cfg` (messages.h:856-860): base followed by the initial per-channel volumes (path.c:570-611).
pub fn encode_peakvol_cfg(base: &ModCfgBase, vols: &[VolumeCfg], out: &mut [u8]) -> Result<usize, IpcError> {
    let mut at = 0;
    put(out, &mut at, &base.to_bytes())?;
    for v in vols {
        put(out, &mut at, &v.to_bytes())?;
    }
    finish(at)
}

// ---- the rest of the module configs the Kaby Lake topologies use ----------------------------------------------------------------
pub const SRC_CFG_SIZE: usize = 44; // messages.h:888
pub const UPDOWN_MIXER_CFG_SIZE: usize = 84; // messages.h:882
pub const MUX_CFG_SIZE: usize = 88; // messages.h:873
pub const AVS_COEFF_CHANNELS_MAX: usize = 8; // messages.h:701

/// `struct avs_src_cfg` (messages.h:884-888): base then the output sampling frequency.
pub fn encode_src_cfg(base: &ModCfgBase, out_freq: u32) -> [u8; SRC_CFG_SIZE] {
    let mut b = [0u8; SRC_CFG_SIZE];
    b[..MODCFG_BASE_SIZE].copy_from_slice(&base.to_bytes());
    b[MODCFG_BASE_SIZE..].copy_from_slice(&out_freq.to_le_bytes());
    b
}

/// `struct avs_updown_mixer_cfg` (messages.h:875-882).
pub fn encode_updown_mixer_cfg(base: &ModCfgBase, out_channel_config: u32, coefficients_select: u32, coefficients: &[i32; AVS_COEFF_CHANNELS_MAX], channel_map: u32) -> [u8; UPDOWN_MIXER_CFG_SIZE] {
    let mut b = [0u8; UPDOWN_MIXER_CFG_SIZE];
    b[..40].copy_from_slice(&base.to_bytes());
    b[40..44].copy_from_slice(&out_channel_config.to_le_bytes());
    b[44..48].copy_from_slice(&coefficients_select.to_le_bytes());
    for (i, c) in coefficients.iter().enumerate() {
        b[48 + 4 * i..52 + 4 * i].copy_from_slice(&c.to_le_bytes());
    }
    b[80..84].copy_from_slice(&channel_map.to_le_bytes());
    b
}

/// `struct avs_mux_cfg` (messages.h:868-873): base, reference format, output format.
pub fn encode_mux_cfg(base: &ModCfgBase, ref_fmt: &AudioFormat, out_fmt: &AudioFormat) -> [u8; MUX_CFG_SIZE] {
    let mut b = [0u8; MUX_CFG_SIZE];
    b[..40].copy_from_slice(&base.to_bytes());
    b[40..64].copy_from_slice(&ref_fmt.to_bytes());
    b[64..88].copy_from_slice(&out_fmt.to_bytes());
    b
}

// ---- copier runtime parameter ------------------------------------------------------------------------------------------------
pub const AVS_COPIER_SET_SINK_FORMAT: u8 = 2; // messages.h:941
pub const COPIER_SINK_FORMAT_SIZE: usize = 52; // messages.h:949

/// `struct avs_copier_sink_format` (messages.h:943-949) as `avs_ipc_copier_set_sink_format` fills it (messages.c:663-678):
/// "Firmware expects driver to resend copier's input format."
pub fn encode_copier_sink_format(sink_id: u32, src_fmt: &AudioFormat, sink_fmt: &AudioFormat) -> [u8; COPIER_SINK_FORMAT_SIZE] {
    let mut b = [0u8; COPIER_SINK_FORMAT_SIZE];
    b[0..4].copy_from_slice(&sink_id.to_le_bytes());
    b[4..28].copy_from_slice(&src_fmt.to_bytes());
    b[28..52].copy_from_slice(&sink_fmt.to_bytes());
    b
}

// ---- module-type UUIDs (messages.h:615-658), as `guid_t` bytes ---------------------------------------------------------------
/// `GUID_INIT(a, b, c, d0..d7)` as `guid_t.b[]`: the first three fields little-endian.
pub const fn guid(a: u32, b: u16, c: u16, d: [u8; 8]) -> [u8; 16] {
    let a = a.to_le_bytes();
    let b = b.to_le_bytes();
    let c = c.to_le_bytes();
    [a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]]
}

pub const AVS_MIXIN_MOD_UUID: [u8; 16] = guid(0x39656EB2, 0x3B71, 0x4049, [0x8D, 0x3F, 0xF9, 0x2C, 0xD5, 0xC4, 0x3C, 0x09]); // messages.h:615
pub const AVS_MIXOUT_MOD_UUID: [u8; 16] = guid(0x3C56505A, 0x24D7, 0x418F, [0xBD, 0xDC, 0xC1, 0xF5, 0xA3, 0xAC, 0x2A, 0xE0]); // messages.h:618
pub const AVS_COPIER_MOD_UUID: [u8; 16] = guid(0x9BA00C83, 0xCA12, 0x4A83, [0x94, 0x3C, 0x1F, 0xA2, 0xE8, 0x2F, 0x9D, 0xDA]); // messages.h:621
pub const AVS_PEAKVOL_MOD_UUID: [u8; 16] = guid(0x8A171323, 0x94A3, 0x4E1D, [0xAF, 0xE9, 0xFE, 0x5D, 0xBA, 0xA4, 0xC3, 0x93]); // messages.h:624
pub const AVS_GAIN_MOD_UUID: [u8; 16] = guid(0x61BCA9A8, 0x18D0, 0x4A18, [0x8E, 0x7B, 0x26, 0x39, 0x21, 0x98, 0x04, 0xB7]); // messages.h:627
pub const AVS_KPBUFF_MOD_UUID: [u8; 16] = guid(0xA8A0CB32, 0x4A77, 0x4DB1, [0x85, 0xC7, 0x53, 0xD7, 0xEE, 0x07, 0xBC, 0xE6]); // messages.h:630
pub const AVS_MICSEL_MOD_UUID: [u8; 16] = guid(0x32FE92C1, 0x1E17, 0x4FC2, [0x97, 0x58, 0xC7, 0xF3, 0x54, 0x2E, 0x98, 0x0A]); // messages.h:633
pub const AVS_MUX_MOD_UUID: [u8; 16] = guid(0x64CE6E35, 0x857A, 0x4878, [0xAC, 0xE8, 0xE2, 0xA2, 0xF4, 0x2E, 0x30, 0x69]); // messages.h:636
pub const AVS_UPDWMIX_MOD_UUID: [u8; 16] = guid(0x42F8060C, 0x832F, 0x4DBF, [0xB2, 0x47, 0x51, 0xE9, 0x61, 0x99, 0x7B, 0x35]); // messages.h:639
pub const AVS_SRCINTC_MOD_UUID: [u8; 16] = guid(0xE61BB28D, 0x149A, 0x4C1F, [0xB7, 0x09, 0x46, 0x82, 0x3E, 0xF5, 0xF5, 0xAE]); // messages.h:642
pub const AVS_PROBE_MOD_UUID: [u8; 16] = guid(0x7CAD0808, 0xAB10, 0xCD23, [0xEF, 0x45, 0x12, 0xAB, 0x34, 0xCD, 0x56, 0xEF]); // messages.h:645
pub const AVS_AEC_MOD_UUID: [u8; 16] = guid(0x46CB87FB, 0xD2C9, 0x4970, [0x96, 0xD2, 0x6D, 0x7E, 0x61, 0x4B, 0xB6, 0x05]); // messages.h:648
pub const AVS_ASRC_MOD_UUID: [u8; 16] = guid(0x66B4402D, 0xB468, 0x42F2, [0x81, 0xA7, 0xB3, 0x71, 0x21, 0x86, 0x3D, 0xD4]); // messages.h:651
pub const AVS_INTELWOV_MOD_UUID: [u8; 16] = guid(0xEC774FA9, 0x28D3, 0x424A, [0x90, 0xE4, 0x69, 0xF9, 0x84, 0xF1, 0xEE, 0xB7]); // messages.h:654
pub const AVS_WOVHOSTM_MOD_UUID: [u8; 16] = guid(0xF9ED62B7, 0x092E, 0x4A90, [0x8F, 0x4D, 0x82, 0xDA, 0xA8, 0xB3, 0x8F, 0x3B]); // messages.h:657
