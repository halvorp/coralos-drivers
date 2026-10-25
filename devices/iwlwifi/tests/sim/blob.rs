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
    /// The IWL_UCODE_TLV_N_SCAN_CHANNELS value (fw/file.h:80): the size of the per-channel arrays in the scan commands.
    pub n_scan: usize,
    /// The IWL_UCODE_TLV_PAGING value: bytes of CPU2 paging image, 0 when the file has none.
    pub paging_size: usize,
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
    /// The RT image's capability, API and flag TLVs of the real -46 file (UMAC scan, LAR, DQA, adaptive dwell v2, EBS, the paging
    /// size of the regular model, 52 scan channels, the SCAN_REQ_UMAC command version 8).
    pub rt: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { phy_sku: Some(0x0033_0018), mq_rx: true, notif_ver: 3, phy_cfg_cmd_ver: 1, init_sections: true, regular_sections: true, rt: false }
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
    // bit 68 = word 2 bit 4 (MULTI_QUEUE_RX); with `rt`: 67 BT_MPLUT (bit 3), 74 CT_KILL_BY_FW (bit 10), 75 TEMP_THS_REPORT (bit 11)
    let w2 = (if o.mq_rx { 1u32 << 4 } else { 0 }) | if o.rt { (1 << 3) | (1 << 10) | (1 << 11) } else { 0 };
    capa.extend_from_slice(&w2.to_le_bytes());
    tlv(&mut b, 30, &capa);
    if o.rt {
        // word 0: 1 LAR_SUPPORT, 2 UMAC_SCAN, 9 DS_PARAM_SET_IE_SUPPORT, 12 DQA_SUPPORT ; word 1: 37 SOC_LATENCY_SUPPORT (bit 5)
        let mut c0 = 0u32.to_le_bytes().to_vec();
        c0.extend_from_slice(&((1u32 << 1) | (1 << 2) | (1 << 9) | (1 << 12)).to_le_bytes());
        tlv(&mut b, 30, &c0);
        let mut c1 = 1u32.to_le_bytes().to_vec();
        c1.extend_from_slice(&(1u32 << 5).to_le_bytes());
        tlv(&mut b, 30, &c1);
        // API: word 0: 9 WIFI_MCC_UPDATE, 30 STA_TYPE ; word 1: 32 ADAPTIVE_DWELL (bit 0), 42 ADAPTIVE_DWELL_V2 (bit 10)
        let mut a0 = 0u32.to_le_bytes().to_vec();
        a0.extend_from_slice(&((1u32 << 9) | (1 << 30)).to_le_bytes());
        tlv(&mut b, 29, &a0);
        let mut a1 = 1u32.to_le_bytes().to_vec();
        a1.extend_from_slice(&((1u32 << 0) | (1 << 10)).to_le_bytes());
        tlv(&mut b, 29, &a1);
        tlv(&mut b, 18, &(1u32 << 25).to_le_bytes()); // IWL_UCODE_TLV_FLAGS_EBS_SUPPORT
        tlv(&mut b, 31, &52u32.to_le_bytes()); // N_SCAN_CHANNELS
        tlv(&mut b, 32, &8192u32.to_le_bytes()); // PAGING: the regular model's paging section
    }
    let mut calib = 1u32.to_le_bytes().to_vec(); // INIT
    calib.extend_from_slice(&0x1f0f_0001u32.to_le_bytes());
    calib.extend_from_slice(&0x0000_0001u32.to_le_bytes());
    tlv(&mut b, 22, &calib);
    let mut cv = vec![];
    cv.extend_from_slice(&[0x01, 0x00, 0, o.notif_ver]); // UCODE_ALIVE_NTFY: cmd 1, group 0
    cv.extend_from_slice(&[0x6a, 0x01, o.phy_cfg_cmd_ver, 99]); // PHY_CONFIGURATION_CMD under LONG_GROUP
    if o.rt {
        cv.extend_from_slice(&[0x0d, 0x01, 8, 0]); // SCAN_REQ_UMAC v8
        cv.extend_from_slice(&[0x0c, 0x01, 2, 0]); // SCAN_CFG_CMD
        cv.extend_from_slice(&[0x18, 0x01, 10, 0]); // ADD_STA v10
        cv.extend_from_slice(&[0xc1, 0x00, 99, 2]); // REPLY_RX_MPDU_CMD notification version 2
    }
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
    Blob { bytes: b, init, regular, n_scan: if o.rt { 52 } else { 0 }, paging_size: if o.rt { 8192 } else { 0 } }
}

/// Build the simulator's image models from a REAL file with a TLV walk written here (the container format of
/// `iwl-drv.c:807-855`), not the driver's: 88-byte header, then (type, length, data) padded to 4.
pub fn from_file(bytes: Vec<u8>) -> Blob {
    let (mut init, mut regular) = (vec![], vec![]);
    let mut o = 88usize;
    let (mut n_scan, mut paging_size) = (0usize, 0usize);
    while o + 8 <= bytes.len() {
        let ty = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[o + 4..o + 8].try_into().unwrap()) as usize;
        let d = &bytes[o + 8..o + 8 + len];
        let s = || SecModel { offset: u32::from_le_bytes(d[0..4].try_into().unwrap()), data: d[4..].to_vec() };
        match ty {
            19 | 24 => regular.push(s()),
            20 | 25 => init.push(s()),
            31 => n_scan = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize,
            32 => paging_size = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize,
            _ => {}
        }
        o += 8 + (len + 3) / 4 * 4;
    }
    Blob { bytes, init: FwImageModel { secs: init }, regular: FwImageModel { secs: regular }, n_scan, paging_size }
}
