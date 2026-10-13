// SPDX-License-Identifier: GPL-2.0-only
//! avs.device - the Intel cAVS audio DSP (Sunrise Point-LP / Kaby Lake, PCI 8086:9d71) and the Dialog DA7219 headset codec of the Pixelbook
//! Go (ATLAS) as a ring-3 CoralOS `.device`. HEADPHONES ONLY. The speaker amplifiers are never addressed: their I2C addresses are refused
//! by name at two layers, their SSP is refused before a pipeline exists, their reset pad is driven low to hold them, and the speaker route
//! is answered `SPEAKER-PROTECTION-UNAVAILABLE` (Astra audio ruling, 2026-10-12).
//!
//! WHAT IT IS. The wave-1 and wave-2 cores are PLANS (data, no effects): avs-fw-core boots the DSP, avs-ipc-core speaks to it,
//! avs-topology-core reads the topology and NHLT, hda-ext-core drives the host stream, avs-boot-core orders all of it, da7219-core plans the
//! codec. This crate EXECUTES them: [`fwexec`] and [`ipcrun`] over the registers and the interrupt [`pump`], [`i2c`] and [`codec`] over the
//! codec's bus, [`gpio`] for the two pads, [`firmware`] for the files, [`session`] for one DSP session, [`device`] / [`serve`] / [`jack`] for
//! the state machine and the protocol (`avs-wire`) Aura.engine speaks.
//!
//! THE FENCES ([`allow`], [`rig`]). Registers: the plans' write sets, register by register and width by width, built once the controller
//! has been read; the loader-facing grant is built FROM them. I2C: the codec's address only. Pads: the speaker-reset pad's `PADCFG0` only.
//! `tests/allow.rs` runs every plan against the lists and requires set equality.
//!
//! IDLE IS BLOCKED. With nothing running the task sleeps in one wait for a request or a jack event. While a stream plays it also wakes on the
//! cAVS interrupt and a watchdog deadline. Every other wait is a bounded step of a plan.
//!
//! THE LOADER'S ENDOWMENT (the launch row): MMIO slots [`rt::SLOT_HDA`] (BAR0), [`rt::SLOT_DSP`] (BAR4), [`rt::SLOT_CFG`] (PCI configuration),
//! [`rt::SLOT_I2C`] (the codec bus's controller), [`rt::SLOT_PAD0`].. (one per pad community); IRQ slot 0 (the cAVS line); DMA budget for
//! ~152 KiB; endpoints [`wire`]. See that module and the commit message for who grants what.
#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]

extern crate alloc;

pub mod allow;
pub mod codec;
pub mod controller;
pub mod device;
pub mod events;
pub mod firmware;
pub mod fwexec;
pub mod gpio;
pub mod i2c;
pub mod ipcrun;
pub mod jack;
pub mod log;
pub mod machine;
pub mod pump;
pub mod rig;
#[cfg(target_arch = "x86_64")]
pub mod rt;
pub mod serve;
pub mod session;
pub mod stream;
pub mod wire;

pub use device::Device;
pub use machine::Machine;
