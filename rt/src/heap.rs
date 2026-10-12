//! A demand-zero heap for a `.device` (HEAP-IS-DEMAND-ZERO) that GROWS with memory (RUNTIME-TABLES-HAVE-NO-CAP) and REUSES what is
//! freed.
//!
//! MECHANISM. The heap is a chain of REGIONS. Each region is one anonymous `MMAP` reservation: address space only, the kernel commits
//! a zeroed frame per page on first touch, so an idle device pays for what it touched, never a zeroed `.bss` arena. The first region
//! is reserved on the first allocation. When no region can satisfy a request, ANOTHER region is reserved (at least one chunk, more for
//! a large request) and linked in. There is no compile-time total: the heap is out of memory only when the kernel REFUSES a
//! reservation, and then `alloc` returns null - which `Vec::try_reserve` and friends report as a named refusal before the thing exists.
//!
//! POLICY. Inside a region, blocks come from `linked_list_allocator::Heap` (first-fit free list with coalescing; MIT OR Apache-2.0,
//! GPL-compatible, already the CoralOS tree's allocator - D9). A freed block is reused; the region's header lives at its start, so the
//! chain costs no storage elsewhere. Regions are never returned to the kernel (there is no unmap call in the ABI).
//!
//! CONCURRENCY. A `.device` is ONE task with ONE thread of control: the ABI has no thread or clone call, so nothing can re-enter the
//! allocator except an interrupt handler, and the ABI delivers interrupts only as `IRQ_WAIT` returns on the same thread. There is
//! therefore NO LOCK on target, and `Sync` is asserted on exactly that ground. A host test harness that runs threads must serialise
//! access itself (the tests wrap the heap in a mutex).
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use linked_list_allocator::Heap;

/// The default region size: 16 MiB of address space per reservation, committed lazily.
pub const DEFAULT_CHUNK: usize = 16 << 20;

/// Reserve `len` bytes of zeroed address space: its base, or 0 when the kernel refuses.
pub type Reserve = fn(len: usize) -> usize;

/// The on-target reservation: anonymous read+write `MMAP`.
pub fn mmap_reserve(len: usize) -> usize {
    let w = crate::mmap(len as u64, 3);
    if w == crate::word::SYSCALL_ERR {
        0
    } else {
        w as usize
    }
}

/// Lives at the start of each region.
struct Region {
    heap: Heap,
    next: *mut Region,
    base: usize,
    len: usize,
}

const HEADER: usize = (core::mem::size_of::<Region>() + 15) & !15;

struct State {
    head: *mut Region,
    regions: usize,
    reserved: usize,
}

/// The global allocator: `#[global_allocator] static HEAP: coralos_rt::heap::LazyHeap = coralos_rt::heap::LazyHeap::new();`
pub struct LazyHeap {
    st: UnsafeCell<State>,
    chunk: usize,
    reserve: Reserve,
}

// SAFETY: a `.device` has a single thread of control (see the module note); the host tests serialise with their own mutex.
unsafe impl Sync for LazyHeap {}

impl LazyHeap {
    /// The on-target heap: regions of `DEFAULT_CHUNK` through `MMAP`.
    pub const fn new() -> Self {
        Self::with_reserve(DEFAULT_CHUNK, mmap_reserve)
    }
    /// A heap over an injected reservation and region size (the host tests).
    pub const fn with_reserve(chunk: usize, reserve: Reserve) -> Self {
        LazyHeap { st: UnsafeCell::new(State { head: core::ptr::null_mut(), regions: 0, reserved: 0 }), chunk, reserve }
    }
    /// Regions reserved so far.
    pub fn regions(&self) -> usize {
        // SAFETY: single thread of control / caller-serialised.
        unsafe { (*self.st.get()).regions }
    }
    /// Address space reserved so far, in bytes.
    pub fn reserved(&self) -> usize {
        // SAFETY: as above.
        unsafe { (*self.st.get()).reserved }
    }
    /// Bytes handed out and not freed.
    pub fn used(&self) -> usize {
        let mut n = 0;
        // SAFETY: the chain holds only regions this heap initialised.
        unsafe {
            let mut r = (*self.st.get()).head;
            while !r.is_null() {
                n += (*r).heap.used();
                r = (*r).next;
            }
        }
        n
    }

    /// Reserve a region able to hold `need` bytes at `align` and link it in front. False if the kernel refuses.
    unsafe fn grow(&self, need: usize, align: usize) -> bool {
        let want = match need.checked_add(HEADER).and_then(|v| v.checked_add(align)).and_then(|v| v.checked_add(4096)) {
            Some(v) => v,
            None => return false,
        };
        let len = match want.max(self.chunk).checked_add(4095) {
            Some(v) => v & !4095,
            None => return false,
        };
        let base = (self.reserve)(len);
        if base == 0 || base % 16 != 0 {
            return false;
        }
        let r = base as *mut Region;
        // SAFETY: `base..base+len` is a fresh zeroed reservation owned by this heap; the header fits (`len >= HEADER`).
        unsafe {
            let st = &mut *self.st.get();
            r.write(Region { heap: Heap::empty(), next: st.head, base, len });
            (*r).heap.init((base + HEADER) as *mut u8, len - HEADER);
            st.head = r;
            st.regions += 1;
            st.reserved += len;
        }
        true
    }
}

impl Default for LazyHeap {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for LazyHeap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        for attempt in 0..2 {
            // SAFETY: the chain holds only regions this heap initialised.
            unsafe {
                let mut r = (*self.st.get()).head;
                while !r.is_null() {
                    if let Ok(p) = (*r).heap.allocate_first_fit(l) {
                        return p.as_ptr();
                    }
                    r = (*r).next;
                }
            }
            // no region could serve it: reserve another (once); a kernel refusal is the only out-of-memory
            if attempt == 1 || !unsafe { self.grow(l.size(), l.align()) } {
                return core::ptr::null_mut();
            }
        }
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        let a = p as usize;
        // SAFETY: as in `alloc`; `p` came from `alloc` with `l`, so exactly one region owns it.
        unsafe {
            let mut r = (*self.st.get()).head;
            while !r.is_null() {
                if a >= (*r).base + HEADER && a < (*r).base + (*r).len {
                    if let Some(nn) = NonNull::new(p) {
                        (*r).heap.deallocate(nn, l);
                    }
                    return;
                }
                r = (*r).next;
            }
        }
        // a pointer no region owns is never freed into one: leaked, as the GlobalAlloc contract makes it the caller's bug
    }
}
