//! Host tests for portio.rs and keywire.rs. `FakeKernel` models the kernel's `PORT_IO` as the ABI in portio.rs states it,
//! including its own enforcement, so a test that gets past this crate's checks still meets the kernel's.
use crate::keywire;
use crate::portio::*;

/// (base, len, rights) per slot; `log` records every call that reached the "kernel".
struct FakeKernel {
    caps: [(u16, u16, u8); SLOTS as usize],
    log: std::vec::Vec<(u64, u64, u64)>,
    data: u32,
}
impl FakeKernel {
    fn new(caps: &[(u16, u16, u8)]) -> Self {
        let mut c = [(0, 0, 0); SLOTS as usize];
        c[..caps.len()].copy_from_slice(caps);
        FakeKernel { caps: c, log: std::vec::Vec::new(), data: 0x1122_3344 }
    }
}
impl PortRaw for FakeKernel {
    fn call(&mut self, op: u64, a: u64, v: u64) -> u64 {
        self.log.push((op, a, v));
        if op == OP_QUERY {
            return match self.caps.get(a as usize) {
                Some(&(b, l, r)) if l != 0 => b as u64 | (l as u64) << 16 | (r as u64) << 32,
                _ => REFUSED,
            };
        }
        let (width, dir, wbit) = match op {
            OP_IN8 => (1u32, rights::IN, rights::W8),
            OP_OUT8 => (1, rights::OUT, rights::W8),
            OP_IN16 => (2, rights::IN, rights::W16),
            OP_OUT16 => (2, rights::OUT, rights::W16),
            OP_IN32 => (4, rights::IN, rights::W32),
            OP_OUT32 => (4, rights::OUT, rights::W32),
            _ => return REFUSED,
        };
        let ok = self.caps.iter().any(|&(b, l, r)| {
            l != 0 && a >= b as u64 && a + width as u64 <= b as u64 + l as u64 && r & dir != 0 && r & wbit != 0
        });
        if !ok {
            return REFUSED;
        }
        if dir == rights::IN { (self.data & (u32::MAX >> (32 - 8 * width))) as u64 } else { 0 }
    }
}

const RW8: u8 = rights::IN | rights::OUT | rights::W8;
/// The i8042's two grants: byte access to 0x60 and to 0x64, as two ranges (not 0x60..0x65 — that would include 0x61-0x63).
fn i8042() -> FakeKernel {
    FakeKernel::new(&[(0x60, 1, RW8), (0x64, 1, RW8)])
}

#[test]
fn open_reports_what_the_kernel_granted_and_an_empty_slot_is_no_range() {
    let mut k = i8042();
    let r = PortRange::open(&mut k, 1).unwrap();
    assert_eq!((r.len(), r.rights()), (1, RW8));
    assert_eq!(PortRange::open(&mut k, 2), Err(PortError::NoRange));
    assert_eq!(PortRange::open(&mut k, SLOTS), Err(PortError::NoRange));
    assert_eq!(k.log.len(), 2, "a slot past the table never reaches the kernel");
}

#[test]
fn find_picks_the_range_that_covers_the_port_and_names_why_not() {
    let mut k = i8042();
    assert!(PortRange::find(&mut k, 0x64, RW8).is_ok());
    assert_eq!(PortRange::find(&mut k, 0x61, RW8), Err(PortError::NoRange), "the port between the two grants");
    assert_eq!(PortRange::find(&mut k, 0x65, RW8), Err(PortError::NoRange), "one past the end");
    assert_eq!(PortRange::find(&mut k, 0x60, rights::W16), Err(PortError::Direction), "covered, but lacks the right asked for");
    let mut none = FakeKernel::new(&[]);
    assert_eq!(PortRange::find(&mut none, 0x60, rights::IN), Err(PortError::NoRange));
}

#[test]
fn a_byte_in_and_out_reach_the_kernel_at_the_absolute_port() {
    let mut k = i8042();
    let cmd = PortRange::find(&mut k, 0x64, RW8).unwrap();
    k.log.clear();
    assert_eq!(cmd.in8(&mut k, 0), Ok(0x44));
    assert_eq!(cmd.out8(&mut k, 0, 0xAD), Ok(()));
    assert_eq!(k.log, [(OP_IN8, 0x64, 0), (OP_OUT8, 0x64, 0xAD)]);
}

#[test]
fn offsets_past_the_range_are_refused_before_the_kernel_is_asked() {
    let mut k = i8042();
    let data = PortRange::find(&mut k, 0x60, RW8).unwrap();
    k.log.clear();
    assert_eq!(data.in8(&mut k, 1), Err(PortError::OutOfRange), "0x61");
    assert_eq!(data.out8(&mut k, 4, 0), Err(PortError::OutOfRange), "0x64 through the 0x60 handle");
    assert_eq!(data.in8(&mut k, u16::MAX), Err(PortError::OutOfRange), "offset arithmetic must not wrap");
    assert!(k.log.is_empty());
}

