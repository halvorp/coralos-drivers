// SPDX-License-Identifier: GPL-2.0-only
//! The NHLT table (actbl2.h:2397-2578, nhlt.c, path.c:277-335), on hand-built tables shaped like the Pixelbook Go's:
//! coreboot's `variant_nhlt_init` (mainboard/google/poppy/variants/atlas/nhlt.c) adds a 2-channel DMIC array, the Dialog DA7219
//! headset codec on SSP1 (render + capture, one format: 2 channels, 48 kHz, 32-bit container, 24 valid bits, TDM-basic config)
//! and the Maxim MAX98373 amplifiers on SSP0. The blobs below are placeholders with the right sizes; the REAL table is the
//! owner's Linux reconnaissance boot to capture (`/sys/firmware/acpi/tables/NHLT`).
use avs_ipc_core::modcfg::*;
use avs_topology_core::nhlt::*;

fn dw(x: u32) -> [u8; 4] {
    x.to_le_bytes()
}

struct Fmt {
    ch: u16,
    rate: u32,
    container: u16,
    valid: u16,
    mask: u32,
    caps: Vec<u8>,
}

fn wave(f: &Fmt) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&0xFFFEu16.to_le_bytes()); // WAVE_FORMAT_EXTENSIBLE
    v.extend_from_slice(&f.ch.to_le_bytes());
    v.extend_from_slice(&f.rate.to_le_bytes());
    v.extend_from_slice(&(f.rate * f.ch as u32 * (f.container as u32 / 8)).to_le_bytes());
    v.extend_from_slice(&(f.ch * (f.container / 8)).to_le_bytes());
    v.extend_from_slice(&f.container.to_le_bytes());
    v.extend_from_slice(&22u16.to_le_bytes()); // extra_format_size
    v.extend_from_slice(&f.valid.to_le_bytes());
    v.extend_from_slice(&f.mask.to_le_bytes());
    v.extend_from_slice(&[0x01, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]); // KSDATAFORMAT_SUBTYPE_PCM
    assert_eq!(v.len(), 40);
    v.extend_from_slice(&dw(f.caps.len() as u32));
    v.extend_from_slice(&f.caps);
    v
}

#[allow(clippy::too_many_arguments)]
fn endpoint(link: u8, instance: u8, device_id: u16, dev_type: u8, dir: u8, vbus: u8, dev_caps: &[u8], fmts: &[Fmt], trailer: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(link);
    body.push(instance);
    body.extend_from_slice(&0x8086u16.to_le_bytes());
    body.extend_from_slice(&device_id.to_le_bytes());
    body.extend_from_slice(&1u16.to_le_bytes()); // revision
    body.extend_from_slice(&0x0000_0123u32.to_le_bytes()); // subsystem
    body.push(dev_type);
    body.push(dir);
    body.push(vbus);
    body.extend_from_slice(&dw(dev_caps.len() as u32));
    body.extend_from_slice(dev_caps);
    body.push(fmts.len() as u8);
    for f in fmts {
        body.extend(wave(f));
    }
    body.extend_from_slice(trailer);
    let mut v = Vec::new();
    v.extend_from_slice(&dw(4 + body.len() as u32));
    v.extend(body);
    v
}

fn table(eps: &[Vec<u8>], tail: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"NHLT");
    v.extend_from_slice(&[0u8; 4]); // length, patched
    v.push(1); // revision
    v.push(0); // checksum, patched
    v.extend_from_slice(b"GOOGLE");
    v.extend_from_slice(b"ATLASMAX");
    v.extend_from_slice(&dw(0)); // oem revision
    v.extend_from_slice(b"CORL");
    v.extend_from_slice(&dw(1));
    assert_eq!(v.len(), 36);
    v.push(eps.len() as u8);
    for e in eps {
        v.extend_from_slice(e);
    }
    v.extend_from_slice(tail);
    let n = v.len() as u32;
    v[4..8].copy_from_slice(&dw(n));
    let sum = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v[9] = 0u8.wrapping_sub(sum);
    v
}

