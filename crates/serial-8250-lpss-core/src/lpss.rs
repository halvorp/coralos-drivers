// SPDX-License-Identifier: GPL-2.0-only
//! Intel LPSS board data, PCI IDs and private M/N clock programming from Linux `8250_lpss.c`.
//!
//! Copyright (C) 2016 Intel Corporation; author Andy Shevchenko.

use crate::regs::bits;

/// `BIT(15) - 1`, the maximum M and N passed to Linux's rational helper.
pub const MAX_CLOCK_DIVIDER: u32 = 32_767; // 8250_lpss.c:78
/// B0 falls back to B9600.
pub const B0_FALLBACK_BAUD: u32 = 9_600; // 8250_lpss.c:82-83
/// The UART uses 16x oversampling.
pub const OVERSAMPLING: u32 = 16; // 8250_lpss.c:77, :83

/// The eight baud rates Linux calls out as requiring adjusted LPSS dividers.
pub const ADJUSTED_BAUDS: [u32; 8] = [
    500_000, 1_000_000, 1_500_000, 2_000_000, 2_500_000, 3_000_000, 3_500_000, 4_000_000,
]; // 8250_lpss.c:89-90

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardKind {
    Quark,
    ElkhartLake,
    BayTrail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Board {
    pub name: &'static str,
    pub kind: BoardKind,
    pub reference_hz: u32,
    pub base_baud: u32,
    pub dma_maxburst: u8,
}

/// The three `lpss8250_board` instances in Linux (`8250_lpss.c:384-:402`).
pub const BOARDS: [Board; 3] = [
    Board {
        name: "byt_board",
        kind: BoardKind::BayTrail,
        reference_hz: 100_000_000,
        base_baud: 2_764_800,
        dma_maxburst: 16,
    }, // 8250_lpss.c:384-:388, :145
    Board {
        name: "ehl_board",
        kind: BoardKind::ElkhartLake,
        reference_hz: 200_000_000,
        base_baud: 12_500_000,
        dma_maxburst: 16,
    }, // 8250_lpss.c:391-:395, :177
    Board {
        name: "qrk_board",
        kind: BoardKind::Quark,
        reference_hz: 44_236_800,
        base_baud: 2_764_800,
        dma_maxburst: 8,
    }, // 8250_lpss.c:398-:402, :234
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciId {
    pub name: &'static str,
    pub device_id: u16,
    pub board: BoardKind,
}

/// Every non-sentinel member of `pci_ids[]`, in Linux order (`8250_lpss.c:405-:419`).
pub const PCI_IDS: [PciId; 13] = [
    PciId {
        name: "QRK_UARTx",
        device_id: 0x0936,
        board: BoardKind::Quark,
    }, // 8250_lpss.c:19, :406
    PciId {
        name: "EHL_UART0",
        device_id: 0x4b96,
        board: BoardKind::ElkhartLake,
    }, // 8250_lpss.c:27, :407
    PciId {
        name: "EHL_UART1",
        device_id: 0x4b97,
        board: BoardKind::ElkhartLake,
    }, // 8250_lpss.c:28, :408
    PciId {
        name: "EHL_UART2",
        device_id: 0x4b98,
        board: BoardKind::ElkhartLake,
    }, // 8250_lpss.c:29, :409
    PciId {
        name: "EHL_UART3",
        device_id: 0x4b99,
        board: BoardKind::ElkhartLake,
    }, // 8250_lpss.c:30, :410
    PciId {
        name: "EHL_UART4",
        device_id: 0x4b9a,
        board: BoardKind::ElkhartLake,
    }, // 8250_lpss.c:31, :411
    PciId {
        name: "EHL_UART5",
        device_id: 0x4b9b,
        board: BoardKind::ElkhartLake,
    }, // 8250_lpss.c:32, :412
    PciId {
        name: "BYT_UART1",
        device_id: 0x0f0a,
        board: BoardKind::BayTrail,
    }, // 8250_lpss.c:21, :413
    PciId {
        name: "BYT_UART2",
        device_id: 0x0f0c,
        board: BoardKind::BayTrail,
    }, // 8250_lpss.c:22, :414
    PciId {
        name: "BSW_UART1",
        device_id: 0x228a,
        board: BoardKind::BayTrail,
    }, // 8250_lpss.c:24, :415
    PciId {
        name: "BSW_UART2",
        device_id: 0x228c,
        board: BoardKind::BayTrail,
    }, // 8250_lpss.c:25, :416
    PciId {
        name: "BDW_UART1",
        device_id: 0x9ce3,
        board: BoardKind::BayTrail,
    }, // 8250_lpss.c:34, :417
    PciId {
        name: "BDW_UART2",
        device_id: 0x9ce4,
        board: BoardKind::BayTrail,
    }, // 8250_lpss.c:35, :418
];

/// Bay Trail/Braswell/Broadwell UART DMA request IDs selected by `byt_serial_setup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaRequestIds {
    pub source: u8,
    pub destination: u8,
}

/// Named refusal from the LPSS UART-to-DMA request mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaRequestError {
    UnsupportedUartDevice { device_id: u16 },
}

