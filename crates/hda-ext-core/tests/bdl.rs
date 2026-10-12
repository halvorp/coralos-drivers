// SPDX-License-Identifier: GPL-2.0-only
//! The buffer descriptor list of a host stream: chunking, IOC placement, the hardware entry bound and the byte layout.
use avs_fw_core::bdl::{BdlEntry, Extent};
use hda_ext_core::bdl::*;
use hda_ext_core::error::HdaError;

fn e(addr: u64, len: u32) -> Extent {
    Extent { addr, len }
}

fn ent(addr: u64, len: u32, ioc: bool) -> BdlEntry {
    BdlEntry { addr, len, ioc }
}

#[test]
fn one_contiguous_buffer_gives_one_entry_per_period_each_with_ioc() {
    let b = plan_bdl(&[e(0x1000_0000, 16384)], 16384, 4096, true).unwrap();
    assert_eq!(b.entries, vec![ent(0x1000_0000, 4096, true), ent(0x1000_1000, 4096, true), ent(0x1000_2000, 4096, true), ent(0x1000_3000, 4096, true)]);
    assert_eq!((b.lvi(), b.buffer_size, b.period_bytes), (3, 16384, 4096));
}

#[test]
fn a_period_made_of_scattered_pages_ends_in_ioc_only_on_its_last_entry() {
    // eight 4 KiB pages, none touching; periods of two pages
    let pages: Vec<_> = (0..8).map(|i| e(0x4000_0000 + i * 0x10000, 4096)).collect();
    let b = plan_bdl(&pages, 8 * 4096, 8192, true).unwrap();
    assert_eq!(b.entries.len(), 8);
    for (i, x) in b.entries.iter().enumerate() {
        assert_eq!(x.addr, 0x4000_0000 + i as u64 * 0x10000);
        assert_eq!(x.len, 4096);
        assert_eq!(x.ioc, i % 2 == 1, "entry {i}");
    }
}

#[test]
fn without_wakeups_no_entry_raises_an_interrupt() {
    let b = plan_bdl(&[e(0x1000_0000, 8192)], 8192, 4096, false).unwrap();
    assert!(b.entries.iter().all(|x| !x.ioc));
    assert_eq!(b.entries.len(), 2);
}

#[test]
fn touching_extents_merge_into_one_chunk_and_a_period_can_straddle_extents() {
    // two touching 4 KiB extents form an 8 KiB run; then a gap; period = 6 KiB straddles the first run's end
    let ex = [e(0x1000, 4096), e(0x2000, 4096), e(0x9000, 4096)];
    let b = plan_bdl(&ex, 12288, 6144, true).unwrap();
    assert_eq!(b.entries, vec![ent(0x1000, 6144, true), ent(0x2800, 2048, false), ent(0x9000, 4096, true)]);
}

#[test]
fn a_buffer_that_is_not_a_whole_number_of_periods_is_refused_where_linux_truncates() {
    assert_eq!(plan_bdl(&[e(0, 10000)], 10000, 4096, true), Err(HdaError::BufferNotWholePeriods { buffer: 10000, period: 4096 }));
    assert_eq!(plan_bdl(&[e(0, 8192)], 0, 4096, true), Err(HdaError::EmptyBuffer));
    assert_eq!(plan_bdl(&[e(0, 8192)], 8192, 0, true), Err(HdaError::EmptyBuffer));
}

#[test]
fn extents_that_do_not_cover_the_buffer_are_refused_by_name() {
    assert_eq!(plan_bdl(&[e(0x1000, 4096)], 8192, 4096, true), Err(HdaError::ExtentsShort { have: 4096, need: 8192 }));
    assert_eq!(plan_bdl(&[], 4096, 4096, true), Err(HdaError::ExtentsShort { have: 0, need: 4096 }));
}

#[test]
fn the_hardware_holds_exactly_256_entries() {
    // 256 non-touching pages, one period = whole buffer: 256 entries is the page (4096 / 16), LVI = 255
    let pages: Vec<_> = (0..256).map(|i| e(i * 0x2000, 4096)).collect();
    let b = plan_bdl(&pages, 256 * 4096, 256 * 4096, true).unwrap();
    assert_eq!(b.entries.len(), 256);
    assert_eq!(b.lvi(), 255);
    assert!(b.entries[255].ioc && !b.entries[254].ioc);
    // 257 is -EINVAL in Linux (stream.c:462) and a named overflow here
    let pages: Vec<_> = (0..257).map(|i| e(i * 0x2000, 4096)).collect();
    assert_eq!(plan_bdl(&pages, 257 * 4096, 257 * 4096, true), Err(HdaError::BdlOverflow { entries: 257, max: 256 }));
}

#[test]
fn the_byte_layout_is_address_low_high_length_ioc_little_endian() {
    let b = plan_bdl(&[e(0x1_2345_6000, 8192)], 8192, 4096, true).unwrap();
    let mut mem = [0u8; 64];
    assert_eq!(b.write_to(&mut mem), Ok(32));
    assert_eq!(&mem[..16], &[0x00, 0x60, 0x45, 0x23, 0x01, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]);
    assert_eq!(&mem[16..20], &0x1_2345_7000u64.to_le_bytes()[..4]);
    assert!(mem[32..].iter().all(|x| *x == 0), "nothing written past the list");
    // a buffer too small for the list is refused, not truncated
    let mut small = [0u8; 20];
    assert!(b.write_to(&mut small).is_err());
}
