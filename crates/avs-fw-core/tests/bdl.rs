// SPDX-License-Identifier: GPL-2.0-only
//! cldma_setup_bdle (cldma.c:205-232) as a pure function of the extents behind the data buffer.
use avs_fw_core::bdl::*;
use avs_fw_core::error::FwError;

const K: u32 = 1024;

#[test]
fn one_contiguous_128k_buffer_is_two_64k_periods_with_ioc_on_the_last() {
    // hda_cldma_setup: cldma_setup_bdle(cl, cl->buffer_size / 2), AVS_CL_DEFAULT_BUFFER_SIZE = 128K (cldma.h:13)
    let l = ClDmaLayout::plan(&[Extent { addr: 0x1_2345_0000, len: 128 * K }], 0xABCD_E000, 128 * K, 1).unwrap();
    assert_eq!(l.num_periods(), 2);
    assert_eq!(l.lvi(), 1); // SD_LVI = num_periods - 1 (cldma.c:240)
    assert_eq!(l.entries()[0], BdlEntry { addr: 0x1_2345_0000, len: 64 * K, ioc: false });
    assert_eq!(l.entries()[1], BdlEntry { addr: 0x1_2346_0000, len: 64 * K, ioc: true });
    assert_eq!((l.buffer_size, l.bdl_addr, l.stream_tag), (128 * K, 0xABCD_E000, 1));
}