/// Return the RX/TX DMA request IDs for an LPSS UART PCI ID.
pub fn byt_dma_request_ids(device_id: u16) -> Result<DmaRequestIds, DmaRequestError> {
    match device_id {
        0x0f0a | 0x228a | 0x9ce3 => Ok(DmaRequestIds {
            source: 3,
            destination: 2,
        }), // 8250_lpss.c:122-:128
        0x0f0c | 0x228c | 0x9ce4 => Ok(DmaRequestIds {
            source: 5,
            destination: 4,
        }), // 8250_lpss.c:129-:134
        _ => Err(DmaRequestError::UnsupportedUartDevice { device_id }), // 8250_lpss.c:135-:136
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockPlan {
    /// Linux assigns this adjusted value to `p->uartclk` (`8250_lpss.c:95`).
    pub uartclk_hz: u32,
    pub m: u32,
    pub n: u32,
    /// First write: clock disabled, no update bit (`8250_lpss.c:98-:99`).
    pub reset_word: u32,
    /// Second write: the same M/N plus enable and update (`8250_lpss.c:100-:101`).
    pub enabled_word: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    BaudTimes16Overflow {
        baud: u32,
        maximum_baud: u32,
    },
    ReferenceBelowRequestedClock {
        reference_hz: u32,
        requested_uartclk_hz: u32,
    },
    NumeratorOutOfRange {
        m: u32,
        maximum: u32,
    },
    DenominatorOutOfRange {
        n: u32,
        minimum: u32,
        maximum: u32,
    },
    NumeratorExceedsDenominator {
        m: u32,
        n: u32,
    },
}

/// Encode the M/N fields for `BYT_PRV_CLK`, refusing values outside Linux's 15-bit bound.
pub fn private_clock_word(m: u32, n: u32, enable_and_update: bool) -> Result<u32, ClockError> {
    if m == 0 || m > MAX_CLOCK_DIVIDER {
        return Err(ClockError::NumeratorOutOfRange {
            m,
            maximum: MAX_CLOCK_DIVIDER,
        });
    }
    if n == 0 || n > MAX_CLOCK_DIVIDER {
        return Err(ClockError::DenominatorOutOfRange {
            n,
            minimum: 1,
            maximum: MAX_CLOCK_DIVIDER,
        });
    }
    if m > n {
        return Err(ClockError::NumeratorExceedsDenominator { m, n });
    }
    let mut word = (m << bits::PRV_CLK_M_VAL_SHIFT) | (n << bits::PRV_CLK_N_VAL_SHIFT); // 8250_lpss.c:98
    if enable_and_update {
        word |= bits::PRV_CLK_EN | bits::PRV_CLK_UPDATE; // 8250_lpss.c:100
    }
    Ok(word)
}

/// Port of `byt_set_termios` clock arithmetic (`8250_lpss.c:75-:101`).
pub fn clock_plan(baud: u32, reference_hz: u32) -> Result<ClockPlan, ClockError> {
    let effective_baud = if baud == 0 { B0_FALLBACK_BAUD } else { baud }; // 8250_lpss.c:82-83
    let initial =
        effective_baud
            .checked_mul(OVERSAMPLING)
            .ok_or(ClockError::BaudTimes16Overflow {
                baud: effective_baud,
                maximum_baud: u32::MAX / OVERSAMPLING,
            })?;
    if initial > reference_hz {
        return Err(ClockError::ReferenceBelowRequestedClock {
            reference_hz,
            requested_uartclk_hz: initial,
        });
    }
    let factor = highest_power_of_two(reference_hz / initial);
    let uartclk_hz = initial * factor; // 8250_lpss.c:86
    let (m, n) = rational_best_approximation(
        uartclk_hz as u64,
        reference_hz as u64,
        MAX_CLOCK_DIVIDER as u64,
        MAX_CLOCK_DIVIDER as u64,
    ); // 8250_lpss.c:94
    let m = m as u32;
    let n = n as u32;
    Ok(ClockPlan {
        uartclk_hz,
        m,
        n,
        reset_word: private_clock_word(m, n, false)?,
        enabled_word: private_clock_word(m, n, true)?,
    })
}

fn highest_power_of_two(value: u32) -> u32 {
    1 << (31 - value.leading_zeros())
}

// Exact continued-fraction helper used by rational_best_approximation(), called at
// 8250_lpss.c:94. Kept private so this crate exposes the UART decision, not a general math API.
fn rational_best_approximation(
    mut numerator: u64,
    mut denominator: u64,
    max_numerator: u64,
    max_denominator: u64,
) -> (u64, u64) {
    let (mut n0, mut d0, mut n1, mut d1) = (0u64, 1u64, 1u64, 0u64);
    loop {
        if denominator == 0 {
            break;
        }
        let previous_denominator = denominator;
        let a = numerator / denominator;
        denominator = numerator % denominator;
        numerator = previous_denominator;
        let n2 = n0 + a * n1;
        let d2 = d0 + a * d1;
        if n2 > max_numerator || d2 > max_denominator {
            let mut t = u64::MAX;
            if d1 != 0 {
                t = (max_denominator - d0) / d1;
            }
            if n1 != 0 {
                t = core::cmp::min(t, (max_numerator - n0) / n1);
            }
            if d1 == 0 || 2 * t > a || (2 * t == a && d0 * previous_denominator > d1 * denominator)
            {
                n1 = n0 + t * n1;
                d1 = d0 + t * d1;
            }
            break;
        }
        n0 = n1;
        n1 = n2;
        d0 = d1;
        d1 = d2;
    }
    (n1, d1)
}
