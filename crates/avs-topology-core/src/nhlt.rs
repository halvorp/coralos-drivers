// SPDX-License-Identifier: GPL-2.0-only
//! The ACPI NHLT ("Non HDAudio Link Table"): the firmware's description of every non-HDA audio endpoint - for the Pixelbook Go,
//! DMIC, the DA7219 headset codec on SSP1 and the MAX98373 amplifiers on SSP0 - with, per supported format, the
//! hardware-specific configuration blob the DSP's gateway is initialised with.
//!
//! Layout: `include/acpi/actbl2.h:2397-2578` (all fields packed, little-endian): the 36-byte ACPI header and the endpoint count,
//! then the endpoints (each `length` bytes: a 19-byte header, the device config, the formats config, optionally a devices-info
//! section), then an optional OED config used by Windows only. Lookups: `drivers/acpi/nhlt.c`. The AVS selection of a gateway
//! blob (`avs_nhlt_config_or_default`): `sound/soc/intel/avs/path.c:275-335`.
//!
//! Linux walks the table through pointer macros that trust every `length` and `capabilities_size`. Here each one is checked
//! against the structure that contains it, and the walk stops at the first inconsistency with a named error.
use avs_ipc_core::modcfg::{AudioFormat, AVS_DMA_DMIC_LINK_INPUT, AVS_DMA_I2S_LINK_INPUT, AVS_DMA_I2S_LINK_OUTPUT};

pub const ACPI_TABLE_HEADER_SIZE: usize = 36; // actbl.h:68-80 struct acpi_table_header
/// `sizeof(struct acpi_table_nhlt)`: the header and `endpoints_count`.
pub const NHLT_TABLE_SIZE: usize = 37; // actbl2.h:2397-2404
/// `sizeof(struct acpi_nhlt_endpoint)`.
pub const ENDPOINT_HEADER_SIZE: usize = 19; // actbl2.h:2406-2422
/// `sizeof(struct acpi_nhlt_wave_formatext)`.
pub const WAVE_FORMAT_SIZE: usize = 40; // actbl2.h:2545-2556
/// `sizeof(struct acpi_nhlt_device_info)`.
pub const DEVICE_INFO_SIZE: usize = 18; // actbl2.h:2568-2572
/// `sizeof(struct acpi_nhlt_vendor_mic_config)`.
pub const VENDOR_MIC_CONFIG_SIZE: usize = 22; // actbl2.h:2493-2506

pub const ACPI_NHLT_LINKTYPE_HDA: u8 = 0; // actbl2.h:2429
pub const ACPI_NHLT_LINKTYPE_DSP: u8 = 1; // actbl2.h:2430
pub const ACPI_NHLT_LINKTYPE_PDM: u8 = 2; // actbl2.h:2431
pub const ACPI_NHLT_LINKTYPE_SSP: u8 = 3; // actbl2.h:2432
pub const ACPI_NHLT_LINKTYPE_SLIMBUS: u8 = 4; // actbl2.h:2433
pub const ACPI_NHLT_LINKTYPE_SDW: u8 = 5; // actbl2.h:2434
pub const ACPI_NHLT_LINKTYPE_UAOL: u8 = 6; // actbl2.h:2435

pub const ACPI_NHLT_DEVICEID_DMIC: u16 = 0xAE20; // actbl2.h:2439
pub const ACPI_NHLT_DEVICEID_BT: u16 = 0xAE30; // actbl2.h:2440
pub const ACPI_NHLT_DEVICEID_I2S: u16 = 0xAE34; // actbl2.h:2441

pub const ACPI_NHLT_DEVICETYPE_PDM: u8 = 0; // actbl2.h:2450
pub const ACPI_NHLT_DEVICETYPE_PDM_SKL: u8 = 1; // actbl2.h:2451
pub const ACPI_NHLT_DEVICETYPE_BT: u8 = 0; // actbl2.h:2453
pub const ACPI_NHLT_DEVICETYPE_FM: u8 = 1; // actbl2.h:2454
pub const ACPI_NHLT_DEVICETYPE_MODEM: u8 = 2; // actbl2.h:2455
pub const ACPI_NHLT_DEVICETYPE_CODEC: u8 = 4; // actbl2.h:2456

