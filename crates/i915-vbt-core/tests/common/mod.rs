// SPDX-License-Identifier: GPL-2.0-only
//! Synthetic VBT builder. Layout per intel_vbt_defs.h: vbt_header 48 bytes (:97), bdb_header 22 (:115), blocks are
//! `id u8, size u16, data` (:131-137 comment).
#![allow(dead_code)]

pub fn le16(v: &mut Vec<u8>, x: u16) { v.extend_from_slice(&x.to_le_bytes()); }
pub fn le32(v: &mut Vec<u8>, x: u32) { v.extend_from_slice(&x.to_le_bytes()); }

/// Build a VBT with BDB `version` and raw blocks (id, data). Fixes size fields and the checksum.
pub fn vbt(version: u16, blocks: &[(u8, Vec<u8>)]) -> Vec<u8> {
    let mut bdb = Vec::new();
    bdb.extend_from_slice(b"BIOS_DATA_BLOCK ");
    le16(&mut bdb, version);
    le16(&mut bdb, 22);
    le16(&mut bdb, 0); // bdb_size patched
    for (id, d) in blocks {
        bdb.push(*id);
        le16(&mut bdb, d.len() as u16);
        bdb.extend_from_slice(d);
    }
    let n = bdb.len() as u16;
    bdb[20..22].copy_from_slice(&n.to_le_bytes());
    let mut v = Vec::new();
    v.extend_from_slice(b"$VBT SKYLAKE        "); // 20
    le16(&mut v, 100);
    le16(&mut v, 48);
    le16(&mut v, (48 + bdb.len()) as u16);
    v.push(0);
    v.push(0);
    le32(&mut v, 48);
    for _ in 0..4 { le32(&mut v, 0); }
    assert_eq!(v.len(), 48);
    v.extend(bdb);
    let sum = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v[26] = 0u8.wrapping_sub(sum);
    v
}

/// A child_device_config of `size` bytes with the listed (offset, value) bytes set.
pub fn child(size: usize, set: &[(usize, u8)]) -> Vec<u8> {
    let mut c = vec![0u8; size];
    for &(o, v) in set { c[o] = v; }
    c
}

/// Block 2 with `children` (each already `child_size` long).
pub fn general_defs(crt_pin: u8, child_size: u8, children: &[Vec<u8>]) -> Vec<u8> {
    let mut d = vec![crt_pin, 0, 0, 0, child_size];
    for c in children { assert_eq!(c.len(), child_size as usize); d.extend_from_slice(c); }
    d
}

/// 18-byte DTD for 1024x768@65000 kHz, hblank 320, vblank 38, hso 24 hsw 136 vso 3 vsw 6, 300x200 mm
/// (hand-encoded; flags byte as given).
pub fn dtd_1024(flags: u8) -> [u8; 18] {
    [0x64, 0x19, 0x00, 0x40, 0x41, 0x00, 0x26, 0x30, 0x18, 0x88, 0x36, 0x00, 0x2c, 0xc8, 0x10, 0, 0, flags]
}

pub struct Lfp {
    pub fp_table_size: u8,
    pub fp_pad: u8,
    pub name_size: u8,
    pub dtds: Vec<[u8; 18]>, // 16
    pub pnp: Vec<[u8; 10]>,  // 16
}
impl Lfp {
    pub fn new() -> Lfp {
        Lfp { fp_table_size: 32, fp_pad: 6, name_size: 13, dtds: vec![dtd_1024(0x18); 16], pnp: vec![[0; 10]; 16] }
    }
    pub fn stride(&self) -> usize { self.fp_table_size as usize + self.fp_pad as usize + 18 + 10 }
    /// block 42 data (16 entries + names tail), terminators placed.
    pub fn data(&self) -> Vec<u8> {
        let mut d = Vec::new();
        for i in 0..16 {
            let mut fp = vec![0u8; (self.fp_table_size + self.fp_pad) as usize];
            fp[0..2].copy_from_slice(&1024u16.to_le_bytes());
            fp[2..4].copy_from_slice(&768u16.to_le_bytes());
            fp[8..12].copy_from_slice(&(0x1000_0000u32 + i as u32).to_le_bytes());
            let n = fp.len();
            fp[n - 2] = 0xff; fp[n - 1] = 0xff;
            d.extend(fp);
            d.extend_from_slice(&self.dtds[i]);
            d.extend_from_slice(&self.pnp[i]);
        }
        for i in 0..16 { let mut nm = [b' '; 13]; nm[0] = b'A' + i as u8; d.extend_from_slice(&nm); }
        d
    }
    /// block 41 data given the BDB offset where block 42's data starts.
    pub fn ptrs(&self, base: u32) -> Vec<u8> {
        let mut p = vec![3u8];
        let s = self.stride() as u32;
        for i in 0..16u32 {
            let fp = base + i * s;
            let dvo = fp + self.fp_table_size as u32 + self.fp_pad as u32;
            let pnp = dvo + 18;
            le16(&mut p, fp as u16); p.push(self.fp_table_size);
            le16(&mut p, dvo as u16); p.push(18);
            le16(&mut p, pnp as u16); p.push(10);
        }
        le16(&mut p, (base + 16 * s) as u16); p.push(self.name_size);
        assert_eq!(p.len(), 148);
        p
    }
    /// blocks 41 then 42 at the start of the BDB.
    pub fn blocks(&self) -> Vec<(u8, Vec<u8>)> {
        let base = 22 + 3 + 148 + 3;
        vec![(41, self.ptrs(base)), (42, self.data())]
    }
}
