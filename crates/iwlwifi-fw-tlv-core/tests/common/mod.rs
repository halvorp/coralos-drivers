// SPDX-License-Identifier: GPL-2.0-only
//! A TLV ucode file builder for the vectors: the 88-byte header (file.h:135-156) with the magic
//! written as the Linux literal 0x0a4c5749 (file.h:132), then each TLV padded to 4 bytes.
#![allow(dead_code)]

pub fn header(ver: u32, build: u32, name: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0x0a4c_5749u32.to_le_bytes());
    let mut hr = [0u8; 64];
    hr[..name.len()].copy_from_slice(name);
    b.extend_from_slice(&hr);
    b.extend_from_slice(&ver.to_le_bytes());
    b.extend_from_slice(&build.to_le_bytes());
    b.extend_from_slice(&[0u8; 8]);
    b
}

pub fn file(tlvs: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut b = header(0, 0, b"");
    for (ty, data) in tlvs {
        b.extend_from_slice(&ty.to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        while b.len() % 4 != 0 {
            b.push(0);
        }
    }
    b
}

pub fn words(ws: &[u32]) -> Vec<u8> {
    ws.iter().flat_map(|w| w.to_le_bytes()).collect()
}
