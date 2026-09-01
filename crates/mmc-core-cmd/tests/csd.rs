// SPDX-License-Identifier: GPL-2.0-only
//! Frozen CSD field and decode vectors from Linux `drivers/mmc/core/mmc.c`.
//! Copyright (C) 2003-2004 Russell King; 2005-2007 Pierre Ossman; 2006 Philip Langdale.
use mmc_core_cmd::csd::*;

#[test]
fn every_csd_field_is_pinned_by_name_start_and_size() {
    let expected = [
        ("STRUCTURE",126,2),("MMCA_VSN",122,4),("TAAC_MANT",115,4),("TAAC_EXP",112,3),("TAAC_CLKS",104,8),
        ("TRAN_MANT",99,4),("TRAN_EXP",96,3),("CMDCLASS",84,12),("READ_BLKBITS",80,4),("READ_PARTIAL",79,1),
        ("WRITE_MISALIGN",78,1),("READ_MISALIGN",77,1),("DSR_IMP",76,1),("C_SIZE",62,12),("C_SIZE_MULT",47,3),
        ("ERASE_GRP_SIZE",42,5),("ERASE_GRP_MULT",37,5),("WP_GRP_SIZE",32,5),("R2W_FACTOR",26,3),
        ("WRITE_BLKBITS",22,4),("WRITE_PARTIAL",21,1), // mmc.c:161-197
    ];
    assert_eq!(CSD_FIELDS.iter().map(|x|(x.name,x.start,x.size)).collect::<Vec<_>>(), expected);
}
#[test]
fn extraction_matches_linux_word_order_and_cross_word_case() {
    let raw=[0x1122_3344,0x5566_7788,0x99aa_bbcc,0xddee_ff00];
    assert_eq!(extract_bits(&raw,96,32),Ok(0x1122_3344));
    assert_eq!(extract_bits(&raw,28,8),Ok(0xcd)); // mmc_ops.h:60-73 algorithm
    assert_eq!(extract_bits(&raw,0,8),Ok(0x00));
    assert_eq!(extract_bits(&raw,128,1),Err(ExtractError::StartOutOfRange{start:128,maximum:127}));
    assert_eq!(extract_bits(&raw,120,16),Err(ExtractError::FieldPastResponse{start:120,size:16,response_bits:128}));
}
#[test]
fn decode_has_a_literal_full_field_vector() {
    // Independent 128-bit response literal. Fields decode to the following Linux formula outputs.
    let raw=[0x5422_2a31,0x1239_b001,0x8000_086b,0x0b20_0000];
    let c=decode(&raw).unwrap();
    assert_eq!(c.structure,1); assert_eq!(c.mmca_vsn,5); assert_eq!(c.taac_ns,150); assert_eq!(c.taac_clks,4200);
    assert_eq!(c.max_dtr,2_500_000); assert_eq!(c.cmdclass,0x123); assert_eq!(c.capacity,28);
    assert_eq!((c.read_blkbits,c.read_partial,c.write_misalign,c.read_misalign,c.dsr_imp),(9,true,false,true,true));
    assert_eq!((c.r2w_factor,c.write_blkbits,c.write_partial),(2,12,true)); assert_eq!((c.erase_size,c.wp_grp_size),(96,11));
    assert_eq!(decode(&[0,0,0,0]),Err(CsdError::UnrecognisedStructure{value:0,forbidden:0})); // mmc.c:162-165
}
