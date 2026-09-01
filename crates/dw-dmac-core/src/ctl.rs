// SPDX-License-Identifier: GPL-2.0-only
//! CTL_LO and CTL_HI fields, ported from Linux `drivers/dma/dw/regs.h` and
//! their use in `drivers/dma/dw/core.c`.
//!
//! Original copyright holders: Atmel Corporation, ST Microelectronics, Intel
//! Corporation, Haavard Skinnemoen, and Viresh Kumar.

/// CTL_LO interrupt-enable bit.
pub const INT_EN: u32 = 1 << 0; // regs.h:148
/// CTL_LO destination transfer-width field shift.
pub const DST_WIDTH_SHIFT: u32 = 1; // regs.h:149
/// CTL_LO source transfer-width field shift.
pub const SRC_WIDTH_SHIFT: u32 = 4; // regs.h:150
/// CTL_LO destination address-mode field shift.
pub const DST_ADDR_MODE_SHIFT: u32 = 7; // regs.h:151-153
/// CTL_LO source address-mode field shift.
pub const SRC_ADDR_MODE_SHIFT: u32 = 9; // regs.h:154-156
/// CTL_LO destination burst-size field shift.
pub const DST_MSIZE_SHIFT: u32 = 11; // regs.h:157
/// CTL_LO source burst-size field shift.
pub const SRC_MSIZE_SHIFT: u32 = 14; // regs.h:158
/// CTL_LO source gather enable.
pub const SRC_GATHER_EN: u32 = 1 << 17; // regs.h:159
/// CTL_LO destination scatter enable.
pub const DST_SCATTER_EN: u32 = 1 << 18; // regs.h:160
/// CTL_LO flow-controller field shift.
pub const FLOW_CONTROL_SHIFT: u32 = 20; // regs.h:161
/// CTL_LO destination-master field shift.
pub const DST_MASTER_SHIFT: u32 = 23; // regs.h:167
/// CTL_LO source-master field shift.
pub const SRC_MASTER_SHIFT: u32 = 25; // regs.h:168
/// CTL_LO destination linked-list enable.
pub const LLP_DST_EN: u32 = 1 << 27; // regs.h:169
/// CTL_LO source linked-list enable.
pub const LLP_SRC_EN: u32 = 1 << 28; // regs.h:170
/// CTL_HI transfer-size field mask.
pub const BLOCK_TS_MASK: u32 = 0x0fff; // regs.h:173
/// CTL_HI descriptor-done bit.
pub const DONE: u32 = 1 << 12; // regs.h:175

/// The three-bit transfer-width encoding used by CTL_LO.
///
/// Linux obtains this value with `__ffs(width)` (`core.c:566`, `core.c:644`,
/// `core.c:694`), so the encodings are log2(bytes).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferWidth {
    Bits8 = 0,
    Bits16 = 1,
    Bits32 = 2,
    Bits64 = 3,
    Bits128 = 4,
    Bits256 = 5,
}

impl TransferWidth {
    /// Encode a byte width as Linux's `__ffs(width)` value.
    pub const fn from_bytes(bytes: u32) -> Result<Self, EncodeError> {
        match bytes {
            1 => Ok(Self::Bits8),
            2 => Ok(Self::Bits16),
            4 => Ok(Self::Bits32),
            8 => Ok(Self::Bits64),
            16 => Ok(Self::Bits128),
            32 => Ok(Self::Bits256),
            _ => Err(EncodeError::UnsupportedTransferWidth {
                bytes,
                maximum_bytes: 32,
            }),
        }
    }

    /// Return the byte width represented by this CTL_LO encoding.
    pub const fn bytes(self) -> u32 {
        1 << self as u8
    }
}

/// Linux's eight `enum dw_dma_msize` burst encodings (`regs.h:132-141`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurstSize {
    Msize1 = 0,
    Msize4 = 1,
    Msize8 = 2,
    Msize16 = 3,
    Msize32 = 4,
    Msize64 = 5,
    Msize128 = 6,
    Msize256 = 7,
}

/// Names of every Linux `dw_dma_msize` member, in declaration order.
pub const BURST_SIZE_NAMES: [&str; 8] = [
    "DW_DMA_MSIZE_1",   // regs.h:133
    "DW_DMA_MSIZE_4",   // regs.h:134
    "DW_DMA_MSIZE_8",   // regs.h:135
    "DW_DMA_MSIZE_16",  // regs.h:136
    "DW_DMA_MSIZE_32",  // regs.h:137
    "DW_DMA_MSIZE_64",  // regs.h:138
    "DW_DMA_MSIZE_128", // regs.h:139
    "DW_DMA_MSIZE_256", // regs.h:140
];

impl BurstSize {
    /// Encode an element count as Linux's `enum dw_dma_msize` value.
    pub const fn from_elements(elements: u16) -> Result<Self, EncodeError> {
        match elements {
            1 => Ok(Self::Msize1),
            4 => Ok(Self::Msize4),
            8 => Ok(Self::Msize8),
            16 => Ok(Self::Msize16),
            32 => Ok(Self::Msize32),
            64 => Ok(Self::Msize64),
            128 => Ok(Self::Msize128),
            256 => Ok(Self::Msize256),
            _ => Err(EncodeError::UnsupportedBurstSize {
                elements,
                maximum_elements: 256,
            }),
        }
    }

    /// Return the element count represented by this CTL_LO encoding.
    pub const fn elements(self) -> u16 {
        match self {
            Self::Msize1 => 1,
            Self::Msize4 => 4,
            Self::Msize8 => 8,
            Self::Msize16 => 16,
            Self::Msize32 => 32,
            Self::Msize64 => 64,
            Self::Msize128 => 128,
            Self::Msize256 => 256,
        }
    }
}

