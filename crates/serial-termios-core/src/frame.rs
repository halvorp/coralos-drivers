// SPDX-License-Identifier: GPL-2.0-only
//! Serial frame cflag decoding and encoding.
//!
//! Ported from Linux `drivers/tty/serial/serial_core.c:440-:451, :2211-:2229`,
//! `drivers/tty/tty_ioctl.c:285-:307`, and the cflag literals in
//! `include/uapi/asm-generic/termbits.h:95-:104` and `termbits-common.h:50-:51`.
//!
//! Based on `drivers/char/serial.c`, by Linus Torvalds and Theodore Ts'o.
//! Copyright 1999 ARM Limited. Copyright (C) 2000-2001 Deep Blue Solutions Ltd.

/// Character-size mask (`CSIZE`). // include/uapi/asm-generic/termbits.h:96
pub const CSIZE: u32 = 0x0000_0030;
/// Five data bits (`CS5`). // include/uapi/asm-generic/termbits.h:97
pub const CS5: u32 = 0x0000_0000;
/// Six data bits (`CS6`). // include/uapi/asm-generic/termbits.h:98
pub const CS6: u32 = 0x0000_0010;
/// Seven data bits (`CS7`). // include/uapi/asm-generic/termbits.h:99
pub const CS7: u32 = 0x0000_0020;
/// Eight data bits (`CS8`). // include/uapi/asm-generic/termbits.h:100
pub const CS8: u32 = 0x0000_0030;
/// Select two stop bits (`CSTOPB`). // include/uapi/asm-generic/termbits.h:101
pub const CSTOPB: u32 = 0x0000_0040;
/// Enable parity (`PARENB`). // include/uapi/asm-generic/termbits.h:103
pub const PARENB: u32 = 0x0000_0100;
/// Select odd parity, or mark parity with `CMSPAR` (`PARODD`). // include/uapi/asm-generic/termbits.h:104
pub const PARODD: u32 = 0x0000_0200;
/// Select mark/space parity (`CMSPAR`). // include/uapi/asm-generic/termbits-common.h:50
pub const CMSPAR: u32 = 0x4000_0000;
/// Enable RTS/CTS flow control (`CRTSCTS`). // include/uapi/asm-generic/termbits-common.h:51
pub const CRTSCTS: u32 = 0x8000_0000;

/// All cflag bits consumed or produced by this module. // termbits.h:96-:104; termbits-common.h:50-:51
pub const FRAME_CFLAG_MASK: u32 = CSIZE | CSTOPB | PARENB | PARODD | CMSPAR | CRTSCTS;

/// The four values represented by Linux's `CSIZE` field. // drivers/tty/tty_ioctl.c:270-:280
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataBits {
    Five,
    Six,
    Seven,
    Eight,
}

/// Linux's `CSTOPB` selection. // drivers/tty/tty_ioctl.c:299-:301
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBits {
    One,
    Two,
}

/// The parity modes represented by `PARENB`, `PARODD`, and `CMSPAR`. // serial_core.c:2220-:2225
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    None,
    Odd,
    Even,
    Mark,
    Space,
}

/// Linux's `CRTSCTS` hardware-flow-control selection. // drivers/tty/serial/serial_core.c:2228-:2229
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowControl {
    None,
    RtsCts,
}

/// Hardware-independent serial frame format decoded from termios. // serial_core.c:440-:451
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameFormat {
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
    pub flow_control: FlowControl,
}

/// Decode `CSIZE/CSTOPB/PARENB/PARODD/CMSPAR/CRTSCTS` from a termios cflag.
///
/// Character size follows `tty_get_char_size`: all four masked values are exhaustive, and the
/// defensive default is eight bits (`tty_ioctl.c:270-:280`). `PARODD` and `CMSPAR` have no frame
/// effect unless `PARENB` is set.
pub fn decode_cflag(cflag: u32) -> FrameFormat {
    let data_bits = match cflag & CSIZE {
        CS5 => DataBits::Five,
        CS6 => DataBits::Six,
        CS7 => DataBits::Seven,
        _ => DataBits::Eight,
    };
    let stop_bits = if cflag & CSTOPB != 0 {
        StopBits::Two
    } else {
        StopBits::One
    };
    let parity = if cflag & PARENB == 0 {
        Parity::None
    } else if cflag & CMSPAR != 0 {
        if cflag & PARODD != 0 {
            Parity::Mark
        } else {
            Parity::Space
        }
    } else if cflag & PARODD != 0 {
        Parity::Odd
    } else {
        Parity::Even
    };
    let flow_control = if cflag & CRTSCTS != 0 {
        FlowControl::RtsCts
    } else {
        FlowControl::None
    };

    FrameFormat {
        data_bits,
        stop_bits,
        parity,
        flow_control,
    }
}

/// Encode a hardware-independent frame as Linux termios cflag format bits.
///
/// This is the inverse of [`decode_cflag`] for canonical cflags. It does not add unrelated termios
/// policy such as `CREAD`, `HUPCL`, or `CLOCAL` (`serial_core.c:2211`).
pub fn encode_cflag(format: FrameFormat) -> u32 {
    let data = match format.data_bits {
        DataBits::Five => CS5,
        DataBits::Six => CS6,
        DataBits::Seven => CS7,
        DataBits::Eight => CS8,
    };
    let stop = match format.stop_bits {
        StopBits::One => 0,
        StopBits::Two => CSTOPB,
    };
    let parity = match format.parity {
        Parity::None => 0,
        Parity::Odd => PARENB | PARODD,
        Parity::Even => PARENB,
        Parity::Mark => PARENB | PARODD | CMSPAR,
        Parity::Space => PARENB | CMSPAR,
    };
    let flow = match format.flow_control {
        FlowControl::None => 0,
        FlowControl::RtsCts => CRTSCTS,
    };

    data | stop | parity | flow
}

/// Number of bits in one serial frame: start, data, parity if enabled, and one or two stop bits.
///
/// Port of `tty_get_frame_size` (`tty_ioctl.c:294-:305`) for the cflag subset in this crate. The
/// Linux-only multidrop `ADDRB` bit is deliberately outside this termios port's requested subset.
pub fn frame_size(format: FrameFormat) -> u8 {
    let data = match format.data_bits {
        DataBits::Five => 5,
        DataBits::Six => 6,
        DataBits::Seven => 7,
        DataBits::Eight => 8,
    };
    let second_stop = u8::from(format.stop_bits == StopBits::Two);
    let parity = u8::from(format.parity != Parity::None);
    2 + data + second_stop + parity
}
