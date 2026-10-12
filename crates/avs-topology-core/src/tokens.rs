// SPDX-License-Identifier: GPL-2.0-only
//! `enum avs_tplg_token` (include/uapi/sound/intel/avs/tokens.h:12-169): the vendor tokens of an AVS topology. Names carry the
//! tuple type Linux parses each with (`_U32` words, `_U16` shorts, `_U8` bytes, `_BOOL`, `_STRING`, `_UUID`).

// struct avs_tplg
pub const AVS_TKN_MANIFEST_NAME_STRING: u32 = 1; // tokens.h:14
pub const AVS_TKN_MANIFEST_VERSION_U32: u32 = 2; // tokens.h:15
pub const AVS_TKN_MANIFEST_NUM_LIBRARIES_U32: u32 = 3; // tokens.h:16
pub const AVS_TKN_MANIFEST_NUM_AFMTS_U32: u32 = 4; // tokens.h:17
pub const AVS_TKN_MANIFEST_NUM_MODCFGS_BASE_U32: u32 = 5; // tokens.h:18
pub const AVS_TKN_MANIFEST_NUM_MODCFGS_EXT_U32: u32 = 6; // tokens.h:19
pub const AVS_TKN_MANIFEST_NUM_PPLCFGS_U32: u32 = 7; // tokens.h:20
pub const AVS_TKN_MANIFEST_NUM_BINDINGS_U32: u32 = 8; // tokens.h:21
pub const AVS_TKN_MANIFEST_NUM_CONDPATH_TMPLS_U32: u32 = 9; // tokens.h:22
pub const AVS_TKN_MANIFEST_NUM_INIT_CONFIGS_U32: u32 = 10; // tokens.h:23
pub const AVS_TKN_MANIFEST_NUM_NHLT_CONFIGS_U32: u32 = 11; // tokens.h:24

// struct avs_tplg_library
pub const AVS_TKN_LIBRARY_ID_U32: u32 = 101; // tokens.h:27
pub const AVS_TKN_LIBRARY_NAME_STRING: u32 = 102; // tokens.h:28

// struct avs_audio_format
pub const AVS_TKN_AFMT_ID_U32: u32 = 201; // tokens.h:31
pub const AVS_TKN_AFMT_SAMPLE_RATE_U32: u32 = 202; // tokens.h:32
pub const AVS_TKN_AFMT_BIT_DEPTH_U32: u32 = 203; // tokens.h:33
pub const AVS_TKN_AFMT_CHANNEL_MAP_U32: u32 = 204; // tokens.h:34
pub const AVS_TKN_AFMT_CHANNEL_CFG_U32: u32 = 205; // tokens.h:35
pub const AVS_TKN_AFMT_INTERLEAVING_U32: u32 = 206; // tokens.h:36
pub const AVS_TKN_AFMT_NUM_CHANNELS_U32: u32 = 207; // tokens.h:37
pub const AVS_TKN_AFMT_VALID_BIT_DEPTH_U32: u32 = 208; // tokens.h:38
pub const AVS_TKN_AFMT_SAMPLE_TYPE_U32: u32 = 209; // tokens.h:39

// struct avs_tplg_modcfg_base
pub const AVS_TKN_MODCFG_BASE_ID_U32: u32 = 301; // tokens.h:42
pub const AVS_TKN_MODCFG_BASE_CPC_U32: u32 = 302; // tokens.h:43
pub const AVS_TKN_MODCFG_BASE_IBS_U32: u32 = 303; // tokens.h:44
pub const AVS_TKN_MODCFG_BASE_OBS_U32: u32 = 304; // tokens.h:45
pub const AVS_TKN_MODCFG_BASE_PAGES_U32: u32 = 305; // tokens.h:46

