//! The lazy heap as the GLOBAL allocator of a device-like harness (a mutex stands in for the device's single thread of control), plus
//! direct tests of reuse, growth and refusal over an injected reservation.
use coralos_rt::heap::LazyHeap;
use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

// ---- the global-allocator harness ------------------------------------------------------------------------------------------------

struct Serial(Mutex<()>, LazyHeap);
unsafe impl GlobalAlloc for Serial {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let _g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { self.1.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        let _g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { self.1.dealloc(p, l) }
    }
}

static GLOBAL_RESERVES: AtomicUsize = AtomicUsize::new(0);
fn global_reserve(len: usize) -> usize {
    GLOBAL_RESERVES.fetch_add(1, Ordering::SeqCst);
    // the host's own allocator is not the global one here (System is reached through mmap-like leaked pages)
    sys_reserve(len)
}
fn sys_reserve(len: usize) -> usize {
    let l = Layout::from_size_align(len, 4096).unwrap();
    unsafe { std::alloc::System.alloc_zeroed(l) as usize }
}

#[global_allocator]
static HEAP: Serial = Serial(Mutex::new(()), LazyHeap::with_reserve(1 << 20, global_reserve));

#[test]
fn a_growable_vec_works_as_the_global_allocator() {
    let mut v: Vec<u64> = Vec::new();
    for i in 0..300_000u64 {
        v.push(i * 3);
    }
    assert_eq!(v[299_999], 299_999 * 3);
    assert!(GLOBAL_RESERVES.load(Ordering::SeqCst) >= 1);
    assert!(v.try_reserve(usize::MAX / 2).is_err(), "an impossible grow is refused, not aborted");
}

// ---- direct tests over an injected reservation -----------------------------------------------------------------------------------

static RESERVED_BYTES: AtomicUsize = AtomicUsize::new(0);
static RESERVE_CALLS: AtomicUsize = AtomicUsize::new(0);
static FAIL_AFTER: AtomicUsize = AtomicUsize::new(usize::MAX);
fn counting_reserve(len: usize) -> usize {
    if RESERVE_CALLS.fetch_add(1, Ordering::SeqCst) >= FAIL_AFTER.load(Ordering::SeqCst) {
        return 0;
    }
    RESERVED_BYTES.fetch_add(len, Ordering::SeqCst);
    sys_reserve(len)
}
// these three tests share the counters: serialise them
static SERIAL: Mutex<()> = Mutex::new(());

fn reset(fail_after: usize) {
    RESERVED_BYTES.store(0, Ordering::SeqCst);
    RESERVE_CALLS.store(0, Ordering::SeqCst);
    FAIL_AFTER.store(fail_after, Ordering::SeqCst);
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

#[test]
fn a_million_mixed_size_allocs_and_frees_stay_within_a_small_multiple_of_the_live_set() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset(usize::MAX);
    let chunk = 1 << 20;
    let h = LazyHeap::with_reserve(chunk, counting_reserve);
    let mut rng = Lcg(7);
    const LIVE: usize = 2000;
    let mut live: Vec<Option<(*mut u8, Layout)>> = vec![None; LIVE];
    let (mut live_bytes, mut peak_live) = (0usize, 0usize);
    for i in 0..1_000_000u32 {
        let slot = (rng.next() as usize) % LIVE;
        if let Some((p, l)) = live[slot].take() {
            unsafe {
                assert_eq!(*p, (slot & 0xff) as u8, "contents survived");
                h.dealloc(p, l)
            };
            live_bytes -= l.size();
        } else {
            let size = 8 + (rng.next() as usize) % 4000;
            let align = [8, 16, 64, 4096][(rng.next() % 4) as usize];
            let l = Layout::from_size_align(size, align).unwrap();
            let p = unsafe { h.alloc(l) };
            assert!(!p.is_null(), "op {i}");
            assert_eq!(p as usize % align, 0);
            unsafe { *p = (slot & 0xff) as u8 };
            live[slot] = Some((p, l));
            live_bytes += size;
            peak_live = peak_live.max(live_bytes);
        }
    }
    let reserved = RESERVED_BYTES.load(Ordering::SeqCst);
    assert!(peak_live > 1 << 20, "the live set is real: {peak_live}");
    // without reuse, 500k allocations of ~2 KiB would reserve a GiB; with it the heap stays near the live set (4096-aligned
    // blocks add padding, hence a multiple, not equality)
    assert!(reserved <= 4 * peak_live + 2 * chunk, "reserved {reserved} for a peak live set of {peak_live}");
    assert_eq!(h.reserved(), reserved);
    for s in live.into_iter().flatten() {
        unsafe { h.dealloc(s.0, s.1) };
    }
    assert_eq!(h.used(), 0, "everything returned");
}

#[test]
fn the_heap_grows_past_its_first_region_and_a_large_request_gets_its_own() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset(usize::MAX);
    let chunk = 64 << 10;
    let h = LazyHeap::with_reserve(chunk, counting_reserve);
    let l = Layout::from_size_align(10_000, 16).unwrap();
    let mut ptrs = vec![];
    for i in 0..100 {
        let p = unsafe { h.alloc(l) };
        assert!(!p.is_null());
        unsafe { p.write_bytes(i as u8, l.size()) };
        ptrs.push(p);
    }
    assert!(h.regions() >= 15, "1 MB of live blocks cannot fit one 64 KiB region: {}", h.regions());
    for (i, p) in ptrs.iter().enumerate() {
        assert_eq!(unsafe { **p }, i as u8);
        assert_eq!(unsafe { *p.add(l.size() - 1) }, i as u8, "no block overlaps another");
    }
    let before = h.regions();
    let big = Layout::from_size_align(1 << 20, 4096).unwrap();
    let p = unsafe { h.alloc(big) };
    assert!(!p.is_null(), "a request larger than a chunk is served by a region sized for it");
    assert_eq!(h.regions(), before + 1);
    unsafe { h.dealloc(p, big) };
    // freed memory is reused: the same request does not reserve again
    let again = unsafe { h.alloc(big) };
    assert!(!again.is_null());
    assert_eq!(h.regions(), before + 1);
}

#[test]
fn a_refused_reservation_is_null_and_the_heap_keeps_working() {
    let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset(1); // the first reservation succeeds, every later one is refused by the "kernel"
    let h = LazyHeap::with_reserve(64 << 10, counting_reserve);
    let l = Layout::from_size_align(8192, 8).unwrap();
    let mut got = vec![];
    loop {
        let p = unsafe { h.alloc(l) };
        if p.is_null() {
            break;
        }
        got.push(p);
        assert!(got.len() < 100, "must run out");
    }
    assert!(!got.is_empty());
    assert_eq!(h.regions(), 1);
    // out of memory is not corruption: a free makes room again
    unsafe { h.dealloc(got.pop().unwrap(), l) };
    assert!(!unsafe { h.alloc(l) }.is_null());
    // and a heap whose FIRST reservation is refused answers null too
    reset(0);
    let h2 = LazyHeap::with_reserve(64 << 10, counting_reserve);
    assert!(unsafe { h2.alloc(l) }.is_null());
    assert_eq!(h2.regions(), 0);
}
