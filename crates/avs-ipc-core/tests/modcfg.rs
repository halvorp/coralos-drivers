// SPDX-License-Identifier: GPL-2.0-only
//! The module configuration blobs (messages.h:615-949), byte for byte against hand-worked vectors.
use avs_ipc_core::error::IpcError;
use avs_ipc_core::modcfg::*;

fn fmt(seed: u32) -> AudioFormat {
    AudioFormat { sampling_freq: 48000 + seed, bit_depth: 32, channel_map: 0xFFFF_3210, channel_config: AVS_CHANNEL_CONFIG_STEREO, interleaving: AVS_INTERLEAVING_PER_SAMPLE, num_channels: 2, valid_bit_depth: 24, sample_type: AVS_SAMPLE_TYPE_INT_MSB as u8 }
}

#[test]
fn audio_format_is_24_bytes_with_the_four_byte_fields_packed_into_the_last_dword() {
    let b = fmt(0).to_bytes();
    assert_eq!(b.len(), 24);
    assert_eq!(&b[0..4], &48000u32.to_le_bytes());
    assert_eq!(&b[4..8], &32u32.to_le_bytes());
    assert_eq!(&b[8..12], &0xFFFF_3210u32.to_le_bytes());
    assert_eq!(&b[12..16], &1u32.to_le_bytes());
    assert_eq!(&b[16..20], &1u32.to_le_bytes());
    assert_eq!(&b[20..24], &[2, 24, 0, 0]); // num_channels:8, valid_bit_depth:8, sample_type:8, reserved:8
    let mut f = fmt(0);
    f.sample_type = AVS_SAMPLE_TYPE_FLOAT as u8;
    assert_eq!(&f.to_bytes()[20..24], &[2, 24, 4, 0]);
}

