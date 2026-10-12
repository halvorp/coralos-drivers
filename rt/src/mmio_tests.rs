//! Host tests for mmio.rs. `Fake` models `MMIO_MAP` / `MMIO_LEN` over a real aligned buffer, so `ReadWindow` dereferences
//! genuine memory.
use crate::mmio::*;

#[repr(align(4096))]
struct Page([u32; 1024]);

struct Fake {
    base: u64,
    len: u64,
    slot: u32,
    maps: u32,
}
impl MmioRaw for Fake {
    fn map(&mut self, slot: u32) -> u64 {
        self.maps += 1;
        if slot == self.slot { self.base } else { 0 }
    }
    fn len(&mut self, slot: u32) -> u64 {
        if slot == self.slot { self.len } else { 0 }
    }
}

fn page() -> std::boxed::Box<Page> {
    let mut p = std::boxed::Box::new(Page([0; 1024]));
    for (i, w) in p.0.iter_mut().enumerate() {
        *w = 0x1000_0000 + i as u32;
    }
    p
}

#[test]
fn reads_are_offsets_into_the_grant() {
    let p = page();
    let mut k = Fake { base: p.0.as_ptr() as u64, len: 4096, slot: 3, maps: 0 };
    let w = ReadWindow::open(&mut k, 3, 4096).unwrap();
    assert_eq!(w.len(), 4096);
    assert!(!w.is_empty());
    assert_eq!(w.read32(0), Ok(0x1000_0000));
    assert_eq!(w.read32(4), Ok(0x1000_0001));
    assert_eq!(w.read32(4092), Ok(0x1000_0000 + 1023));
    assert_eq!(w.read16(4), Ok(0x0001));
    assert_eq!(w.read16(6), Ok(0x1000));
    assert_eq!(w.read8(7), Ok(0x10));
    assert_eq!(w.read8(4), Ok(0x01));
}

#[test]
fn reads_past_the_end_are_refused_before_any_dereference() {
    let p = page();
    let mut k = Fake { base: p.0.as_ptr() as u64, len: 4096, slot: 0, maps: 0 };
    let w = ReadWindow::open(&mut k, 0, 16).unwrap();
    assert_eq!(w.read32(4096), Err(MmioError::OutOfRange { off: 4096, width: 4, len: 4096 }));
    assert_eq!(w.read32(4100), Err(MmioError::OutOfRange { off: 4100, width: 4, len: 4096 }));
    assert_eq!(w.read16(4096), Err(MmioError::OutOfRange { off: 4096, width: 2, len: 4096 }));
    assert_eq!(w.read8(4096), Err(MmioError::OutOfRange { off: 4096, width: 1, len: 4096 }));
    assert_eq!(w.read32(u64::MAX - 3), Err(MmioError::OutOfRange { off: u64::MAX - 3, width: 4, len: 4096 }));
    assert_eq!(w.read8(u64::MAX), Err(MmioError::OutOfRange { off: u64::MAX, width: 1, len: 4096 }));
    // the last valid offset of each width still works
    assert!(w.read32(4092).is_ok() && w.read16(4094).is_ok() && w.read8(4095).is_ok());
}

#[test]
fn a_window_shorter_than_the_page_cuts_reads_at_its_own_length() {
    let p = page();
    let mut k = Fake { base: p.0.as_ptr() as u64, len: 64, slot: 1, maps: 0 };
    let w = ReadWindow::open(&mut k, 1, 64).unwrap();
    assert!(w.read32(60).is_ok());
    assert_eq!(w.read32(64), Err(MmioError::OutOfRange { off: 64, width: 4, len: 64 }));
}

#[test]
fn misaligned_reads_are_refused() {
    let p = page();
    let mut k = Fake { base: p.0.as_ptr() as u64, len: 4096, slot: 0, maps: 0 };
    let w = ReadWindow::open(&mut k, 0, 16).unwrap();
    for off in [1u64, 2, 3, 5] {
        assert_eq!(w.read32(off), Err(MmioError::Misaligned { off, width: 4 }));
    }
    assert_eq!(w.read16(1), Err(MmioError::Misaligned { off: 1, width: 2 }));
    assert!(w.read8(1).is_ok()); // a byte has no alignment to violate
}

