// SPDX-License-Identifier: GPL-2.0-only
//! OCR vectors from Linux `drivers/mmc/core/sd.c`, `core.c`, and `include/linux/mmc/sd.h`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_sd_init_core::ocr::*;

#[test]
fn request_and_response_1v8_bits_are_pinned_independently() {
    assert_eq!(SD_OCR_S18R, 0x0100_0000); // include/linux/mmc/sd.h:40
    assert_eq!(SD_ROCR_S18A, 0x0100_0000); // include/linux/mmc/sd.h:41
}

#[test]
fn every_sd_ocr_literal_is_pinned_by_name_and_mask() {
    let expected = [
        ("S18R_S18A", 0x0100_0000),            // include/linux/mmc/sd.h:40-41
        ("2T", 0x0800_0000),                   // include/linux/mmc/sd.h:42
        ("XPC", 0x1000_0000),                  // include/linux/mmc/sd.h:43
        ("CCS", 0x4000_0000),                  // include/linux/mmc/sd.h:44
        ("INVALID_LOW_VOLTAGES", 0x0000_7fff), // drivers/mmc/core/sd.c:1880-1884
    ];
    assert_eq!(OCR_BITS.len(), 5);
    assert_eq!(OCR_BITS.map(|x| (x.name, x.mask)), expected);
}

#[test]
fn voltage_negotiation_sanitizes_intersects_and_selects_highest_pair() {
    // sd.c:1884 strips 0x7fff; core.c:1134 intersects; core.c:1145-1152 retains the top pair.
    assert_eq!(sanitize_card_ocr(0x00ff_ffff), 0x00ff_8000);
    assert_eq!(
        select_voltage_window(0x00ff_ffff, 0x003f_0000),
        Ok(0x0030_0000)
    );
    assert_eq!(
        select_voltage_window(0x0000_7fff, 0x0030_0000),
        Err(OcrError::NoCommonVoltage {
            card_ocr: 0,
            host_ocr: 0x0030_0000
        })
    );
}

#[test]
fn acmd41_request_sets_capacity_uhs_and_power_bits_independently() {
    // sd.c:855-876: IF_COND adds CCS+2T, UHS adds S18R, >150mA adds XPC.
    assert_eq!(build_op_cond_ocr(0x0030_0000, true, true, 151), 0x5930_0000);
    assert_eq!(
        build_op_cond_ocr(0x0030_0000, false, false, 150),
        0x0030_0000
    );
}

#[test]
fn voltage_switch_requires_native_bus_request_and_acceptance() {
    // drivers/mmc/core/sd.c:883-891.
    assert!(should_switch_to_1v8(false, 0x0100_0000, 0x0100_0000));
    assert!(!should_switch_to_1v8(true, 0x0100_0000, 0x0100_0000));
    assert!(!should_switch_to_1v8(false, 0, 0x0100_0000));
    assert!(!should_switch_to_1v8(false, 0x0100_0000, 0));
}

#[test]
fn real_r3_ocr_words_decode_the_exact_signalling_decision() {
    // A powered-up SDHC response with S18A set: busy, CCS, S18A, and the voltage window.
    let accepted_r3 = 0xc1ff_8000;
    // The same real-style R3/OCR response with S18A clear must remain at 3.3 V.
    let refused_r3 = 0xc0ff_8000;
    let request_ocr = 0x4130_0000;

    assert!(should_switch_to_1v8(false, request_ocr, accepted_r3));
    assert!(!should_switch_to_1v8(false, request_ocr, refused_r3));
    // include/linux/mmc/sd.h:40-41; drivers/mmc/core/sd.c:883-891
}
