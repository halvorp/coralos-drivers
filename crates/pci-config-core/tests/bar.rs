// SPDX-License-Identifier: GPL-2.0-only
//! Frozen BAR decode and sizing vectors from Linux PCI probe logic.
//!
//! Ported from `drivers/pci/probe.c` and `include/uapi/linux/pci_regs.h`.
//! Copyright Drew Eckhardt, Martin Mares, and the Linux PCI authors.

use pci_config_core::bar::{
    bar_size, bars, decode_bar, size_bar, Bar, BarError, BarKind, BAR_NAMES, BASE_ADDRESS_IO_MASK,
    BASE_ADDRESS_MEM_MASK,
};

fn put(c: &mut [u8], offset: usize, value: u32) {
    c[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn io_memory32_memory64_and_prefetch_decode_match_linux() {
    let mut c = [0u8; 256];
    put(&mut c, 0x10, 0x0000_c001); // I/O, pci_regs.h:102-111
    put(&mut c, 0x14, 0x8123_4008); // 32-bit memory + prefetch
    put(&mut c, 0x18, 0x3456_7004); // 64-bit memory low
    put(&mut c, 0x1c, 0x0000_0012); // 64-bit memory high

    assert_eq!(
        decode_bar(&c, 0),
        Ok(Bar {
            index: 0,
            offset: 0x10,
            kind: BarKind::Io,
            address: 0x0000_c000,
            prefetchable: false,
            slots: 1,
        })
    );
    assert_eq!(
        decode_bar(&c, 1),
        Ok(Bar {
            index: 1,
            offset: 0x14,
            kind: BarKind::Memory32,
            address: 0x8123_4000,
            prefetchable: true,
            slots: 1,
        })
    );
    assert_eq!(
        decode_bar(&c, 2),
        Ok(Bar {
            index: 2,
            offset: 0x18,
            kind: BarKind::Memory64,
            address: 0x0000_0012_3456_7000,
            prefetchable: false,
            slots: 2,
        })
    );
}

#[test]
fn a_64_bit_bar_consumes_two_slots_in_the_walk() {
    let mut c = [0u8; 256];
    put(&mut c, 0x10, 0x0000_1004); // BAR 0 low, 64 bit
    put(&mut c, 0x14, 0x0000_0001); // BAR 0 high; would look like I/O if wrongly visited
    put(&mut c, 0x18, 0x0000_2001); // BAR 2, I/O
    let got: Vec<Bar> = bars(&c).collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(
        got.len(),
        5,
        "six physical slots minus one consumed upper slot"
    );
    let indexes: Vec<u8> = got.iter().map(|bar| bar.index).collect();
    assert_eq!(
        indexes,
        vec![0, 2, 3, 4, 5],
        "probe.c:376-380 increments pos for a 64-bit BAR"
    );
    assert_eq!(got[1].address, 0x2000);
}

#[test]
fn bar_kind_literals_and_standard_count_are_pinned() {
    // pci_regs.h:102-111.
    let expected = [
        ("SPACE", 0x01u32),
        ("SPACE_IO", 0x01),
        ("MEM_TYPE_MASK", 0x06),
        ("MEM_TYPE_32", 0x00),
        ("MEM_TYPE_1M", 0x02),
        ("MEM_TYPE_64", 0x04),
        ("MEM_PREFETCH", 0x08),
        ("MEM_MASK", 0xffff_fff0),
        ("IO_MASK", 0xffff_fffc),
    ];
    assert_eq!(expected.len(), 9);
    assert_eq!(BASE_ADDRESS_MEM_MASK, expected[7].1);
    assert_eq!(BASE_ADDRESS_IO_MASK, expected[8].1);
    assert_eq!(pci_config_core::regs::STD_NUM_BARS, 6, "pci_regs.h:37");
    assert_eq!(BAR_NAMES.len(), 6);
    assert_eq!(
        BAR_NAMES,
        ["BAR 0", "BAR 1", "BAR 2", "BAR 3", "BAR 4", "BAR 5"],
        "pci.c:802-807"
    );
}

#[test]
fn sizing_arithmetic_matches_linux_pci_size() {
    // probe.c:112-131. A memory probe returning ffff_f000 means a 4 KiB aperture.
    assert_eq!(bar_size(0x8000_0000, 0xffff_f000, 0xffff_fff0), Ok(0x1000));
    // The same arithmetic spans the upper dword for a 64-bit 8 GiB aperture.
    assert_eq!(
        bar_size(0x10_0000_0000, 0xffff_fffe_0000_0000, 0xffff_ffff_ffff_fff0),
        Ok(0x2_0000_0000)
    );
    assert_eq!(
        bar_size(0, 0, 0xffff_fff0),
        Err(BarError::SizeMaskIsZero { mask: 0 }),
        "probe.c:114-116"
    );
}

#[test]
fn size_bar_applies_the_kind_specific_mask() {
    let mut c = [0u8; 256];
    put(&mut c, 0x10, 0x0000_c001);
    put(&mut c, 0x14, 0x8000_0000);
    put(&mut c, 0x18, 0x0000_0004);
    put(&mut c, 0x1c, 0x0000_0010);
    assert_eq!(
        size_bar(decode_bar(&c, 0).unwrap(), 0xffff_ff01, None),
        Ok(0x100)
    );
    assert_eq!(
        size_bar(decode_bar(&c, 1).unwrap(), 0xffff_e000, None),
        Ok(0x2000)
    );
    assert_eq!(
        size_bar(decode_bar(&c, 2).unwrap(), 0xffff_fff0, Some(0xffff_ffff)),
        Ok(0x10)
    );
}

#[test]
fn impossible_bar_inputs_are_named() {
    let c = [0u8; 32];
    assert_eq!(
        decode_bar(&c, 6),
        Err(BarError::BarIndexOutOfRange { index: 6, count: 6 })
    );
    let short = [0u8; 0x12];
    assert_eq!(
        decode_bar(&short, 0),
        Err(BarError::ConfigTooShort {
            length: 0x12,
            required: 0x14
        })
    );
    let mut all_ones = [0u8; 256];
    put(&mut all_ones, 0x10, 0xffff_ffff);
    assert_eq!(
        decode_bar(&all_ones, 0),
        Err(BarError::ProbeValueAllOnes { value: 0xffff_ffff }),
        "probe.c:240-245"
    );
    let ordinary = Bar {
        index: 0,
        offset: 0x10,
        kind: BarKind::Memory32,
        address: 0,
        prefetchable: false,
        slots: 1,
    };
    assert_eq!(
        size_bar(ordinary, 0xffff_ffff, None),
        Err(BarError::ProbeValueAllOnes { value: 0xffff_ffff }),
        "probe.c:231-238"
    );
    let mut last = [0u8; 256];
    put(&mut last, 0x24, 0x0000_0004);
    assert_eq!(
        decode_bar(&last, 5),
        Err(BarError::Memory64MissingUpperSlot { index: 5, count: 6 })
    );
}