#[test]
fn open_names_each_way_it_can_fail() {
    let p = page();
    let good = p.0.as_ptr() as u64;
    // empty slot
    let mut k = Fake { base: good, len: 4096, slot: 2, maps: 0 };
    assert_eq!(ReadWindow::open(&mut k, 5, 4).unwrap_err(), MmioError::NoWindow { slot: 5 });
    assert_eq!(k.maps, 0, "an empty slot is not mapped");
    // too short: refuses BEFORE mapping
    assert_eq!(ReadWindow::open(&mut k, 2, 4097).unwrap_err(), MmioError::TooShort { slot: 2, len: 4096, need: 4097 });
    assert_eq!(k.maps, 0);
    // exactly enough is enough
    assert!(ReadWindow::open(&mut k, 2, 4096).is_ok());
    // the kernel's "no window" (0) and an error word are not bases
    for bad in [0u64, u64::MAX, u64::MAX - 1, 1 << 47, 0xffff_8000_0000_0000] {
        let mut k = Fake { base: bad, len: 4096, slot: 2, maps: 0 };
        assert_eq!(ReadWindow::open(&mut k, 2, 4).unwrap_err(), MmioError::MapFailed { slot: 2, word: bad });
    }
    // the highest user address is still a base
    let mut k = Fake { base: (1 << 47) - 4096, len: 4096, slot: 2, maps: 0 };
    assert!(ReadWindow::open(&mut k, 2, 4).is_ok());
}

#[test]
fn error_names_are_stable() {
    assert_eq!(MmioError::NoWindow { slot: 0 }.name(), "MMIO-NO-WINDOW");
    assert_eq!(MmioError::TooShort { slot: 0, len: 0, need: 0 }.name(), "MMIO-TOO-SHORT");
    assert_eq!(MmioError::MapFailed { slot: 0, word: 0 }.name(), "MMIO-MAP-FAILED");
    assert_eq!(MmioError::OutOfRange { off: 0, width: 0, len: 0 }.name(), "MMIO-OUT-OF-RANGE");
    assert_eq!(MmioError::Misaligned { off: 0, width: 0 }.name(), "MMIO-MISALIGNED");
    assert_eq!(USER_VA_END, 0x8000_0000_0000);
}

// ---- WriteWindow ------------------------------------------------------------------------------------------------------

static ALLOW: [WriteRange; 2] = [WriteRange::reg32(0x10), WriteRange { off: 0x100, len: 8 }];

fn open_w(p: &mut Page) -> WriteWindow {
    let mut k = Fake { base: p.0.as_mut_ptr() as u64, len: 4096, slot: 3, maps: 0 };
    WriteWindow::open(&mut k, 3, 4096, &ALLOW).unwrap()
}

#[test]
fn an_allowed_write_lands_and_reads_back() {
    let mut p = page();
    let mut w = open_w(&mut p);
    assert_eq!(w.write32(0x10, 0xdead_beef), Ok(()));
    assert_eq!(w.write32(0x100, 1), Ok(()));
    assert_eq!(w.write32(0x104, 2), Ok(()), "the second word of an 8-byte range");
    assert_eq!(w.read32(0x10), Ok(0xdead_beef));
    drop(w);
    assert_eq!(p.0[4], 0xdead_beef);
    assert_eq!(p.0[64], 1);
    assert_eq!(p.0[65], 2);
}

#[test]
fn a_write_outside_the_allow_list_is_refused_by_name_and_touches_nothing() {
    let mut p = page();
    let mut w = open_w(&mut p);
    for off in [0x0u64, 0xc, 0x14, 0xfc, 0x108, 0x200, 0xffc] {
        let e = w.write32(off, 0xffff_ffff).unwrap_err();
        assert_eq!(e, MmioError::WriteNotAllowed { off, width: 4 }, "{off:#x}");
        assert_eq!(e.name(), "MMIO-WRITE-NOT-ALLOWED");
    }
    drop(w);
    for (i, v) in p.0.iter().enumerate() {
        assert_eq!(*v, 0x1000_0000 + i as u32, "word {i} was modified");
    }
}

