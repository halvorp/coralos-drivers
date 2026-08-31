// SPDX-License-Identifier: GPL-2.0-only
//! Frozen LPSS vectors from Linux `8250_lpss.c`.
//!
//! Copyright (C) 2016 Intel Corporation; author Andy Shevchenko.

use serial_8250_lpss_core::lpss::{
    byt_dma_request_ids, clock_plan, private_clock_word, BoardKind, ClockError, DmaRequestError,
    DmaRequestIds, ADJUSTED_BAUDS, B0_FALLBACK_BAUD, BOARDS, MAX_CLOCK_DIVIDER, OVERSAMPLING,
    PCI_IDS,
};

const LINUX_BOARD_NAMES: [&str; 3] = ["byt_board", "ehl_board", "qrk_board"];
const LINUX_PCI_NAMES: [&str; 13] = [
    "QRK_UARTx",
    "EHL_UART0",
    "EHL_UART1",
    "EHL_UART2",
    "EHL_UART3",
    "EHL_UART4",
    "EHL_UART5",
    "BYT_UART1",
    "BYT_UART2",
    "BSW_UART1",
    "BSW_UART2",
    "BDW_UART1",
    "BDW_UART2",
];
const LINUX_PCI_VALUES: [u16; 13] = [
    0x0936, 0x4b96, 0x4b97, 0x4b98, 0x4b99, 0x4b9a, 0x4b9b, 0x0f0a, 0x0f0c, 0x228a, 0x228c, 0x9ce3,
    0x9ce4,
]; // 8250_lpss.c:19-:35, in pci_ids[] order :406-:418

#[test]
fn all_three_linux_boards_are_pinned_by_name_and_values() {
    assert_eq!(BOARDS.len(), 3); // 8250_lpss.c:384-:402
    assert_eq!(BOARDS.map(|b| b.name), LINUX_BOARD_NAMES);
    assert_eq!(
        BOARDS.map(|b| (b.kind, b.reference_hz, b.base_baud, b.dma_maxburst)),
        [
            (BoardKind::BayTrail, 100_000_000, 2_764_800, 16), // :385-:386, :145
            (BoardKind::ElkhartLake, 200_000_000, 12_500_000, 16), // :392-:393, :177
            (BoardKind::Quark, 44_236_800, 2_764_800, 8),      // :399-:400, :234
        ]
    );
}

#[test]
fn all_thirteen_non_sentinel_pci_ids_are_pinned_by_name_and_value() {
    assert_eq!(PCI_IDS.len(), 13); // 8250_lpss.c:405-:419
    assert_eq!(PCI_IDS.map(|id| id.name), LINUX_PCI_NAMES);
    assert_eq!(PCI_IDS.map(|id| id.device_id), LINUX_PCI_VALUES);
    assert_eq!(PCI_IDS[0].board, BoardKind::Quark);
    assert!(PCI_IDS[1..7]
        .iter()
        .all(|id| id.board == BoardKind::ElkhartLake));
    assert!(PCI_IDS[7..]
        .iter()
        .all(|id| id.board == BoardKind::BayTrail));
}

#[test]
fn all_eight_adjusted_bauds_are_pinned_by_count_and_value() {
    assert_eq!(ADJUSTED_BAUDS.len(), 8); // 8250_lpss.c:89-:90
    assert_eq!(
        ADJUSTED_BAUDS,
        [500_000, 1_000_000, 1_500_000, 2_000_000, 2_500_000, 3_000_000, 3_500_000, 4_000_000]
    ); // 8250_lpss.c:89-:90
}

#[test]
fn byt_dma_request_ids_match_linuxs_two_uart_groups() {
    for device_id in [0x0f0a, 0x228a, 0x9ce3] {
        assert_eq!(
            byt_dma_request_ids(device_id),
            Ok(DmaRequestIds {
                source: 3,
                destination: 2
            })
        ); // 8250_lpss.c:122-:128
    }
    for device_id in [0x0f0c, 0x228c, 0x9ce4] {
        assert_eq!(
            byt_dma_request_ids(device_id),
            Ok(DmaRequestIds {
                source: 5,
                destination: 4
            })
        ); // 8250_lpss.c:129-:134
    }
    assert_eq!(
        byt_dma_request_ids(0x0936),
        Err(DmaRequestError::UnsupportedUartDevice { device_id: 0x0936 })
    ); // default returns -EINVAL, 8250_lpss.c:135-:136
}

