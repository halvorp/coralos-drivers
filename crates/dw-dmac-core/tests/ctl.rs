// SPDX-License-Identifier: GPL-2.0-only
//! CTL word vectors from Linux `drivers/dma/dw/regs.h` and `core.c`.
//!
//! Original copyright holders: Atmel Corporation, ST Microelectronics, Intel
//! Corporation, Haavard Skinnemoen, and Viresh Kumar.

use dw_dmac_core::ctl::*;

const LINUX_BURST_NAMES: [&str; 8] = [
    "DW_DMA_MSIZE_1",
    "DW_DMA_MSIZE_4",
    "DW_DMA_MSIZE_8",
    "DW_DMA_MSIZE_16",
    "DW_DMA_MSIZE_32",
    "DW_DMA_MSIZE_64",
    "DW_DMA_MSIZE_128",
    "DW_DMA_MSIZE_256",
]; // regs.h:133-140

const LINUX_FLOW_NAMES: [&str; 8] = [
    "DW_DMA_FC_D_M2M",
    "DW_DMA_FC_D_M2P",
    "DW_DMA_FC_D_P2M",
    "DW_DMA_FC_D_P2P",
    "DW_DMA_FC_P_P2M",
    "DW_DMA_FC_SP_P2P",
    "DW_DMA_FC_P_M2P",
    "DW_DMA_FC_DP_P2P",
]; // regs.h:22-29

const LINUX_ADDRESS_MODE_NAMES: [&str; 6] = [
    "DWC_CTLL_DST_INC",
    "DWC_CTLL_DST_DEC",
    "DWC_CTLL_DST_FIX",
    "DWC_CTLL_SRC_INC",
    "DWC_CTLL_SRC_DEC",
    "DWC_CTLL_SRC_FIX",
]; // regs.h:151-156

#[test]
fn linux_enum_counts_and_names_are_frozen() {
    assert_eq!(BURST_SIZE_NAMES.len(), 8); // regs.h:132-141
    assert_eq!(BURST_SIZE_NAMES, LINUX_BURST_NAMES);
    assert_eq!(FLOW_CONTROL_NAMES.len(), 8); // regs.h:21-30
    assert_eq!(FLOW_CONTROL_NAMES, LINUX_FLOW_NAMES);
    assert_eq!(ADDRESS_MODE_NAMES.len(), 6); // regs.h:151-156
    assert_eq!(ADDRESS_MODE_NAMES, LINUX_ADDRESS_MODE_NAMES);
}

#[test]
fn every_burst_encoding_matches_linux_literals() {
    let vectors = [
        (1, BurstSize::Msize1, 0),
        (4, BurstSize::Msize4, 1),
        (8, BurstSize::Msize8, 2),
        (16, BurstSize::Msize16, 3),
        (32, BurstSize::Msize32, 4),
        (64, BurstSize::Msize64, 5),
        (128, BurstSize::Msize128, 6),
        (256, BurstSize::Msize256, 7),
    ]; // regs.h:133-140
    for (elements, encoding, literal) in vectors {
        assert_eq!(BurstSize::from_elements(elements), Ok(encoding));
        assert_eq!(encoding as u8, literal);
        assert_eq!(encoding.elements(), elements);
    }
    assert_eq!(
        BurstSize::from_elements(2),
        Err(EncodeError::UnsupportedBurstSize {
            elements: 2,
            maximum_elements: 256
        })
    );
}

#[test]
fn every_width_encoding_is_log2_bytes() {
    for (bytes, width, literal) in [
        (1, TransferWidth::Bits8, 0),
        (2, TransferWidth::Bits16, 1),
        (4, TransferWidth::Bits32, 2),
        (8, TransferWidth::Bits64, 3),
        (16, TransferWidth::Bits128, 4),
        (32, TransferWidth::Bits256, 5),
    ] {
        // core.c:566 and regs.h:149-150
        assert_eq!(TransferWidth::from_bytes(bytes), Ok(width));
        assert_eq!(width as u8, literal);
        assert_eq!(width.bytes(), bytes);
    }
    assert_eq!(
        TransferWidth::from_bytes(3),
        Err(EncodeError::UnsupportedTransferWidth {
            bytes: 3,
            maximum_bytes: 32
        })
    );
}

