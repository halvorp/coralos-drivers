// SPDX-License-Identifier: GPL-2.0-only
//! Channel-status vectors from Linux `drivers/dma/dw/core.c`.
//!
//! Original copyright holders: Atmel Corporation, ST Microelectronics, Intel
//! Corporation, Haavard Skinnemoen, and Viresh Kumar.

use dw_dmac_core::status::{
    all_channel_mask, decode_channel, ChannelStatus, StatusError, MAX_CHANNELS,
};

#[test]
fn channel_decode_uses_linux_error_before_transfer_precedence() {
    assert_eq!(decode_channel(0, 0, 3, false), Ok(ChannelStatus::Idle));
    assert_eq!(
        decode_channel(1 << 3, 0, 3, false),
        Ok(ChannelStatus::TransferComplete)
    );
    assert_eq!(
        decode_channel(0, 1 << 3, 3, false),
        Ok(ChannelStatus::Error)
    );
    assert_eq!(
        decode_channel(1 << 3, 1 << 3, 3, false),
        Ok(ChannelStatus::Error)
    );
    // core.c:481 tests ERROR before core.c:483 tests XFER.
    assert_eq!(
        decode_channel(1 << 2, 1 << 2, 3, false),
        Ok(ChannelStatus::Idle)
    );
}

#[test]
fn cyclic_state_wins_before_both_raw_status_words() {
    assert_eq!(
        decode_channel(1 << 7, 1 << 7, 7, true),
        Ok(ChannelStatus::CyclicUnsupported)
    ); // core.c:479-480 precedes ERROR and XFER
}

#[test]
fn channel_index_refusal_names_value_and_bound() {
    assert_eq!(
        decode_channel(0, 0, 8, false),
        Err(StatusError::ChannelOutOfRange {
            channel: 8,
            maximum_channel: 7
        })
    );
}

#[test]
fn all_channel_mask_matches_linux_literal_expression() {
    assert_eq!(all_channel_mask(1), Ok(0x0000_0001));
    assert_eq!(all_channel_mask(4), Ok(0x0000_000f));
    assert_eq!(all_channel_mask(8), Ok(0x0000_00ff));
    assert_eq!(MAX_CHANNELS, 8); // core.c:525, `(1 << 8) - 1`
                                 // core.c:1207, `(1 << pdata->nr_channels) - 1`.
    assert_eq!(
        all_channel_mask(0),
        Err(StatusError::ChannelCountOutOfRange {
            channels: 0,
            minimum_channels: 1,
            maximum_channels: 8,
        })
    );
    assert_eq!(
        all_channel_mask(9),
        Err(StatusError::ChannelCountOutOfRange {
            channels: 9,
            minimum_channels: 1,
            maximum_channels: 8,
        })
    );
}
