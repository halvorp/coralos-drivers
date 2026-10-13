// SPDX-License-Identifier: GPL-2.0-only
//! The Dialog DA7219 headset codec of the Pixelbook Go (ATLAS) as pure I2C transaction PLANS: register map, PLL/MCLK and DAI
//! configuration for 48 kHz I2S on SSP1, the headphone path's power-up / power-down / mute / unmute sequences, accessory-detect
//! (jack insertion and button decode), codec initialisation, and a SAFE-STATE plan whose result is verified by readback.
//!
//! Ported from Linux `sound/soc/codecs/da7219.c`, `da7219.h`, `da7219-aad.c`, `da7219-aad.h`, `include/sound/da7219.h`,
//! `include/sound/da7219-aad.h`, with the machine-driver values of `sound/soc/intel/avs/boards/da7219.c` and the DAPM ordering rules
//! of `sound/soc/soc-dapm.c` (Linux v7.0-rc6). Copyright (c) 2015 Dialog Semiconductor; author Adam Thomson. The Linux files are
//! `GPL-2.0-or-later`; this port is distributed under `GPL-2.0-only`.
//!
//! THE CRATE DOES NOT TOUCH THE BUS. A plan is a [`bus::Plan`]; the [`runner::Runner`] hands the `.device` one [`bus::Step`] at a time
//! and judges its answer; every wait is bounded by the plan AND counted by the runner, and every failure has a name.
//!
//! SPEAKERS ARE NOT HERE. Every transaction this crate can build addresses the codec of a [`bus::Wiring`] (board data handed in by the
//! caller); the speaker amplifiers on the same bus are refused by name in [`bus::Wiring::check_target`] (Astra audio ruling: headphones
//! first, speakers refused until the protection path exists).
//!
//! WHAT IS DELIBERATELY NOT PORTED (each named, each a decision for the owner or wave 3): the headphone-impedance test
//! (`da7219_aad_hptest_work`, a tone through the jack at 0 dB gains: [`aad::JackReport`] reports a 3-pole jack as `Headphone` and
//! says `hp_test_skipped`), the microphone/ADC/ALC paths, the tone generator, the DAI clock provider (the codec is the I2S consumer on
//! Atlas), and TDM.

#![no_std]
#![forbid(unsafe_code)]

pub mod aad;
pub mod aad_fields;
pub mod bus;
pub mod clock;
pub mod dai;
pub mod fields;
pub mod hp;
pub mod init;
pub mod regs;
pub mod runner;
pub mod safe;

/// A plan builder failed before any step was issued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// A builder pushed past [`bus::PLAN_CAP`] steps.
    Overflow { cap: usize },
}
