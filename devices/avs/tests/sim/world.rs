// SPDX-License-Identifier: GPL-2.0-only
//! The world around the device in a test: the board record, the topology (the real `da7219-tplg.bin` from the recovery image when it is
//! there), a hand-built NHLT shaped like coreboot's atlas variant, and synthetic firmware images with a valid envelope.
#![allow(dead_code)]
use avs_fw_core::regs::*;
use avs_wire::board::{Board, Pad};

pub const FW_DIR: &str = "/home/peter/Projects/coral_handoffs/atlas/recovery/configs/lib/firmware/intel/avs";

/// The topology bytes, or None (and a loud message) when the recovery image is not on this machine. `CORAL_AVS_REQUIRE_BLOBS=1` makes the
/// absence a failure, so a gate that must exercise the boot cannot pass vacuously.
pub fn tplg_bytes() -> Option<Vec<u8>> {
    let d = std::env::var("CORAL_AVS_FW_DIR").unwrap_or_else(|_| FW_DIR.to_string());
    let p = std::path::Path::new(&d).join("da7219-tplg.bin");
    if !p.exists() {
        assert!(std::env::var("CORAL_AVS_REQUIRE_BLOBS").is_err(), "da7219-tplg.bin is required (CORAL_AVS_REQUIRE_BLOBS) and is not at {}", p.display());
        eprintln!("da7219-tplg.bin: SKIPPED (set CORAL_AVS_FW_DIR=<.../intel/avs>)");
        return None;
    }
    Some(std::fs::read(p).unwrap())
}

/// The upstream speaker topology (thesofproject/avs-topology-xml@221e4a6 `i2s/max98373-tplg.xml` built by avsdk + alsatplg, DECISIONS D17), or
/// None (loudly). `CORAL_AVS_REQUIRE_BLOBS=1` makes its absence a failure.
pub fn spk_tplg_bytes() -> Option<Vec<u8>> {
    let p = std::env::var("CORAL_AVS_SPK_TPLG").unwrap_or_else(|_| "/home/peter/Projects/coral_handoffs/atlas/recovery/max98373-tplg.upstream-221e4a6.bin".to_string());
    let p = std::path::Path::new(&p);
    if !p.exists() {
        assert!(std::env::var("CORAL_AVS_REQUIRE_BLOBS").is_err(), "max98373-tplg.bin is required (CORAL_AVS_REQUIRE_BLOBS) and is not at {}", p.display());
        eprintln!("max98373-tplg.bin: SKIPPED (set CORAL_AVS_SPK_TPLG)");
        return None;
    }
    Some(std::fs::read(p).unwrap())
}

/// The Atlas board row (ATLAS-PERIPHERALS-FROM-RECOVERY.md section 3.2).
pub fn atlas_board() -> Board {
    Board {
        pci_vendor: 0x8086,
        pci_device: 0x9d71,
        codec_addr: 0x1a,
        amps: [0x31, 0x32, 0, 0],
        n_amps: 2,
        headset_ssp: 1,
        speaker_ssp: [0, 0, 0, 0],
        n_speaker_ssp: 1,
        playback_stream: 0,
        mclk_hz: 24_576_000,
        i2c_bus_hz: 400_000,
        i2c_fs_hcnt: 0x96,
        i2c_fs_lcnt: 0xe4,
        i2c_sda_hold: 0,
        jack_pad: Pad { community: 2, pad: 10 },
        spkr_rst_pad: Pad { community: 2, pad: 11 },
        pad_window_len: 0x10000,
    }
}

pub fn board_bytes(b: &Board) -> Vec<u8> {
    let mut out = [0u8; 128];
    let n = b.encode(&mut out).unwrap();
    out[..n].to_vec()
}

// ---- a hand-built NHLT shaped like coreboot's atlas variant (see avs-topology-core/tests/nhlt.rs) -----------------------------------
fn dw(x: u32) -> [u8; 4] {
    x.to_le_bytes()
}

pub struct NFmt {
    pub ch: u16,
    pub rate: u32,
    pub container: u16,
    pub valid: u16,
    pub mask: u32,
    pub caps: Vec<u8>,
}

