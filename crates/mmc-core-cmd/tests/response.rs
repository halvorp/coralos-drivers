// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for R1/R2/R3 response decoding.
//! Ported from `include/linux/mmc/{core.h,mmc.h}` and `drivers/mmc/core/mmc_ops.c`.
//! Copyright 2006-2007 Pierre Ossman and the Linux MMC authors.
use mmc_core_cmd::response::*;

/// `include/linux/mmc/core.h:35-39` — pin the five independent native-response flags directly.
/// Composite-format vectors alone are insufficient: changing a source bit and its dependent
/// composites together can preserve every relationship while corrupting the wire contract.
#[test]
fn every_native_response_flag_is_pinned_to_its_linux_literal() {
    let expected = [
        ("RSP_PRESENT", RSP_PRESENT, 0x01), // include/linux/mmc/core.h:35
        ("RSP_136", RSP_136, 0x02),       // include/linux/mmc/core.h:36
        ("RSP_CRC", RSP_CRC, 0x04),       // include/linux/mmc/core.h:37
        ("RSP_BUSY", RSP_BUSY, 0x08),     // include/linux/mmc/core.h:38
        ("RSP_OPCODE", RSP_OPCODE, 0x10), // include/linux/mmc/core.h:39
    ];

    assert_eq!(expected.len(), 5);
    for (name, actual, linux_literal) in expected {
        assert_eq!(actual, linux_literal, "{name} changed from its Linux literal");
        assert_eq!(actual.count_ones(), 1, "{name} is not exactly one bit");
    }
}

#[test]
fn response_formats_are_pinned_by_count_name_and_flags() {
    let got: Vec<(&str,u32)> = RESPONSE_FORMATS.iter().map(|x|(x.name,x.flags)).collect();
    assert_eq!(got, [("R1",0x15),("R2",0x07),("R3",0x01)]); // include/linux/mmc/core.h:58,61-62

    // Assert Linux's named composition as well as the frozen literals above. This catches a
    // composite that happens to have the right number while selecting the wrong response bits.
    assert_eq!(R1_FLAGS, RSP_PRESENT | RSP_CRC | RSP_OPCODE); // include/linux/mmc/core.h:58
    assert_eq!(R2_FLAGS, RSP_PRESENT | RSP_136 | RSP_CRC); // include/linux/mmc/core.h:61
    assert_eq!(R3_FLAGS, RSP_PRESENT); // include/linux/mmc/core.h:62
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
    assert_eq!(R1_STATUS_MASK, 0xfff9_a000); // include/linux/mmc/mmc.h:153
}

#[test]
fn every_spi_response_error_is_pinned() {
    let r1 = [("ERASE_RESET",1,0x2),("ILLEGAL_COMMAND",2,0x4),("COM_CRC",3,0x8),("ERASE_SEQ",4,0x10),("ADDRESS",5,0x20),("PARAMETER",6,0x40)]; // include/linux/mmc/mmc.h:185-190
    let r2 = [("CARD_LOCKED",8,0x100),("WP_ERASE_SKIP_OR_LOCK_UNLOCK_FAIL",9,0x200),("ERROR",10,0x400),("CC_ERROR",11,0x800),("CARD_ECC_ERROR",12,0x1000),("WP_VIOLATION",13,0x2000),("ERASE_PARAM",14,0x4000),("OUT_OF_RANGE_OR_CSD_OVERWRITE",15,0x8000)]; // include/linux/mmc/mmc.h:192-201
    assert_eq!(SPI_R1_ERROR_BITS.iter().map(|x|(x.name,x.bit,x.mask)).collect::<Vec<_>>(), r1);
    assert_eq!(SPI_R2_ERROR_BITS.iter().map(|x|(x.name,x.bit,x.mask)).collect::<Vec<_>>(), r2);
}

#[test]
fn decode_functions_have_literal_vectors() {
    assert_eq!(r1_errors(0x8040_0000).map(|x|x.name).collect::<Vec<_>>(), ["OUT_OF_RANGE","ILLEGAL_COMMAND"]);
    assert_eq!(spi_r1_errors(0x0048).map(|x|x.name).collect::<Vec<_>>(), ["COM_CRC","PARAMETER"]);
    assert_eq!(spi_r2_errors(0x9000).map(|x|x.name).collect::<Vec<_>>(), ["CARD_ECC_ERROR","OUT_OF_RANGE_OR_CSD_OVERWRITE"]);
    assert_eq!(r1_status_errors(0xffff_ffff), 0xfff9_a000); // include/linux/mmc/mmc.h:153
    assert!(r3_power_up_complete(0x8000_0000)); assert!(!r3_power_up_complete(0x4000_0000)); // include/linux/mmc/mmc.h:206
}

/// `include/linux/mmc/mmc.h:206` — unlike R1 flags, the R3 power-up bit is a protocol literal and
/// must be pinned independently of the decoder that consumes it.
#[test]
fn r3_card_busy_is_pinned_to_the_linux_literal() {
    assert_eq!(R3_CARD_BUSY, 0x8000_0000); // include/linux/mmc/mmc.h:206
    assert_eq!(R3_CARD_BUSY.count_ones(), 1);
}
