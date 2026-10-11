// SPDX-License-Identifier: GPL-2.0-only
//! i8042.device — the PS/2 keyboard controller behind the Pixelbook Go's (EC-emulated) keyboard and QEMU q35's, as a
//! CoralOS ring-3 device. It replaces the kernel's `ps2kbd.rs`: when this device owns the controller the kernel stands
//! down (reads neither the data port nor the IRQ), so exactly one component touches the hardware.
//!
//! WHAT IT DOES, in order: [`init`] runs the ported bring-up (`i8042-core`: flush, controller check, CTR, AUX off, port
//! enable, then the AT keyboard's GETID / scan set / LEDs / typematic / enable) and names every step on the task log;
//! [`pipeline::Pipeline`] decodes the keyboard's bytes (`atkbd-scancode-core`) into Linux keycodes and maps them to USB HID
//! usages (`hid-input-keyboard-core`); [`run`] posts the PHYSICAL key events — press, release, repeat — as
//! `coralos_rt::keywire` frames on the keyboard endpoint. It decides nothing about text: layout, left/right modifiers,
//! CapsLock and any top-row action policy belong to the input layer that reads the frames.
//!
//! PIXELBOOK GO KEYBOARD (from the board's recovery image; the ChromeOS layout-2 list is from memory): there is no CapsLock
//! key - the Search key sits there and sends set-2 E0 1F, emitted as LeftGUI/LeftMeta (0xE3); the top row sends plain
//! F1-F10, emitted as F-keys. Their meaning (Back, Forward, Refresh, Fullscreen, Overview, Brightness-/+, Mute, Vol-/+) is
//! input-layer policy, not this device's. The Assistant key (atkbd 0xD8) is not in the EC's table; whatever it sends is
//! counted as unmapped if the HID table has no usage for it.
//!
//! WHAT IT HOLDS: byte access to ports 0x60 and 0x64 (two single-byte ranges, so 0x61-0x63 are unreachable) and IRQ 1,
//! granted by the loader before exec. Nothing else.
//!
//! Copyright (c) 1999-2004 Vojtech Pavlik; Copyright (c) 2004 Dmitry Torokhov; the Linux input authors (the cores).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod init;
pub mod io;
pub mod log;
pub mod pacer;
pub mod pipeline;
pub mod run;
#[cfg(target_arch = "x86_64")]
pub mod service;

pub use init::{init, Device};
pub use run::{drain, Sink};
#[cfg(target_arch = "x86_64")]
pub use service::{on_panic, service};
