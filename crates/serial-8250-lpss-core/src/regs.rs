// SPDX-License-Identifier: GPL-2.0-only
//! Register indices, LPSS byte offsets and fields from Linux `8250_dw.c`, `8250_lpss.c`,
//! `8250_dwlib.c`, and `include/uapi/linux/serial_reg.h`.
//!
//! Copyright 2011 Picochip, Jamie Iles; Copyright 2013, 2016 Intel Corporation; copyright
//! 1992, 1994 Theodore Ts'o.

/// 8250 register indices. LPSS uses `regshift = 2` (`8250_lpss.c:340`).
pub mod index {
    pub const RX: u32 = 0; // serial_reg.h:19
    pub const TX: u32 = 0; // serial_reg.h:20
    pub const IER: u32 = 1; // serial_reg.h:22
    pub const IIR: u32 = 2; // serial_reg.h:31
    pub const FCR: u32 = 2; // serial_reg.h:50
    pub const LCR: u32 = 3; // serial_reg.h:105
    pub const MCR: u32 = 4; // serial_reg.h:128
    pub const LSR: u32 = 5; // serial_reg.h:141
    pub const MSR: u32 = 6; // serial_reg.h:152
    pub const USR: u32 = 0x1f; // 8250_dw.c:38
}

/// DesignWare extension register byte offsets (not shifted 8250 indices).
pub mod dw {
    pub const DLF: u32 = 0xc0; // 8250_dwlib.c:23
}

/// Intel LPSS private register byte offsets.
pub mod lpss {
    pub const PRV_CLK: u32 = 0x800; // 8250_lpss.c:39
    pub const TX_OVF_INT: u32 = 0x820; // 8250_lpss.c:45
}

/// Register fields and literal encodings.
pub mod bits {
    pub const IIR_IID_MASK: u32 = 0x0f; // 8250_dw.c:47  GENMASK(3, 0)
    pub const IIR_RX_TIMEOUT_MASK: u32 = 0x3f; // 8250_dw.c:426
    pub const IIR_BUSY: u32 = 0x07; // serial_reg.h:42, used at 8250_dw.c:444
    pub const IIR_RX_TIMEOUT: u32 = 0x0c; // serial_reg.h:44, used at 8250_dw.c:426
    pub const USR_BUSY: u32 = 1 << 0; // 8250_dw.c:51

    pub const LCR_SPAR: u32 = 0x20; // serial_reg.h:112, used at 8250_dw.c:253
    pub const LCR_DLAB: u32 = 0x80; // serial_reg.h:108, used at 8250_dw.c:224

    pub const FCR_ENABLE_FIFO: u8 = 0x01; // serial_reg.h:51
    pub const FCR_CLEAR_RCVR: u8 = 0x02; // serial_reg.h:52
    pub const FCR_CLEAR_XMIT: u8 = 0x04; // serial_reg.h:53
    pub const FCR_DMA_SELECT: u8 = 0x08; // serial_reg.h:54
    pub const FCR_TRIGGER_1: u8 = 0x00; // serial_reg.h:84
    pub const FCR_TRIGGER_4: u8 = 0x40; // serial_reg.h:85
    pub const FCR_TRIGGER_8: u8 = 0x80; // serial_reg.h:86
    pub const FCR_TRIGGER_14: u8 = 0xc0; // serial_reg.h:87
    pub const FCR_TRIGGER_MASK: u8 = 0xc0; // serial_reg.h:83

    pub const MCR_AFE: u8 = 0x20; // serial_reg.h:132; UART_CAP_AFE at 8250_lpss.c:343

    pub const PRV_CLK_EN: u32 = 1 << 0; // 8250_lpss.c:40
    pub const PRV_CLK_M_VAL_SHIFT: u32 = 1; // 8250_lpss.c:41
    pub const PRV_CLK_N_VAL_SHIFT: u32 = 16; // 8250_lpss.c:42
    pub const PRV_CLK_UPDATE: u32 = 1 << 31; // 8250_lpss.c:43
    pub const TX_OVF_INT_MASK: u32 = 1 << 1; // 8250_lpss.c:46
}