/// Address update modes from the CTL_LO literals (`regs.h:151-156`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    Increment = 0,
    Decrement = 1,
    Fixed = 2,
}

/// Names of every address-mode value Linux defines, with destination names first.
pub const ADDRESS_MODE_NAMES: [&str; 6] = [
    "DWC_CTLL_DST_INC", // regs.h:151
    "DWC_CTLL_DST_DEC", // regs.h:152
    "DWC_CTLL_DST_FIX", // regs.h:153
    "DWC_CTLL_SRC_INC", // regs.h:154
    "DWC_CTLL_SRC_DEC", // regs.h:155
    "DWC_CTLL_SRC_FIX", // regs.h:156
];

/// The eight flow-controller encodings in Linux's `enum dw_dma_fc` (`regs.h:21-30`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowControl {
    DmacMemoryToMemory = 0,
    DmacMemoryToPeripheral = 1,
    DmacPeripheralToMemory = 2,
    DmacPeripheralToPeripheral = 3,
    PeripheralPeripheralToMemory = 4,
    SourcePeripheralToPeripheral = 5,
    PeripheralMemoryToPeripheral = 6,
    DestinationPeripheralToPeripheral = 7,
}

/// Names of every Linux `dw_dma_fc` member, in declaration order.
pub const FLOW_CONTROL_NAMES: [&str; 8] = [
    "DW_DMA_FC_D_M2M",  // regs.h:22
    "DW_DMA_FC_D_M2P",  // regs.h:23
    "DW_DMA_FC_D_P2M",  // regs.h:24
    "DW_DMA_FC_D_P2P",  // regs.h:25
    "DW_DMA_FC_P_P2M",  // regs.h:26
    "DW_DMA_FC_SP_P2P", // regs.h:27
    "DW_DMA_FC_P_M2P",  // regs.h:28
    "DW_DMA_FC_DP_P2P", // regs.h:29
];

/// A named refusal from CTL word construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    UnsupportedTransferWidth {
        bytes: u32,
        maximum_bytes: u32,
    },
    UnsupportedBurstSize {
        elements: u16,
        maximum_elements: u16,
    },
    MasterOutOfRange {
        master: u8,
        maximum_master: u8,
    },
    BlockTransferSizeOutOfRange {
        transfers: u32,
        maximum_transfers: u32,
    },
}

/// Inputs to one CTL_LO word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlLow {
    pub interrupt_enabled: bool,
    pub destination_width: TransferWidth,
    pub source_width: TransferWidth,
    pub destination_mode: AddressMode,
    pub source_mode: AddressMode,
    pub destination_burst: BurstSize,
    pub source_burst: BurstSize,
    pub source_gather: bool,
    pub destination_scatter: bool,
    pub flow_control: FlowControl,
    pub destination_master: u8,
    pub source_master: u8,
    pub destination_llp: bool,
    pub source_llp: bool,
}

/// Build CTL_LO using Linux's shifts in `regs.h:148-170`.
pub const fn encode_ctl_lo(control: ControlLow) -> Result<u32, EncodeError> {
    if control.destination_master > 3 {
        return Err(EncodeError::MasterOutOfRange {
            master: control.destination_master,
            maximum_master: 3,
        });
    }
    if control.source_master > 3 {
        return Err(EncodeError::MasterOutOfRange {
            master: control.source_master,
            maximum_master: 3,
        });
    }

    let mut word = (control.destination_width as u32) << DST_WIDTH_SHIFT
        | (control.source_width as u32) << SRC_WIDTH_SHIFT
        | (control.destination_mode as u32) << DST_ADDR_MODE_SHIFT
        | (control.source_mode as u32) << SRC_ADDR_MODE_SHIFT
        | (control.destination_burst as u32) << DST_MSIZE_SHIFT
        | (control.source_burst as u32) << SRC_MSIZE_SHIFT
        | (control.flow_control as u32) << FLOW_CONTROL_SHIFT
        | (control.destination_master as u32) << DST_MASTER_SHIFT
        | (control.source_master as u32) << SRC_MASTER_SHIFT;
    if control.interrupt_enabled {
        word |= INT_EN;
    }
    if control.source_gather {
        word |= SRC_GATHER_EN;
    }
    if control.destination_scatter {
        word |= DST_SCATTER_EN;
    }
    if control.destination_llp {
        word |= LLP_DST_EN;
    }
    if control.source_llp {
        word |= LLP_SRC_EN;
    }
    Ok(word)
}

/// Build CTL_HI's transfer-size field and optional DONE bit (`regs.h:173-175`).
///
/// The argument is the already encoded BLOCK_TS value, matching Linux's
/// `DWC_CTLH_BLOCK_TS(x)` macro rather than inventing a minus-one policy.
pub const fn encode_ctl_hi(block_transfers: u32, done: bool) -> Result<u32, EncodeError> {
    if block_transfers > BLOCK_TS_MASK {
        return Err(EncodeError::BlockTransferSizeOutOfRange {
            transfers: block_transfers,
            maximum_transfers: BLOCK_TS_MASK,
        });
    }
    Ok(block_transfers | if done { DONE } else { 0 })
}

/// Decode CTL_HI's BLOCK_TS field (`regs.h:173-174`).
pub const fn block_transfers(ctl_hi: u32) -> u32 {
    ctl_hi & BLOCK_TS_MASK
}

/// Decode CTL_HI's DONE bit (`regs.h:175`).
pub const fn is_done(ctl_hi: u32) -> bool {
    ctl_hi & DONE != 0
}