#[test]
fn entry_bytes_are_four_little_endian_dwords() {
    let e = BdlEntry { addr: 0x1_2345_6000, len: 0x1_0000, ioc: true };
    assert_eq!(e.to_le_bytes(), [0x00, 0x60, 0x45, 0x23, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00]);
    let e = BdlEntry { addr: 0xFFFF_FFFF_0000_1000, len: 7, ioc: false };
    assert_eq!(e.to_le_bytes(), [0x00, 0x10, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn scattered_pages_make_one_entry_per_page_and_only_the_last_has_ioc() {
    // snd_sgbuf_get_chunk_size() stops at a page that is not physically adjacent
    let ex: Vec<Extent> = (0..32).map(|i| Extent { addr: 0x1000_0000 + (i as u64) * 0x2000, len: 4 * K }).collect();
    let l = ClDmaLayout::plan(&ex, 0x5000, 128 * K, 1).unwrap();
    assert_eq!(l.num_periods(), 32);
    assert_eq!(l.lvi(), 31);
    for (i, e) in l.entries().iter().enumerate() {
        assert_eq!(e.addr, 0x1000_0000 + (i as u64) * 0x2000);
        assert_eq!(e.len, 4 * K);
        assert_eq!(e.ioc, i == 31, "entry {i}");
    }
}

#[test]
fn touching_extents_merge_into_one_run_up_to_the_period() {
    let ex = [Extent { addr: 0x10000, len: 4 * K }, Extent { addr: 0x11000, len: 4 * K }, Extent { addr: 0x20000, len: 8 * K }];
    let l = ClDmaLayout::plan(&ex, 0, 16 * K, 1).unwrap(); // period = 8K
    assert_eq!(l.num_periods(), 2);
    assert_eq!(l.entries()[0], BdlEntry { addr: 0x10000, len: 8 * K, ioc: false });
    assert_eq!(l.entries()[1], BdlEntry { addr: 0x20000, len: 8 * K, ioc: true });
}

#[test]
fn a_period_may_straddle_an_extent_boundary_and_continue_in_the_next_entry() {
    // extents of 6K and 10K, period 8K: 6K, 2K (rest of period from the second extent), 8K
    let ex = [Extent { addr: 0x10000, len: 6 * K }, Extent { addr: 0x40000, len: 10 * K }];
    let l = ClDmaLayout::plan(&ex, 0, 16 * K, 1).unwrap();
    let got: Vec<(u64, u32, bool)> = l.entries().iter().map(|e| (e.addr, e.len, e.ioc)).collect();
    assert_eq!(got, vec![(0x10000, 6 * K, false), (0x40000, 8 * K, false), (0x42000, 2 * K, true)]);
}

#[test]
fn exactly_256_entries_fit_the_bdl_page_and_257_do_not() {
    // BDL_SIZE 4096 / 16 (hda_register.h:144-145). Linux has no check and writes past the page.
    let ex: Vec<Extent> = (0..257).map(|i| Extent { addr: (i as u64) * 0x2000, len: 4 * K }).collect();
    let ok = ClDmaLayout::plan(&ex, 0, 256 * 4 * K, 1).unwrap();
    assert_eq!(ok.num_periods(), 256);
    assert_eq!(ok.lvi(), 255);
    // period = 4K+2 (buffer 2*(4K+2)... use a buffer one page larger): 257 pages -> 257 entries
    let e = ClDmaLayout::plan(&ex, 0, 257 * 4 * K, 1).unwrap_err();
    assert_eq!(e, FwError::BdlOverflow { entries: 257, max: 256 });
    assert_eq!(e.name(), "bdl-overflow");
}

#[test]
fn a_zero_or_one_byte_buffer_is_refused_instead_of_looping_forever() {
    // cldma.c:205-232: bdle_size == 0 -> chunk 0 -> `while (remaining > 0)` never ends
    for b in [0u32, 1] {
        let e = ClDmaLayout::plan(&[Extent { addr: 0, len: 64 }], 0, b, 1).unwrap_err();
        assert_eq!(e, FwError::CldmaBufferTooSmall { buffer_size: b });
    }
    assert!(ClDmaLayout::plan(&[Extent { addr: 0, len: 64 }], 0, 2, 1).is_ok());
}

#[test]
fn extents_that_do_not_cover_the_buffer_are_refused() {
    let e = ClDmaLayout::plan(&[Extent { addr: 0, len: 64 * K }], 0, 128 * K, 1).unwrap_err();
    assert_eq!(e, FwError::ExtentsShort { covered: 64 * K as u64, buffer_size: 128 * K });
    let e = ClDmaLayout::plan(&[], 0, 128 * K, 1).unwrap_err();
    assert_eq!(e, FwError::ExtentsShort { covered: 0, buffer_size: 128 * K });
}

#[test]
fn an_odd_buffer_does_not_overshoot() {
    // buffer 9, period 4: 4 + 4 + 1 (Linux would describe 4 + 4 + 4)
    let l = ClDmaLayout::plan(&[Extent { addr: 0x100, len: 64 }], 0, 9, 1).unwrap();
    let got: Vec<(u32, bool)> = l.entries().iter().map(|e| (e.len, e.ioc)).collect();
    assert_eq!(got, vec![(4, false), (4, false), (1, true)]);
}

#[test]
fn the_stream_tag_must_fit_the_four_bit_field() {
    // cldma.c:16-17: tag << 20 masked with GENMASK(23, 20)
    for t in [0u32, 1, 15] {
        assert!(ClDmaLayout::plan(&[Extent { addr: 0, len: 64 }], 0, 32, t).is_ok());
    }
    for t in [16u32, 17, 255] {
        assert_eq!(ClDmaLayout::plan(&[Extent { addr: 0, len: 64 }], 0, 32, t).unwrap_err(), FwError::StreamTagWidth { tag: t });
    }
}

#[test]
fn a_run_that_starts_inside_an_extent_ends_with_that_extent() {
    // extent 1 is 10K, extent 2 is elsewhere; period 8K. The second period starts 8K into extent 1: only 2K remain there.
    let ex = [Extent { addr: 0x10000, len: 10 * K }, Extent { addr: 0x90000, len: 6 * K }];
    let l = ClDmaLayout::plan(&ex, 0, 16 * K, 1).unwrap();
    let got: Vec<(u64, u32, bool)> = l.entries().iter().map(|e| (e.addr, e.len, e.ioc)).collect();
    assert_eq!(got, vec![(0x10000, 8 * K, false), (0x12000, 2 * K, false), (0x90000, 6 * K, true)]);
}

#[test]
fn three_touching_extents_merge_into_one_run() {
    let ex = [Extent { addr: 0x10000, len: 2 * K }, Extent { addr: 0x10800, len: 2 * K }, Extent { addr: 0x11000, len: 2 * K }];
    let l = ClDmaLayout::plan(&ex, 0, 6 * K, 1).unwrap(); // period 3K
    let got: Vec<(u64, u32, bool)> = l.entries().iter().map(|e| (e.addr, e.len, e.ioc)).collect();
    assert_eq!(got, vec![(0x10000, 3 * K, false), (0x10C00, 3 * K, true)]);
}

#[test]
fn eight_touching_one_kilobyte_extents_are_two_four_kilobyte_periods() {
    // a merge that skipped every second extent would cut the run short at 2K
    let ex: Vec<Extent> = (0..8).map(|i| Extent { addr: 0x10_0000 + i * 0x400, len: K }).collect();
    let l = ClDmaLayout::plan(&ex, 0, 8 * K, 1).unwrap();
    let got: Vec<(u64, u32, bool)> = l.entries().iter().map(|e| (e.addr, e.len, e.ioc)).collect();
    assert_eq!(got, vec![(0x10_0000, 4 * K, false), (0x10_1000, 4 * K, true)]);
}
