// SPDX-License-Identifier: GPL-2.0-only
//! The orchestrator that takes a Pixelbook Go (ATLAS) from a powered-off cAVS DSP to a headphone playback path that is RUNNING, composing
//! the wave-1 and wave-2 cores in Linux's AVS order and gating every step on the verified result of the one before:
//!
//! 1. PCI gating off (`avs_load_firmware`, loader.c:625-627) - [`avs_fw_core::programs::gating`];
//! 2. core power, reset, code-loader DMA, ROM, base firmware, FW_READY - [`avs_fw_core::boot::BaseFwBoot`] (it owns `avs_dsp_core_power`
//!    and the rest of `avs_cldma_load_basefw`);
//! 3. hardware and firmware configuration (`avs_dsp_alloc_resources`, loader.c:690-698), with the firmware version, the SSP port the path
//!    needs and the pipeline/library limits checked;
//! 4. the topology's libraries (`avs_dsp_load_libraries`, loader.c:489-552) - [`avs_fw_core::boot::CodeLoad`];
//! 5. PCI gating on (loader.c:643-646) and the modules table (`avs_module_info_init`, utils.c:157-181);
//! 6. the playback PATH, back end first as Linux creates it (`avs_path_create`, path.c:1321-1355): create pipelines, initialise modules
//!    with their copier/peakvol configuration, bind the modules of each pipeline (`avs_path_arm`, path.c:1131-1154), then bind the front
//!    end to the back end (`avs_path_bind`, path.c:1389-1420);
//! 7. the host stream (hda-ext-core: decouple, reset, BDL, descriptor setup);
//! 8. a GATE: the codec must be proven in its safe state (da7219-core) before any I2S clock is started;
//! 9. start (`avs_dai_fe_trigger`, pcm.c:861-925, front end first as DPCM does): host DMA, pause then run the FE path, pause then run the BE
//!    path, each pipeline's state READ BACK and compared.
//!
//! Ported from Linux `sound/soc/intel/avs/{loader,dsp,path,pcm,utils,core}.c` (v7.0-rc6); the Linux files are `GPL-2.0-only`.
//!
//! THE CRATE DOES NOT TOUCH THE DEVICE. [`boot::Boot`] is a coroutine: it answers each [`boot::Event`] with the next [`boot::Action`]
//! (a hardware step in avs-fw-core's vocabulary, an IPC message, the BDL bytes, a codec-safe question, or the end). Every failure is a
//! [`error::BootFailure`] naming the stage, the step and the reason; after the first failure nothing further is asked of the DSP except
//! restoring the PCI gating, and the caller is told whether the DSP must be powered down ([`error::BootFailure::dsp_dirty`]).
//!
//! SPEAKERS ARE PLANNED ONLY AS THE PROTECTED SPEAKER PATH (DECISIONS D17, amending D11/D14). The only gateways a path may hold are the
//! host-output copier, a gateway-less copier in the front end ([`plan::AVS_DMA_NO_GATEWAY`]) and the I2S-output copier of the one SSP the caller
//! asked for. A back end on one of the board's speaker SSPs ([`config::PlaybackSel::speaker_ssp`]) is refused by name before a single message is
//! sent ([`plan::PlanError::SpeakerPathRefused`]) UNLESS the caller names the speaker ceiling ([`config::PlaybackSel::speaker_ceiling`]): then the
//! back end MUST be on a speaker SSP, the path MUST hold a peakvol module (a path nothing in the DSP could attenuate is refused,
//! [`plan::PlanError::SpeakerPathWithoutVolume`]) and that module's ceiling is the caller's, lowered - never raised - by what the topology
//! declares.
//!
//! THE VOLUME CEILING. A peakvol module's `target_volume` in the TOPOLOGY is its ceiling ([`volume`]): the module is initialised silent
//! (Linux initialises it at `S32_MAX`, path.c:589-593, and sets the user volume afterwards), a topology that declares a zero or
//! unrepresentable ceiling is refused, and [`volume::PeakvolHandle::set_volume`] refuses a request above it. A headphone path with no peakvol
//! module (the da7219 topology has none) reports that fact; the DA7219 gain ceiling (da7219-core) is then the hardware limit. A SPEAKER path
//! with none is refused.

#![no_std]
#![forbid(unsafe_code)]

// `alloc`: the message script, the per-path handle lists and the IPC payload copies are sized by the topology file and the firmware's
// answers, never by a constant (DECISIONS.md D10, doctrine RUNTIME-TABLES-HAVE-NO-CAP); every growth is fallible and exhaustion is the
// named `OutOfMemory`.
extern crate alloc;

pub mod boot;
pub mod config;
pub mod error;
pub mod msg;
pub mod plan;
pub mod running;
pub mod volume;