#[test]
fn enum_values() {
    assert_eq!((AVS_CHANNEL_LEFT, AVS_CHANNEL_RIGHT, AVS_CHANNEL_CENTER, AVS_CHANNEL_LEFT_SURROUND, AVS_CHANNEL_RIGHT_SURROUND, AVS_CHANNEL_LFE, AVS_CHANNEL_INVALID), (0, 1, 2, 3, 4, 7, 0xF));
    let cc = [
        AVS_CHANNEL_CONFIG_MONO,
        AVS_CHANNEL_CONFIG_STEREO,
        AVS_CHANNEL_CONFIG_2_1,
        AVS_CHANNEL_CONFIG_3_0,
        AVS_CHANNEL_CONFIG_3_1,
        AVS_CHANNEL_CONFIG_QUATRO,
        AVS_CHANNEL_CONFIG_4_0,
        AVS_CHANNEL_CONFIG_5_0,
        AVS_CHANNEL_CONFIG_5_1,
        AVS_CHANNEL_CONFIG_DUAL_MONO,
        AVS_CHANNEL_CONFIG_I2S_DUAL_STEREO_0,
        AVS_CHANNEL_CONFIG_I2S_DUAL_STEREO_1,
        AVS_CHANNEL_CONFIG_7_1,
    ];
    assert_eq!(cc, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    assert_eq!((AVS_INTERLEAVING_PER_CHANNEL, AVS_INTERLEAVING_PER_SAMPLE), (0, 1));
    assert_eq!((AVS_SAMPLE_TYPE_INT_MSB, AVS_SAMPLE_TYPE_INT_LSB, AVS_SAMPLE_TYPE_INT_SIGNED, AVS_SAMPLE_TYPE_INT_UNSIGNED, AVS_SAMPLE_TYPE_FLOAT), (0, 1, 2, 3, 4));
    assert_eq!(
        (AVS_DMA_HDA_HOST_OUTPUT, AVS_DMA_HDA_HOST_INPUT, AVS_DMA_HDA_LINK_OUTPUT, AVS_DMA_HDA_LINK_INPUT, AVS_DMA_DMIC_LINK_INPUT, AVS_DMA_I2S_LINK_OUTPUT, AVS_DMA_I2S_LINK_INPUT),
        (0, 1, 8, 9, 11, 12, 13)
    );
    assert_eq!((INVALID_NODE_ID, INVALID_PIPELINE_ID), (0xFFFF_FFFF, 0xFF));
    assert_eq!((AVS_PEAKVOL_VOLUME, AVS_PEAKVOL_MUTE, AVS_VENDOR_CONFIG, AVS_COPIER_SET_SINK_FORMAT), (0, 3, 0xFF, 2));
    assert_eq!((AVS_AUDIO_CURVE_NONE, AVS_AUDIO_CURVE_WINDOWS_FADE, AVS_ALL_CHANNELS_MASK, AVS_COEFF_CHANNELS_MAX), (0, 1, u32::MAX, 8));
}

#[test]
fn modcfg_base_is_40_bytes() {
    let b = ModCfgBase { cpc: 1, ibs: 2, obs: 3, is_pages: 4, audio_fmt: fmt(0) };
    let x = b.to_bytes();
    assert_eq!(x.len(), 40);
    assert_eq!(&x[0..16], &[1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0]);
    assert_eq!(&x[16..], &fmt(0).to_bytes());
}

#[test]
fn modcfg_ext_lists_input_pins_before_output_pins() {
    let base = ModCfgBase { cpc: 1, ibs: 2, obs: 3, is_pages: 4, audio_fmt: fmt(0) };
    let ins = [PinFormat { pin_index: 0, iobs: 0x100, audio_fmt: fmt(1) }];
    let outs = [PinFormat { pin_index: 1, iobs: 0x200, audio_fmt: fmt(2) }, PinFormat { pin_index: 2, iobs: 0x300, audio_fmt: fmt(3) }];
    let mut buf = [0xAAu8; 256];
    let n = encode_modcfg_ext(&base, &ins, &outs, &mut buf).unwrap();
    assert_eq!(n, 56 + 3 * 32);
    assert_eq!(&buf[..40], &base.to_bytes());
    assert_eq!(&buf[40..44], &[1, 0, 2, 0]); // num_input_pins, num_output_pins
    assert_eq!(&buf[44..56], &[0u8; 12]);
    assert_eq!(&buf[56..88], &ins[0].to_bytes());
    assert_eq!(&buf[88..120], &outs[0].to_bytes());
    assert_eq!(&buf[120..152], &outs[1].to_bytes());
    assert_eq!(&ins[0].to_bytes()[0..8], &[0, 0, 0, 0, 0, 1, 0, 0]);
    // too small a destination is refused
    let mut small = [0u8; 100];
    assert!(matches!(encode_modcfg_ext(&base, &ins, &outs, &mut small), Err(IpcError::PayloadTooBig { .. })));
}

#[test]
fn node_ids_follow_avs_init_node_id() {
    // vindex in bits 0..7, dma_type in bits 8..12
    // statically assigned vindex (I2S, DMIC)
    assert_eq!(node_id(AVS_DMA_I2S_LINK_OUTPUT, 0x10, 5), 0x0C10);
    assert_eq!(node_id(AVS_DMA_I2S_LINK_INPUT, 0x21, 5), 0x0D21);
    assert_eq!(node_id(AVS_DMA_DMIC_LINK_INPUT, 0x02, 5), 0x0B02);
    // dynamically assigned with the DMA id (HDA host)
    assert_eq!(node_id(AVS_DMA_HDA_HOST_OUTPUT, 0x77, 5), 0x0005);
    assert_eq!(node_id(AVS_DMA_HDA_HOST_INPUT, 0x77, 6), 0x0106);
    // HDA link: topology vindex OR the DMA id
    assert_eq!(node_id(AVS_DMA_HDA_LINK_OUTPUT, 0x30, 0x05), 0x0835);
    assert_eq!(node_id(AVS_DMA_HDA_LINK_INPUT, 0x30, 0x05), 0x0935);
    // anything else is INVALID_NODE_ID
    assert_eq!(node_id(2, 0, 0), INVALID_NODE_ID);
    assert_eq!(node_id(14, 0, 0), INVALID_NODE_ID);
    assert_eq!(node_id(31, 0, 0), INVALID_NODE_ID);
}

#[test]
fn i2s_virtual_index_is_time_slot_then_instance() {
    // union avs_virtual_index.i2s: time_slot:4 then instance:4 (messages.h:757-758)
    assert_eq!(i2s_vindex(1, 0), 0x10);
    assert_eq!(i2s_vindex(0, 3), 0x03);
    assert_eq!(i2s_vindex(2, 1), 0x21);
    assert_eq!(i2s_vindex(0x1F, 0x1F), 0xFF, "fields are four bits wide");
}

#[test]
fn copier_cfg_is_base_out_format_features_then_the_gateway_with_its_blob() {
    let base = ModCfgBase { cpc: 0x10, ibs: 0x20, obs: 0x30, is_pages: 1, audio_fmt: fmt(0) };
    let out_fmt = fmt(5);
    let blob = [0xDE, 0xAD, 0xBE, 0xEF, 1, 2, 3, 4];
    let mut buf = [0u8; 256];
    let n = encode_copier_cfg(&base, &out_fmt, 0x77, node_id(AVS_DMA_I2S_LINK_OUTPUT, i2s_vindex(1, 0), 0), 0x4000, &blob, &mut buf).unwrap();
    assert_eq!(n, 68 + 12 + 8, "offsetof(gtw_cfg.config) = 68, gateway header 12, blob 8");
    assert_eq!(&buf[..40], &base.to_bytes());
    assert_eq!(&buf[40..64], &out_fmt.to_bytes());
    assert_eq!(&buf[64..68], &0x77u32.to_le_bytes());
    assert_eq!(&buf[68..72], &0x0C10u32.to_le_bytes()); // node id
    assert_eq!(&buf[72..76], &0x4000u32.to_le_bytes()); // dma_buffer_size
    assert_eq!(&buf[76..80], &2u32.to_le_bytes()); // config_length in DWORDS (path.c:398)
    assert_eq!(&buf[80..88], &blob);
    // an empty blob: config_length 0 and the structure ends at the header
    let n = encode_copier_cfg(&base, &out_fmt, 0, 0, 0, &[], &mut buf).unwrap();
    assert_eq!(n, 80);
    assert_eq!(&buf[76..80], &0u32.to_le_bytes());
}

#[test]
fn copier_cfg_refusals() {
    let base = ModCfgBase::default();
    let f = AudioFormat::default();
    let mut buf = [0u8; 8192];
    // a blob that is not a whole number of dwords would silently lose its tail in Linux (`gtw_size / sizeof(u32)`)
    assert!(matches!(encode_copier_cfg(&base, &f, 0, 0, 0, &[1, 2, 3], &mut buf), Err(IpcError::Truncated { .. })));
    // beyond the mailbox (path.c:392-393 -E2BIG)
    let big = vec![0u8; 4096 - 80 + 4];
    assert!(matches!(encode_copier_cfg(&base, &f, 0, 0, 0, &big, &mut buf), Err(IpcError::PayloadTooBig { len: 4100, max: 4096 })));
    let ok = vec![0u8; 4096 - 80];
    assert_eq!(encode_copier_cfg(&base, &f, 0, 0, 0, &ok, &mut buf), Ok(4096));
    // destination too small
    let mut small = [0u8; 70];
    assert!(matches!(encode_copier_cfg(&base, &f, 0, 0, 0, &[], &mut small), Err(IpcError::PayloadTooBig { .. })));
}

#[test]
fn volume_and_mute_are_24_bytes_each() {
    let v = VolumeCfg { channel_id: AVS_ALL_CHANNELS_MASK, target_volume: 0x7FFF_FFFF, curve_type: AVS_AUDIO_CURVE_WINDOWS_FADE, curve_duration: 0x0102_0304_0506_0708 };
    let b = v.to_bytes();
    assert_eq!(&b[0..4], &[0xFF; 4]);
    assert_eq!(&b[4..8], &0x7FFF_FFFFu32.to_le_bytes());
    assert_eq!(&b[8..12], &1u32.to_le_bytes());
    assert_eq!(&b[12..16], &[0; 4]); // reserved (alignment)
    assert_eq!(&b[16..24], &[8, 7, 6, 5, 4, 3, 2, 1]);
    let m = MuteCfg { channel_id: 1, mute: true, curve_type: AVS_AUDIO_CURVE_NONE, curve_duration: 9 };
    let b = m.to_bytes();
    assert_eq!(&b[0..4], &1u32.to_le_bytes());
    assert_eq!(&b[4..8], &1u32.to_le_bytes());
    assert_eq!(&b[8..12], &0u32.to_le_bytes());
    assert_eq!(&b[16..24], &9u64.to_le_bytes());
    assert_eq!(&MuteCfg { mute: false, ..m }.to_bytes()[4..8], &0u32.to_le_bytes());
}

#[test]
fn peakvol_tlv_lists_carry_one_24_byte_entry_each() {
    let a = VolumeCfg { channel_id: 0, target_volume: 1, curve_type: 0, curve_duration: 0 }.to_bytes();
    let b = VolumeCfg { channel_id: 1, target_volume: 2, curve_type: 0, curve_duration: 0 }.to_bytes();
    let mut buf = [0u8; 128];
    let n = encode_peakvol_tlvs(AVS_PEAKVOL_VOLUME, &[a, b], &mut buf).unwrap();
    assert_eq!(n, 64);
    assert_eq!(&buf[0..8], &[0, 0, 0, 0, 24, 0, 0, 0]); // type 0 (VOLUME), length 24
    assert_eq!(&buf[8..32], &a);
    assert_eq!(&buf[32..40], &[0, 0, 0, 0, 24, 0, 0, 0]);
    assert_eq!(&buf[40..64], &b);
    let n = encode_peakvol_tlvs(AVS_PEAKVOL_MUTE, &[a], &mut buf).unwrap();
    assert_eq!((n, buf[0]), (32, 3));
    // 128 entries * 32 bytes = 4096 fits, 129 does not (messages.c:720 `size > AVS_MAILBOX_SIZE`)
    let many = vec![a; 128];
    let mut big = vec![0u8; 8192];
    assert_eq!(encode_peakvol_tlvs(AVS_PEAKVOL_VOLUME, &many, &mut big), Ok(4096));
    let many = vec![a; 129];
    assert!(matches!(encode_peakvol_tlvs(AVS_PEAKVOL_VOLUME, &many, &mut big), Err(IpcError::PayloadTooBig { len: 4128, max: 4096 })));
}

#[test]
fn peakvol_cfg_is_the_base_followed_by_the_volumes() {
    let base = ModCfgBase { cpc: 9, ..ModCfgBase::default() };
    let v0 = VolumeCfg { channel_id: 0, target_volume: 0x1000, curve_type: 0, curve_duration: 0 };
    let v1 = VolumeCfg { channel_id: 1, target_volume: 0x2000, curve_type: 0, curve_duration: 0 };
    let mut buf = [0u8; 128];
    let n = encode_peakvol_cfg(&base, &[v0, v1], &mut buf).unwrap();
    assert_eq!(n, 40 + 48);
    assert_eq!(&buf[40..64], &v0.to_bytes());
    assert_eq!(&buf[64..88], &v1.to_bytes());
    assert_eq!(encode_peakvol_cfg(&base, &[], &mut buf), Ok(40));
}

#[test]
fn src_updown_mux_and_sink_format_layouts() {
    let base = ModCfgBase { cpc: 7, ibs: 8, obs: 9, is_pages: 1, audio_fmt: fmt(0) };
    let s = encode_src_cfg(&base, 44100);
    assert_eq!(s.len(), 44);
    assert_eq!(&s[40..44], &44100u32.to_le_bytes());
    let coef = [1, -2, 3, -4, 5, -6, 7, -8];
    let u = encode_updown_mixer_cfg(&base, AVS_CHANNEL_CONFIG_STEREO, 2, &coef, 0xFFFF_FF10);
    assert_eq!(u.len(), 84);
    assert_eq!(&u[40..44], &1u32.to_le_bytes());
    assert_eq!(&u[44..48], &2u32.to_le_bytes());
    assert_eq!(&u[48..52], &1i32.to_le_bytes());
    assert_eq!(&u[52..56], &(-2i32).to_le_bytes());
    assert_eq!(&u[76..80], &(-8i32).to_le_bytes());
    assert_eq!(&u[80..84], &0xFFFF_FF10u32.to_le_bytes());
    let m = encode_mux_cfg(&base, &fmt(1), &fmt(2));
    assert_eq!(m.len(), 88);
    assert_eq!(&m[40..64], &fmt(1).to_bytes());
    assert_eq!(&m[64..88], &fmt(2).to_bytes());
    let k = encode_copier_sink_format(3, &fmt(1), &fmt(2));
    assert_eq!(k.len(), 52);
    assert_eq!(&k[0..4], &3u32.to_le_bytes());
    assert_eq!(&k[4..28], &fmt(1).to_bytes());
    assert_eq!(&k[28..52], &fmt(2).to_bytes());
}

/// The text form of a UUID to `guid_t` bytes, an independent derivation of what `GUID_INIT` lays out.
fn guid_from_text(t: &str) -> [u8; 16] {
    let h: Vec<u8> = t.replace('-', "").as_bytes().chunks(2).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap()).collect();
    let mut g = [0u8; 16];
    g[0..4].copy_from_slice(&[h[3], h[2], h[1], h[0]]);
    g[4..6].copy_from_slice(&[h[5], h[4]]);
    g[6..8].copy_from_slice(&[h[7], h[6]]);
    g[8..].copy_from_slice(&h[8..]);
    g
}

