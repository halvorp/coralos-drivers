// SPDX-License-Identifier: GPL-2.0-only
//! Register corpus used by the Cherry Trail LPSS SPI decisions.
//!
//! Ported from Linux `include/linux/pxa2xx_ssp.h`, as included by
//! `drivers/spi/spi-pxa2xx.h`, and from LPSS-private definitions in
//! `drivers/spi/spi-pxa2xx.c`.
//!
//! Copyright (C) 2003 Russell King
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

/// SSP register offsets relative to the SSP MMIO base.
pub mod off {
    pub const SSCR0: u32 = 0x00; // include/linux/pxa2xx_ssp.h:34
    pub const SSCR1: u32 = 0x04; // include/linux/pxa2xx_ssp.h:35
    pub const SSSR: u32 = 0x08; // include/linux/pxa2xx_ssp.h:36
    pub const SSDR: u32 = 0x10; // include/linux/pxa2xx_ssp.h:38
    pub const SSTO: u32 = 0x28; // include/linux/pxa2xx_ssp.h:40
    pub const SSPSP: u32 = 0x2c; // include/linux/pxa2xx_ssp.h:41
    pub const SSITF: u32 = 0x44; // include/linux/pxa2xx_ssp.h:202
    pub const SSIRF: u32 = 0x48; // include/linux/pxa2xx_ssp.h:206
}

/// SSCR0 fields used to form a Motorola SPI control word.
pub mod sscr0 {
    pub const DSS_MASK: u32 = 0x0f; // include/linux/pxa2xx_ssp.h:49
    pub const MOTOROLA: u32 = 0x00; // include/linux/pxa2xx_ssp.h:52
    pub const SSE: u32 = 1 << 7; // include/linux/pxa2xx_ssp.h:56
    pub const SCR_SHIFT: u32 = 8; // include/linux/pxa2xx_ssp.h:57
    pub const SCR_MASK: u32 = 0x0fff << SCR_SHIFT; // spi-pxa2xx.c:902
    pub const EDSS: u32 = 1 << 20; // include/linux/pxa2xx_ssp.h:60
}

/// SSCR1 fields used for SPI mode, FIFO triggers and interrupt enables.
pub mod sscr1 {
    pub const RIE: u32 = 1 << 0; // include/linux/pxa2xx_ssp.h:70
    pub const TIE: u32 = 1 << 1; // include/linux/pxa2xx_ssp.h:71
    pub const LBM: u32 = 1 << 2; // include/linux/pxa2xx_ssp.h:72
    pub const SPO: u32 = 1 << 3; // include/linux/pxa2xx_ssp.h:73
    pub const SPH: u32 = 1 << 4; // include/linux/pxa2xx_ssp.h:74
    pub const TFT_MASK: u32 = 0x0f << 6; // include/linux/pxa2xx_ssp.h:91
    pub const RFT_MASK: u32 = 0x0f << 10; // include/linux/pxa2xx_ssp.h:93
    pub const TINTE: u32 = 1 << 19; // include/linux/pxa2xx_ssp.h:137
    pub const RSRE: u32 = 1 << 20; // include/linux/pxa2xx_ssp.h:138
    pub const TSRE: u32 = 1 << 21; // include/linux/pxa2xx_ssp.h:139
    pub const TRAIL: u32 = 1 << 22; // include/linux/pxa2xx_ssp.h:140
}

/// SSSR status bits consumed by the transfer-state decoder.
pub mod sssr {
    pub const RNE: u32 = 1 << 3; // include/linux/pxa2xx_ssp.h:79
    pub const BSY: u32 = 1 << 4; // include/linux/pxa2xx_ssp.h:80
    pub const TFS: u32 = 1 << 5; // include/linux/pxa2xx_ssp.h:81
    pub const RFS: u32 = 1 << 6; // include/linux/pxa2xx_ssp.h:82
    pub const ROR: u32 = 1 << 7; // include/linux/pxa2xx_ssp.h:83
    pub const TFL_MASK: u32 = 0x0f << 8; // include/linux/pxa2xx_ssp.h:88
    pub const RFL_MASK: u32 = 0x0f << 12; // include/linux/pxa2xx_ssp.h:89
    pub const TINT: u32 = 1 << 19; // include/linux/pxa2xx_ssp.h:152
    pub const TUR: u32 = 1 << 21; // include/linux/pxa2xx_ssp.h:154
}

/// LPSS private-register constants for Cherry Trail's `LPSS_BSW_SSP` entry.
pub mod lpss {
    pub const GENERAL_RXTO_HOLDOFF_DISABLE: u32 = 1 << 24; // spi-pxa2xx.c:69
    pub const CS_CONTROL_SW_MODE: u32 = 1 << 0; // spi-pxa2xx.c:70
    pub const CS_CONTROL_CS_HIGH: u32 = 1 << 1; // spi-pxa2xx.c:71
    pub const BSW_PRIVATE_OFFSET: u32 = 0x400; // spi-pxa2xx.c:122
    pub const BSW_GENERAL: u32 = 0x08; // spi-pxa2xx.c:123
    pub const BSW_SSP: u32 = 0x0c; // spi-pxa2xx.c:124
    pub const BSW_CS_CONTROL: u32 = 0x18; // spi-pxa2xx.c:125
    pub const BSW_CS_SELECT_SHIFT: u32 = 2; // spi-pxa2xx.c:130
    pub const BSW_CS_SELECT_MASK: u32 = 1 << 2; // spi-pxa2xx.c:131
}

/// Default interrupt/DMA masks selected for non-PXA25x-compatible LPSS.
pub mod transfer {
    pub const INT_CR1: u32 = super::sscr1::TIE | super::sscr1::RIE | super::sscr1::TINTE; // spi-pxa2xx.c:1327
    pub const DMA_CR1: u32 = super::sscr1::TSRE | super::sscr1::RSRE | super::sscr1::TRAIL; // spi-pxa2xx.h:125
    pub const CLEAR_SR: u32 = super::sssr::ROR | super::sssr::TINT; // spi-pxa2xx.c:1329
    pub const MASK_SR: u32 = super::sssr::TINT
        | super::sssr::RFS
        | super::sssr::TFS
        | super::sssr::ROR
        | super::sssr::TUR; // spi-pxa2xx.c:1330-1331
}
