// SPDX-License-Identifier: GPL-2.0-only
//! A firmware FILE for the simulated card, built here from the Linux container format (`fw/file.h`,
//! `iwl-drv.c:807-855`) with its own TLV writer, so the driver's parser is tested against bytes it did not
//! write. Section contents are deterministic noise; their addresses and sizes are chosen to exercise every
//! branch of the upload: a section larger than one 128 KiB chunk, one in the extended-address window, the
//! CPU1/CPU2 and PAGING separators, an INIT image with no separator at all (as the real -46 file has).
use super::fwmodel::{FwImageModel, SecModel, CPU1_CPU2_SEP, PAGING_SEP};

pub struct Blob {
    pub bytes: Vec<u8>,
    pub init: FwImageModel,
    pub regular: FwImageModel,
}

fn noise(seed: u32, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2654435761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect()
}

pub fn tlv(out: &mut Vec<u8>, ty: u32, data: &[u8]) {
    out.extend_from_slice(&ty.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn sec_tlv(out: &mut Vec<u8>, ty: u32, s: &SecModel) {
    let mut d = s.offset.to_le_bytes().to_vec();
    d.extend_from_slice(&s.data);
    tlv(out, ty, &d);
}

pub struct Opts {
    pub phy_sku: Option<u32>,
    pub mq_rx: bool,
    pub notif_ver: u8,
    pub phy_cfg_cmd_ver: u8,
    pub init_sections: bool,
    pub regular_sections: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { phy_sku: Some(0x0033_0018), mq_rx: true, notif_ver: 3, phy_cfg_cmd_ver: 1, init_sections: true, regular_sections: true }
    }
}

pub fn build(o: &Opts) -> Blob {
    let init = FwImageModel {
        secs: vec![
            SecModel { offset: 0x0040_2000, data: noise(1, 712) },
            SecModel { offset: 0x0080_0000, data: noise(2, 300_000) }, // larger than two 128 KiB chunks
            SecModel { offset: 0x0000_0000, data: noise(3, 5000) },
            SecModel { offset: 0x0004_8000, data: noise(4, 70_000) }, // inside the extended-address window
            SecModel { offset: 0x0040_4000, data: noise(5, 696) },
        ],
    };
    let regular = FwImageModel {
        secs: vec![
            SecModel { offset: 0x0040_2000, data: noise(11, 696) },
            SecModel { offset: 0x0080_0000, data: noise(12, 98_304) },
            SecModel { offset: CPU1_CPU2_SEP, data: vec![0; 4] },
            SecModel { offset: 0x0041_0000, data: noise(13, 40_000) },
            SecModel { offset: 0x0042_0000, data: noise(14, 2048) },
            SecModel { offset: PAGING_SEP, data: vec![0; 4] },
            SecModel { offset: 0x0000_0000, data: noise(15, 664) }, // paging CSS
            SecModel { offset: 0x0100_0000, data: noise(16, 8192) }, // paging blocks
            SecModel { offset: CPU1_CPU2_SEP, data: vec![0; 4] },
        ],
    };
    let mut b = vec![0u8; 88];
    b[4..8].copy_from_slice(&0x0a4c_5749u32.to_le_bytes());
    b[8..8 + 25].copy_from_slice(b"release/core43::sim-blob!");
    b[72..76].copy_from_slice(&0x2eu32.to_le_bytes());
    b[76..80].copy_from_slice(&1u32.to_le_bytes());
    tlv(&mut b, 27, &2u32.to_le_bytes()); // NUM_OF_CPU
    if let Some(p) = o.phy_sku {
        tlv(&mut b, 23, &p.to_le_bytes());
    }
    let mut capa = 2u32.to_le_bytes().to_vec(); // word 2
    capa.extend_from_slice(&(if o.mq_rx { 1u32 << 4 } else { 0 }).to_le_bytes()); // bit 68 = word 2 bit 4
    tlv(&mut b, 30, &capa);
    let mut calib = 1u32.to_le_bytes().to_vec(); // INIT
    calib.extend_from_slice(&0x1f0f_0001u32.to_le_bytes());
    calib.extend_from_slice(&0x0000_0001u32.to_le_bytes());
    tlv(&mut b, 22, &calib);
    let mut cv = vec![];
    cv.extend_from_slice(&[0x01, 0x00, 0, o.notif_ver]); // UCODE_ALIVE_NTFY: cmd 1, group 0
    cv.extend_from_slice(&[0x6a, 0x01, o.phy_cfg_cmd_ver, 99]); // PHY_CONFIGURATION_CMD under LONG_GROUP
    tlv(&mut b, 48, &cv);
    if o.init_sections {
        for s in &init.secs {
            sec_tlv(&mut b, 20, s);
        }
    }
    if o.regular_sections {
        for (i, s) in regular.secs.iter().enumerate() {
            sec_tlv(&mut b, if i % 2 == 0 { 19 } else { 24 }, s); // both RT TLV types
        }
    }
    Blob { bytes: b, init, regular }
}

/// Build the simulator's image models from a REAL file with a TLV walk written here (the container format of
/// `iwl-drv.c:807-855`), not the driver's: 88-byte header, then (type, length, data) padded to 4.
pub fn from_file(bytes: Vec<u8>) -> Blob {
    let (mut init, mut regular) = (vec![], vec![]);
    let mut o = 88usize;
    while o + 8 <= bytes.len() {
        let ty = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[o + 4..o + 8].try_into().unwrap()) as usize;
        let d = &bytes[o + 8..o + 8 + len];
        let s = || SecModel { offset: u32::from_le_bytes(d[0..4].try_into().unwrap()), data: d[4..].to_vec() };
        match ty {
            19 | 24 => regular.push(s()),
            20 | 25 => init.push(s()),
            _ => {}
        }
        o += 8 + (len + 3) / 4 * 4;
    }
    Blob { bytes, init: FwImageModel { secs: init }, regular: FwImageModel { secs: regular } }
}
