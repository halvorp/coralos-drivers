// SPDX-License-Identifier: GPL-2.0-only
//! MMC CID extraction for legacy and modern MMCA versions.
//!
//! Ported from Linux `drivers/mmc/core/mmc.c` and `mmc_ops.h`.
//! Copyright (C) 2003-2004 Russell King; 2005-2007 Pierre Ossman; MMCv4
//! support Copyright (C) 2006 Philip Langdale.

use crate::csd::extract_bits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CidField { pub name: &'static str, pub start: u8, pub size: u8 }
pub const LEGACY_CID_FIELDS: [CidField; 13] = [
    CidField { name: "MANFID", start: 104, size: 24 }, // mmc.c:82
    CidField { name: "PROD_NAME_0", start: 96, size: 8 }, // mmc.c:83
    CidField { name: "PROD_NAME_1", start: 88, size: 8 }, // mmc.c:84
    CidField { name: "PROD_NAME_2", start: 80, size: 8 }, // mmc.c:85
    CidField { name: "PROD_NAME_3", start: 72, size: 8 }, // mmc.c:86
    CidField { name: "PROD_NAME_4", start: 64, size: 8 }, // mmc.c:87
    CidField { name: "PROD_NAME_5", start: 56, size: 8 }, // mmc.c:88
    CidField { name: "PROD_NAME_6", start: 48, size: 8 }, // mmc.c:89
    CidField { name: "HWREV", start: 44, size: 4 }, // mmc.c:90
    CidField { name: "FWREV", start: 40, size: 4 }, // mmc.c:91
    CidField { name: "SERIAL", start: 16, size: 24 }, // mmc.c:92
    CidField { name: "MONTH", start: 12, size: 4 }, // mmc.c:93
    CidField { name: "YEAR", start: 8, size: 4 }, // mmc.c:94
];
pub const MODERN_CID_FIELDS: [CidField; 12] = [
    CidField { name: "MANFID", start: 120, size: 8 }, // mmc.c:100
    CidField { name: "OEMID", start: 104, size: 16 }, // mmc.c:101
    CidField { name: "PROD_NAME_0", start: 96, size: 8 }, // mmc.c:102
    CidField { name: "PROD_NAME_1", start: 88, size: 8 }, // mmc.c:103
    CidField { name: "PROD_NAME_2", start: 80, size: 8 }, // mmc.c:104
    CidField { name: "PROD_NAME_3", start: 72, size: 8 }, // mmc.c:105
    CidField { name: "PROD_NAME_4", start: 64, size: 8 }, // mmc.c:106
    CidField { name: "PROD_NAME_5", start: 56, size: 8 }, // mmc.c:107
    CidField { name: "PRV", start: 48, size: 8 }, // mmc.c:108
    CidField { name: "SERIAL", start: 16, size: 32 }, // mmc.c:109
    CidField { name: "MONTH", start: 12, size: 4 }, // mmc.c:110
    CidField { name: "YEAR", start: 8, size: 4 }, // mmc.c:111
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cid { pub manfid: u32, pub oemid: u16, pub prod_name: [u8; 7], pub hwrev: u8, pub fwrev: u8, pub prv: u8, pub serial: u32, pub month: u8, pub year: u16, pub prod_name_len: u8 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CidError { UnknownMmcaVersion { value: u8, maximum_supported: u8 } }

pub fn decode(resp: &[u32; 4], mmca_vsn: u8) -> Result<Cid, CidError> {
    let get = |start, size| extract_bits(resp, start, size).expect("CID's frozen fields fit the 128-bit response");
    match mmca_vsn {
        0 | 1 => Ok(Cid { manfid: get(104, 24), oemid: 0,
            prod_name: [get(96,8) as u8,get(88,8) as u8,get(80,8) as u8,get(72,8) as u8,get(64,8) as u8,get(56,8) as u8,get(48,8) as u8],
            hwrev: get(44,4) as u8, fwrev: get(40,4) as u8, prv: 0, serial: get(16,24), month: get(12,4) as u8, year: get(8,4) as u16 + 1997, prod_name_len: 7 }),
        2..=4 => Ok(Cid { manfid: get(120,8), oemid: get(104,16) as u16,
            prod_name: [get(96,8) as u8,get(88,8) as u8,get(80,8) as u8,get(72,8) as u8,get(64,8) as u8,get(56,8) as u8,0],
            hwrev: 0, fwrev: 0, prv: get(48,8) as u8, serial: get(16,32), month: get(12,4) as u8, year: get(8,4) as u16 + 1997, prod_name_len: 6 }),
        value => Err(CidError::UnknownMmcaVersion { value, maximum_supported: 4 }),
    }
}
