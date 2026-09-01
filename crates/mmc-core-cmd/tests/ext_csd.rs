// SPDX-License-Identifier: GPL-2.0-only
//! Frozen EXT_CSD offsets from Linux `include/linux/mmc/mmc.h`, used by `mmc.c`/`mmc_ops.c`.
//! Copyright (C) 2003-2004 Russell King; 2005-2007 Pierre Ossman; 2006 Philip Langdale.
use mmc_core_cmd::ext_csd::*;

#[test]
fn every_linux_ext_csd_offset_is_pinned_by_count_name_offset_and_width() {
    let expected=[
("CMDQ_MODE_EN",15,1),("FLUSH_CACHE",32,1),("CACHE_CTRL",33,1),("POWER_OFF_NOTIFICATION",34,1),("EXP_EVENTS_STATUS",54,2),("EXP_EVENTS_CTRL",56,2),("DATA_SECTOR_SIZE",61,1),("GP_SIZE_MULT",143,12),("PARTITION_SETTING_COMPLETED",155,1),("PARTITION_ATTRIBUTE",156,1),("PARTITION_SUPPORT",160,1),("HPI_MGMT",161,1),("RST_N_FUNCTION",162,1),("BKOPS_EN",163,1),("BKOPS_START",164,1),("SANITIZE_START",165,1),("WR_REL_PARAM",166,1),("RPMB_MULT",168,1),("FW_CONFIG",169,1),("BOOT_WP",173,1),("ERASE_GROUP_DEF",175,1),("PART_CONFIG",179,1),("ERASED_MEM_CONT",181,1),("BUS_WIDTH",183,1),("STROBE_SUPPORT",184,1),("HS_TIMING",185,1),("POWER_CLASS",187,1),("REV",192,1),("STRUCTURE",194,1),("CARD_TYPE",196,1),("DRIVER_STRENGTH",197,1),("OUT_OF_INTERRUPT_TIME",198,1),("PART_SWITCH_TIME",199,1),("PWR_CL_52_195",200,1),("PWR_CL_26_195",201,1),("PWR_CL_52_360",202,1),("PWR_CL_26_360",203,1),("SEC_CNT",212,4),("S_A_TIMEOUT",217,1),("HC_WP_GRP_SIZE",221,1),("REL_WR_SEC_C",222,1),("ERASE_TIMEOUT_MULT",223,1),("HC_ERASE_GRP_SIZE",224,1),("BOOT_MULT",226,1),("SEC_TRIM_MULT",229,1),("SEC_ERASE_MULT",230,1),("SEC_FEATURE_SUPPORT",231,1),("TRIM_MULT",232,1),("PWR_CL_200_195",236,1),("PWR_CL_200_360",237,1),("PWR_CL_DDR_52_195",238,1),("PWR_CL_DDR_52_360",239,1),("BKOPS_STATUS",246,1),("POWER_OFF_LONG_TIME",247,1),("GENERIC_CMD6_TIME",248,1),("CACHE_SIZE",249,4),("PWR_CL_DDR_200_360",253,1),("FIRMWARE_VERSION",254,8),("PRE_EOL_INFO",267,1),("DEVICE_LIFE_TIME_EST_TYP_A",268,1),("DEVICE_LIFE_TIME_EST_TYP_B",269,1),("CMDQ_DEPTH",307,1),("CMDQ_SUPPORT",308,1),("SUPPORTED_MODE",493,1),("TAG_UNIT_SIZE",498,1),("DATA_TAG_SUPPORT",499,1),("BKOPS_SUPPORT",502,1),("HPI_FEATURES",503,1), // mmc.h:256-323
    ];
    assert_eq!(EXT_CSD_FIELDS.iter().map(|x|(x.name,x.offset,x.width)).collect::<Vec<_>>(),expected);
    assert_eq!(EXT_CSD_LEN,512); // mmc_ops.c:386
}
#[test]
fn little_endian_reader_and_named_refusals_have_vectors() {
    let mut block=[0u8;512]; block[212]=0x78;block[213]=0x56;block[214]=0x34;block[215]=0x12;
    assert_eq!(read_le(&block,ExtCsdField{name:"SEC_CNT",offset:212,width:4}),Ok(0x1234_5678)); // mmc.c:456-463
    block[254..262].copy_from_slice(b"FIRMWARE");
    assert_eq!(read_bytes(&block,ExtCsdField{name:"FIRMWARE_VERSION",offset:254,width:8}),Ok(&b"FIRMWARE"[..])); // mmc.c:683-684
    assert_eq!(read_le(&block,ExtCsdField{name:"FW",offset:254,width:8}),Err(ExtCsdError::WidthUnsupported{width:8,maximum:4}));
    assert_eq!(read_le(&block,ExtCsdField{name:"BAD",offset:511,width:2}),Err(ExtCsdError::FieldOutsideBlock{offset:511,width:2,block_len:512}));
    assert_eq!(read_bytes(&block,ExtCsdField{name:"BAD",offset:511,width:2}),Err(ExtCsdError::FieldOutsideBlock{offset:511,width:2,block_len:512}));
}