fn blob(n: usize, seed: u8) -> Vec<u8> {
    (0..n).map(|i| seed.wrapping_add(i as u8)).collect()
}

fn da7219_fmt() -> Fmt {
    Fmt { ch: 2, rate: 48000, container: 32, valid: 24, mask: 3, caps: blob(88, 0x10) }
}

/// DMIC 2ch + DA7219 render/capture on SSP1 + MAX98373 render (SSP0) and feedback capture.
fn atlas_like() -> Vec<u8> {
    let tdm_basic = [0u8, 0]; // acpi_nhlt_gendevice_config { virtual_slot 0, config_type generic }
    let dmic_cfg = [0u8, ACPI_NHLT_CONFIGTYPE_MICARRAY, ACPI_NHLT_ARRAYTYPE_LINEAR2_SMALL];
    let dmic_fmt = |container: u16, caps| Fmt { ch: 2, rate: 48000, container, valid: container, mask: 3, caps };
    let max_fmt = Fmt { ch: 4, rate: 48000, container: 32, valid: 32, mask: 0xF, caps: blob(88, 0x40) };
    table(
        &[
            endpoint(ACPI_NHLT_LINKTYPE_PDM, 0, ACPI_NHLT_DEVICEID_DMIC, ACPI_NHLT_DEVICETYPE_PDM_SKL, ACPI_NHLT_DIR_CAPTURE, 0, &dmic_cfg, &[dmic_fmt(16, blob(8, 1)), dmic_fmt(32, blob(12, 2))], &[]),
            endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_RENDER, 1, &tdm_basic, &[da7219_fmt()], &[0u8]),
            endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_CAPTURE, 1, &tdm_basic, &[Fmt { caps: blob(88, 0x20), ..da7219_fmt() }], &[0u8]),
            endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_RENDER, 0, &tdm_basic, &[max_fmt], &[0u8]),
        ],
        &[0, 0, 0, 0], // an OED config (Windows only): zero-sized
    )
}

#[test]
fn constants() {
    assert_eq!((ACPI_TABLE_HEADER_SIZE, NHLT_TABLE_SIZE, ENDPOINT_HEADER_SIZE, WAVE_FORMAT_SIZE, DEVICE_INFO_SIZE, VENDOR_MIC_CONFIG_SIZE), (36, 37, 19, 40, 18, 22));
    assert_eq!((ACPI_NHLT_LINKTYPE_HDA, ACPI_NHLT_LINKTYPE_DSP, ACPI_NHLT_LINKTYPE_PDM, ACPI_NHLT_LINKTYPE_SSP, ACPI_NHLT_LINKTYPE_SLIMBUS, ACPI_NHLT_LINKTYPE_SDW, ACPI_NHLT_LINKTYPE_UAOL), (0, 1, 2, 3, 4, 5, 6));
    assert_eq!((ACPI_NHLT_DEVICEID_DMIC, ACPI_NHLT_DEVICEID_BT, ACPI_NHLT_DEVICEID_I2S), (0xAE20, 0xAE30, 0xAE34));
    assert_eq!((ACPI_NHLT_DEVICETYPE_PDM, ACPI_NHLT_DEVICETYPE_PDM_SKL, ACPI_NHLT_DEVICETYPE_BT, ACPI_NHLT_DEVICETYPE_FM, ACPI_NHLT_DEVICETYPE_MODEM, ACPI_NHLT_DEVICETYPE_CODEC), (0, 1, 0, 1, 2, 4));
    assert_eq!((ACPI_NHLT_DIR_RENDER, ACPI_NHLT_DIR_CAPTURE, ACPI_NHLT_CONFIGTYPE_GENERIC, ACPI_NHLT_CONFIGTYPE_MICARRAY), (0, 1, 0, 1));
    assert_eq!(
        (ACPI_NHLT_ARRAYTYPE_LINEAR2_SMALL, ACPI_NHLT_ARRAYTYPE_LINEAR2_BIG, ACPI_NHLT_ARRAYTYPE_LINEAR4_GEO1, ACPI_NHLT_ARRAYTYPE_PLANAR4_LSHAPED, ACPI_NHLT_ARRAYTYPE_LINEAR4_GEO2, ACPI_NHLT_ARRAYTYPE_VENDOR),
        (0xA, 0xB, 0xC, 0xD, 0xE, 0xF)
    );
    assert_eq!((STREAM_PLAYBACK, STREAM_CAPTURE), (0, 1));
    assert_eq!((DEFAULT_BLOB, DEFAULT_BLOB_SIZE), ([0u8; 4], 4));
}