#[test]
fn lpss_clock_literals_and_half_ratio_words_match_linux() {
    assert_eq!(MAX_CLOCK_DIVIDER, 32_767); // BIT(15)-1, 8250_lpss.c:78
    assert_eq!(B0_FALLBACK_BAUD, 9_600); // 8250_lpss.c:83
    assert_eq!(OVERSAMPLING, 16); // 8250_lpss.c:77
    assert_eq!(private_clock_word(1, 2, false), Ok(0x0002_0002)); // (1<<1)|(2<<16), :98
    assert_eq!(private_clock_word(1, 2, true), Ok(0x8002_0003)); // plus BIT(0)|BIT(31), :100
}

#[test]
fn b0_uses_b9600_and_linuxs_power_of_two_scaling() {
    let plan = clock_plan(0, 100_000_000).unwrap();
    assert_eq!(plan.uartclk_hz, 78_643_200); // 9600*16*rounddown_pow_of_two(100000000/153600), :83-:86
    assert_eq!((plan.m, plan.n), (12_288, 15_625)); // exact 78,643,200 / 100,000,000, :94
    assert_eq!(plan.reset_word, 0x3d09_6000); // (12288<<1)|(15625<<16), :98
    assert_eq!(plan.enabled_word, 0xbd09_6001); // reset word | BIT(0) | BIT(31), :100
}

#[test]
fn special_high_bauds_receive_the_linux_adjusted_dividers() {
    let vectors = [
        (500_000, 64_000_000, 16, 25, 0x0019_0020, 0x8019_0021),
        (1_000_000, 64_000_000, 16, 25, 0x0019_0020, 0x8019_0021),
        (1_500_000, 96_000_000, 24, 25, 0x0019_0030, 0x8019_0031),
        (2_000_000, 64_000_000, 16, 25, 0x0019_0020, 0x8019_0021),
        (2_500_000, 80_000_000, 4, 5, 0x0005_0008, 0x8005_0009),
        (3_000_000, 96_000_000, 24, 25, 0x0019_0030, 0x8019_0031),
        (3_500_000, 56_000_000, 14, 25, 0x0019_001c, 0x8019_001d),
        (4_000_000, 64_000_000, 16, 25, 0x0019_0020, 0x8019_0021),
    ]; // rates named at 8250_lpss.c:89-:90; arithmetic is :77-:100
    for (baud, uartclk, m, n, reset, enabled) in vectors {
        let plan = clock_plan(baud, 100_000_000).unwrap();
        assert_eq!(
            (plan.uartclk_hz, plan.m, plan.n),
            (uartclk, m, n),
            "baud={baud}"
        );
        assert_eq!(
            (plan.reset_word, plan.enabled_word),
            (reset, enabled),
            "baud={baud}"
        );
    }
}

#[test]
fn private_clock_refusals_name_values_and_bounds() {
    assert_eq!(
        private_clock_word(0, 1, false),
        Err(ClockError::NumeratorOutOfRange {
            m: 0,
            maximum: 32_767
        })
    );
    assert_eq!(
        private_clock_word(1, 32_768, false),
        Err(ClockError::DenominatorOutOfRange {
            n: 32_768,
            minimum: 1,
            maximum: 32_767
        })
    );
    assert_eq!(
        private_clock_word(2, 1, false),
        Err(ClockError::NumeratorExceedsDenominator { m: 2, n: 1 })
    );
    assert_eq!(
        clock_plan(10_000_000, 100_000_000),
        Err(ClockError::ReferenceBelowRequestedClock {
            reference_hz: 100_000_000,
            requested_uartclk_hz: 160_000_000
        })
    );
    assert_eq!(
        clock_plan(268_435_456, u32::MAX),
        Err(ClockError::BaudTimes16Overflow {
            baud: 268_435_456,
            maximum_baud: 268_435_455
        })
    );
}
