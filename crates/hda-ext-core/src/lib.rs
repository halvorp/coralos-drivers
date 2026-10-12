// SPDX-License-Identifier: GPL-2.0-only
//! The HD-audio controller of the Sunrise Point-LP cAVS (PCI 8086:9d71) as pure PLANS: the capability chain walk (processing-pipe,
//! multi-link, SPIB, DMA-resume), link reset with bounded waits, interrupt control, the processing-pipe enables, and a host playback
//! stream - reset, descriptor setup, buffer descriptor list, SPIB, start and stop - in the order Linux's AVS PCM path runs them.
//!
//! Ported from Linux `sound/hda/core/controller.c` (`snd_hdac_bus_parse_capabilities`, `snd_hdac_bus_reset_link`, `snd_hdac_bus_init_chip`,
//! `snd_hdac_bus_stop_chip`), `sound/hda/core/stream.c` (`snd_hdac_stream_init/reset/setup/start/stop/cleanup`, `setup_bdle`,
//! `snd_hdac_stream_spbcap_enable`, `snd_hdac_stream_set_spib`), `sound/hda/core/ext/controller.c` (`snd_hdac_ext_bus_ppcap_enable`,
//! `_ppcap_int_enable`, `snd_hdac_ext_bus_get_ml_capabilities`), `sound/hda/core/ext/stream.c` (`snd_hdac_ext_stream_decouple`),
//! `sound/hda/core/device.c` (`snd_hdac_stream_format`), `sound/soc/intel/avs/core.c` (`avs_hdac_bus_init_chip`,
//! `avs_hdac_bus_init_streams`, the gating helpers) and `sound/soc/intel/avs/pcm.c` (the front-end prepare/trigger order), with the register
//! file of `include/sound/hda_register.h` and `include/sound/hda_verbs.h` (Linux v7.0-rc6). The Linux files are `GPL-2.0-only`
//! (`GPL-2.0` in the headers); this port is distributed under `GPL-2.0-only`.
//!
//! THE CRATE DOES NOT TOUCH THE DEVICE. Every plan is an [`avs_fw_core::runner::Program`] of [`avs_fw_core::effect::Step`]s that the
//! `.device` performs against BAR0 or PCI configuration and judges with [`avs_fw_core::runner::Runner`]; the two walks that depend on
//! what the hardware answers ([`caps::CapWalk`], [`caps::MlProbe`]) are coroutines. Every wait is a bounded `Poll` (Linux's silent
//! time-outs - `snd_hdac_bus_enter_link_reset`, `snd_hdac_stream_reset`, the capability walk - become NAMED failures).
//!
//! NOT PORTED, each by name: the CORB/RIRB command rings and codec probing (`snd_hdac_bus_init_cmd_io`, `avs_hdac_bus_probe_codecs`: the
//! HDMI/DP codec 8086:280B is a later wave; the DA7219 is an I2S codec and never on the HDA link), the processing-pipe link/host
//! controller register blocks (`PPLC`/`PPHC` offsets are computed in [`stream::pplc_base`] / [`stream::pphc_base`] but no program uses them),
//! the stripe control, the DMA-resume (`DRSM`) and wall-clock/GTS blocks, the Apollo Lake host-stream setup variant, and stream
//! position reads (the `.device` reads the position buffer).

#![no_std]
#![forbid(unsafe_code)]

// `alloc` for the runtime-sized lists a controller states about itself: the multi-link list (a link count read from `MLCD`) and the
// buffer descriptor list a client's buffer sizes. Growth is fallible and exhaustion is the named `HdaError::OutOfMemory`
// (DECISIONS.md D10, doctrine RUNTIME-TABLES-HAVE-NO-CAP). Hardware-defined counts stay fixed, each with its reason where declared.
extern crate alloc;

pub mod bdl;
pub mod caps;
pub mod consts;
pub mod controller;
pub mod error;
pub mod regs;
pub mod stream;
