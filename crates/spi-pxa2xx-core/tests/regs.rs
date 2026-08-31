// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux literal vectors for registers and masks ported from
//! `include/linux/pxa2xx_ssp.h` and `drivers/spi/spi-pxa2xx.c`.
//!
//! Copyright (C) 2003 Russell King
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

use spi_pxa2xx_core::regs::{lpss, off, sscr0, sscr1, sssr, transfer};

#[test]
fn register_offsets_match_linux_literals() {
    // include/linux/pxa2xx_ssp.h:34-41,202,206.
    assert_eq!(
        [
            off::SSCR0,
            off::SSCR1,
            off::SSSR,
            off::SSDR,
            off::SSTO,
            off::SSPSP,
            off::SSITF,
            off::SSIRF
        ],
        [0x00, 0x04, 0x08, 0x10, 0x28, 0x2c, 0x44, 0x48]
    );
}

#[test]
fn control_and_status_masks_match_linux_literals() {
    // include/linux/pxa2xx_ssp.h:49,56-60,70-74,91,93,137-140.
    assert_eq!(sscr0::DSS_MASK, 0x0000_000f);
    assert_eq!(sscr0::SSE, 0x0000_0080);
    assert_eq!(sscr0::SCR_SHIFT, 8);
    assert_eq!(sscr0::SCR_MASK, 0x000f_ff00);
    assert_eq!(sscr0::EDSS, 0x0010_0000);
    assert_eq!(
        [sscr1::RIE, sscr1::TIE, sscr1::LBM, sscr1::SPO, sscr1::SPH],
        [0x1, 0x2, 0x4, 0x8, 0x10]
    );
    assert_eq!(sscr1::TFT_MASK, 0x0000_03c0);
    assert_eq!(sscr1::RFT_MASK, 0x0000_3c00);
    assert_eq!(
        [sscr1::TINTE, sscr1::RSRE, sscr1::TSRE, sscr1::TRAIL],
        [0x0008_0000, 0x0010_0000, 0x0020_0000, 0x0040_0000]
    );

    // include/linux/pxa2xx_ssp.h:79-89,152,154.
    assert_eq!(
        [sssr::RNE, sssr::BSY, sssr::TFS, sssr::RFS, sssr::ROR],
        [0x08, 0x10, 0x20, 0x40, 0x80]
    );
    assert_eq!(sssr::TFL_MASK, 0x0000_0f00);
    assert_eq!(sssr::RFL_MASK, 0x0000_f000);
    assert_eq!(sssr::TINT, 0x0008_0000);
    assert_eq!(sssr::TUR, 0x0020_0000);
}

#[test]
fn cherry_trail_private_literals_are_pinned() {
    // spi-pxa2xx.c:69-71,121-131.
    assert_eq!(lpss::GENERAL_RXTO_HOLDOFF_DISABLE, 0x0100_0000);
    assert_eq!(lpss::CS_CONTROL_SW_MODE, 0x1);
    assert_eq!(lpss::CS_CONTROL_CS_HIGH, 0x2);
    assert_eq!(lpss::BSW_PRIVATE_OFFSET, 0x400);
    assert_eq!(lpss::BSW_GENERAL, 0x08);
    assert_eq!(lpss::BSW_SSP, 0x0c);
    assert_eq!(lpss::BSW_CS_CONTROL, 0x18);
    assert_eq!(lpss::BSW_CS_SELECT_SHIFT, 2);
    assert_eq!(lpss::BSW_CS_SELECT_MASK, 0x4);
}

#[test]
fn lpss_transfer_masks_match_linux_or_expressions() {
    // spi-pxa2xx.c:1327-1331 and spi-pxa2xx.h:125.
    assert_eq!(transfer::INT_CR1, 0x0008_0003);
    assert_eq!(transfer::DMA_CR1, 0x0070_0000);
    assert_eq!(transfer::CLEAR_SR, 0x0008_0080);
    assert_eq!(transfer::MASK_SR, 0x0028_00e0);
}
