// SPDX-License-Identifier: GPL-2.0-only
//! Transfer-state vectors from `drivers/spi/spi-pxa2xx.c:630-705,974-986`.
//!
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

use spi_pxa2xx_core::regs::{sssr, transfer};
use spi_pxa2xx_core::state::{
    decode_transfer_state, final_rx_threshold, transfer_width, TransferState, TransferWidth,
    WidthError,
};

#[test]
fn transfer_width_boundaries_match_linux_branches() {
    // spi-pxa2xx.c:974-985; LPSS advertised range is 4..32 at :1326.
    assert_eq!(transfer_width(4), Ok(TransferWidth::U8));
    assert_eq!(transfer_width(8), Ok(TransferWidth::U8));
    assert_eq!(transfer_width(9), Ok(TransferWidth::U16));
    assert_eq!(transfer_width(16), Ok(TransferWidth::U16));
    assert_eq!(transfer_width(17), Ok(TransferWidth::U32));
    assert_eq!(transfer_width(32), Ok(TransferWidth::U32));
    assert_eq!(TransferWidth::U8.bytes(), 1);
    assert_eq!(TransferWidth::U16.bytes(), 2);
    assert_eq!(TransferWidth::U32.bytes(), 4);
}

#[test]
fn transfer_width_refusals_name_value_and_bound() {
    assert_eq!(
        transfer_width(3),
        Err(WidthError::BitsPerWordBelowMinimum {
            bits: 3,
            minimum: 4
        })
    );
    assert_eq!(
        transfer_width(33),
        Err(WidthError::BitsPerWordAboveMaximum {
            bits: 33,
            maximum: 32
        })
    );
}

#[test]
fn state_decode_matches_linux_precedence() {
    let mask = transfer::MASK_SR;
    assert_eq!(decode_transfer_state(0, mask, true), TransferState::Idle);
    assert_eq!(
        decode_transfer_state(sssr::RFS, mask, true),
        TransferState::Service
    );
    assert_eq!(
        decode_transfer_state(sssr::TFS, mask, true),
        TransferState::Service
    );
    assert_eq!(
        decode_transfer_state(sssr::TINT, mask, true),
        TransferState::ReceiverTimeout
    );
    assert_eq!(
        decode_transfer_state(sssr::TUR, mask, true),
        TransferState::FifoUnderrun
    );
    assert_eq!(
        decode_transfer_state(sssr::ROR, mask, true),
        TransferState::FifoOverrun
    );

    // spi-pxa2xx.c:638-648: ROR wins over TUR, which wins over TINT/service.
    assert_eq!(
        decode_transfer_state(sssr::ROR | sssr::TUR | sssr::TINT | sssr::RFS, mask, true),
        TransferState::FifoOverrun
    );
    assert_eq!(
        decode_transfer_state(sssr::TUR | sssr::TINT | sssr::RFS, mask, true),
        TransferState::FifoUnderrun
    );
}

#[test]
fn disabled_or_masked_transmit_service_is_idle() {
    // spi-pxa2xx.c:634-636 removes TFS when SSCR1_TIE is clear.
    assert_eq!(
        decode_transfer_state(sssr::TFS, transfer::MASK_SR, false),
        TransferState::Idle
    );
    // Status outside the caller's mask does not become transfer work.
    assert_eq!(
        decode_transfer_state(sssr::ROR, sssr::RFS, true),
        TransferState::Idle
    );
}

#[test]
fn final_rx_threshold_counts_words_then_caps_default() {
    // spi-pxa2xx.c:685-697: bytes are shifted by storage width, then capped.
    assert_eq!(final_rx_threshold(7, TransferWidth::U8, 8), 7);
    assert_eq!(final_rx_threshold(16, TransferWidth::U8, 8), 8);
    assert_eq!(final_rx_threshold(6, TransferWidth::U16, 8), 3);
    assert_eq!(final_rx_threshold(12, TransferWidth::U32, 8), 3);
}
