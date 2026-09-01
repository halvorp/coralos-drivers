// SPDX-License-Identifier: GPL-2.0-only
//! OCR voltage-window and initialisation-request negotiation.
//!
//! Ported from Linux `drivers/mmc/core/sd.c`, `drivers/mmc/core/core.c`,
//! `include/linux/mmc/sd.h`, and `include/linux/mmc/host.h`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

pub const SD_OCR_S18R: u32 = 1 << 24; // include/linux/mmc/sd.h:40
pub const SD_ROCR_S18A: u32 = SD_OCR_S18R; // include/linux/mmc/sd.h:41
pub const SD_OCR_2T: u32 = 1 << 27; // include/linux/mmc/sd.h:42
pub const SD_OCR_XPC: u32 = 1 << 28; // include/linux/mmc/sd.h:43
pub const SD_OCR_CCS: u32 = 1 << 30; // include/linux/mmc/sd.h:44
pub const SD_INVALID_LOW_VOLTAGES: u32 = 0x0000_7fff; // drivers/mmc/core/sd.c:1880-1884

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcrBit {
    pub name: &'static str,
    pub mask: u32,
}

pub const OCR_BITS: [OcrBit; 5] = [
    OcrBit {
        name: "S18R_S18A",
        mask: SD_OCR_S18R,
    }, // include/linux/mmc/sd.h:40-41
    OcrBit {
        name: "2T",
        mask: SD_OCR_2T,
    }, // include/linux/mmc/sd.h:42
    OcrBit {
        name: "XPC",
        mask: SD_OCR_XPC,
    }, // include/linux/mmc/sd.h:43
    OcrBit {
        name: "CCS",
        mask: SD_OCR_CCS,
    }, // include/linux/mmc/sd.h:44
    OcrBit {
        name: "INVALID_LOW_VOLTAGES",
        mask: SD_INVALID_LOW_VOLTAGES,
    }, // drivers/mmc/core/sd.c:1880-1884
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrError {
    NoCommonVoltage { card_ocr: u32, host_ocr: u32 },
}

/// Remove the out-of-spec low voltage claims rejected by SD attach.
pub fn sanitize_card_ocr(card_ocr: u32) -> u32 {
    card_ocr & !SD_INVALID_LOW_VOLTAGES // drivers/mmc/core/sd.c:1880-1884
}

/// Intersect card and host windows and retain Linux's highest adjacent pair.
pub fn select_voltage_window(card_ocr: u32, host_ocr: u32) -> Result<u32, OcrError> {
    let card_ocr = sanitize_card_ocr(card_ocr);
    let common = card_ocr & host_ocr; // drivers/mmc/core/core.c:1134
    if common == 0 {
        return Err(OcrError::NoCommonVoltage { card_ocr, host_ocr }); // drivers/mmc/core/core.c:1135-1137
    }
    let highest = 31 - common.leading_zeros(); // drivers/mmc/core/core.c:1145
    let pair = if highest == 0 { 1 } else { 3 << (highest - 1) }; // drivers/mmc/core/core.c:1147-1152
    Ok(common & pair) // drivers/mmc/core/core.c:1152
}

/// Add the capacity, UHS signalling, and extra-power requests used for ACMD41.
pub fn build_op_cond_ocr(
    voltage_window: u32,
    if_cond_succeeded: bool,
    request_uhs: bool,
    max_current_ma: u32,
) -> u32 {
    let mut ocr = voltage_window;
    if if_cond_succeeded {
        ocr |= SD_OCR_CCS | SD_OCR_2T; // drivers/mmc/core/sd.c:850-860
    }
    if request_uhs {
        ocr |= SD_OCR_S18R; // drivers/mmc/core/sd.c:862-869
    }
    if max_current_ma > 150 {
        ocr |= SD_OCR_XPC; // drivers/mmc/core/sd.c:870-876
    }
    ocr
}

/// Whether Linux proceeds with CMD11 after the ACMD41 response.
pub fn should_switch_to_1v8(is_spi: bool, request_ocr: u32, response_ocr: u32) -> bool {
    !is_spi && request_ocr & SD_OCR_S18R != 0 && response_ocr & SD_ROCR_S18A != 0
    // drivers/mmc/core/sd.c:882-891
}
