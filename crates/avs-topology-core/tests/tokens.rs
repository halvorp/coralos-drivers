// SPDX-License-Identifier: GPL-2.0-only
//! Every token of `enum avs_tplg_token` (include/uapi/sound/intel/avs/tokens.h:12-169) against its value, generated from that header.
use avs_topology_core::tokens::*;

#[test]
fn every_token_has_the_value_tokens_h_gives_it() {
    assert_eq!(AVS_TKN_MANIFEST_NAME_STRING, 1); // tokens.h:14
    assert_eq!(AVS_TKN_MANIFEST_VERSION_U32, 2); // tokens.h:15
    assert_eq!(AVS_TKN_MANIFEST_NUM_LIBRARIES_U32, 3); // tokens.h:16
    assert_eq!(AVS_TKN_MANIFEST_NUM_AFMTS_U32, 4); // tokens.h:17
    assert_eq!(AVS_TKN_MANIFEST_NUM_MODCFGS_BASE_U32, 5); // tokens.h:18
    assert_eq!(AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32, 6); // tokens.h:19
    assert_eq!(AVS_TKN_MANIFEST_NUM_PPLCFGS_U32, 7); // tokens.h:20
    assert_eq!(AVS_TKN_MANIFEST_NUM_BINDINGS_U32, 8); // tokens.h:21
    assert_eq!(AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32, 9); // tokens.h:22
    assert_eq!(AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32, 10); // tokens.h:23
    assert_eq!(AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32, 11); // tokens.h:24
    assert_eq!(AVS_TKN_LIBRARY_ID_U32, 101); // tokens.h:27
    assert_eq!(AVS_TKN_LIBRARY_NAME_STRING, 102); // tokens.h:28
    assert_eq!(AVS_TKN_AFMT_ID_U32, 201); // tokens.h:31
    assert_eq!(AVS_TKN_AFMT_SAMPLE_RATE_U32, 202); // tokens.h:32
    assert_eq!(AVS_TKN_AFMT_BIT_DEPTH_U32, 203); // tokens.h:33
    assert_eq!(AVS_TKN_AFMT_CHANNEL_MAP_U32, 204); // tokens.h:34
    assert_eq!(AVS_TKN_AFMT_CHANNEL_CFG_U32, 205); // tokens.h:35
    assert_eq!(AVS_TKN_AFMT_INTERLEAVING_U32, 206); // tokens.h:36
    assert_eq!(AVS_TKN_AFMT_NUM_CHANNELS_U32, 207); // tokens.h:37
    assert_eq!(AVS_TKN_AFMT_VALID_BIT_DEPTH_U32, 208); // tokens.h:38
    assert_eq!(AVS_TKN_AFMT_SAMPLE_TYPE_U32, 209); // tokens.h:39
    assert_eq!(AVS_TKN_MODCFG_BASE_ID_U32, 301); // tokens.h:42
    assert_eq!(AVS_TKN_MODCFG_BASE_CPC_U32, 302); // tokens.h:43
    assert_eq!(AVS_TKN_MODCFG_BASE_IBS_U32, 303); // tokens.h:44
    assert_eq!(AVS_TKN_MODCFG_BASE_OBS_U32, 304); // tokens.h:45
    assert_eq!(AVS_TKN_MODCFG_BASE_PAGES_U32, 305); // tokens.h:46
    assert_eq!(AVS_TKN_MODCFG_EXT_ID_U32, 401); // tokens.h:49
    assert_eq!(AVS_TKN_MODCFG_EXT_TYPE_UUID, 402); // tokens.h:50
    assert_eq!(AVS_TKN_MODCFG_CPR_OUT_AFMT_ID_U32, 403); // tokens.h:51
    assert_eq!(AVS_TKN_MODCFG_CPR_FEATURE_MASK_U32, 404); // tokens.h:52
    assert_eq!(AVS_TKN_MODCFG_CPR_DMA_TYPE_U32, 405); // tokens.h:53
    assert_eq!(AVS_TKN_MODCFG_CPR_DMABUFF_SIZE_U32, 406); // tokens.h:54
    assert_eq!(AVS_TKN_MODCFG_CPR_VINDEX_U8, 407); // tokens.h:55
    assert_eq!(AVS_TKN_MODCFG_CPR_BLOB_FMT_ID_U32, 408); // tokens.h:56
    assert_eq!(AVS_TKN_MODCFG_MICSEL_OUT_AFMT_ID_U32, 409); // tokens.h:57
    assert_eq!(AVS_TKN_MODCFG_INTELWOV_CPC_LP_MODE_U32, 410); // tokens.h:58
    assert_eq!(AVS_TKN_MODCFG_SRC_OUT_FREQ_U32, 411); // tokens.h:59
    assert_eq!(AVS_TKN_MODCFG_MUX_REF_AFMT_ID_U32, 412); // tokens.h:60
    assert_eq!(AVS_TKN_MODCFG_MUX_OUT_AFMT_ID_U32, 413); // tokens.h:61
    assert_eq!(AVS_TKN_MODCFG_AEC_REF_AFMT_ID_U32, 414); // tokens.h:62
    assert_eq!(AVS_TKN_MODCFG_AEC_OUT_AFMT_ID_U32, 415); // tokens.h:63
    assert_eq!(AVS_TKN_MODCFG_AEC_CPC_LP_MODE_U32, 416); // tokens.h:64
    assert_eq!(AVS_TKN_MODCFG_ASRC_OUT_FREQ_U32, 417); // tokens.h:65
    assert_eq!(AVS_TKN_MODCFG_ASRC_MODE_U8, 418); // tokens.h:66
    assert_eq!(AVS_TKN_MODCFG_ASRC_DISABLE_JITTER_U8, 419); // tokens.h:67
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_OUT_CHAN_CFG_U32, 420); // tokens.h:68
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_SELECT_U32, 421); // tokens.h:69
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_0_S32, 422); // tokens.h:70
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_1_S32, 423); // tokens.h:71
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_2_S32, 424); // tokens.h:72
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_3_S32, 425); // tokens.h:73
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_4_S32, 426); // tokens.h:74
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_5_S32, 427); // tokens.h:75
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_6_S32, 428); // tokens.h:76
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_7_S32, 429); // tokens.h:77
    assert_eq!(AVS_TKN_MODCFG_UPDOWN_MIX_CHAN_MAP_U32, 430); // tokens.h:78
    assert_eq!(AVS_TKN_MODCFG_EXT_NUM_INPUT_PINS_U16, 431); // tokens.h:79
    assert_eq!(AVS_TKN_MODCFG_EXT_NUM_OUTPUT_PINS_U16, 432); // tokens.h:80
    assert_eq!(AVS_TKN_MODCFG_WHM_REF_AFMT_ID_U32, 433); // tokens.h:81
    assert_eq!(AVS_TKN_MODCFG_WHM_OUT_AFMT_ID_U32, 434); // tokens.h:82
    assert_eq!(AVS_TKN_MODCFG_WHM_WAKE_TICK_PERIOD_U32, 435); // tokens.h:83
    assert_eq!(AVS_TKN_MODCFG_WHM_VINDEX_U8, 436); // tokens.h:84
    assert_eq!(AVS_TKN_MODCFG_WHM_DMA_TYPE_U32, 437); // tokens.h:85
    assert_eq!(AVS_TKN_MODCFG_WHM_DMABUFF_SIZE_U32, 438); // tokens.h:86
    assert_eq!(AVS_TKN_MODCFG_WHM_BLOB_AFMT_ID_U32, 439); // tokens.h:87
    assert_eq!(AVS_TKN_MODCFG_PEAKVOL_VOLUME_U32, 440); // tokens.h:88
    assert_eq!(AVS_TKN_MODCFG_PEAKVOL_CHANNEL_ID_U32, 441); // tokens.h:89
    assert_eq!(AVS_TKN_MODCFG_PEAKVOL_CURVE_TYPE_U32, 442); // tokens.h:90
    assert_eq!(AVS_TKN_MODCFG_PEAKVOL_CURVE_DURATION_U32, 443); // tokens.h:91
    assert_eq!(AVS_TKN_PPLCFG_ID_U32, 1401); // tokens.h:94
    assert_eq!(AVS_TKN_PPLCFG_REQ_SIZE_U16, 1402); // tokens.h:95
    assert_eq!(AVS_TKN_PPLCFG_PRIORITY_U8, 1403); // tokens.h:96
    assert_eq!(AVS_TKN_PPLCFG_LOW_POWER_BOOL, 1404); // tokens.h:97
    assert_eq!(AVS_TKN_PPLCFG_ATTRIBUTES_U16, 1405); // tokens.h:98
    assert_eq!(AVS_TKN_PPLCFG_TRIGGER_U32, 1406); // tokens.h:99
    assert_eq!(AVS_TKN_BINDING_ID_U32, 1501); // tokens.h:102
    assert_eq!(AVS_TKN_BINDING_TARGET_TPLG_NAME_STRING, 1502); // tokens.h:103
    assert_eq!(AVS_TKN_BINDING_TARGET_PATH_TMPL_ID_U32, 1503); // tokens.h:104
    assert_eq!(AVS_TKN_BINDING_TARGET_PPL_ID_U32, 1504); // tokens.h:105
    assert_eq!(AVS_TKN_BINDING_TARGET_MOD_ID_U32, 1505); // tokens.h:106
    assert_eq!(AVS_TKN_BINDING_TARGET_MOD_PIN_U8, 1506); // tokens.h:107
    assert_eq!(AVS_TKN_BINDING_MOD_ID_U32, 1507); // tokens.h:108
    assert_eq!(AVS_TKN_BINDING_MOD_PIN_U8, 1508); // tokens.h:109
    assert_eq!(AVS_TKN_BINDING_IS_SINK_U8, 1509); // tokens.h:110
    assert_eq!(AVS_TKN_PPL_ID_U32, 1601); // tokens.h:113
    assert_eq!(AVS_TKN_PPL_PPLCFG_ID_U32, 1602); // tokens.h:114
    assert_eq!(AVS_TKN_PPL_NUM_BINDING_IDS_U32, 1603); // tokens.h:115
    assert_eq!(AVS_TKN_PPL_BINDING_ID_U32, 1604); // tokens.h:116
    assert_eq!(AVS_TKN_MOD_ID_U32, 1701); // tokens.h:119
    assert_eq!(AVS_TKN_MOD_MODCFG_BASE_ID_U32, 1702); // tokens.h:120
    assert_eq!(AVS_TKN_MOD_IN_AFMT_ID_U32, 1703); // tokens.h:121
    assert_eq!(AVS_TKN_MOD_CORE_ID_U8, 1704); // tokens.h:122
    assert_eq!(AVS_TKN_MOD_PROC_DOMAIN_U8, 1705); // tokens.h:123
    assert_eq!(AVS_TKN_MOD_MODCFG_EXT_ID_U32, 1706); // tokens.h:124
    assert_eq!(AVS_TKN_MOD_KCONTROL_ID_U32, 1707); // tokens.h:125
    assert_eq!(AVS_TKN_MOD_INIT_CONFIG_NUM_IDS_U32, 1708); // tokens.h:126
    assert_eq!(AVS_TKN_MOD_INIT_CONFIG_ID_U32, 1709); // tokens.h:127
    assert_eq!(AVS_TKN_MOD_NHLT_CONFIG_ID_U32, 1710); // tokens.h:128
    assert_eq!(AVS_TKN_PATH_TMPL_ID_U32, 1801); // tokens.h:131
    assert_eq!(AVS_TKN_PATH_ID_U32, 1901); // tokens.h:134
    assert_eq!(AVS_TKN_PATH_FE_FMT_ID_U32, 1902); // tokens.h:135
    assert_eq!(AVS_TKN_PATH_BE_FMT_ID_U32, 1903); // tokens.h:136
    assert_eq!(AVS_TKN_CONDPATH_TMPL_ID_U32, 1801); // tokens.h:139
    assert_eq!(AVS_TKN_CONDPATH_TMPL_SOURCE_TPLG_NAME_STRING, 2002); // tokens.h:140
    assert_eq!(AVS_TKN_CONDPATH_TMPL_SOURCE_PATH_TMPL_ID_U32, 2003); // tokens.h:141
    assert_eq!(AVS_TKN_CONDPATH_TMPL_SINK_TPLG_NAME_STRING, 2004); // tokens.h:142
    assert_eq!(AVS_TKN_CONDPATH_TMPL_SINK_PATH_TMPL_ID_U32, 2005); // tokens.h:143
    assert_eq!(AVS_TKN_CONDPATH_TMPL_COND_TYPE_U32, 2006); // tokens.h:144
    assert_eq!(AVS_TKN_CONDPATH_TMPL_OVERRIDABLE_BOOL, 2007); // tokens.h:145
    assert_eq!(AVS_TKN_CONDPATH_TMPL_PRIORITY_U8, 2008); // tokens.h:146
    assert_eq!(AVS_TKN_CONDPATH_ID_U32, 1901); // tokens.h:149
    assert_eq!(AVS_TKN_CONDPATH_SOURCE_PATH_ID_U32, 2102); // tokens.h:150
    assert_eq!(AVS_TKN_CONDPATH_SINK_PATH_ID_U32, 2103); // tokens.h:151
    assert_eq!(AVS_TKN_PIN_FMT_INDEX_U32, 2201); // tokens.h:154
    assert_eq!(AVS_TKN_PIN_FMT_IOBS_U32, 2202); // tokens.h:155
    assert_eq!(AVS_TKN_PIN_FMT_AFMT_ID_U32, 2203); // tokens.h:156
    assert_eq!(AVS_TKN_KCONTROL_ID_U32, 2301); // tokens.h:159
    assert_eq!(AVS_TKN_INIT_CONFIG_ID_U32, 2401); // tokens.h:162
    assert_eq!(AVS_TKN_INIT_CONFIG_PARAM_U8, 2402); // tokens.h:163
    assert_eq!(AVS_TKN_INIT_CONFIG_LENGTH_U32, 2403); // tokens.h:164
    assert_eq!(AVS_TKN_NHLT_CONFIG_ID_U32, 2501); // tokens.h:167
    assert_eq!(AVS_TKN_NHLT_CONFIG_SIZE_U32, 2502); // tokens.h:168
}

#[test]
fn tokens_h_defines_123_tokens_and_the_crate_names_all_of_them() {
    let src = include_str!("tokens_h_names.txt");
    assert_eq!(src.lines().count(), 123);
    let ours = include_str!("../src/tokens.rs");
    for name in src.lines() {
        assert!(ours.contains(&format!("pub const {name}: u32")), "{name} missing from src/tokens.rs");
    }
}