#[test]
fn write_range_and_alignment_are_checked_before_the_allow_list() {
    let mut p = page();
    let mut w = open_w(&mut p);
    assert_eq!(w.write32(0x12, 1), Err(MmioError::Misaligned { off: 0x12, width: 4 }));
    assert_eq!(w.write32(4096, 1), Err(MmioError::OutOfRange { off: 4096, width: 4, len: 4096 }));
    assert_eq!(w.write32(u64::MAX - 3, 1), Err(MmioError::OutOfRange { off: u64::MAX - 3, width: 4, len: 4096 }), "offset + width overflows");
    assert!(w.allows32(0x10) && !w.allows32(0x14) && !w.allows32(0xe) && w.allows32(0x104) && !w.allows32(0x108));
}

#[test]
fn an_allow_list_outside_the_grant_or_misshapen_is_refused_at_open() {
    let mut p = page();
    let base = p.0.as_mut_ptr() as u64;
    let mk = |allow: &'static [WriteRange]| {
        let mut k = Fake { base, len: 4096, slot: 3, maps: 0 };
        WriteWindow::open(&mut k, 3, 4096, allow)
    };
    static PAST: [WriteRange; 1] = [WriteRange::reg32(4096)];
    static STRADDLE: [WriteRange; 1] = [WriteRange { off: 4092, len: 8 }];
    static UNALIGNED: [WriteRange; 1] = [WriteRange { off: 2, len: 4 }];
    static ZERO: [WriteRange; 1] = [WriteRange { off: 8, len: 0 }];
    static ODD: [WriteRange; 1] = [WriteRange { off: 8, len: 6 }];
    static WRAP: [WriteRange; 1] = [WriteRange { off: u64::MAX - 1, len: 8 }];
    for a in [&PAST[..], &STRADDLE, &UNALIGNED, &ZERO, &ODD, &WRAP] {
        let e = mk(a).unwrap_err();
        assert_eq!(e.name(), "MMIO-ALLOW-LIST-INVALID", "{a:?}");
    }
    static LAST: [WriteRange; 1] = [WriteRange::reg32(4092)];
    assert!(mk(&LAST).is_ok(), "the last word of the window is legal");
    static EMPTY: [WriteRange; 0] = [];
    assert!(mk(&EMPTY).is_ok(), "an empty list is a read-only window");
}

#[test]
fn an_empty_allow_list_refuses_every_write() {
    static EMPTY: [WriteRange; 0] = [];
    let mut p = page();
    let mut k = Fake { base: p.0.as_mut_ptr() as u64, len: 4096, slot: 3, maps: 0 };
    let mut w = WriteWindow::open(&mut k, 3, 4096, &EMPTY).unwrap();
    assert_eq!(w.write32(0, 1), Err(MmioError::WriteNotAllowed { off: 0, width: 4 }));
}

#[test]
fn write_window_open_failures_are_the_read_window_ones() {
    let mut p = page();
    let mut k = Fake { base: p.0.as_mut_ptr() as u64, len: 4096, slot: 3, maps: 0 };
    assert_eq!(WriteWindow::open(&mut k, 9, 4096, &ALLOW).unwrap_err(), MmioError::NoWindow { slot: 9 });
    assert_eq!(WriteWindow::open(&mut k, 3, 8192, &ALLOW).unwrap_err(), MmioError::TooShort { slot: 3, len: 4096, need: 8192 });
    let w = open_w(&mut p);
    assert_eq!(w.len(), 4096);
    assert!(!w.is_empty());
    assert_eq!(w.read8(0), Ok(0x00));
    assert_eq!(w.read16(2), Ok(0x1000));
}