#[test]
fn the_table_header_is_validated_and_decoded() {
    let t = atlas_like();
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints_count, 4);
    assert_eq!(&n.oem_id, b"GOOGLE");
    assert_eq!(&n.oem_table_id, b"ATLASMAX");
    assert_eq!(n.oem_revision, 0);
    // refusals
    assert!(matches!(Nhlt::parse(&t[..36], true), Err(NhltError::Truncated { what: "NHLT table header", need: 37, have: 36 })));
    let mut b = t.clone();
    b[0] = b'X';
    assert_eq!(Nhlt::parse(&b, true).err(), Some(NhltError::BadSignature));
    let mut b = t.clone();
    b[9] = b[9].wrapping_add(1);
    assert_eq!(Nhlt::parse(&b, true).err(), Some(NhltError::BadChecksum { sum: 1 }));
    assert!(Nhlt::parse(&b, false).is_ok(), "the checksum check is optional");
    let mut b = t.clone();
    b[4..8].copy_from_slice(&dw(t.len() as u32 + 1));
    assert_eq!(Nhlt::parse(&b, false).err(), Some(NhltError::BadLength { length: t.len() as u32 + 1, have: t.len() }));
    let mut b = t.clone();
    b[4..8].copy_from_slice(&dw(36));
    assert_eq!(Nhlt::parse(&b, false).err(), Some(NhltError::BadLength { length: 36, have: t.len() }));
    // bytes after the table's own length are not part of it
    let mut longer = t.clone();
    longer.extend_from_slice(&[0xEE; 9]);
    assert!(Nhlt::parse(&longer, true).is_ok());
}