fn wave(f: &NFmt) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&0xFFFEu16.to_le_bytes());
    v.extend_from_slice(&f.ch.to_le_bytes());
    v.extend_from_slice(&f.rate.to_le_bytes());
    v.extend_from_slice(&(f.rate * f.ch as u32 * (f.container as u32 / 8)).to_le_bytes());
    v.extend_from_slice(&(f.ch * (f.container / 8)).to_le_bytes());
    v.extend_from_slice(&f.container.to_le_bytes());
    v.extend_from_slice(&22u16.to_le_bytes());
    v.extend_from_slice(&f.valid.to_le_bytes());
    v.extend_from_slice(&f.mask.to_le_bytes());
    v.extend_from_slice(&[0x01, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    v.extend_from_slice(&dw(f.caps.len() as u32));
    v.extend_from_slice(&f.caps);
    v
}

#[allow(clippy::too_many_arguments)]
fn endpoint(link: u8, instance: u8, device_id: u16, dev_type: u8, dir: u8, vbus: u8, dev_caps: &[u8], fmts: &[NFmt], trailer: &[u8]) -> Vec<u8> {
    let mut body = vec![link, instance];
    body.extend_from_slice(&0x8086u16.to_le_bytes());
    body.extend_from_slice(&device_id.to_le_bytes());
    body.extend_from_slice(&1u16.to_le_bytes());
    body.extend_from_slice(&0x0000_0123u32.to_le_bytes());
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
    let mut v = dw(4 + body.len() as u32).to_vec();
    v.extend(body);
    v
}

pub fn blob(n: usize, seed: u8) -> Vec<u8> {
    (0..n).map(|i| seed.wrapping_add(i as u8)).collect()
}

/// The DA7219 render blob on SSP1 and the MAX98373 render blob on SSP0 have DIFFERENT seeds, so a test can prove which one a message carried.
pub const DA7219_RENDER_SEED: u8 = 0x10;
pub const MAX98373_SEED: u8 = 0x40;

pub fn nhlt_bytes() -> Vec<u8> {
    use avs_topology_core::nhlt::*;
    let tdm = [0u8, 0];
    let da = |seed| NFmt { ch: 2, rate: 48000, container: 32, valid: 24, mask: 3, caps: blob(88, seed) };
    let max = NFmt { ch: 2, rate: 48000, container: 16, valid: 16, mask: 3, caps: blob(88, MAX98373_SEED) };
    let eps = [
        endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_RENDER, 1, &tdm, &[da(DA7219_RENDER_SEED)], &[0u8]),
        endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_CAPTURE, 1, &tdm, &[da(0x20)], &[0u8]),
        endpoint(ACPI_NHLT_LINKTYPE_SSP, 0, ACPI_NHLT_DEVICEID_I2S, ACPI_NHLT_DEVICETYPE_CODEC, ACPI_NHLT_DIR_RENDER, 0, &tdm, &[max], &[0u8]),
    ];
    table(&eps)
}

fn table(eps: &[Vec<u8>]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"NHLT");
    v.extend_from_slice(&[0u8; 4]);
    v.push(1);
    v.push(0);
    v.extend_from_slice(b"GOOGLE");
    v.extend_from_slice(b"ATLASMAX");
    v.extend_from_slice(&dw(0));
    v.extend_from_slice(b"CORL");
    v.extend_from_slice(&dw(1));
    v.push(eps.len() as u8);
    for e in eps {
        v.extend_from_slice(e);
    }
    let n = v.len() as u32;
    v[4..8].copy_from_slice(&dw(n));
    let sum = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v[9] = 0u8.wrapping_sub(sum);
    v
}


/// A firmware or library image with a valid Skylake envelope: an extended manifest (16 bytes, stripped), the family magic, the manifest at
/// 0x284 carrying `name` and `version`, and a pattern body. `body_len` is the length AFTER stripping.
pub fn synth_image(name: &[u8; 8], version: (u16, u16, u16, u16), body_len: usize, seed: u8) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&AVS_EXT_MANIFEST_MAGIC.to_le_bytes());
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    let mut body = vec![0u8; body_len];
    for (i, b) in body.iter_mut().enumerate() {
        *b = seed.wrapping_add((i * 7) as u8);
    }
    body[0..4].copy_from_slice(&SKL_MANIFEST_MAGIC.to_le_bytes());
    let m = SKL_ADSPFW_OFFSET as usize;
    body[m..m + 4].copy_from_slice(&0u32.to_le_bytes());
    body[m + 4..m + 8].copy_from_slice(&(body_len as u32).to_le_bytes());
    body[m + 8..m + 16].copy_from_slice(name);
    body[m + 16..m + 36].fill(0);
    let (a, b, c, d) = version;
    body[m + 28..m + 30].copy_from_slice(&a.to_le_bytes());
    body[m + 30..m + 32].copy_from_slice(&b.to_le_bytes());
    body[m + 32..m + 34].copy_from_slice(&c.to_le_bytes());
    body[m + 34..m + 36].copy_from_slice(&d.to_le_bytes());
    v.extend_from_slice(&body);
    v
}

pub const BASEFW_VERSION: (u16, u16, u16, u16) = (9, 21, 0, 4899);
