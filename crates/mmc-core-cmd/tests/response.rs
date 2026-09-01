// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for R1/R2/R3 response decoding.
//! Ported from `include/linux/mmc/{core.h,mmc.h}` and `drivers/mmc/core/mmc_ops.c`.
//! Copyright 2006-2007 Pierre Ossman and the Linux MMC authors.
use mmc_core_cmd::response::*;

#[test]
fn response_formats_are_pinned_by_count_name_and_flags() {
    let got: Vec<(&str,u32)> = RESPONSE_FORMATS.iter().map(|x|(x.name,x.flags)).collect();
    assert_eq!(got, [("R1",0x15),("R2",0x07),("R3",0x01)]); // include/linux/mmc/core.h:58,61-62
}

#[test]
fn every_native_r1_bit_is_pinned_by_name_bit_and_mask() {
    let expected = [
        ("OUT_OF_RANGE",31,0x8000_0000),("ADDRESS_ERROR",30,0x4000_0000),("BLOCK_LEN_ERROR",29,0x2000_0000),
        ("ERASE_SEQ_ERROR",28,0x1000_0000),("ERASE_PARAM",27,0x0800_0000),("WP_VIOLATION",26,0x0400_0000),
        ("CARD_IS_LOCKED",25,0x0200_0000),("LOCK_UNLOCK_FAILED",24,0x0100_0000),("COM_CRC_ERROR",23,0x0080_0000),
        ("ILLEGAL_COMMAND",22,0x0040_0000),("CARD_ECC_FAILED",21,0x0020_0000),("CC_ERROR",20,0x0010_0000),
        ("ERROR",19,0x0008_0000),("UNDERRUN",18,0x0004_0000),("OVERRUN",17,0x0002_0000),
        ("CID_CSD_OVERWRITE",16,0x0001_0000),("WP_ERASE_SKIP",15,0x0000_8000),("CARD_ECC_DISABLED",14,0x0000_4000),
        ("ERASE_RESET",13,0x0000_2000),("READY_FOR_DATA",8,0x0000_0100),("SWITCH_ERROR",7,0x0000_0080),
        ("EXCEPTION_EVENT",6,0x0000_0040),("APP_CMD",5,0x0000_0020), // include/linux/mmc/mmc.h:134-158
    ];
    assert_eq!(R1_STATUS_BITS.iter().map(|x|(x.name,x.bit,x.mask)).collect::<Vec<_>>(), expected);
    let errors = ["OUT_OF_RANGE","ADDRESS_ERROR","BLOCK_LEN_ERROR","ERASE_SEQ_ERROR","ERASE_PARAM","WP_VIOLATION","LOCK_UNLOCK_FAILED","COM_CRC_ERROR","ILLEGAL_COMMAND","CARD_ECC_FAILED","CC_ERROR","ERROR","UNDERRUN","OVERRUN","CID_CSD_OVERWRITE"];
    assert_eq!(R1_ERROR_BITS.iter().map(|x|x.name).collect::<Vec<_>>(), errors);
    assert_eq!(R1_STATUS_MASK, 0xfff9_a000); // mmc.h:153
}

#[test]
fn every_spi_response_error_is_pinned() {
    let r1 = [("ERASE_RESET",1,0x2),("ILLEGAL_COMMAND",2,0x4),("COM_CRC",3,0x8),("ERASE_SEQ",4,0x10),("ADDRESS",5,0x20),("PARAMETER",6,0x40)]; // mmc.h:185-190
    let r2 = [("CARD_LOCKED",8,0x100),("WP_ERASE_SKIP_OR_LOCK_UNLOCK_FAIL",9,0x200),("ERROR",10,0x400),("CC_ERROR",11,0x800),("CARD_ECC_ERROR",12,0x1000),("WP_VIOLATION",13,0x2000),("ERASE_PARAM",14,0x4000),("OUT_OF_RANGE_OR_CSD_OVERWRITE",15,0x8000)]; // mmc.h:192-201
    assert_eq!(SPI_R1_ERROR_BITS.iter().map(|x|(x.name,x.bit,x.mask)).collect::<Vec<_>>(), r1);
    assert_eq!(SPI_R2_ERROR_BITS.iter().map(|x|(x.name,x.bit,x.mask)).collect::<Vec<_>>(), r2);
}

#[test]
fn decode_functions_have_literal_vectors() {
    assert_eq!(r1_errors(0x8040_0000).map(|x|x.name).collect::<Vec<_>>(), ["OUT_OF_RANGE","ILLEGAL_COMMAND"]);
    assert_eq!(spi_r1_errors(0x0048).map(|x|x.name).collect::<Vec<_>>(), ["COM_CRC","PARAMETER"]);
    assert_eq!(spi_r2_errors(0x9000).map(|x|x.name).collect::<Vec<_>>(), ["CARD_ECC_ERROR","OUT_OF_RANGE_OR_CSD_OVERWRITE"]);
    assert_eq!(r1_status_errors(0xffff_ffff), 0xfff9_a000); // mmc.h:153
    assert!(r3_power_up_complete(0x8000_0000)); assert!(!r3_power_up_complete(0x4000_0000)); // mmc.h:206
}