// struct avs_tplg_modcfg_ext
pub const AVS_TKN_MODCFG_EXT_ID_U32: u32 = 401; // tokens.h:49
pub const AVS_TKN_MODCFG_EXT_TYPE_UUID: u32 = 402; // tokens.h:50
pub const AVS_TKN_MODCFG_CPR_OUT_AFMT_ID_U32: u32 = 403; // tokens.h:51
pub const AVS_TKN_MODCFG_CPR_FEATURE_MASK_U32: u32 = 404; // tokens.h:52
pub const AVS_TKN_MODCFG_CPR_DMA_TYPE_U32: u32 = 405; // tokens.h:53
pub const AVS_TKN_MODCFG_CPR_DMABUFF_SIZE_U32: u32 = 406; // tokens.h:54
pub const AVS_TKN_MODCFG_CPR_VINDEX_U8: u32 = 407; // tokens.h:55
pub const AVS_TKN_MODCFG_CPR_BLOB_FMT_ID_U32: u32 = 408; // tokens.h:56
pub const AVS_TKN_MODCFG_MICSEL_OUT_AFMT_ID_U32: u32 = 409; // tokens.h:57
pub const AVS_TKN_MODCFG_INTELWOV_CPC_LP_MODE_U32: u32 = 410; // tokens.h:58
pub const AVS_TKN_MODCFG_SRC_OUT_FREQ_U32: u32 = 411; // tokens.h:59
pub const AVS_TKN_MODCFG_MUX_REF_AFMT_ID_U32: u32 = 412; // tokens.h:60
pub const AVS_TKN_MODCFG_MUX_OUT_AFMT_ID_U32: u32 = 413; // tokens.h:61
pub const AVS_TKN_MODCFG_AEC_REF_AFMT_ID_U32: u32 = 414; // tokens.h:62
pub const AVS_TKN_MODCFG_AEC_OUT_AFMT_ID_U32: u32 = 415; // tokens.h:63
pub const AVS_TKN_MODCFG_AEC_CPC_LP_MODE_U32: u32 = 416; // tokens.h:64
pub const AVS_TKN_MODCFG_ASRC_OUT_FREQ_U32: u32 = 417; // tokens.h:65
pub const AVS_TKN_MODCFG_ASRC_MODE_U8: u32 = 418; // tokens.h:66
pub const AVS_TKN_MODCFG_ASRC_DISABLE_JITTER_U8: u32 = 419; // tokens.h:67
pub const AVS_TKN_MODCFG_UPDOWN_MIX_OUT_CHAN_CFG_U32: u32 = 420; // tokens.h:68
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_SELECT_U32: u32 = 421; // tokens.h:69
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_0_S32: u32 = 422; // tokens.h:70
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_1_S32: u32 = 423; // tokens.h:71
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_2_S32: u32 = 424; // tokens.h:72
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_3_S32: u32 = 425; // tokens.h:73
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_4_S32: u32 = 426; // tokens.h:74
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_5_S32: u32 = 427; // tokens.h:75
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_6_S32: u32 = 428; // tokens.h:76
pub const AVS_TKN_MODCFG_UPDOWN_MIX_COEFF_7_S32: u32 = 429; // tokens.h:77
pub const AVS_TKN_MODCFG_UPDOWN_MIX_CHAN_MAP_U32: u32 = 430; // tokens.h:78
pub const AVS_TKN_MODCFG_EXT_NUM_INPUT_PINS_U16: u32 = 431; // tokens.h:79
pub const AVS_TKN_MODCFG_EXT_NUM_OUTPUT_PINS_U16: u32 = 432; // tokens.h:80
pub const AVS_TKN_MODCFG_WHM_REF_AFMT_ID_U32: u32 = 433; // tokens.h:81
pub const AVS_TKN_MODCFG_WHM_OUT_AFMT_ID_U32: u32 = 434; // tokens.h:82
pub const AVS_TKN_MODCFG_WHM_WAKE_TICK_PERIOD_U32: u32 = 435; // tokens.h:83
pub const AVS_TKN_MODCFG_WHM_VINDEX_U8: u32 = 436; // tokens.h:84
pub const AVS_TKN_MODCFG_WHM_DMA_TYPE_U32: u32 = 437; // tokens.h:85
pub const AVS_TKN_MODCFG_WHM_DMABUFF_SIZE_U32: u32 = 438; // tokens.h:86
pub const AVS_TKN_MODCFG_WHM_BLOB_AFMT_ID_U32: u32 = 439; // tokens.h:87
pub const AVS_TKN_MODCFG_PEAKVOL_VOLUME_U32: u32 = 440; // tokens.h:88
pub const AVS_TKN_MODCFG_PEAKVOL_CHANNEL_ID_U32: u32 = 441; // tokens.h:89 (reserved)
pub const AVS_TKN_MODCFG_PEAKVOL_CURVE_TYPE_U32: u32 = 442; // tokens.h:90
pub const AVS_TKN_MODCFG_PEAKVOL_CURVE_DURATION_U32: u32 = 443; // tokens.h:91