#[test]
fn module_uuids_match_their_canonical_text() {
    let table: [(&[u8; 16], &str); 15] = [
        (&AVS_MIXIN_MOD_UUID, "39656EB2-3B71-4049-8D3F-F92CD5C43C09"),
        (&AVS_MIXOUT_MOD_UUID, "3C56505A-24D7-418F-BDDC-C1F5A3AC2AE0"),
        (&AVS_COPIER_MOD_UUID, "9BA00C83-CA12-4A83-943C-1FA2E82F9DDA"),
        (&AVS_PEAKVOL_MOD_UUID, "8A171323-94A3-4E1D-AFE9-FE5DBAA4C393"),
        (&AVS_GAIN_MOD_UUID, "61BCA9A8-18D0-4A18-8E7B-2639219804B7"),
        (&AVS_KPBUFF_MOD_UUID, "A8A0CB32-4A77-4DB1-85C7-53D7EE07BCE6"),
        (&AVS_MICSEL_MOD_UUID, "32FE92C1-1E17-4FC2-9758-C7F3542E980A"),
        (&AVS_MUX_MOD_UUID, "64CE6E35-857A-4878-ACE8-E2A2F42E3069"),
        (&AVS_UPDWMIX_MOD_UUID, "42F8060C-832F-4DBF-B247-51E961997B35"),
        (&AVS_SRCINTC_MOD_UUID, "E61BB28D-149A-4C1F-B709-46823EF5F5AE"),
        (&AVS_PROBE_MOD_UUID, "7CAD0808-AB10-CD23-EF45-12AB34CD56EF"),
        (&AVS_AEC_MOD_UUID, "46CB87FB-D2C9-4970-96D2-6D7E614BB605"),
        (&AVS_ASRC_MOD_UUID, "66B4402D-B468-42F2-81A7-B37121863DD4"),
        (&AVS_INTELWOV_MOD_UUID, "EC774FA9-28D3-424A-90E4-69F984F1EEB7"),
        (&AVS_WOVHOSTM_MOD_UUID, "F9ED62B7-092E-4A90-8F4D-82DAA8B38F3B"),
    ];
    for (i, (g, text)) in table.iter().enumerate() {
        let text = *text;
        assert_eq!(**g, guid_from_text(&text), "uuid #{i} {text}");
    }
    // pairwise distinct
    for (i, (a, _)) in table.iter().enumerate() {
        for (b, _) in table.iter().skip(i + 1) {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn guid_lays_the_first_three_fields_out_little_endian() {
    assert_eq!(guid(0x01020304, 0x0506, 0x0708, [9, 10, 11, 12, 13, 14, 15, 16]), [4, 3, 2, 1, 6, 5, 8, 7, 9, 10, 11, 12, 13, 14, 15, 16]);
}

#[test]
fn structure_sizes_are_the_static_asserts_of_messages_h() {
    assert_eq!(AUDIO_FORMAT_SIZE, 24); // messages.h:717
    assert_eq!(MODCFG_BASE_SIZE, 40); // messages.h:726
    assert_eq!(PIN_FORMAT_SIZE, 32); // messages.h:733
    assert_eq!(MODCFG_EXT_SIZE, 56); // messages.h:743
    assert_eq!(COPIER_GTW_HEADER_SIZE, 12); // messages.h:828 (16) minus the 4-byte config union
    assert_eq!(COPIER_CFG_FIXED_SIZE, 68); // path.c:417 offsetof(gtw_cfg.config) = 84 - 16
    assert_eq!(VOLUME_CFG_SIZE, 24); // messages.h:845
    assert_eq!(MUTE_CFG_SIZE, 24); // messages.h:854
    assert_eq!(SRC_CFG_SIZE, 44); // messages.h:888
    assert_eq!(UPDOWN_MIXER_CFG_SIZE, 84); // messages.h:882
    assert_eq!(MUX_CFG_SIZE, 88); // messages.h:873
    assert_eq!(COPIER_SINK_FORMAT_SIZE, 52); // messages.h:949
    // and they are what the encoders write
    let base = ModCfgBase::default();
    assert_eq!(base.to_bytes().len(), MODCFG_BASE_SIZE);
    assert_eq!(PinFormat::default().to_bytes().len(), PIN_FORMAT_SIZE);
    let mut buf = [0u8; 256];
    assert_eq!(encode_modcfg_ext(&base, &[], &[], &mut buf), Ok(MODCFG_EXT_SIZE));
    assert_eq!(encode_copier_cfg(&base, &AudioFormat::default(), 0, 0, 0, &[], &mut buf), Ok(COPIER_CFG_FIXED_SIZE + COPIER_GTW_HEADER_SIZE));
    assert_eq!(VolumeCfg::default().to_bytes().len(), VOLUME_CFG_SIZE);
    assert_eq!(MuteCfg::default().to_bytes().len(), MUTE_CFG_SIZE);
}

#[test]
fn encoders_accept_an_exactly_sized_destination_and_name_the_shortfall() {
    let base = ModCfgBase::default();
    let f = AudioFormat::default();
    let mut exact = [0u8; 56];
    assert_eq!(encode_modcfg_ext(&base, &[], &[], &mut exact), Ok(56));
    let mut short = [0u8; 55];
    assert_eq!(encode_modcfg_ext(&base, &[], &[], &mut short), Err(IpcError::PayloadTooBig { len: 56, max: 55 }));
    // the shortfall is reported at the first piece that does not fit
    let mut tiny = [0u8; 10];
    assert_eq!(encode_modcfg_ext(&base, &[], &[], &mut tiny), Err(IpcError::PayloadTooBig { len: 40, max: 10 }));
    let mut exact = [0u8; 80];
    assert_eq!(encode_copier_cfg(&base, &f, 0, 0, 0, &[], &mut exact), Ok(80));
    let mut short = [0u8; 79];
    assert!(matches!(encode_copier_cfg(&base, &f, 0, 0, 0, &[], &mut short), Err(IpcError::PayloadTooBig { len: 80, max: 79 })));
    let mut exact = [0u8; 88];
    assert_eq!(encode_copier_cfg(&base, &f, 0, 0, 0, &[1, 2, 3, 4, 5, 6, 7, 8], &mut exact), Ok(88));
    let mut short = [0u8; 87];
    assert!(matches!(encode_copier_cfg(&base, &f, 0, 0, 0, &[1, 2, 3, 4, 5, 6, 7, 8], &mut short), Err(IpcError::PayloadTooBig { len: 88, max: 87 })));
    let mut exact = [0u8; 88];
    assert_eq!(encode_peakvol_cfg(&base, &[VolumeCfg::default(), VolumeCfg::default()], &mut exact), Ok(88));
    let mut short = [0u8; 87];
    assert!(encode_peakvol_cfg(&base, &[VolumeCfg::default(), VolumeCfg::default()], &mut short).is_err());
}

#[test]
fn a_gateway_blob_that_is_not_dword_sized_names_the_dword_size_it_would_need() {
    let base = ModCfgBase::default();
    let f = AudioFormat::default();
    let mut buf = [0u8; 256];
    for (len, need) in [(1usize, 4usize), (2, 4), (3, 4), (5, 8), (6, 8), (7, 8), (9, 12)] {
        let blob = vec![0u8; len];
        assert_eq!(
            encode_copier_cfg(&base, &f, 0, 0, 0, &blob, &mut buf),
            Err(IpcError::Truncated { what: "gateway config blob (not a whole number of dwords)", need, have: len }),
            "{len} bytes"
        );
    }
    assert!(encode_copier_cfg(&base, &f, 0, 0, 0, &[0u8; 4], &mut buf).is_ok());
}
