// SPDX-License-Identifier: GPL-2.0-only
//! Register offsets, bits and masks — extracted from i2c-designware-core.h.
//! Line comments are that file's line numbers.

/// Register offsets, all relative to the controller's MMIO base.
pub mod off {
    pub const CON: u32 = 0x00; // :61
    pub const TAR: u32 = 0x04; // :62
    pub const SAR: u32 = 0x08; // :63
    pub const DATA_CMD: u32 = 0x10; // :64
    pub const SS_SCL_HCNT: u32 = 0x14; // :65
    pub const SS_SCL_LCNT: u32 = 0x18; // :66
    pub const FS_SCL_HCNT: u32 = 0x1c; // :67
    pub const FS_SCL_LCNT: u32 = 0x20; // :68
    pub const HS_SCL_HCNT: u32 = 0x24; // :69
    pub const HS_SCL_LCNT: u32 = 0x28; // :70
    pub const INTR_STAT: u32 = 0x2c; // :71
    pub const INTR_MASK: u32 = 0x30; // :72
    pub const RAW_INTR_STAT: u32 = 0x34; // :73
    pub const RX_TL: u32 = 0x38; // :74
    pub const TX_TL: u32 = 0x3c; // :75
    pub const CLR_INTR: u32 = 0x40; // :76
    pub const CLR_RX_UNDER: u32 = 0x44; // :77
    pub const CLR_RX_OVER: u32 = 0x48; // :78
    pub const CLR_TX_OVER: u32 = 0x4c; // :79
    pub const CLR_RD_REQ: u32 = 0x50; // :80
    pub const CLR_TX_ABRT: u32 = 0x54; // :81
    pub const CLR_RX_DONE: u32 = 0x58; // :82
    pub const CLR_ACTIVITY: u32 = 0x5c; // :83
    pub const CLR_STOP_DET: u32 = 0x60; // :84
    pub const CLR_START_DET: u32 = 0x64; // :85
    pub const CLR_GEN_CALL: u32 = 0x68; // :86
    pub const ENABLE: u32 = 0x6c; // :87
    pub const STATUS: u32 = 0x70; // :88
    pub const TXFLR: u32 = 0x74; // :89
    pub const RXFLR: u32 = 0x78; // :90
    pub const SDA_HOLD: u32 = 0x7c; // :91
    pub const TX_ABRT_SOURCE: u32 = 0x80; // :92
    pub const ENABLE_STATUS: u32 = 0x9c; // :93
    pub const CLR_RESTART_DET: u32 = 0xa8; // :94
    pub const SMBUS_INTR_MASK: u32 = 0xcc; // :95
    pub const COMP_PARAM_1: u32 = 0xf4; // :96
    pub const COMP_VERSION: u32 = 0xf8; // :97
    pub const COMP_TYPE: u32 = 0xfc; // :99
    pub const ERR_TX_ABRT: u32 = 0x01; // :139
}

/// Bit definitions and composite masks. A composite is the OR of the bits Linux selects —
/// listed here as its computed value, with the C expression beside it so the two can be compared.
pub mod bits {
    pub const CON_MASTER: u32 = 0x1; // :28  BIT(0)
    pub const CON_SPEED_STD: u32 = 0x2; // :29  (1 << 1)
    pub const CON_SPEED_FAST: u32 = 0x4; // :30  (2 << 1)
    pub const CON_SPEED_HIGH: u32 = 0x6; // :31  (3 << 1)
    pub const CON_SPEED_MASK: u32 = 0x6; // :32  GENMASK(2, 1)
    pub const CON_10BITADDR_SLAVE: u32 = 0x8; // :33  BIT(3)
    pub const CON_10BITADDR_MASTER: u32 = 0x10; // :34  BIT(4)
    pub const CON_RESTART_EN: u32 = 0x20; // :35  BIT(5)
    pub const CON_SLAVE_DISABLE: u32 = 0x40; // :36  BIT(6)
    pub const CON_STOP_DET_IFADDRESSED: u32 = 0x80; // :37  BIT(7)
    pub const CON_TX_EMPTY_CTRL: u32 = 0x100; // :38  BIT(8)
    pub const CON_RX_FIFO_FULL_HLD_CTRL: u32 = 0x200; // :39  BIT(9)
    pub const CON_BUS_CLEAR_CTRL: u32 = 0x800; // :40  BIT(11)
    pub const DATA_CMD_DAT: u32 = 0xff; // :42  GENMASK(7, 0)
    pub const DATA_CMD_FIRST_DATA_BYTE: u32 = 0x800; // :43  BIT(11)

