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