#[test]
fn endpoints_are_walked_in_order_with_every_field() {
    let t = atlas_like();
    let n = Nhlt::parse(&t, true).unwrap();
    let eps: Vec<_> = n.endpoints().map(|e| e.unwrap()).collect();
    assert_eq!(eps.len(), 4);
    assert_eq!(eps.iter().map(|e| e.index).collect::<Vec<_>>(), [0, 1, 2, 3]);
    let e = &eps[1];
    assert_eq!((e.link_type, e.instance_id, e.vendor_id, e.device_id, e.revision_id, e.subsystem_id, e.device_type, e.direction, e.virtual_bus_id), (3, 0, 0x8086, 0xAE34, 1, 0x123, 4, 0, 1));
    assert_eq!(e.device_caps, &[0, 0]);
    assert_eq!(e.formats_count, 1);
    assert_eq!(e.trailer, &[0u8], "the devices-info section's count");
    let f: Vec<_> = e.formats().map(|f| f.unwrap()).collect();
    assert_eq!(f.len(), 1);
    let w = f[0].format;
    assert_eq!((w.format_tag, w.channel_count, w.samples_per_sec, w.avg_bytes_per_sec, w.block_align, w.bits_per_sample, w.extra_format_size, w.valid_bits_per_sample, w.channel_mask), (0xFFFE, 2, 48000, 384000, 8, 32, 22, 24, 3));
    assert_eq!(w.subformat, [0x01, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    assert_eq!(f[0].caps, &blob(88, 0x10)[..]);
    // two formats in the DMIC endpoint, in order
    let d: Vec<_> = eps[0].formats().map(|f| f.unwrap()).collect();
    assert_eq!(d.len(), 2);
    assert_eq!((d[0].format.bits_per_sample, d[0].caps.len()), (16, 8));
    assert_eq!((d[1].format.bits_per_sample, d[1].caps.len()), (32, 12));
    assert_eq!(eps[2].virtual_bus_id, 1);
    assert_eq!(eps[3].virtual_bus_id, 0);
}

#[test]
fn endpoint_matching_ignores_unset_criteria() {
    let t = atlas_like();
    let n = Nhlt::parse(&t, true).unwrap();
    let find = |l, d, r, b| n.find_endpoint(l, d, r, b).unwrap().map(|e| e.index);
    assert_eq!(find(None, None, None, None), Some(0), "everything unset: the first endpoint");
    assert_eq!(find(Some(ACPI_NHLT_LINKTYPE_SSP), None, None, None), Some(1));
    assert_eq!(find(Some(ACPI_NHLT_LINKTYPE_SSP), Some(ACPI_NHLT_DEVICETYPE_CODEC), Some(ACPI_NHLT_DIR_RENDER), Some(1)), Some(1));
    assert_eq!(find(Some(ACPI_NHLT_LINKTYPE_SSP), Some(ACPI_NHLT_DEVICETYPE_CODEC), Some(ACPI_NHLT_DIR_CAPTURE), Some(1)), Some(2));
    assert_eq!(find(Some(ACPI_NHLT_LINKTYPE_SSP), Some(ACPI_NHLT_DEVICETYPE_CODEC), Some(ACPI_NHLT_DIR_RENDER), Some(0)), Some(3));
    assert_eq!(find(Some(ACPI_NHLT_LINKTYPE_SSP), Some(ACPI_NHLT_DEVICETYPE_CODEC), Some(ACPI_NHLT_DIR_CAPTURE), Some(0)), None);
    assert_eq!(find(Some(ACPI_NHLT_LINKTYPE_PDM), None, None, None), Some(0));
    assert_eq!(find(Some(ACPI_NHLT_LINKTYPE_HDA), None, None, None), None);
    assert_eq!(find(None, Some(ACPI_NHLT_DEVICETYPE_PDM_SKL), None, None), Some(0));
    assert_eq!(find(None, None, Some(ACPI_NHLT_DIR_CAPTURE), Some(1)), Some(2));
    assert_eq!(find(None, None, None, Some(7)), None);
}

#[test]
fn format_lookup_matches_all_four_parameters() {
    let t = atlas_like();
    let n = Nhlt::parse(&t, true).unwrap();
    let ssp = Some(ACPI_NHLT_LINKTYPE_SSP);
    let render = Some(ACPI_NHLT_DIR_RENDER);
    let f = n.find_fmtcfg(ssp, None, render, Some(1), 2, 48000, 24, 32).unwrap().unwrap();
    assert_eq!(f.caps, &blob(88, 0x10)[..]);
    // each parameter wrong in turn
    assert!(n.find_fmtcfg(ssp, None, render, Some(1), 1, 48000, 24, 32).unwrap().is_none(), "channels");
    assert!(n.find_fmtcfg(ssp, None, render, Some(1), 2, 44100, 24, 32).unwrap().is_none(), "rate");
    assert!(n.find_fmtcfg(ssp, None, render, Some(1), 2, 48000, 32, 32).unwrap().is_none(), "valid bits");
    assert!(n.find_fmtcfg(ssp, None, render, Some(1), 2, 48000, 24, 24).unwrap().is_none(), "container bits");
    // the 4-channel speaker endpoint is on bus 0
    assert!(n.find_fmtcfg(ssp, None, render, Some(0), 4, 48000, 32, 32).unwrap().is_some());
    assert!(n.find_fmtcfg(ssp, None, render, Some(1), 4, 48000, 32, 32).unwrap().is_none(), "other bus");
    // endpoint-level lookup
    let e = n.find_endpoint(Some(ACPI_NHLT_LINKTYPE_PDM), None, None, None).unwrap().unwrap();
    assert_eq!(e.find_fmtcfg(2, 48000, 32, 32).unwrap().unwrap().caps.len(), 12);
    assert_eq!(e.find_fmtcfg(2, 48000, 16, 16).unwrap().unwrap().caps.len(), 8);
    assert!(e.find_fmtcfg(2, 48000, 16, 32).unwrap().is_none());
    // a matching format on a later endpoint of the same kind is found past a non-matching one
    let two = table(
        &[
            endpoint(3, 0, 0xAE34, 4, 0, 1, &[0, 0], &[Fmt { ch: 2, rate: 44100, container: 32, valid: 24, mask: 3, caps: blob(4, 1) }], &[]),
            endpoint(3, 0, 0xAE34, 4, 0, 1, &[0, 0], &[Fmt { ch: 2, rate: 48000, container: 32, valid: 24, mask: 3, caps: blob(4, 9) }], &[]),
        ],
        &[],
    );
    let n2 = Nhlt::parse(&two, true).unwrap();
    assert_eq!(n2.find_fmtcfg(ssp, None, render, Some(1), 2, 48000, 24, 32).unwrap().unwrap().caps, &blob(4, 9)[..]);
}

fn fmt_of(ch: u8, rate: u32, container: u32, valid: u8) -> AudioFormat {
    AudioFormat { sampling_freq: rate, bit_depth: container, num_channels: ch, valid_bit_depth: valid, ..AudioFormat::default() }
}

#[test]
fn the_gateway_blob_is_chosen_by_dma_type_bus_and_format() {
    // avs_nhlt_config_or_default (path.c:277-335)
    let t = atlas_like();
    let n = Nhlt::parse(&t, true).unwrap();
    let f24 = fmt_of(2, 48000, 32, 24);
    let other = fmt_of(2, 48000, 16, 16);
    // I2S OUTPUT uses the copier's OUT format, direction render, bus = vindex >> 4
    let b = n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x10, &other, &f24, None).unwrap();
    assert_eq!(b, &blob(88, 0x10)[..]);
    // I2S INPUT uses the module's IN format, direction capture
    let b = n.config_or_default(AVS_DMA_I2S_LINK_INPUT, 0x10, &f24, &other, None).unwrap();
    assert_eq!(b, &blob(88, 0x20)[..]);
    // the time slot (low nibble) does not select the bus
    assert!(n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x13, &other, &f24, None).is_ok());
    // bus 0 is the speaker amplifier endpoint (4 channels)
    let spk = fmt_of(4, 48000, 32, 32);
    assert_eq!(n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x00, &spk, &spk, None).unwrap(), &blob(88, 0x40)[..]);
    // the blob format overrides the selection format
    assert_eq!(n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x10, &other, &other, Some(&f24)).unwrap(), &blob(88, 0x10)[..]);
    // DMIC: PDM link, any device type, bus 0, capture, the module's IN format
    assert_eq!(n.config_or_default(AVS_DMA_DMIC_LINK_INPUT, 0, &fmt_of(2, 48000, 16, 16), &other, None).unwrap(), &blob(8, 1)[..]);
    assert_eq!(n.config_or_default(AVS_DMA_DMIC_LINK_INPUT, 0, &fmt_of(2, 48000, 32, 32), &other, None).unwrap(), &blob(12, 2)[..]);
    // anything else gets the default blob: just the gateway attributes
    for dma in [AVS_DMA_HDA_HOST_OUTPUT, AVS_DMA_HDA_HOST_INPUT, AVS_DMA_HDA_LINK_OUTPUT, AVS_DMA_HDA_LINK_INPUT, 99, u32::MAX] {
        assert_eq!(n.config_or_default(dma, 0, &f24, &f24, None).unwrap(), &[0u8; 4], "dma type {dma}");
    }
}