#[test]
fn every_ctl_field_constant_matches_its_linux_literal() {
    assert_eq!(INT_EN, 0x0000_0001); // regs.h:148, 1 << 0
    assert_eq!(DST_WIDTH_SHIFT, 1); // regs.h:149, n << 1
    assert_eq!(SRC_WIDTH_SHIFT, 4); // regs.h:150, n << 4
    assert_eq!(DST_ADDR_MODE_SHIFT, 7); // regs.h:151-153, values << 7
    assert_eq!(SRC_ADDR_MODE_SHIFT, 9); // regs.h:154-156, values << 9
    assert_eq!(DST_MSIZE_SHIFT, 11); // regs.h:157, n << 11
    assert_eq!(SRC_MSIZE_SHIFT, 14); // regs.h:158, n << 14
    assert_eq!(SRC_GATHER_EN, 0x0002_0000); // regs.h:159, 1 << 17
    assert_eq!(DST_SCATTER_EN, 0x0004_0000); // regs.h:160, 1 << 18
    assert_eq!(FLOW_CONTROL_SHIFT, 20); // regs.h:161, n << 20
    assert_eq!(DST_MASTER_SHIFT, 23); // regs.h:167, n << 23
    assert_eq!(SRC_MASTER_SHIFT, 25); // regs.h:168, n << 25
    assert_eq!(LLP_DST_EN, 0x0800_0000); // regs.h:169, 1 << 27
    assert_eq!(LLP_SRC_EN, 0x1000_0000); // regs.h:170, 1 << 28
    assert_eq!(BLOCK_TS_MASK, 0x0000_0fff); // regs.h:173, GENMASK(11, 0)
    assert_eq!(DONE, 0x0000_1000); // regs.h:175, 1 << 12
}

fn empty_control_low() -> ControlLow {
    ControlLow {
        interrupt_enabled: false,
        destination_width: TransferWidth::Bits8,
        source_width: TransferWidth::Bits8,
        destination_mode: AddressMode::Increment,
        source_mode: AddressMode::Increment,
        destination_burst: BurstSize::Msize1,
        source_burst: BurstSize::Msize1,
        source_gather: false,
        destination_scatter: false,
        flow_control: FlowControl::DmacMemoryToMemory,
        destination_master: 0,
        source_master: 0,
        destination_llp: false,
        source_llp: false,
    }
}

#[test]
fn every_ctl_low_field_has_an_isolated_linux_vector() {
    let base = empty_control_low();
    assert_eq!(encode_ctl_lo(base), Ok(0x0000_0000)); // regs.h:148-170

    let vectors = [
        (
            ControlLow {
                interrupt_enabled: true,
                ..base
            },
            0x0000_0001,
        ), // regs.h:148
        (
            ControlLow {
                destination_width: TransferWidth::Bits16,
                ..base
            },
            0x0000_0002,
        ), // regs.h:149
        (
            ControlLow {
                source_width: TransferWidth::Bits16,
                ..base
            },
            0x0000_0010,
        ), // regs.h:150
        (
            ControlLow {
                destination_mode: AddressMode::Decrement,
                ..base
            },
            0x0000_0080,
        ), // regs.h:152
        (
            ControlLow {
                source_mode: AddressMode::Decrement,
                ..base
            },
            0x0000_0200,
        ), // regs.h:155
        (
            ControlLow {
                destination_burst: BurstSize::Msize4,
                ..base
            },
            0x0000_0800,
        ), // regs.h:157
        (
            ControlLow {
                source_burst: BurstSize::Msize4,
                ..base
            },
            0x0000_4000,
        ), // regs.h:158
        (
            ControlLow {
                source_gather: true,
                ..base
            },
            0x0002_0000,
        ), // regs.h:159
        (
            ControlLow {
                destination_scatter: true,
                ..base
            },
            0x0004_0000,
        ), // regs.h:160
        (
            ControlLow {
                flow_control: FlowControl::DmacMemoryToPeripheral,
                ..base
            },
            0x0010_0000,
        ), // regs.h:161-163
        (
            ControlLow {
                destination_master: 1,
                ..base
            },
            0x0080_0000,
        ), // regs.h:167
        (
            ControlLow {
                source_master: 1,
                ..base
            },
            0x0200_0000,
        ), // regs.h:168
        (
            ControlLow {
                destination_llp: true,
                ..base
            },
            0x0800_0000,
        ), // regs.h:169
        (
            ControlLow {
                source_llp: true,
                ..base
            },
            0x1000_0000,
        ), // regs.h:170
    ];

    for (control, linux_literal) in vectors {
        assert_eq!(encode_ctl_lo(control), Ok(linux_literal));
    }
}