// struct avs_tplg_pplcfg
pub const AVS_TKN_PPLCFG_ID_U32: u32 = 1401; // tokens.h:94
pub const AVS_TKN_PPLCFG_REQ_SIZE_U16: u32 = 1402; // tokens.h:95
pub const AVS_TKN_PPLCFG_PRIORITY_U8: u32 = 1403; // tokens.h:96
pub const AVS_TKN_PPLCFG_LOW_POWER_BOOL: u32 = 1404; // tokens.h:97
pub const AVS_TKN_PPLCFG_ATTRIBUTES_U16: u32 = 1405; // tokens.h:98
pub const AVS_TKN_PPLCFG_TRIGGER_U32: u32 = 1406; // tokens.h:99

// struct avs_tplg_binding
pub const AVS_TKN_BINDING_ID_U32: u32 = 1501; // tokens.h:102
pub const AVS_TKN_BINDING_TARGET_TPLG_NAME_STRING: u32 = 1502; // tokens.h:103
pub const AVS_TKN_BINDING_TARGET_PATH_TMPL_ID_U32: u32 = 1503; // tokens.h:104
pub const AVS_TKN_BINDING_TARGET_PPL_ID_U32: u32 = 1504; // tokens.h:105
pub const AVS_TKN_BINDING_TARGET_MOD_ID_U32: u32 = 1505; // tokens.h:106
pub const AVS_TKN_BINDING_TARGET_MOD_PIN_U8: u32 = 1506; // tokens.h:107
pub const AVS_TKN_BINDING_MOD_ID_U32: u32 = 1507; // tokens.h:108
pub const AVS_TKN_BINDING_MOD_PIN_U8: u32 = 1508; // tokens.h:109
pub const AVS_TKN_BINDING_IS_SINK_U8: u32 = 1509; // tokens.h:110

// struct avs_tplg_pipeline
pub const AVS_TKN_PPL_ID_U32: u32 = 1601; // tokens.h:113
pub const AVS_TKN_PPL_PPLCFG_ID_U32: u32 = 1602; // tokens.h:114
pub const AVS_TKN_PPL_NUM_BINDING_IDS_U32: u32 = 1603; // tokens.h:115
pub const AVS_TKN_PPL_BINDING_ID_U32: u32 = 1604; // tokens.h:116