    // THE COMMAND BITS ARE NOT NAMED IN THE HEADER. Linux writes them as bare literals inside
    // i2c_dw_xfer_msg (i2c-designware-master.c): `cmd |= BIT(9)` at :429 for STOP, `cmd |= BIT(10)`
    // at :432 for RESTART, and `cmd | 0x100` at :442 for a read request. They are named here — with
    // the lines that give them meaning — because a driver that ORs a bare BIT(9) into a register is
    // one typo away from a silent wrong transfer, and the reviewer has nothing to check it against.
    pub const DATA_CMD_STOP: u32 = 1 << 9; // master.c:429 — last byte of the last message
    pub const DATA_CMD_RESTART: u32 = 1 << 10; // master.c:432 — a repeated start is needed
    pub const DATA_CMD_READ: u32 = 1 << 8; // master.c:442, written as 0x100 — a read request
    pub const REG_STEP_BYTES: u32 = 0x2; // :48  2
    pub const REG_WORD_SHIFT: u32 = 0x10; // :49  16
    pub const FIFO_TX_FIELD: u32 = 0xff0000; // :54  GENMASK(23, 16)
    pub const FIFO_RX_FIELD: u32 = 0xff00; // :55  GENMASK(15, 8)
    pub const FIFO_MIN_DEPTH: u32 = 0x2; // :56  2
    pub const INTR_RX_UNDER: u32 = 0x1; // :102  BIT(0)
    pub const INTR_RX_OVER: u32 = 0x2; // :103  BIT(1)
    pub const INTR_RX_FULL: u32 = 0x4; // :104  BIT(2)
    pub const INTR_TX_OVER: u32 = 0x8; // :105  BIT(3)
    pub const INTR_TX_EMPTY: u32 = 0x10; // :106  BIT(4)
    pub const INTR_RD_REQ: u32 = 0x20; // :107  BIT(5)
    pub const INTR_TX_ABRT: u32 = 0x40; // :108  BIT(6)
    pub const INTR_RX_DONE: u32 = 0x80; // :109  BIT(7)
    pub const INTR_ACTIVITY: u32 = 0x100; // :110  BIT(8)
    pub const INTR_STOP_DET: u32 = 0x200; // :111  BIT(9)
    pub const INTR_START_DET: u32 = 0x400; // :112  BIT(10)
    pub const INTR_GEN_CALL: u32 = 0x800; // :113  BIT(11)
    pub const INTR_RESTART_DET: u32 = 0x1000; // :114  BIT(12)
    pub const INTR_MST_ON_HOLD: u32 = 0x2000; // :115  BIT(13)
    pub const INTR_DEFAULT_MASK: u32 = 0x244; // :117  (DW_IC_INTR_RX_FULL | 						 DW_IC_INTR_TX_ABRT | 						 DW_IC_INTR_STOP_DET)
    pub const INTR_MASTER_MASK: u32 = 0x254; // :120  (DW_IC_INTR_DEFAULT_MASK | 						 DW_IC_INTR_TX_EMPTY)
    pub const INTR_SLAVE_MASK: u32 = 0x265; // :122  (DW_IC_INTR_DEFAULT_MASK | 						 DW_IC_INTR_RX_UNDER | 						 DW_IC_INTR_RD_REQ)
    pub const ENABLE_ENABLE: u32 = 0x1; // :126  BIT(0)
    pub const ENABLE_ABORT: u32 = 0x2; // :127  BIT(1)
    pub const STATUS_ACTIVITY: u32 = 0x1; // :129  BIT(0)
    pub const STATUS_TFE: u32 = 0x4; // :130  BIT(2)
    pub const STATUS_RFNE: u32 = 0x8; // :131  BIT(3)
    pub const STATUS_MASTER_ACTIVITY: u32 = 0x20; // :132  BIT(5)
    pub const STATUS_SLAVE_ACTIVITY: u32 = 0x40; // :133  BIT(6)
    pub const STATUS_MASTER_HOLD_TX_FIFO_EMPTY: u32 = 0x80; // :134  BIT(7)
    pub const SDA_HOLD_RX_SHIFT: u32 = 0x10; // :136  16
    pub const SDA_HOLD_RX_MASK: u32 = 0xff0000; // :137  GENMASK(23, 16)
    pub const TAR_10BITADDR_MASTER: u32 = 0x1000; // :141  BIT(12)
    pub const COMP_PARAM_1_SPEED_MODE_HIGH: u32 = 0xc; // :143  (BIT(2) | BIT(3))
    pub const COMP_PARAM_1_SPEED_MODE_MASK: u32 = 0xc; // :144  GENMASK(3, 2)
    pub const MASTER: u32 = 0x0; // :157  0
    pub const SLAVE: u32 = 0x1; // :158  1
}