pub const ACPI_NHLT_DIR_RENDER: u8 = 0; // actbl2.h:2460
pub const ACPI_NHLT_DIR_CAPTURE: u8 = 1; // actbl2.h:2461

pub const ACPI_NHLT_CONFIGTYPE_GENERIC: u8 = 0; // actbl2.h:2475
pub const ACPI_NHLT_CONFIGTYPE_MICARRAY: u8 = 1; // actbl2.h:2476

pub const ACPI_NHLT_ARRAYTYPE_LINEAR2_SMALL: u8 = 0xA; // actbl2.h:2486
pub const ACPI_NHLT_ARRAYTYPE_LINEAR2_BIG: u8 = 0xB; // actbl2.h:2487
pub const ACPI_NHLT_ARRAYTYPE_LINEAR4_GEO1: u8 = 0xC; // actbl2.h:2488
pub const ACPI_NHLT_ARRAYTYPE_PLANAR4_LSHAPED: u8 = 0xD; // actbl2.h:2489
pub const ACPI_NHLT_ARRAYTYPE_LINEAR4_GEO2: u8 = 0xE; // actbl2.h:2490
pub const ACPI_NHLT_ARRAYTYPE_VENDOR: u8 = 0xF; // actbl2.h:2491

/// `SNDRV_PCM_STREAM_PLAYBACK` / `_CAPTURE` as `avs_nhlt_config_or_default` passes them as the NHLT direction (path.c:289, :296).
pub const STREAM_PLAYBACK: u8 = 0; // asound.h:167
pub const STREAM_CAPTURE: u8 = 1; // asound.h:168

/// "Every BLOB contains at least gateway attributes": `default_blob` is `{ capabilities_size = 4, capabilities = 0 }`
/// (path.c:275).
pub const DEFAULT_BLOB: [u8; 4] = [0; 4]; // path.c:275
pub const DEFAULT_BLOB_SIZE: u32 = 4; // path.c:275

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NhltError {
    Truncated { what: &'static str, need: usize, have: usize },
    /// The table does not begin with "NHLT".
    BadSignature,
    /// The table's `length` is shorter than its own header, or longer than the bytes given.
    BadLength { length: u32, have: usize },
    /// The ACPI checksum does not make the table sum to zero.
    BadChecksum { sum: u8 },
    /// An endpoint's `length` is smaller than the fixed parts it must contain.
    EndpointTooShort { index: usize, length: u32, need: usize },
    /// An endpoint's `length` runs past the table.
    EndpointPastTable { index: usize, length: u32, left: usize },
    /// A capabilities size, format count or device count runs past its endpoint.
    PastEndpoint { index: usize, what: &'static str, need: usize, have: usize },
    /// `endpoints_count` states more endpoints than the table holds.
    MissingEndpoints { states: u8, found: usize },
    /// No endpoint of the wanted kind has the wanted format (`-ENOENT`, path.c:327-330).
    FormatNotFound,
    /// The blob is smaller than the 4 bytes of gateway attributes every blob carries (`-ETOOSMALL`, path.c:332-333).
    BlobTooSmall { size: u32 },
    /// "The firmware expects the payload to be DWORD-aligned" (`-EINVAL`, path.c:334-336).
    BlobNotDwordAligned { size: u32 },
    /// A vendor microphone array whose size does not match its microphone count (`-EINVAL`, nhlt.c:233-239).
    BadMicArray,
    /// `mic_count` asked of an endpoint that is not on a PDM link (`-EINVAL`, nhlt.c:251).
    NotPdm,
}

fn rd16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn rd32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// `struct acpi_nhlt_wave_formatext`: Microsoft's WAVEFORMATEXTENSIBLE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveFormat {
    pub format_tag: u16,
    pub channel_count: u16,
    pub samples_per_sec: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub extra_format_size: u16,
    pub valid_bits_per_sample: u16,
    pub channel_mask: u32,
    pub subformat: [u8; 16],
}

/// One supported format and its capabilities blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatConfig<'a> {
    pub format: WaveFormat,
    pub caps: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint<'a> {
    pub index: usize,
    pub link_type: u8,
    pub instance_id: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub revision_id: u16,
    pub subsystem_id: u32,
    pub device_type: u8,
    pub direction: u8,
    pub virtual_bus_id: u8,
    /// `device_config.capabilities`.
    pub device_caps: &'a [u8],
    pub formats_count: u8,
    formats: &'a [u8],
    /// Whatever follows the formats inside the endpoint (the devices-info section).
    pub trailer: &'a [u8],
}

