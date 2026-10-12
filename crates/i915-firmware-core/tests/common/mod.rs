// SPDX-License-Identifier: GPL-2.0-only
//! Synthetic DMC package builder (layout per intel_dmc.c:262-396, sizes literal: css 128, package 16 + N*12,
//! v1 dmc header 128 bytes, v3 header 0x?? see below).
#![allow(dead_code)]

pub fn le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// CSS header: 32 dwords, header_len (dwords) at dword 1, version at dword 22.
pub fn css(version: u32, header_len_dw: u32) -> Vec<u8> {
    let mut v = Vec::new();
    for i in 0..32u32 {
        le(&mut v, match i { 0 => 9, 1 => header_len_dw, 2 => 0x10000, 22 => version, _ => 0 });
    }
    v
}

/// Package header: header_len(dw) u8, header_ver u8, 10 reserved, num_entries; then `max` fw_info (12 bytes) slots.
/// entries: (dmc_id, stepping, substepping, offset_dwords)
pub fn package(ver: u8, entries: &[(u8, u8, u8, u32)], header_len_dw: Option<u8>) -> Vec<u8> {
    let max = if ver == 1 { 20 } else { 32 };
    let mut v = vec![header_len_dw.unwrap_or(((16 + max * 12) / 4) as u8), ver];
    v.extend_from_slice(&[0u8; 10]);
    le(&mut v, entries.len() as u32);
    for i in 0..max {
        if let Some(&(id, st, sub, off)) = entries.get(i) {
            v.extend_from_slice(&[0, id, st, sub]);
            le(&mut v, off);
            le(&mut v, 0);
        } else {
            v.extend_from_slice(&[0u8; 12]);
        }
    }
    v
}

/// v1 DMC header (128 bytes) + payload dwords. header_len is BYTES for v1.
pub fn dmc_v1(mmio: &[(u32, u32)], payload: &[u32], mmio_count: Option<u32>, header_len: Option<u8>) -> Vec<u8> {
    let mut v = Vec::new();
    le(&mut v, 0x40403E3E);
    v.push(header_len.unwrap_or(128));
    v.push(1);
    v.extend_from_slice(&[0, 0]);
    le(&mut v, 0x0101);
    le(&mut v, payload.len() as u32);
    le(&mut v, 0x0001_0004);
    le(&mut v, mmio_count.unwrap_or(mmio.len() as u32));
    for i in 0..8 { le(&mut v, mmio.get(i).map(|m| m.0).unwrap_or(0)); }
    for i in 0..8 { le(&mut v, mmio.get(i).map(|m| m.1).unwrap_or(0)); }
    v.extend_from_slice(&[0u8; 32]);
    le(&mut v, 0);
    le(&mut v, 0);
    assert_eq!(v.len(), 128);
    for &w in payload { le(&mut v, w); }
    v
}

/// Whole package with ONE main image at offset 0 right after the package header.
pub fn simple(ver_css: u32, mmio: &[(u32, u32)], payload: &[u32]) -> Vec<u8> {
    let mut v = css(ver_css, 32);
    v.extend(package(1, &[(0, b'*', b'*', 0)], None));
    v.extend(dmc_v1(mmio, payload, None, None));
    v
}
