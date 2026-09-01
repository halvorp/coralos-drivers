// SPDX-License-Identifier: GPL-2.0-only
//! Standard baud corpus, directional closest selection, and termios encoding tolerance.
//!
//! Ported from Linux `drivers/tty/tty_baudrate.c:17-:44, :119-:193` and
//! `include/linux/util_macros.h:24-:52`, reached by `uart_get_baud_rate` in
//! `drivers/tty/serial/serial_core.c:477-:554`.
//!
//! Copyright (C) 1991-1994 Linus Torvalds. Copyright 1999 ARM Limited.
//! Copyright (C) 2000-2001 Deep Blue Solutions Ltd.

/// Output-baud mask (`CBAUD`). // include/uapi/asm-generic/termbits.h:95
pub const CBAUD: u32 = 0x0000_100f;
/// Extended baud selector (`CBAUDEX`) and arbitrary-rate token (`BOTHER`).
/// // include/uapi/asm-generic/termbits.h:107-:108
pub const BOTHER: u32 = 0x0000_1000;

/// One Linux standard baud-table entry. // drivers/tty/tty_baudrate.c:20-:42
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaudRate {
    pub name: &'static str,
    pub rate: u32,
    pub cflag_bits: u32,
}

/// Number of non-SPARC entries in Linux's parallel `baud_table[]` and `baud_bits[]`.
/// // drivers/tty/tty_baudrate.c:20-:44
pub const BAUD_RATE_COUNT: usize = 31;

/// Linux's non-SPARC standard output-baud corpus.
///
/// Rates: `tty_baudrate.c:20-:30`; parallel names/bits: `tty_baudrate.c:32-:42`.
/// `BOTHER` is not an entry: it is the token for an arbitrary rate.
pub const BAUD_RATES: [BaudRate; BAUD_RATE_COUNT] = [
    BaudRate {
        name: "B0",
        rate: 0,
        cflag_bits: 0x0000,
    },
    BaudRate {
        name: "B50",
        rate: 50,
        cflag_bits: 0x0001,
    },
    BaudRate {
        name: "B75",
        rate: 75,
        cflag_bits: 0x0002,
    },
    BaudRate {
        name: "B110",
        rate: 110,
        cflag_bits: 0x0003,
    },
    BaudRate {
        name: "B134",
        rate: 134,
        cflag_bits: 0x0004,
    },
    BaudRate {
        name: "B150",
        rate: 150,
        cflag_bits: 0x0005,
    },
    BaudRate {
        name: "B200",
        rate: 200,
        cflag_bits: 0x0006,
    },
    BaudRate {
        name: "B300",
        rate: 300,
        cflag_bits: 0x0007,
    },
    BaudRate {
        name: "B600",
        rate: 600,
        cflag_bits: 0x0008,
    },
    BaudRate {
        name: "B1200",
        rate: 1_200,
        cflag_bits: 0x0009,
    },
    BaudRate {
        name: "B1800",
        rate: 1_800,
        cflag_bits: 0x000a,
    },
    BaudRate {
        name: "B2400",
        rate: 2_400,
        cflag_bits: 0x000b,
    },
    BaudRate {
        name: "B4800",
        rate: 4_800,
        cflag_bits: 0x000c,
    },
    BaudRate {
        name: "B9600",
        rate: 9_600,
        cflag_bits: 0x000d,
    },
    BaudRate {
        name: "B19200",
        rate: 19_200,
        cflag_bits: 0x000e,
    },
    BaudRate {
        name: "B38400",
        rate: 38_400,
        cflag_bits: 0x000f,
    },
    BaudRate {
        name: "B57600",
        rate: 57_600,
        cflag_bits: 0x1001,
    },
    BaudRate {
        name: "B115200",
        rate: 115_200,
        cflag_bits: 0x1002,
    },
    BaudRate {
        name: "B230400",
        rate: 230_400,
        cflag_bits: 0x1003,
    },
    BaudRate {
        name: "B460800",
        rate: 460_800,
        cflag_bits: 0x1004,
    },
    BaudRate {
        name: "B500000",
        rate: 500_000,
        cflag_bits: 0x1005,
    },
    BaudRate {
        name: "B576000",
        rate: 576_000,
        cflag_bits: 0x1006,
    },
    BaudRate {
        name: "B921600",
        rate: 921_600,
        cflag_bits: 0x1007,
    },
    BaudRate {
        name: "B1000000",
        rate: 1_000_000,
        cflag_bits: 0x1008,
    },
    BaudRate {
        name: "B1152000",
        rate: 1_152_000,
        cflag_bits: 0x1009,
    },
    BaudRate {
        name: "B1500000",
        rate: 1_500_000,
        cflag_bits: 0x100a,
    },
    BaudRate {
        name: "B2000000",
        rate: 2_000_000,
        cflag_bits: 0x100b,
    },
    BaudRate {
        name: "B2500000",
        rate: 2_500_000,
        cflag_bits: 0x100c,
    },
    BaudRate {
        name: "B3000000",
        rate: 3_000_000,
        cflag_bits: 0x100d,
    },
    BaudRate {
        name: "B3500000",
        rate: 3_500_000,
        cflag_bits: 0x100e,
    },
    BaudRate {
        name: "B4000000",
        rate: 4_000_000,
        cflag_bits: 0x100f,
    },
];

