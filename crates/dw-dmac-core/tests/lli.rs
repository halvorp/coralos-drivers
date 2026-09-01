// SPDX-License-Identifier: GPL-2.0-only
//! LLI vectors from Linux `drivers/dma/dw/regs.h`.
//!
//! Original copyright holders: Atmel Corporation, ST Microelectronics, Intel
//! Corporation, Haavard Skinnemoen, and Viresh Kumar.

use dw_dmac_core::lli::{
    encode_pointer, list_master, location, offset, Lli, LlpError, LLI_FIELD_NAMES, LLI_SIZE,
};

const LINUX_LLI_NAMES: [&str; 7] = ["sar", "dar", "llp", "ctllo", "ctlhi", "sstat", "dstat"]; // regs.h:371-381

#[test]
fn all_seven_lli_words_have_linux_names_offsets_and_size() {
    assert_eq!(LLI_FIELD_NAMES.len(), 7); // regs.h:369-382
    assert_eq!(LLI_FIELD_NAMES, LINUX_LLI_NAMES);
    assert_eq!(
        [
            offset::SAR,
            offset::DAR,
            offset::LLP,
            offset::CTL_LO,
            offset::CTL_HI,
            offset::SSTAT,
            offset::DSTAT
        ],
        [0x00, 0x04, 0x08, 0x0c, 0x10, 0x14, 0x18]
    ); // regs.h:371-381, seven consecutive __le32 words
    assert_eq!(LLI_SIZE, 0x1c); // regs.h:369-382
    assert_eq!(core::mem::size_of::<Lli>(), 0x1c);
}

#[test]
fn lli_constructor_preserves_words_in_linux_order_and_little_endian() {
    let lli = Lli::new(
        0x1122_3344,
        0x5566_7788,
        0x99aa_bbcc,
        0xddee_ff00,
        0x0123_4567,
        0x89ab_cdef,
        0xfedc_ba98,
    );
    assert_eq!(
        lli.words(),
        [
            0x1122_3344,
            0x5566_7788,
            0x99aa_bbcc,
            0xddee_ff00,
            0x0123_4567,
            0x89ab_cdef,
            0xfedc_ba98,
        ]
    ); // regs.h:371-381
    assert_eq!(
        lli.bytes(),
        [
            0x44, 0x33, 0x22, 0x11, 0x88, 0x77, 0x66, 0x55, 0xcc, 0xbb, 0xaa, 0x99, 0x00, 0xff,
            0xee, 0xdd, 0x67, 0x45, 0x23, 0x01, 0xef, 0xcd, 0xab, 0x89, 0x98, 0xba, 0xdc, 0xfe,
        ]
    ); // regs.h:371-381, each field is __le32
}

#[test]
fn llp_helpers_split_the_linux_two_bit_master_field() {
    assert_eq!(list_master(0x1234_567b), 3); // regs.h:144, (x) & 3
    assert_eq!(location(0x1234_567b), 0x1234_5678); // regs.h:145, (x) & ~3
    assert_eq!(encode_pointer(0x1234_5678, 3), Ok(0x1234_567b)); // core.c:590
}

#[test]
fn llp_builder_names_alignment_and_master_refusals() {
    assert_eq!(
        encode_pointer(0x1002, 0),
        Err(LlpError::AddressNotFourByteAligned {
            address: 0x1002,
            required_alignment: 4
        })
    ); // regs.h:145 reserves address bits 1:0
    assert_eq!(
        encode_pointer(0x1000, 4),
        Err(LlpError::MasterOutOfRange {
            master: 4,
            maximum_master: 3
        })
    ); // regs.h:144 masks exactly two bits
}
