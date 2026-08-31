// SPDX-License-Identifier: GPL-2.0-only
//! Per-channel status decoding, ported from Linux `drivers/dma/dw/core.c`.
//!
//! Original copyright holders: Atmel Corporation, ST Microelectronics, Intel
//! Corporation, Haavard Skinnemoen, and Viresh Kumar.

/// The eight channels covered by Linux's recovery mask `(1 << 8) - 1`.
pub const MAX_CHANNELS: u8 = 8; // core.c:525

/// The action Linux chooses for one channel in `dw_dma_tasklet` (`core.c:477-484`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelStatus {
    /// `DW_DMA_IS_CYCLIC` is checked first (`core.c:479-480`).
    CyclicUnsupported,
    /// RAW.ERROR wins over RAW.XFER (`core.c:481-482`).
    Error,
    /// A transfer-complete bit with no error (`core.c:483-484`).
    TransferComplete,
    /// Neither status word names this channel.
    Idle,
}

/// Decode RAW.XFER and RAW.ERROR for one channel in Linux's precedence order.
pub const fn decode_channel(
    status_xfer: u32,
    status_error: u32,
    channel: u8,
    cyclic: bool,
) -> Result<ChannelStatus, StatusError> {
    if channel >= MAX_CHANNELS {
        return Err(StatusError::ChannelOutOfRange {
            channel,
            maximum_channel: MAX_CHANNELS - 1,
        });
    }
    let mask = 1u32 << channel;
    if cyclic {
        Ok(ChannelStatus::CyclicUnsupported)
    } else if status_error & mask != 0 {
        Ok(ChannelStatus::Error)
    } else if status_xfer & mask != 0 {
        Ok(ChannelStatus::TransferComplete)
    } else {
        Ok(ChannelStatus::Idle)
    }
}

/// Return the channel mask Linux computes as `(1 << nr_channels) - 1` (`core.c:1207`).
pub const fn all_channel_mask(nr_channels: u8) -> Result<u32, StatusError> {
    if nr_channels == 0 {
        return Err(StatusError::ChannelCountOutOfRange {
            channels: nr_channels,
            minimum_channels: 1,
            maximum_channels: MAX_CHANNELS,
        });
    }
    if nr_channels > MAX_CHANNELS {
        return Err(StatusError::ChannelCountOutOfRange {
            channels: nr_channels,
            minimum_channels: 1,
            maximum_channels: MAX_CHANNELS,
        });
    }
    Ok((1u32 << nr_channels) - 1)
}

/// A named refusal from channel-status decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusError {
    ChannelOutOfRange {
        channel: u8,
        maximum_channel: u8,
    },
    ChannelCountOutOfRange {
        channels: u8,
        minimum_channels: u8,
        maximum_channels: u8,
    },
}
