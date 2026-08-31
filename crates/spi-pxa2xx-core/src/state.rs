// SPDX-License-Identifier: GPL-2.0-only
//! Transfer-width and interrupt-state decoding.
//!
//! Ported from Linux `drivers/spi/spi-pxa2xx.c:630-705,974-986` and transfer
//! state fields in `drivers/spi/spi-pxa2xx.h:63-71`.
//!
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

use crate::regs::sssr;

/// Storage width Linux selects for a transfer word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferWidth {
    U8,
    U16,
    U32,
}

impl TransferWidth {
    /// Number of bytes advanced by Linux's selected reader/writer.
    pub const fn bytes(self) -> u8 {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }
}

/// Why transfer width selection was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthError {
    BitsPerWordBelowMinimum { bits: u8, minimum: u8 },
    BitsPerWordAboveMaximum { bits: u8, maximum: u8 },
}

/// Select the reader/writer storage width from bits per word.
///
/// Linux chooses one byte through 8 bits, two through 16, and four through 32
/// (`spi-pxa2xx.c:974-985`). The probe limits LPSS to 4..32 bits.
pub fn transfer_width(bits: u8) -> Result<TransferWidth, WidthError> {
    match bits {
        0..=3 => Err(WidthError::BitsPerWordBelowMinimum { bits, minimum: 4 }),
        4..=8 => Ok(TransferWidth::U8),
        9..=16 => Ok(TransferWidth::U16),
        17..=32 => Ok(TransferWidth::U32),
        _ => Err(WidthError::BitsPerWordAboveMaximum { bits, maximum: 32 }),
    }
}

/// Named result of decoding one masked SSSR interrupt status word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferState {
    /// `interrupt_transfer: FIFO overrun` (`spi-pxa2xx.c:638-640`).
    FifoOverrun,
    /// `interrupt_transfer: FIFO underrun` (`spi-pxa2xx.c:643-645`).
    FifoUnderrun,
    /// Receiver timeout is handled before ordinary drain/fill work.
    ReceiverTimeout,
    /// At least one RX/TX service request requires drain/fill work.
    Service,
    /// No transfer status selected by the supplied mask is pending.
    Idle,
}

/// Decode the status precedence used by `interrupt_transfer`.
///
/// Linux masks SSSR first, suppresses TFS when TIE is clear, then tests ROR,
/// TUR and TINT in that order (`spi-pxa2xx.c:634-648`). RFS/TFS lead to the
/// drain/fill loop (`spi-pxa2xx.c:656-662`).
pub fn decode_transfer_state(
    status: u32,
    mask: u32,
    transmit_interrupt_enabled: bool,
) -> TransferState {
    let mut irq_status = status & mask;
    if !transmit_interrupt_enabled {
        irq_status &= !sssr::TFS;
    }
    if irq_status & sssr::ROR != 0 {
        TransferState::FifoOverrun
    } else if irq_status & sssr::TUR != 0 {
        TransferState::FifoUnderrun
    } else if irq_status & sssr::TINT != 0 {
        TransferState::ReceiverTimeout
    } else if irq_status & (sssr::RFS | sssr::TFS) != 0 {
        TransferState::Service
    } else {
        TransferState::Idle
    }
}

/// Convert remaining bytes to words and cap the final RX threshold.
///
/// This mirrors `bytes_left >>= 2/1`, followed by `min(default, bytes_left)`
/// (`spi-pxa2xx.c:685-697`).
pub fn final_rx_threshold(bytes_left: u32, width: TransferWidth, default_threshold: u32) -> u32 {
    let words_left = bytes_left
        >> match width {
            TransferWidth::U8 => 0,
            TransferWidth::U16 => 1,
            TransferWidth::U32 => 2,
        };
    default_threshold.min(words_left)
}
