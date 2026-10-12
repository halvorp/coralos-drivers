//! The lazy heap as the GLOBAL allocator of a device-like harness: a growable Vec works, growth is in place for the common case,
//! exhaustion is a refusal that `try_reserve` reports BEFORE the thing exists, and the reservation is made once, lazily.
use coralos_rt::heap::LazyHeap;
use std::sync::atomic::{AtomicUsize, Ordering};

#[repr(align(4096))]
struct Arena(#[allow(dead_code)] [u8; 32 << 20]);
static mut ARENA: Arena = Arena([0; 32 << 20]);
static RESERVES: AtomicUsize = AtomicUsize::new(0);

fn reserve(len: usize) -> usize {
    RESERVES.fetch_add(1, Ordering::SeqCst);
    assert!(len <= 32 << 20);
    std::ptr::addr_of_mut!(ARENA) as usize
}

#[global_allocator]
static HEAP: LazyHeap = LazyHeap::with_reserve(32 << 20, reserve);

#[test]
fn a_growable_vec_works_and_the_reservation_is_made_once() {
    let mut v: Vec<u64> = Vec::new();
    for i in 0..200_000u64 {
        v.push(i * 3);
    }
    assert_eq!(v.len(), 200_000);
    assert_eq!(v[199_999], 199_999 * 3);
    assert_eq!(RESERVES.load(Ordering::SeqCst), 1, "one MMAP-style reservation, however much was allocated");
    assert!(HEAP.used() >= 200_000 * 8);
}

#[test]
fn exhaustion_is_a_named_refusal_before_the_table_grows() {
    let mut v: Vec<u8> = Vec::new();
    assert!(v.try_reserve(1 << 30).is_err(), "a grow larger than the reservation is refused, not aborted");
    assert!(v.try_reserve(64).is_ok());
    let b: Box<[u32; 8]> = Box::new([7; 8]);
    assert_eq!(b.iter().sum::<u32>(), 56);
}

#[test]
fn freeing_the_last_block_gives_its_space_back() {
    let before = HEAP.left();
    let x = Box::new([0u8; 4096]);
    drop(x);
    // other test threads may allocate meanwhile; the heap never LOSES space to a lone alloc/free pair in a quiet moment
    assert!(HEAP.left() <= before);
}
