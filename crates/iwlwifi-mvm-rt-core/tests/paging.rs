// SPDX-License-Identifier: GPL-2.0-only
//! The paging plan and FW_PAGING_BLOCK_CMD. Numbers are the real 9260-46 file's: PAGING TLV 0x3b000 bytes.
use iwlwifi_mvm_rt_core::encode::EncodeError;
use iwlwifi_mvm_rt_core::ids::*;
use iwlwifi_mvm_rt_core::paging::*;

const SEP_CPU: u32 = 0xFFFF_CCCC;

fn image(paging_len: usize, css_len: usize) -> Vec<SecInfo> {
    vec![
        SecInfo { offset: 0x0020_0000, len: 100 },
        SecInfo { offset: SEP_CPU, len: 0 },
        SecInfo { offset: 0x0100_0000, len: 200 },
        SecInfo { offset: PAGING_SEPARATOR_SECTION, len: 0 },
        SecInfo { offset: 0x0000_0000, len: css_len },
        SecInfo { offset: 0x0000_1000, len: paging_len },
    ]
}

#[test]
fn constants_are_the_linux_literals() {
    // fw/img.h:93-117
    assert_eq!((PAGE_2_EXP_SIZE, FW_PAGING_SIZE, PAGE_PER_GROUP_2_EXP_SIZE, NUM_OF_PAGE_PER_GROUP), (12, 4096, 3, 8));
    assert_eq!((PAGING_BLOCK_SIZE, BLOCK_2_EXP_SIZE), (32768, 15));
    assert_eq!((PAGING_CMD_IS_SECURED, PAGING_CMD_IS_ENABLED, PAGING_CMD_NUM_OF_PAGES_IN_LAST_GRP_POS), (0x200, 0x100, 0));
    assert_eq!((NUM_OF_FW_PAGING_BLOCKS, FW_PAGING_CMD_LEN), (33, 144));
    assert_eq!(PAGING_SEPARATOR_SECTION, 0xAAAA_BBBB);
}

#[test]
fn the_real_file_is_eight_blocks_with_three_pages_in_the_last() {
    // 0x3b000 = 241664 bytes = 59 pages; 59 / 8 -> 8 blocks; 59 - 8 * 7 = 3 pages in the last (fw/paging.c:58-64)
    let l = plan(&image(241_664, 4096), 241_664).unwrap();
    assert_eq!((l.num_blk, l.pages_in_last, l.css_sec, l.data_sec), (8, 3, 4, 5));
    assert_eq!(l.dram_blocks(), 9);
    assert_eq!((l.block_size(0), l.block_size(1), l.block_size(8)), (4096, 32768, 32768));
    assert_eq!(l.copy_range(1), (0, 32768));
    assert_eq!(l.copy_range(7), (6 * 32768, 32768));
    assert_eq!(l.copy_range(8), (7 * 32768, 3 * 4096));
}

#[test]
fn an_exact_multiple_of_a_block_has_a_full_last_block() {
    let l = plan(&image(65_536, 100), 65_536).unwrap();
    assert_eq!((l.num_blk, l.pages_in_last), (2, 8));
}

#[test]
fn layout_failures_are_named() {
    // no paging separator: sec_idx runs off the end
    let mut no_sep = image(241_664, 4096);
    no_sep.remove(3);
    assert!(matches!(plan(&no_sep, 241_664), Err(PagingError::MissingSections { .. })));
    // separator last: no CSS, no data
    let sep_last = vec![SecInfo { offset: 1, len: 1 }, SecInfo { offset: PAGING_SEPARATOR_SECTION, len: 0 }];
    assert!(matches!(plan(&sep_last, 4096), Err(PagingError::MissingSections { .. })));
    // only the CSS after the separator
    let mut css_only = image(0, 100);
    css_only.pop();
    assert!(matches!(plan(&css_only, 4096), Err(PagingError::MissingSections { .. })));
    assert_eq!(plan(&image(241_664, 4097), 241_664), Err(PagingError::CssTooLarge { len: 4097 }));
    // the last block must hold exactly pages_in_last pages
    assert_eq!(plan(&image(241_664 + 4096, 100), 241_664), Err(PagingError::LastBlockMismatch { remaining: 16_384, expected: 12_288 }));
    assert_eq!(plan(&image(241_664 - 4096, 100), 241_664), Err(PagingError::LastBlockMismatch { remaining: 8192, expected: 12_288 }));
    // a middle block short of data: 3 blocks promised, 2 blocks of data
    assert_eq!(plan(&image(65_536, 100), 98_304), Err(PagingError::NotEnoughData { block: 3, remaining: 0 }));
    assert_eq!(plan(&image(100, 100), 0), Err(PagingError::NoPaging));
    // 32 data blocks + CSS is the most the command carries; 33 blocks is not representable
    assert_eq!(plan(&image(1, 100), 33 * 32768), Err(PagingError::TooManyBlocks { blocks: 33 }));
}

#[test]
fn the_paging_cmd_bytes() {
    let l = plan(&image(241_664, 4096), 241_664).unwrap();
    // addresses of the CSS and 8 data blocks; the command carries phys >> 12 as le32
    let phys: Vec<u64> = (0..9).map(|i| 0x1000_0000 + i * 0x8000).collect();
    let mut b = vec![0xaau8; 160];
    assert_eq!(encode_fw_paging_cmd(&l, &phys, &mut b), Ok(144));
    // struct iwl_fw_paging_cmd { le32 flags; le32 block_size; le32 block_num; le32 device_phy_addr[33] }  (fw/api/paging.h:19-27)
    // flags = SECURED 0x200 | ENABLED 0x100 | 3 pages in the last group = 0x303 ; block_size = 15 ; block_num = 8
    assert_eq!(&b[0..12], &[0x03, 0x03, 0, 0, 15, 0, 0, 0, 8, 0, 0, 0]);
    assert_eq!(&b[12..16], &0x0001_0000u32.to_le_bytes(), "0x10000000 >> 12");
    assert_eq!(&b[16..20], &0x0001_0008u32.to_le_bytes(), "0x10008000 >> 12");
    assert_eq!(&b[12 + 4 * 8..12 + 4 * 9], &0x0001_0040u32.to_le_bytes());
    assert!(b[12 + 4 * 9..144].iter().all(|&x| x == 0), "unused block slots are zero");
    assert_eq!(b[144], 0xaa);
    // the wrong number of addresses is refused by name
    assert_eq!(encode_fw_paging_cmd(&l, &phys[..8], &mut b), Err(EncodeError::TooManyPagingBlocks { n: 8 }));
}