impl<'a> Endpoint<'a> {
    /// `acpi_nhlt_endpoint_match` (nhlt.c:68-77): `None` ignores the criterion.
    pub fn matches(&self, link_type: Option<u8>, dev_type: Option<u8>, dir: Option<u8>, bus_id: Option<u8>) -> bool {
        link_type.map_or(true, |v| self.link_type == v)
            && dev_type.map_or(true, |v| self.device_type == v)
            && bus_id.map_or(true, |v| self.virtual_bus_id == v)
            && dir.map_or(true, |v| self.direction == v)
    }

    /// The format configurations, in order (`for_each_nhlt_endpoint_fmtcfg`).
    pub fn formats(&self) -> FormatIter<'a> {
        FormatIter { index: self.index, left: self.formats, remaining: self.formats_count as usize, failed: false }
    }

    /// `acpi_nhlt_endpoint_find_fmtcfg` (nhlt.c:140-156).
    pub fn find_fmtcfg(&self, ch: u16, rate: u32, vbps: u16, bps: u16) -> Result<Option<FormatConfig<'a>>, NhltError> {
        for f in self.formats() {
            let f = f?;
            let w = &f.format;
            if w.valid_bits_per_sample == vbps && w.samples_per_sec == rate && w.bits_per_sample == bps && w.channel_count == ch {
                return Ok(Some(f));
            }
        }
        Ok(None)
    }

    /// `acpi_nhlt_endpoint_mic_count` (nhlt.c:247-286): how many microphones a PDM endpoint has.
    pub fn mic_count(&self) -> Result<u32, NhltError> {
        if self.link_type != ACPI_NHLT_LINKTYPE_PDM {
            return Err(NhltError::NotPdm);
        }
        let mut max_ch = 0u16;
        for f in self.formats() {
            max_ch = max_ch.max(f?.format.channel_count);
        }
        let caps = self.device_caps;
        // "If @ep is not a mic array, fallback to channels count."
        if caps.len() < 3 || caps[1] != ACPI_NHLT_CONFIGTYPE_MICARRAY {
            return Ok(max_ch as u32);
        }
        match caps[2] {
            ACPI_NHLT_ARRAYTYPE_LINEAR2_SMALL | ACPI_NHLT_ARRAYTYPE_LINEAR2_BIG => Ok(2),
            ACPI_NHLT_ARRAYTYPE_LINEAR4_GEO1 | ACPI_NHLT_ARRAYTYPE_PLANAR4_LSHAPED | ACPI_NHLT_ARRAYTYPE_LINEAR4_GEO2 => Ok(4),
            ACPI_NHLT_ARRAYTYPE_VENDOR => {
                // acpi_nhlt_config_is_vendor_micdevice: the size must be 4 + mics_count * 22
                if caps.len() < 4 || caps.len() != 4 + caps[3] as usize * VENDOR_MIC_CONFIG_SIZE {
                    return Err(NhltError::BadMicArray);
                }
                Ok(caps[3] as u32)
            }
            // "undefined mic array type": Linux warns and returns the channel count
            _ => Ok(max_ch as u32),
        }
    }
}

pub struct FormatIter<'a> {
    index: usize,
    left: &'a [u8],
    remaining: usize,
    failed: bool,
}

impl<'a> Iterator for FormatIter<'a> {
    type Item = Result<FormatConfig<'a>, NhltError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let b = self.left;
        let fixed = WAVE_FORMAT_SIZE + 4;
        if b.len() < fixed {
            self.failed = true;
            return Some(Err(NhltError::PastEndpoint { index: self.index, what: "format configuration", need: fixed, have: b.len() }));
        }
        let size = rd32(b, WAVE_FORMAT_SIZE) as usize;
        if size > b.len() - fixed {
            self.failed = true;
            return Some(Err(NhltError::PastEndpoint { index: self.index, what: "format capabilities", need: size, have: b.len() - fixed }));
        }
        let mut sub = [0u8; 16];
        sub.copy_from_slice(&b[24..40]);
        let w = WaveFormat {
            format_tag: rd16(b, 0),
            channel_count: rd16(b, 2),
            samples_per_sec: rd32(b, 4),
            avg_bytes_per_sec: rd32(b, 8),
            block_align: rd16(b, 12),
            bits_per_sample: rd16(b, 14),
            extra_format_size: rd16(b, 16),
            valid_bits_per_sample: rd16(b, 18),
            channel_mask: rd32(b, 20),
            subformat: sub,
        };
        self.left = &b[fixed + size..];
        Some(Ok(FormatConfig { format: w, caps: &b[fixed..fixed + size] }))
    }
}

