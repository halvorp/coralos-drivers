// SPDX-License-Identifier: GPL-2.0-only
//! NHLT edges: exact-size tables and endpoints and the exact error payloads.
use avs_topology_core::nhlt::*;

fn dw(x: u32) -> [u8; 4] {
    x.to_le_bytes()
}

fn header(count: u8, extra: &[u8]) -> Vec<u8> {
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
    v.push(count);
    v.extend_from_slice(extra);
    let n = v.len() as u32;
    v[4..8].copy_from_slice(&dw(n));
    let sum = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v[9] = 0u8.wrapping_sub(sum);
    v
}

/// An endpoint with a device config of `dev` bytes and `fmts` formats each with a `caps`-byte blob.
fn ep(dev: usize, fmts: u8, caps: usize) -> Vec<u8> {
    let mut b = vec![3, 0];
    b.extend_from_slice(&0x8086u16.to_le_bytes());
    b.extend_from_slice(&0xAE34u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&dw(0));
    b.extend_from_slice(&[4, 0, 1]);
    b.extend_from_slice(&dw(dev as u32));
    b.extend(vec![0u8; dev]);
    b.push(fmts);
    for _ in 0..fmts {
        let mut w = vec![0xFE, 0xFF, 2, 0];
        w.extend_from_slice(&dw(48000));
        w.extend_from_slice(&dw(384000));
        w.extend_from_slice(&8u16.to_le_bytes());
        w.extend_from_slice(&32u16.to_le_bytes());
        w.extend_from_slice(&22u16.to_le_bytes());
        w.extend_from_slice(&24u16.to_le_bytes());
        w.extend_from_slice(&dw(3));
        w.extend_from_slice(&[0u8; 16]);
        assert_eq!(w.len(), 40);
        w.extend_from_slice(&dw(caps as u32));
        w.extend(vec![0xCD; caps]);
        b.extend(w);
    }
    let mut v = dw(4 + b.len() as u32).to_vec();
    v.extend(b);
    v
}

#[test]
fn a_table_of_exactly_37_bytes_is_a_valid_empty_table() {
    let t = header(0, &[]);
    assert_eq!(t.len(), 37);
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints().count(), 0);
    assert!(n.find_endpoint(None, None, None, None).unwrap().is_none());
    // 36 bytes cannot hold the endpoint count
    assert_eq!(Nhlt::parse(&t[..36], false).err(), Some(NhltError::Truncated { what: "NHLT table header", need: 37, have: 36 }));
    // a length field of exactly 37 is fine, 36 is not
    let mut b = t.clone();
    b[4..8].copy_from_slice(&dw(36));
    assert_eq!(Nhlt::parse(&b, false).err(), Some(NhltError::BadLength { length: 36, have: 37 }));
}

#[test]
fn the_smallest_endpoint_is_24_bytes() {
    // 19 header + 4 (device config size, 0) + 1 (formats count, 0)
    let e = ep(0, 0, 0);
    assert_eq!(e.len(), 24);
    let t = header(1, &e);
    let n = Nhlt::parse(&t, true).unwrap();
    let got = n.endpoints().next().unwrap().unwrap();
    assert_eq!((got.device_caps.len(), got.formats_count, got.trailer.len()), (0, 0, 0));
    // 23 bytes is too short
    let mut short = e.clone();
    short.pop();
    short[0..4].copy_from_slice(&dw(23));
    let t = header(1, &short);
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints().next().unwrap().err(), Some(NhltError::EndpointTooShort { index: 0, length: 23, need: 24 }));
}

#[test]
fn device_config_sizes_at_the_edge_of_the_endpoint() {
    // device config of 5 bytes in an endpoint with room for exactly 5 + the formats-count byte
    let e = ep(5, 0, 0);
    let t = header(1, &e);
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints().next().unwrap().unwrap().device_caps.len(), 5);
    // the same endpoint with the device-config size raised by one: the formats-count byte would be swallowed
    let mut b = e.clone();
    b[19..23].copy_from_slice(&dw(6));
    let t = header(1, &b);
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints().next().unwrap().err(), Some(NhltError::PastEndpoint { index: 0, what: "device config", need: 7, have: 6 }));
    // and by a lot
    let mut b = e.clone();
    b[19..23].copy_from_slice(&dw(1000));
    let t = header(1, &b);
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints().next().unwrap().err(), Some(NhltError::PastEndpoint { index: 0, what: "device config", need: 1001, have: 6 }));
}

#[test]
fn format_capability_sizes_report_what_they_needed_and_what_was_there() {
    let e = ep(2, 1, 8);
    let ok = header(1, &e);
    assert!(Nhlt::parse(&ok, true).unwrap().endpoints().next().unwrap().is_ok());
    // raise the blob size by one: it no longer fits
    let mut b = e.clone();
    let at = 19 + 4 + 2 + 1 + 40;
    b[at..at + 4].copy_from_slice(&dw(9));
    let t = header(1, &b);
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints().next().unwrap().err(), Some(NhltError::PastEndpoint { index: 0, what: "format capabilities", need: 9, have: 8 }));
    // an endpoint with a format header cut short reports the 44 bytes it needed
    let mut b = ep(2, 1, 0);
    let keep = b.len() - 10;
    b.truncate(keep);
    let l = b.len() as u32;
    b[0..4].copy_from_slice(&dw(l));
    let t = header(1, &b);
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints().next().unwrap().err(), Some(NhltError::PastEndpoint { index: 0, what: "format configuration", need: 44, have: 34 }));
    // a trailer after the last format is exposed exactly
    let mut b = ep(0, 1, 4);
    b.extend_from_slice(&[9, 9, 9]);
    let l = b.len() as u32;
    b[0..4].copy_from_slice(&dw(l));
    let t = header(1, &b);
    let n = Nhlt::parse(&t, true).unwrap();
    assert_eq!(n.endpoints().next().unwrap().unwrap().trailer, &[9, 9, 9]);
}
