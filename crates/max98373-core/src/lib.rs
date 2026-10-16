// SPDX-License-Identifier: GPL-2.0-only
//! max98373-core - the Maxim MAX98373 speaker amplifier of the Pixelbook Go (ATLAS) as DATA and PLANS: the registers ([`regs`]), the mixer
//! controls ChromeOS's UCM sets and the arithmetic that turns a UCM number into register bits ([`ctl`]), the register plans of Linux's
//! driver ([`plan`]: reset, probe, DAI format, the protection set and its readback, the DAPM power sequences, the safe checks) and the
//! speaker path's hard gain ceiling ([`gain`]). It performs no I/O; the `.device` performs each step over I2C.
//!
//! Ported from Linux `sound/soc/codecs/max98373{.c,.h,-i2c.c}` (Copyright (c) 2017 Maxim Integrated; `GPL-2.0`) and the Intel AVS machine
//! driver `sound/soc/intel/avs/boards/max98373.c`; this port is distributed under `GPL-2.0-only`. ChromeOS's UCM and CRAS curve are the
//! recovery image's (`usr/share/alsa/ucm/avs_max98373/HiFi.conf`, `etc/cras/avs_max98373`); their sha256 are recorded in DECISIONS D17.
//!
//! THE AMPLIFIERS ARE PROTECTED OR THEY DO NOT RUN (DECISIONS D17, amending D14): the protection set is written and read back before the
//! part is enabled, the enable comes after the audio clock exists, and the gain that reaches the part can never exceed [`gain::CEILING_LINEAR`].
#![no_std]
#![forbid(unsafe_code)]

pub mod ctl;
pub mod gain;
pub mod plan;
pub mod regs;
