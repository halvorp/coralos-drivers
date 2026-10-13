// SPDX-License-Identifier: GPL-2.0-only
//! Everything the orchestrator is given. Nothing here is read from the device or from a file by this crate: the `.device` reads the
//! firmware, topology and NHLT files and the controller's capabilities and passes the results in.
use alloc::vec::Vec;
use avs_fw_core::bdl::ClDmaLayout;
use avs_fw_core::regs::AVS_LIB_NAME_SIZE;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::Topology;
use hda_ext_core::bdl::Bdl;
use hda_ext_core::stream::Streams;

/// A PCM's hardware parameters as `avs_test_hw_params` compares them (path.c:85-92).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamParams {
    pub rate: u32,
    pub channels: u32,
    /// `params_physical_width`: the container, 16 or 32.
    pub container_bits: u32,
    /// `snd_pcm_hw_params_bits`: the valid bits.
    pub valid_bits: u32,
}

/// A library image the `.device` has read: its manifest name (`man->name`) and its stripped length. Aligned by index with the topology's
/// library table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibImage {
    pub name: [u8; AVS_LIB_NAME_SIZE],
    pub image_len: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct PlaybackSel<'a> {
    /// The topology's playback widgets: the front end (host copier) and the back end (SSP copier). Names of the topology file
    /// (the Atlas headset's are `ssp1p_fe` / `ssp1p_be` in `da7219-tplg.bin`).
    pub fe_widget: &'a [u8],
    pub be_widget: &'a [u8],
    pub fe: StreamParams,
    pub be: StreamParams,
    /// The SSP the back end must be on: BOARD data from the board row (ZERO-HW-OUTSIDE-BSP). Atlas' headset codec is on SSP1.
    pub ssp_instance: u32,
    /// The SSPs of the board's speaker amplifiers, from the same row (Atlas: SSP0, the two MAX98373). A back end on any of them is the
    /// speaker path and is refused by name ([`crate::plan::PlanError::SpeakerPathRefused`]), whatever `ssp_instance` says.
    pub speaker_ssp: &'a [u32],
}

/// The host stream and its DMA memory, prepared by the `.device` from hda-ext-core's walks and planners.
#[derive(Clone, Debug)]
pub struct HostSetup {
    pub streams: Streams,
    /// Which playback stream (0-based) carries this path; its tag minus one is the DSP's DMA id.
    pub playback_stream: u32,
    /// `Caps::pp` (BAR0 offset of the processing-pipe capability).
    pub pp: u32,
    /// `Caps::spb`, when SPIB is wanted.
    pub spb: Option<u32>,
    pub bdl: Bdl,
    pub bdl_addr: u64,
    pub posbuf: bool,
    /// The SPIB value to write (usually the buffer size), if the SPB capability is used.
    pub spib: Option<u32>,
    /// Bits per sample for the stream format word (`snd_hdac_stream_format_bits`, pcm.c:770).
    pub format_bits: u32,
}

pub struct BootConfig<'a> {
    pub topology: &'a Topology<'a>,
    pub nhlt: &'a Nhlt<'a>,
    pub layout: ClDmaLayout,
    /// The stripped base-firmware image length (after `avs_fw_manifest_strip_verify`).
    pub basefw_len: u32,
    pub libs: Vec<LibImage>,
    pub playback: PlaybackSel<'a>,
    pub host: HostSetup,
    /// The oldest firmware accepted (`skl_desc.min_fw_version`, core.c:817).
    pub min_fw: (u16, u16, u16, u16),
}