#[test]
fn the_gateway_blob_refusals() {
    let t = atlas_like();
    let n = Nhlt::parse(&t, true).unwrap();
    // "Endpoint format configuration not found." (path.c:327-330)
    assert_eq!(n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x10, &fmt_of(1, 48000, 32, 24), &fmt_of(1, 48000, 32, 24), None), Err(NhltError::FormatNotFound));
    assert_eq!(n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x20, &fmt_of(2, 48000, 32, 24), &fmt_of(2, 48000, 32, 24), None), Err(NhltError::FormatNotFound), "SSP2 has no endpoint");
    let one = |caps: Vec<u8>| table(&[endpoint(3, 0, 0xAE34, 4, 0, 1, &[0, 0], &[Fmt { caps, ..da7219_fmt() }], &[])], &[]);
    let f = fmt_of(2, 48000, 32, 24);
    // smaller than the 4-byte gateway attributes (path.c:332-333, -ETOOSMALL)
    for size in [0usize, 1, 2, 3] {
        let tb = one(blob(size, 0));
        let n = Nhlt::parse(&tb, true).unwrap();
        assert_eq!(n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x10, &f, &f, None), Err(NhltError::BlobTooSmall { size: size as u32 }), "size {size}");
    }
    // not a whole number of dwords (path.c:334-336, -EINVAL)
    for size in [5usize, 6, 7, 9, 90] {
        let tb = one(blob(size, 0));
        let n = Nhlt::parse(&tb, true).unwrap();
        assert_eq!(n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x10, &f, &f, None), Err(NhltError::BlobNotDwordAligned { size: size as u32 }), "size {size}");
    }
    // 4 bytes exactly is fine
    let tb = one(blob(4, 7));
    let n = Nhlt::parse(&tb, true).unwrap();
    assert_eq!(n.config_or_default(AVS_DMA_I2S_LINK_OUTPUT, 0x10, &f, &f, None).unwrap(), &blob(4, 7)[..]);
}