#[test]
fn ctl_low_all_fields_land_at_linux_positions() {
    let all = ControlLow {
        interrupt_enabled: true,
        destination_width: TransferWidth::Bits32,
        source_width: TransferWidth::Bits64,
        destination_mode: AddressMode::Decrement,
        source_mode: AddressMode::Fixed,
        destination_burst: BurstSize::Msize16,
        source_burst: BurstSize::Msize32,
        source_gather: true,
        destination_scatter: true,
        flow_control: FlowControl::DestinationPeripheralToPeripheral,
        destination_master: 2,
        source_master: 3,
        destination_llp: true,
        source_llp: true,
    };
    assert_eq!(encode_ctl_lo(all), Ok(0x1f77_1cb5)); // regs.h:148-170 literal shifts

    let memcpy = ControlLow {
        interrupt_enabled: false,
        destination_width: TransferWidth::Bits32,
        source_width: TransferWidth::Bits32,
        destination_mode: AddressMode::Increment,
        source_mode: AddressMode::Increment,
        destination_burst: BurstSize::Msize1,
        source_burst: BurstSize::Msize1,
        source_gather: false,
        destination_scatter: false,
        flow_control: FlowControl::DmacMemoryToMemory,
        destination_master: 0,
        source_master: 0,
        destination_llp: false,
        source_llp: false,
    };
    assert_eq!(encode_ctl_lo(memcpy), Ok(0x0000_0024)); // core.c:569-573
}

#[test]
fn ctl_low_refuses_master_values_that_escape_two_bits() {
    let c = ControlLow {
        interrupt_enabled: false,
        destination_width: TransferWidth::Bits8,
        source_width: TransferWidth::Bits8,
        destination_mode: AddressMode::Increment,
        source_mode: AddressMode::Increment,
        destination_burst: BurstSize::Msize1,
        source_burst: BurstSize::Msize1,
        source_gather: false,
        destination_scatter: false,
        flow_control: FlowControl::DmacMemoryToMemory,
        destination_master: 4,
        source_master: 0,
        destination_llp: false,
        source_llp: false,
    };
    assert_eq!(
        encode_ctl_lo(c),
        Err(EncodeError::MasterOutOfRange {
            master: 4,
            maximum_master: 3
        })
    ); // regs.h:167, DMS is below SMS by two bits
}

#[test]
fn ctl_high_masks_transfer_size_and_decodes_done() {
    assert_eq!(BLOCK_TS_MASK, 0x0fff); // regs.h:173, GENMASK(11, 0)
    assert_eq!(DONE, 0x1000); // regs.h:175, 1 << 12
    assert_eq!(encode_ctl_hi(0xabc, false), Ok(0x0abc));
    assert_eq!(encode_ctl_hi(0xabc, true), Ok(0x1abc));
    assert_eq!(block_transfers(0xffff_fabc), 0xabc);
    assert!(is_done(0x1000));
    assert!(!is_done(0x0fff));
    assert_eq!(
        encode_ctl_hi(0x1000, false),
        Err(EncodeError::BlockTransferSizeOutOfRange {
            transfers: 0x1000,
            maximum_transfers: 0x0fff,
        })
    );
}
