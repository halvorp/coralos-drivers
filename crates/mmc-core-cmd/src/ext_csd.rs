// SPDX-License-Identifier: GPL-2.0-only
//! eMMC EXT_CSD byte offsets consumed by Linux's MMC core.
//!
//! Ported from Linux `drivers/mmc/core/mmc.c` and `mmc_ops.c`; offsets are the
//! `include/linux/mmc/mmc.h` literals used by those files. Copyright (C)
//! 2003-2004 Russell King; 2005-2007 Pierre Ossman; 2006 Philip Langdale.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtCsdField { pub name: &'static str, pub offset: u16, pub width: u8 }
pub const EXT_CSD_LEN: usize = 512; // mmc_ops.c:386
pub const EXT_CSD_FIELDS: [ExtCsdField; 68] = [
    ExtCsdField{name:"CMDQ_MODE_EN",offset:15,width:1}, // include/linux/mmc/mmc.h:256
    ExtCsdField{name:"FLUSH_CACHE",offset:32,width:1}, // include/linux/mmc/mmc.h:257
    ExtCsdField{name:"CACHE_CTRL",offset:33,width:1}, // include/linux/mmc/mmc.h:258
    ExtCsdField{name:"POWER_OFF_NOTIFICATION",offset:34,width:1}, // include/linux/mmc/mmc.h:259
    ExtCsdField{name:"EXP_EVENTS_STATUS",offset:54,width:2}, // include/linux/mmc/mmc.h:260
    ExtCsdField{name:"EXP_EVENTS_CTRL",offset:56,width:2}, // include/linux/mmc/mmc.h:261
    ExtCsdField{name:"DATA_SECTOR_SIZE",offset:61,width:1}, // include/linux/mmc/mmc.h:262
    ExtCsdField{name:"GP_SIZE_MULT",offset:143,width:12}, // mmc.c:366-379 (four 3-byte fields)
    ExtCsdField{name:"PARTITION_SETTING_COMPLETED",offset:155,width:1}, // include/linux/mmc/mmc.h:264
    ExtCsdField{name:"PARTITION_ATTRIBUTE",offset:156,width:1}, // include/linux/mmc/mmc.h:265
    ExtCsdField{name:"PARTITION_SUPPORT",offset:160,width:1}, // include/linux/mmc/mmc.h:266
    ExtCsdField{name:"HPI_MGMT",offset:161,width:1}, // include/linux/mmc/mmc.h:267
    ExtCsdField{name:"RST_N_FUNCTION",offset:162,width:1}, // include/linux/mmc/mmc.h:268
    ExtCsdField{name:"BKOPS_EN",offset:163,width:1}, // include/linux/mmc/mmc.h:269
    ExtCsdField{name:"BKOPS_START",offset:164,width:1}, // include/linux/mmc/mmc.h:270
    ExtCsdField{name:"SANITIZE_START",offset:165,width:1}, // include/linux/mmc/mmc.h:271
    ExtCsdField{name:"WR_REL_PARAM",offset:166,width:1}, // include/linux/mmc/mmc.h:272
    ExtCsdField{name:"RPMB_MULT",offset:168,width:1}, // include/linux/mmc/mmc.h:273
    ExtCsdField{name:"FW_CONFIG",offset:169,width:1}, // include/linux/mmc/mmc.h:274
    ExtCsdField{name:"BOOT_WP",offset:173,width:1}, // include/linux/mmc/mmc.h:275
    ExtCsdField{name:"ERASE_GROUP_DEF",offset:175,width:1}, // include/linux/mmc/mmc.h:276
    ExtCsdField{name:"PART_CONFIG",offset:179,width:1}, // include/linux/mmc/mmc.h:277
    ExtCsdField{name:"ERASED_MEM_CONT",offset:181,width:1}, // include/linux/mmc/mmc.h:278
    ExtCsdField{name:"BUS_WIDTH",offset:183,width:1}, // include/linux/mmc/mmc.h:279
    ExtCsdField{name:"STROBE_SUPPORT",offset:184,width:1}, // include/linux/mmc/mmc.h:280
    ExtCsdField{name:"HS_TIMING",offset:185,width:1}, // include/linux/mmc/mmc.h:281
    ExtCsdField{name:"POWER_CLASS",offset:187,width:1}, // include/linux/mmc/mmc.h:282
    ExtCsdField{name:"REV",offset:192,width:1}, // include/linux/mmc/mmc.h:283
    ExtCsdField{name:"STRUCTURE",offset:194,width:1}, // include/linux/mmc/mmc.h:284
    ExtCsdField{name:"CARD_TYPE",offset:196,width:1}, // include/linux/mmc/mmc.h:285
    ExtCsdField{name:"DRIVER_STRENGTH",offset:197,width:1}, // include/linux/mmc/mmc.h:286
    ExtCsdField{name:"OUT_OF_INTERRUPT_TIME",offset:198,width:1}, // include/linux/mmc/mmc.h:287
    ExtCsdField{name:"PART_SWITCH_TIME",offset:199,width:1}, // include/linux/mmc/mmc.h:288
    ExtCsdField{name:"PWR_CL_52_195",offset:200,width:1}, // include/linux/mmc/mmc.h:289
    ExtCsdField{name:"PWR_CL_26_195",offset:201,width:1}, // include/linux/mmc/mmc.h:290
    ExtCsdField{name:"PWR_CL_52_360",offset:202,width:1}, // include/linux/mmc/mmc.h:291
    ExtCsdField{name:"PWR_CL_26_360",offset:203,width:1}, // include/linux/mmc/mmc.h:292
    ExtCsdField{name:"SEC_CNT",offset:212,width:4}, // include/linux/mmc/mmc.h:293
    ExtCsdField{name:"S_A_TIMEOUT",offset:217,width:1}, // include/linux/mmc/mmc.h:294
    ExtCsdField{name:"HC_WP_GRP_SIZE",offset:221,width:1}, // include/linux/mmc/mmc.h:296
    ExtCsdField{name:"REL_WR_SEC_C",offset:222,width:1}, // include/linux/mmc/mmc.h:295
    ExtCsdField{name:"ERASE_TIMEOUT_MULT",offset:223,width:1}, // include/linux/mmc/mmc.h:297
    ExtCsdField{name:"HC_ERASE_GRP_SIZE",offset:224,width:1}, // include/linux/mmc/mmc.h:298
    ExtCsdField{name:"BOOT_MULT",offset:226,width:1}, // include/linux/mmc/mmc.h:299
    ExtCsdField{name:"SEC_TRIM_MULT",offset:229,width:1}, // include/linux/mmc/mmc.h:300
    ExtCsdField{name:"SEC_ERASE_MULT",offset:230,width:1}, // include/linux/mmc/mmc.h:301
    ExtCsdField{name:"SEC_FEATURE_SUPPORT",offset:231,width:1}, // include/linux/mmc/mmc.h:302
    ExtCsdField{name:"TRIM_MULT",offset:232,width:1}, // include/linux/mmc/mmc.h:303
    ExtCsdField{name:"PWR_CL_200_195",offset:236,width:1}, // include/linux/mmc/mmc.h:304
    ExtCsdField{name:"PWR_CL_200_360",offset:237,width:1}, // include/linux/mmc/mmc.h:305
    ExtCsdField{name:"PWR_CL_DDR_52_195",offset:238,width:1}, // include/linux/mmc/mmc.h:306
    ExtCsdField{name:"PWR_CL_DDR_52_360",offset:239,width:1}, // include/linux/mmc/mmc.h:307
    ExtCsdField{name:"BKOPS_STATUS",offset:246,width:1}, // include/linux/mmc/mmc.h:308
    ExtCsdField{name:"POWER_OFF_LONG_TIME",offset:247,width:1}, // include/linux/mmc/mmc.h:309
    ExtCsdField{name:"GENERIC_CMD6_TIME",offset:248,width:1}, // include/linux/mmc/mmc.h:310
    ExtCsdField{name:"CACHE_SIZE",offset:249,width:4}, // include/linux/mmc/mmc.h:311
    ExtCsdField{name:"PWR_CL_DDR_200_360",offset:253,width:1}, // include/linux/mmc/mmc.h:312
    ExtCsdField{name:"FIRMWARE_VERSION",offset:254,width:8}, // include/linux/mmc/mmc.h:313
    ExtCsdField{name:"PRE_EOL_INFO",offset:267,width:1}, // include/linux/mmc/mmc.h:314
    ExtCsdField{name:"DEVICE_LIFE_TIME_EST_TYP_A",offset:268,width:1}, // include/linux/mmc/mmc.h:315
    ExtCsdField{name:"DEVICE_LIFE_TIME_EST_TYP_B",offset:269,width:1}, // include/linux/mmc/mmc.h:316
    ExtCsdField{name:"CMDQ_DEPTH",offset:307,width:1}, // include/linux/mmc/mmc.h:317
    ExtCsdField{name:"CMDQ_SUPPORT",offset:308,width:1}, // include/linux/mmc/mmc.h:318
    ExtCsdField{name:"SUPPORTED_MODE",offset:493,width:1}, // include/linux/mmc/mmc.h:319
    ExtCsdField{name:"TAG_UNIT_SIZE",offset:498,width:1}, // include/linux/mmc/mmc.h:320
    ExtCsdField{name:"DATA_TAG_SUPPORT",offset:499,width:1}, // include/linux/mmc/mmc.h:321
    ExtCsdField{name:"BKOPS_SUPPORT",offset:502,width:1}, // include/linux/mmc/mmc.h:322
    ExtCsdField{name:"HPI_FEATURES",offset:503,width:1}, // include/linux/mmc/mmc.h:323
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtCsdError { FieldOutsideBlock { offset: u16, width: u8, block_len: usize }, WidthUnsupported { width: u8, maximum: u8 } }
pub fn read_bytes<'a>(ext_csd: &'a [u8; EXT_CSD_LEN], field: ExtCsdField) -> Result<&'a [u8], ExtCsdError> {
    let start = usize::from(field.offset);
    let end = start + usize::from(field.width);
    if end > EXT_CSD_LEN { return Err(ExtCsdError::FieldOutsideBlock { offset: field.offset, width: field.width, block_len: EXT_CSD_LEN }); }
    Ok(&ext_csd[start..end])
}
pub fn read_le(ext_csd: &[u8; EXT_CSD_LEN], field: ExtCsdField) -> Result<u32, ExtCsdError> {
    if field.width == 0 || field.width > 4 { return Err(ExtCsdError::WidthUnsupported { width: field.width, maximum: 4 }); }
    let bytes = read_bytes(ext_csd, field)?;
    let mut value = 0u32; let mut i = 0usize;
    while i < bytes.len() { value |= u32::from(bytes[i]) << (8*i); i += 1; }
    Ok(value)
}