#[test]
fn microphone_counts() {
    let t = atlas_like();
    let n = Nhlt::parse(&t, true).unwrap();
    let dmic = n.find_endpoint(Some(ACPI_NHLT_LINKTYPE_PDM), None, None, None).unwrap().unwrap();
    assert_eq!(dmic.mic_count(), Ok(2), "LINEAR2_SMALL");
    let ssp = n.find_endpoint(Some(ACPI_NHLT_LINKTYPE_SSP), None, None, None).unwrap().unwrap();
    assert_eq!(ssp.mic_count(), Err(NhltError::NotPdm));
    let pdm = |cfg: &[u8], ch: u16| table(&[endpoint(2, 0, 0xAE20, 1, 1, 0, cfg, &[Fmt { ch, rate: 48000, container: 16, valid: 16, mask: 1, caps: blob(4, 0) }], &[])], &[]);
    let count = |cfg: &[u8], ch| {
        let tb = pdm(cfg, ch);
        let n = Nhlt::parse(&tb, true).unwrap();
        n.find_endpoint(Some(2), None, None, None).unwrap().unwrap().mic_count()
    };
    assert_eq!(count(&[0, 1, ACPI_NHLT_ARRAYTYPE_LINEAR2_BIG], 9), Ok(2));
    assert_eq!(count(&[0, 1, ACPI_NHLT_ARRAYTYPE_LINEAR4_GEO1], 9), Ok(4));
    assert_eq!(count(&[0, 1, ACPI_NHLT_ARRAYTYPE_PLANAR4_LSHAPED], 9), Ok(4));
    assert_eq!(count(&[0, 1, ACPI_NHLT_ARRAYTYPE_LINEAR4_GEO2], 9), Ok(4));
    // not a mic array (generic config, or too short to be one): the channel count
    assert_eq!(count(&[0, ACPI_NHLT_CONFIGTYPE_GENERIC, 0xA], 5), Ok(5));
    assert_eq!(count(&[0, 1], 3), Ok(3));
    assert_eq!(count(&[], 4), Ok(4));
    // an undefined array type: the channel count too
    assert_eq!(count(&[0, 1, 0x1], 6), Ok(6));
    // vendor array: 3 microphones need 4 + 3*22 bytes
    let mut v = vec![0u8, 1, ACPI_NHLT_ARRAYTYPE_VENDOR, 3];
    v.extend(vec![0u8; 66]);
    assert_eq!(count(&v, 1), Ok(3));
    v.pop();
    assert_eq!(count(&v, 1), Err(NhltError::BadMicArray));
    assert_eq!(count(&[0, 1, ACPI_NHLT_ARRAYTYPE_VENDOR], 1), Err(NhltError::BadMicArray));
    let mut zero = vec![0u8, 1, ACPI_NHLT_ARRAYTYPE_VENDOR, 0];
    assert_eq!(count(&zero, 1), Ok(0));
    zero.push(0);
    assert_eq!(count(&zero, 1), Err(NhltError::BadMicArray));
}