/// A validated NHLT table.
#[derive(Clone, Copy, Debug)]
pub struct Nhlt<'a> {
    table: &'a [u8],
    pub endpoints_count: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
}

impl<'a> Nhlt<'a> {
    /// Validate the table header. `verify_checksum` applies the ACPI rule (all bytes of the table sum to 0 mod 256), which ACPICA
    /// applies before Linux ever sees the table.
    pub fn parse(bytes: &'a [u8], verify_checksum: bool) -> Result<Self, NhltError> {
        if bytes.len() < NHLT_TABLE_SIZE {
            return Err(NhltError::Truncated { what: "NHLT table header", need: NHLT_TABLE_SIZE, have: bytes.len() });
        }
        if &bytes[0..4] != b"NHLT" {
            return Err(NhltError::BadSignature);
        }
        let length = rd32(bytes, 4);
        if (length as usize) < NHLT_TABLE_SIZE || length as usize > bytes.len() {
            return Err(NhltError::BadLength { length, have: bytes.len() });
        }
        let table = &bytes[..length as usize];
        if verify_checksum {
            let sum = table.iter().fold(0u8, |a, &b| a.wrapping_add(b));
            if sum != 0 {
                return Err(NhltError::BadChecksum { sum });
            }
        }
        let mut oem_id = [0u8; 6];
        oem_id.copy_from_slice(&table[10..16]);
        let mut oem_table_id = [0u8; 8];
        oem_table_id.copy_from_slice(&table[16..24]);
        Ok(Self { table, endpoints_count: table[ACPI_TABLE_HEADER_SIZE], oem_id, oem_table_id, oem_revision: rd32(table, 24) })
    }

    /// The endpoints in table order (`for_each_nhlt_endpoint`).
    pub fn endpoints(&self) -> EndpointIter<'a> {
        EndpointIter { left: &self.table[NHLT_TABLE_SIZE..], remaining: self.endpoints_count as usize, index: 0, failed: false, states: self.endpoints_count }
    }

    /// `acpi_nhlt_tb_find_endpoint` (nhlt.c:94-102): the first endpoint matching all criteria that is given.
    pub fn find_endpoint(&self, link_type: Option<u8>, dev_type: Option<u8>, dir: Option<u8>, bus_id: Option<u8>) -> Result<Option<Endpoint<'a>>, NhltError> {
        for e in self.endpoints() {
            let e = e?;
            if e.matches(link_type, dev_type, dir, bus_id) {
                return Ok(Some(e));
            }
        }
        Ok(None)
    }

    /// `acpi_nhlt_tb_find_fmtcfg` (nhlt.c:180-198): the first format, among all matching endpoints, with these parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn find_fmtcfg(
        &self,
        link_type: Option<u8>,
        dev_type: Option<u8>,
        dir: Option<u8>,
        bus_id: Option<u8>,
        ch: u16,
        rate: u32,
        vbps: u16,
        bps: u16,
    ) -> Result<Option<FormatConfig<'a>>, NhltError> {
        for e in self.endpoints() {
            let e = e?;
            if !e.matches(link_type, dev_type, dir, bus_id) {
                continue;
            }
            if let Some(f) = e.find_fmtcfg(ch, rate, vbps, bps)? {
                return Ok(Some(f));
            }
        }
        Ok(None)
    }

    /// `avs_nhlt_config_or_default` (path.c:277-335): the capabilities blob a copier's gateway is initialised with.
    ///
    /// `dma_type` and `vindex` are the topology's copier settings (for I2S, `vindex >> 4` is the SSP port = the NHLT virtual bus
    /// id); `in_fmt` is the module's input format, `out_fmt` the copier's output format and `blob_fmt` the optional override.
    /// A gateway that is not I2S or DMIC gets [`DEFAULT_BLOB`].
    pub fn config_or_default(&self, dma_type: u32, vindex: u8, in_fmt: &AudioFormat, out_fmt: &AudioFormat, blob_fmt: Option<&AudioFormat>) -> Result<&'a [u8], NhltError>
    {
        let (link, dev, bus, dir, fmt) = match dma_type {
            AVS_DMA_I2S_LINK_OUTPUT => (ACPI_NHLT_LINKTYPE_SSP, Some(ACPI_NHLT_DEVICETYPE_CODEC), vindex >> 4, STREAM_PLAYBACK, out_fmt),
            AVS_DMA_I2S_LINK_INPUT => (ACPI_NHLT_LINKTYPE_SSP, Some(ACPI_NHLT_DEVICETYPE_CODEC), vindex >> 4, STREAM_CAPTURE, in_fmt),
            AVS_DMA_DMIC_LINK_INPUT => (ACPI_NHLT_LINKTYPE_PDM, None, 0, STREAM_CAPTURE, in_fmt),
            _ => return Ok(&DEFAULT_BLOB_STATIC),
        };
        // "Override format selection if necessary."
        let fmt = blob_fmt.unwrap_or(fmt);
        let f = self
            .find_fmtcfg(Some(link), dev, Some(dir), Some(bus), fmt.num_channels as u16, fmt.sampling_freq, fmt.valid_bit_depth as u16, fmt.bit_depth as u16)?
            .ok_or(NhltError::FormatNotFound)?;
        let size = f.caps.len() as u32;
        if size < DEFAULT_BLOB_SIZE {
            return Err(NhltError::BlobTooSmall { size });
        }
        if size % 4 != 0 {
            return Err(NhltError::BlobNotDwordAligned { size });
        }
        Ok(f.caps)
    }
}

