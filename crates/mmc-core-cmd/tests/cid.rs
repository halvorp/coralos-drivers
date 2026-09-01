// SPDX-License-Identifier: GPL-2.0-only
//! Frozen CID vectors from Linux `drivers/mmc/core/mmc.c`.
//! Copyright (C) 2003-2004 Russell King; 2005-2007 Pierre Ossman; 2006 Philip Langdale.
use mmc_core_cmd::cid::*;

#[test]
fn cid_field_counts_names_and_positions_are_pinned() {
    let legacy=[("MANFID",104,24),("PROD_NAME_0",96,8),("PROD_NAME_1",88,8),("PROD_NAME_2",80,8),("PROD_NAME_3",72,8),("PROD_NAME_4",64,8),("PROD_NAME_5",56,8),("PROD_NAME_6",48,8),("HWREV",44,4),("FWREV",40,4),("SERIAL",16,24),("MONTH",12,4),("YEAR",8,4)]; // mmc.c:82-94
    let modern=[("MANFID",120,8),("OEMID",104,16),("PROD_NAME_0",96,8),("PROD_NAME_1",88,8),("PROD_NAME_2",80,8),("PROD_NAME_3",72,8),("PROD_NAME_4",64,8),("PROD_NAME_5",56,8),("PRV",48,8),("SERIAL",16,32),("MONTH",12,4),("YEAR",8,4)]; // mmc.c:100-111
    assert_eq!(LEGACY_CID_FIELDS.iter().map(|x|(x.name,x.start,x.size)).collect::<Vec<_>>(),legacy);
    assert_eq!(MODERN_CID_FIELDS.iter().map(|x|(x.name,x.start,x.size)).collect::<Vec<_>>(),modern);
}
#[test]
fn modern_and_legacy_cids_decode_literal_responses() {
    let modern=[0x1512_3443,0x4f52_414c,0x5321_0789,0xabc5_a500];
    let c=decode(&modern,4).unwrap();
    assert_eq!((c.manfid,c.oemid,c.prod_name,c.prv,c.serial,c.month,c.year,c.prod_name_len),(0x15,0x1234,*b"CORALS\0",0x21,0x0789_ab_c5,0xa,2002,6));
    let legacy=[0xa1b2_c341,0x4243_4445,0x4647_a512,0x3456_7b90];
    let c=decode(&legacy,1).unwrap();
    assert_eq!((c.manfid,c.prod_name,c.hwrev,c.fwrev,c.serial,c.month,c.year,c.prod_name_len),(0xa1b2c3,*b"ABCDEFG",0xa,5,0x123456,7,2008,7));
}
#[test]
fn unknown_version_names_value_and_bound() {
    assert_eq!(decode(&[0;4],5),Err(CidError::UnknownMmcaVersion{value:5,maximum_supported:4})); // mmc.c:114-117
}