#[test]
fn an_inconsistent_table_stops_the_walk_with_a_named_error() {
    let t = atlas_like();
    let n = Nhlt::parse(&t, true).unwrap();
    // endpoints_count promises more than the table holds
    let mut b = t.clone();
    b[36] = 9;
    let n9 = Nhlt::parse(&b, false).unwrap();
    let r: Vec<_> = n9.endpoints().collect();
    assert!(r.iter().take(4).all(|x| x.is_ok()));
    // after the four real endpoints the 4 OED bytes remain: they parse as an endpoint of length 0 -> too short
    assert!(matches!(r[4], Err(NhltError::EndpointTooShort { index: 4, length: 0, .. })));
    assert_eq!(r.len(), 5, "the walk stops at the first error");
    let _ = n;
    // an endpoint length that runs past the table
    let mut b = t.clone();
    let first = 37;
    b[first..first + 4].copy_from_slice(&dw(100_000));
    let nb = Nhlt::parse(&b, false).unwrap();
    assert!(matches!(nb.endpoints().next().unwrap(), Err(NhltError::EndpointPastTable { index: 0, length: 100_000, .. })));
    // an endpoint shorter than its mandatory parts
    let mut b = t.clone();
    b[first..first + 4].copy_from_slice(&dw(20));
    let nb = Nhlt::parse(&b, false).unwrap();
    assert!(matches!(nb.endpoints().next().unwrap(), Err(NhltError::EndpointTooShort { index: 0, length: 20, need: 24 })));
    // a device-config size that runs past the endpoint
    let mut b = t.clone();
    b[first + 19..first + 23].copy_from_slice(&dw(1000));
    let nb = Nhlt::parse(&b, false).unwrap();
    assert!(matches!(nb.endpoints().next().unwrap(), Err(NhltError::PastEndpoint { index: 0, what: "device config", .. })));
    // a format count that promises more formats than the endpoint holds
    let mut b = t.clone();
    let e0_len = u32::from_le_bytes([b[first], b[first + 1], b[first + 2], b[first + 3]]) as usize;
    let _ = e0_len;
    let fmt_count_at = first + 19 + 4 + 3; // DMIC device config is 3 bytes
    b[fmt_count_at] = 3;
    let nb = Nhlt::parse(&b, false).unwrap();
    assert!(matches!(nb.endpoints().next().unwrap(), Err(NhltError::PastEndpoint { index: 0, what: "format configuration", .. })));
    // a format's capabilities size that runs past the endpoint
    let mut b = t.clone();
    b[fmt_count_at + 1 + 40..fmt_count_at + 1 + 44].copy_from_slice(&dw(5000));
    let nb = Nhlt::parse(&b, false).unwrap();
    assert!(matches!(nb.endpoints().next().unwrap(), Err(NhltError::PastEndpoint { index: 0, what: "format capabilities", need: 5000, .. })));
}

#[test]
fn no_prefix_or_corruption_of_the_table_panics() {
    let t = atlas_like();
    for n in 0..t.len() {
        let r = Nhlt::parse(&t[..n], false);
        if let Ok(nh) = r {
            for e in nh.endpoints() {
                if let Ok(e) = e {
                    for f in e.formats() {
                        let _ = f;
                    }
                    let _ = e.mic_count();
                }
            }
            let _ = nh.find_fmtcfg(None, None, None, None, 2, 48000, 24, 32);
        }
    }
    for i in 0..t.len() {
        let mut c = t.clone();
        c[i] ^= 0xFF;
        if let Ok(nh) = Nhlt::parse(&c, false) {
            for e in nh.endpoints().flatten() {
                for f in e.formats() {
                    let _ = f;
                }
                let _ = e.mic_count();
            }
        }
    }
}