static DEFAULT_BLOB_STATIC: [u8; 4] = DEFAULT_BLOB;

pub struct EndpointIter<'a> {
    left: &'a [u8],
    remaining: usize,
    index: usize,
    failed: bool,
    states: u8,
}

impl<'a> Iterator for EndpointIter<'a> {
    type Item = Result<Endpoint<'a>, NhltError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let index = self.index;
        self.index += 1;
        let b = self.left;
        let fail = |s: &mut Self, e: NhltError| {
            s.failed = true;
            Some(Err(e))
        };
        if b.len() < 4 {
            return fail(self, NhltError::MissingEndpoints { states: self.states, found: index });
        }
        let length = rd32(b, 0);
        // header + device config size + formats count at the least
        let need = ENDPOINT_HEADER_SIZE + 4 + 1;
        if (length as usize) < need {
            return fail(self, NhltError::EndpointTooShort { index, length, need });
        }
        if length as usize > b.len() {
            return fail(self, NhltError::EndpointPastTable { index, length, left: b.len() });
        }
        let ep = &b[..length as usize];
        let dcs = rd32(ep, ENDPOINT_HEADER_SIZE) as usize;
        let after_dev = ENDPOINT_HEADER_SIZE + 4;
        if dcs > ep.len() - after_dev || ep.len() - after_dev - dcs < 1 {
            return fail(self, NhltError::PastEndpoint { index, what: "device config", need: dcs + 1, have: ep.len() - after_dev });
        }
        let device_caps = &ep[after_dev..after_dev + dcs];
        let fmt_at = after_dev + dcs;
        let formats_count = ep[fmt_at];
        // The formats span from here to the end of the last one; walk them once to find where, so `trailer` is exact.
        let mut it = FormatIter { index, left: &ep[fmt_at + 1..], remaining: formats_count as usize, failed: false };
        for f in it.by_ref() {
            if let Err(e) = f {
                return fail(self, e);
            }
        }
        let formats_len = ep.len() - (fmt_at + 1) - it.left.len();
        self.left = &b[length as usize..];
        Some(Ok(Endpoint {
            index,
            link_type: ep[4],
            instance_id: ep[5],
            vendor_id: rd16(ep, 6),
            device_id: rd16(ep, 8),
            revision_id: rd16(ep, 10),
            subsystem_id: rd32(ep, 12),
            device_type: ep[16],
            direction: ep[17],
            virtual_bus_id: ep[18],
            device_caps,
            formats_count,
            formats: &ep[fmt_at + 1..fmt_at + 1 + formats_len],
            trailer: it.left,
        }))
    }
}

