//! The session wire as the kernel writes it (userland/coral-firmament-abi/src/session_wire.rs tests), and the wait-set words.
use coralos_rt::session::*;

#[test]
fn an_announce_decodes_as_the_kernel_writes_it() {
    // session_wire.rs: announce(0x1234, 0x5678, 0x0102_0304_0506_0708) == [0xC0, 0x34, 0x12, 0x78, 0x56, 8, 7, 6, 5, 4, 3, 2, 1]
    let a = [0xC0, 0x34, 0x12, 0x78, 0x56, 8, 7, 6, 5, 4, 3, 2, 1];
    assert_eq!(decode_announce(&a), Some(Announce { req: 0x1234, evt: 0x5678, client: 0x0102_0304_0506_0708 }));
    assert_eq!((ANNOUNCE_TAG, ANNOUNCE_LEN), (0xC0, 13));
}

#[test]
fn a_short_or_mistagged_announce_is_refused() {
    let a = [0xC0, 0x34, 0x12, 0x78, 0x56, 8, 7, 6, 5, 4, 3, 2, 1];
    assert_eq!(decode_announce(&a[..12]), None, "one byte short");
    assert_eq!(decode_announce(&[]), None);
    let mut b = a;
    b[0] = TOMBSTONE_TAG;
    assert_eq!(decode_announce(&b), None);
}

#[test]
fn only_exactly_nine_bytes_starting_dd_is_a_tombstone() {
    let t = [0xDD, 42, 0, 0, 0, 0, 0, 0, 0];
    assert!(is_tombstone(&t));
    assert!(!is_tombstone(&t[..8]));
    let mut long = [0u8; 10];
    long[..9].copy_from_slice(&t);
    assert!(!is_tombstone(&long));
    let mut other = t;
    other[0] = 0xC0;
    assert!(!is_tombstone(&other));
    assert_eq!((TOMBSTONE_TAG, TOMBSTONE_LEN), (0xDD, 9));
}

#[test]
fn the_wait_set_puts_each_endpoint_in_its_own_bit_and_refuses_what_does_not_fit() {
    assert_eq!(ep_set([]), Ok((0, 0)));
    assert_eq!(ep_set([0]), Ok((1, 0)));
    assert_eq!(ep_set([63]), Ok((1 << 63, 0)));
    assert_eq!(ep_set([64]), Ok((0, 1)), "endpoint 64 is bit 0 of the high word");
    assert_eq!(ep_set([65, 70, 127]), Ok((0, (1 << 1) | (1 << 6) | (1 << 63))));
    assert_eq!(ep_set([5, 100]), Ok((1 << 5, 1 << 36)));
    // 128 would wrap onto endpoint 0 (or 64) if shifted: refused by name instead
    assert_eq!(ep_set([3, 128]), Err(128));
    assert_eq!(ep_set([200]), Err(200));
    assert_eq!(WAIT_SET_EPS, 128);
}