#[test]
fn a_wider_access_than_granted_is_refused_by_width() {
    let mut k = i8042();
    let data = PortRange::find(&mut k, 0x60, RW8).unwrap();
    assert_eq!(data.in16(&mut k, 0), Err(PortError::Width));
    assert_eq!(data.out32(&mut k, 0, 1), Err(PortError::Width));
}

#[test]
fn a_range_wide_enough_in_length_still_refuses_a_width_it_lacks_and_one_that_crosses_its_end() {
    let mut k = FakeKernel::new(&[(0x200, 8, rights::IN | rights::OUT | rights::W8 | rights::W16)]);
    let r = PortRange::open(&mut k, 0).unwrap();
    assert_eq!(r.in16(&mut k, 6), Ok(0x3344));
    assert_eq!(r.in16(&mut k, 7), Err(PortError::OutOfRange), "a 16-bit access at the last byte crosses the end");
    assert_eq!(r.in32(&mut k, 0), Err(PortError::Width));
}

#[test]
fn a_read_only_grant_cannot_write_and_a_write_only_grant_cannot_read() {
    let mut k = FakeKernel::new(&[(0x60, 1, rights::IN | rights::W8), (0x64, 1, rights::OUT | rights::W8)]);
    let ro = PortRange::open(&mut k, 0).unwrap();
    let wo = PortRange::open(&mut k, 1).unwrap();
    assert_eq!(ro.out8(&mut k, 0, 1), Err(PortError::Direction));
    assert_eq!(wo.in8(&mut k, 0), Err(PortError::Direction));
    assert_eq!(ro.in8(&mut k, 0), Ok(0x44));
    assert_eq!(wo.out8(&mut k, 0, 1), Ok(()));
}

#[test]
fn a_handle_the_kernel_no_longer_backs_is_refused_by_the_kernel() {
    // A stale capability: the handle was valid when opened; the grant is gone by the time it is used.
    let mut k = i8042();
    let r = PortRange::open(&mut k, 0).unwrap();
    k.caps[0] = (0, 0, 0);
    assert_eq!(r.in8(&mut k, 0), Err(PortError::Refused));
}

#[test]
fn error_names_are_distinct() {
    let all = [PortError::NoRange, PortError::OutOfRange, PortError::Direction, PortError::Width, PortError::Refused];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a.name(), b.name());
        }
    }
}

#[test]
fn the_key_frame_round_trips_and_refuses_what_is_not_one() {
    assert_eq!(keywire::encode(0x04, keywire::PRESS), [0x90, 0x04, 1]);
    assert_eq!(keywire::decode(&[0x90, 0xE1, 2]), Some(keywire::KeyFrame { usage: 0xE1, state: keywire::REPEAT }));
    assert_eq!(keywire::decode(&[0x90, 0x04, 4]), None, "a state past ALL_UP");
    assert_eq!(keywire::decode(&[0x91, 0x04, 1]), None);
    assert_eq!(keywire::decode(&[0x90, 0x04]), None);
    assert_eq!(keywire::decode(&[0x90, 0x04, 1, 0]), None);
    assert_eq!(keywire::decode(b"a"), None, "text on the same endpoint is not a frame");
    assert_eq!((keywire::KEY_EP, keywire::MARKER, keywire::LEN), (12, 0x90, 3));
    assert!((0x80..=0xBF).contains(&keywire::MARKER), "the lead byte must lie in the UTF-8 continuation range");
}

#[test]
fn the_rights_bits_and_ops_are_the_published_values() {
    assert_eq!((rights::IN, rights::OUT, rights::W8, rights::W16, rights::W32), (1, 2, 4, 8, 16));
    assert_eq!([OP_IN8, OP_OUT8, OP_IN16, OP_OUT16, OP_IN32, OP_OUT32, OP_QUERY], [0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(crate::nr::PORT_IO, 52);
    assert_eq!(SLOTS, 4);
}

#[test]
fn an_offset_reaches_the_port_base_plus_offset() {
    let mut k = FakeKernel::new(&[(0x200, 8, RW8)]);
    let r = PortRange::open(&mut k, 0).unwrap();
    k.log.clear();
    assert_eq!(r.in8(&mut k, 3), Ok(0x44));
    r.out8(&mut k, 7, 9).unwrap();
    assert_eq!(k.log, [(OP_IN8, 0x203, 0), (OP_OUT8, 0x207, 9)]);
}