// struct avs_tplg_module
pub const AVS_TKN_MOD_ID_U32: u32 = 1701; // tokens.h:119
pub const AVS_TKN_MOD_MODCFG_BASE_ID_U32: u32 = 1702; // tokens.h:120
pub const AVS_TKN_MOD_IN_AFMT_ID_U32: u32 = 1703; // tokens.h:121
pub const AVS_TKN_MOD_CORE_ID_U8: u32 = 1704; // tokens.h:122
pub const AVS_TKN_MOD_PROC_DOMAIN_U8: u32 = 1705; // tokens.h:123
pub const AVS_TKN_MOD_MODCFG_EXT_ID_U32: u32 = 1706; // tokens.h:124
pub const AVS_TKN_MOD_KCONTROL_ID_U32: u32 = 1707; // tokens.h:125
pub const AVS_TKN_MOD_INIT_CONFIG_NUM_IDS_U32: u32 = 1708; // tokens.h:126
pub const AVS_TKN_MOD_INIT_CONFIG_ID_U32: u32 = 1709; // tokens.h:127
pub const AVS_TKN_MOD_NHLT_CONFIG_ID_U32: u32 = 1710; // tokens.h:128

// struct avs_tplg_path_template
pub const AVS_TKN_PATH_TMPL_ID_U32: u32 = 1801; // tokens.h:131

// struct avs_tplg_path
pub const AVS_TKN_PATH_ID_U32: u32 = 1901; // tokens.h:134
pub const AVS_TKN_PATH_FE_FMT_ID_U32: u32 = 1902; // tokens.h:135
pub const AVS_TKN_PATH_BE_FMT_ID_U32: u32 = 1903; // tokens.h:136

// struct avs_tplg_path_template (conditional)
pub const AVS_TKN_CONDPATH_TMPL_ID_U32: u32 = 1801; // tokens.h:139
pub const AVS_TKN_CONDPATH_TMPL_SOURCE_TPLG_NAME_STRING: u32 = 2002; // tokens.h:140
pub const AVS_TKN_CONDPATH_TMPL_SOURCE_PATH_TMPL_ID_U32: u32 = 2003; // tokens.h:141
pub const AVS_TKN_CONDPATH_TMPL_SINK_TPLG_NAME_STRING: u32 = 2004; // tokens.h:142
pub const AVS_TKN_CONDPATH_TMPL_SINK_PATH_TMPL_ID_U32: u32 = 2005; // tokens.h:143
pub const AVS_TKN_CONDPATH_TMPL_COND_TYPE_U32: u32 = 2006; // tokens.h:144
pub const AVS_TKN_CONDPATH_TMPL_OVERRIDABLE_BOOL: u32 = 2007; // tokens.h:145
pub const AVS_TKN_CONDPATH_TMPL_PRIORITY_U8: u32 = 2008; // tokens.h:146

// struct avs_tplg_path (conditional)
pub const AVS_TKN_CONDPATH_ID_U32: u32 = 1901; // tokens.h:149
pub const AVS_TKN_CONDPATH_SOURCE_PATH_ID_U32: u32 = 2102; // tokens.h:150
pub const AVS_TKN_CONDPATH_SINK_PATH_ID_U32: u32 = 2103; // tokens.h:151

// struct avs_tplg_pin_format
pub const AVS_TKN_PIN_FMT_INDEX_U32: u32 = 2201; // tokens.h:154
pub const AVS_TKN_PIN_FMT_IOBS_U32: u32 = 2202; // tokens.h:155
pub const AVS_TKN_PIN_FMT_AFMT_ID_U32: u32 = 2203; // tokens.h:156

// struct avs_tplg_kcontrol
pub const AVS_TKN_KCONTROL_ID_U32: u32 = 2301; // tokens.h:159

// struct avs_tplg_init_config
pub const AVS_TKN_INIT_CONFIG_ID_U32: u32 = 2401; // tokens.h:162
pub const AVS_TKN_INIT_CONFIG_PARAM_U8: u32 = 2402; // tokens.h:163
pub const AVS_TKN_INIT_CONFIG_LENGTH_U32: u32 = 2403; // tokens.h:164

// struct avs_tplg_nhlt_config
pub const AVS_TKN_NHLT_CONFIG_ID_U32: u32 = 2501; // tokens.h:167
pub const AVS_TKN_NHLT_CONFIG_SIZE_U32: u32 = 2502; // tokens.h:168