/// Locate the closest standard baud, with Linux's directional tie rule.
///
/// This is `find_closest` (`include/linux/util_macros.h:36-:52`) specialized to the frozen baud
/// table. The midpoint comparison is `requested <= midpoint`, and the right neighbour wins only
/// when it is *strictly* closer. Therefore an exact midpoint selects the lower rate. A common
/// `(x + half_gap) / gap` replacement incorrectly selects the upper rate at that point.
pub fn closest_standard_baud(requested: u32) -> BaudRate {
    let last = BAUD_RATES.len() - 1;
    for index in 0..last {
        let left_rate = BAUD_RATES[index].rate;
        let right_rate = BAUD_RATES[index + 1].rate;
        let midpoint = (left_rate as u64 + right_rate as u64) / 2;
        if requested as u64 <= midpoint {
            let left_distance = requested as i64 - left_rate as i64;
            let right_distance = right_rate as i64 - requested as i64;
            return if right_distance < left_distance {
                BAUD_RATES[index + 1]
            } else {
                BAUD_RATES[index]
            };
        }
    }
    BAUD_RATES[last]
}

/// Linux's allowed fuzz when reporting a requested speed as a standard `Bxxxx` token.
///
/// `tty_termios_encode_baud_rate` uses integer `baud / 50`, i.e. 2% rounded down
/// (`tty_baudrate.c:126`). `BOTHER` requests override this to zero (`:141-:148`).
pub fn baud_tolerance(baud: u32, precise_bother_request: bool) -> u32 {
    if precise_bother_request {
        0
    } else {
        baud / 50
    }
}

/// Whether a standard candidate lies inside Linux's inclusive encoding window.
///
/// Port of `requested - close <= candidate && requested + close >= candidate`
/// (`tty_baudrate.c:170-:171`). Widening avoids arithmetic wrap while preserving the policy.
pub fn within_baud_tolerance(requested: u32, candidate: u32, precise_bother_request: bool) -> bool {
    let close = baud_tolerance(requested, precise_bother_request) as u64;
    let requested = requested as u64;
    let candidate = candidate as u64;
    candidate >= requested.saturating_sub(close) && candidate <= requested + close
}

/// The cflag representation selected for an actual baud. // drivers/tty/tty_baudrate.c:168-:192
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaudEncoding {
    /// A close standard rate is reported using its POSIX `Bxxxx` token.
    Standard(BaudRate),
    /// No permitted standard match exists, so Linux reports `BOTHER`.
    Other { cflag_bits: u32, rate: u32 },
}

/// Select the output cflag encoding using Linux's 2% compatibility policy.
///
/// Like `tty_termios_encode_baud_rate` (`tty_baudrate.c:168-:192`), this walks upward and retains
/// the last in-window match. A caller carrying an original `BOTHER` request must pass
/// `precise_bother_request = true`, which permits only an exact standard-rate match.
pub fn encode_baud(baud: u32, precise_bother_request: bool) -> BaudEncoding {
    let mut selected = None;
    for candidate in BAUD_RATES {
        if within_baud_tolerance(baud, candidate.rate, precise_bother_request) {
            selected = Some(candidate);
        }
    }
    match selected {
        Some(candidate) => BaudEncoding::Standard(candidate),
        None => BaudEncoding::Other {
            cflag_bits: BOTHER,
            rate: baud,
        },
    }
}
