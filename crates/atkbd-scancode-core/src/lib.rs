// SPDX-License-Identifier: GPL-2.0-only
//! AT/PS2 keyboard scancode translation and decoding, without hardware access or I/O.
//!
//! Ported mechanically from Linux `drivers/input/keyboard/atkbd.c`: its set 2 and set 3
//! keycode tables, translated-mode table, protocol bytes, compatibility encoding, and receive
//! state machine.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux atkbd authors.

#![no_std]
#![forbid(unsafe_code)]

pub mod decode;
pub mod scancode;
pub mod tables;
