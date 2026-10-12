//! A demand-zero heap for a `.device` (HEAP-IS-DEMAND-ZERO): the heap is ONE anonymous reservation made with `MMAP` on the first
//! allocation, never a zeroed `.bss` arena. `MMAP` reserves address space only; the kernel commits a zeroed frame per page on first
//! touch, so an idle device pays for what it has touched and nothing more, on every exec path.
//!
//! POLICY (spec-written here; the mechanism is the ABI's `MMAP`, nothing else): a bump allocator over the reservation that grows
//! its working set from the bottom. A free of the LAST block rolls the bump pointer back; a `realloc` of the last block grows or
//! shrinks it in place (the common case of one growing `Vec`); a free of any other block is a no-op (the space is not reused).
//! That is deliberate and bounded - the reservation is the cap, and exhausting it is a NAMED refusal: `alloc` returns null, which
//! `Vec::try_reserve` reports as an error before the thing exists. Doctrine RUNTIME-TABLES-HAVE-NO-CAP: tables grow with memory;
//! only the reservation bounds them.
//!
//! TODO(heap-reuse) delete-when: a silicon run of the device's steady state (free of a non-last block, then a new block) leaves
//! `LazyHeap::left()` under 25% of `DEFAULT_RESERVE` after 10 minutes, or the first `ENDOWMENT`-style refusal that names the heap - replace the bump with a first-fit free list then. The interface does not change.
use core::alloc::{GlobalAlloc, Layout};
#[cfg(test)]
extern crate std;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

/// The default reservation: 64 MiB of address space, committed lazily.
pub const DEFAULT_RESERVE: usize = 64 << 20;

/// Reserve `len` bytes of zeroed address space: its base, or 0.
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

struct State {
    base: usize,
    top: usize,
    next: usize,
    /// Start of the most recent block (for rollback / in-place realloc).
    last: usize,
    tried: bool,
}

/// The global allocator: `#[global_allocator] static HEAP: coralos_rt::heap::LazyHeap = coralos_rt::heap::LazyHeap::new();`
pub struct LazyHeap {
    st: UnsafeCell<State>,
    lock: AtomicBool,
    len: usize,
    reserve: Reserve,
}

// SAFETY: every access to `st` is under the `lock` spin flag.
unsafe impl Sync for LazyHeap {}

impl LazyHeap {
    /// The on-target heap: `DEFAULT_RESERVE` bytes through `MMAP`.
    pub const fn new() -> Self {
        Self::with_reserve(DEFAULT_RESERVE, mmap_reserve)
    }
    /// A heap over an injected reservation (the host tests).
    pub const fn with_reserve(len: usize, reserve: Reserve) -> Self {
        LazyHeap { st: UnsafeCell::new(State { base: 0, top: 0, next: 0, last: 0, tried: false }), lock: AtomicBool::new(false), len, reserve }
    }
    /// Run `f` under the lock. A `.device` is one task with one thread of control, so the lock is never contended on target; if it
    /// somehow is, the caller is REFUSED (`None` -> a null allocation, a named refusal) rather than spun on - a heap never spins
    /// (NO-POLL-BLOCK-ON-WAKE). The host test harness runs threads, so under `cfg(test)` only, contention waits for the holder.
    fn with<R>(&self, f: impl FnOnce(&mut State, usize, Reserve) -> R) -> Option<R> {
        loop {
            if self.lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
                break;
            }
            #[cfg(not(test))]
            return None;
            #[cfg(test)]
            std::thread::yield_now();
        }
        // SAFETY: the flag is held.
        let r = f(unsafe { &mut *self.st.get() }, self.len, self.reserve);
        self.lock.store(false, Ordering::Release);
        Some(r)
    }
    /// Bytes handed out and not rolled back (a measure, for the device's log).
    pub fn used(&self) -> usize {
        self.with(|s, _, _| s.next - s.base).unwrap_or(0)
    }
    /// Bytes still reserved.
    pub fn left(&self) -> usize {
        self.with(|s, _, _| s.top - s.next).unwrap_or(0)
    }
}

impl Default for LazyHeap {
    fn default() -> Self {
        Self::new()
    }
}

fn ensure(s: &mut State, len: usize, reserve: Reserve) -> bool {
    if !s.tried {
        s.tried = true;
        let base = reserve(len);
        if base != 0 {
            s.base = base;
            s.next = base;
            s.last = base;
            s.top = base.saturating_add(len);
        }
    }
    s.top != 0
}

unsafe impl GlobalAlloc for LazyHeap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        self.with(|s, len, reserve| {
            if !ensure(s, len, reserve) {
                return core::ptr::null_mut();
            }
            let at = match s.next.checked_add(l.align() - 1) {
                Some(v) => v & !(l.align() - 1),
                None => return core::ptr::null_mut(),
            };
            match at.checked_add(l.size()) {
                Some(end) if end <= s.top => {
                    s.last = at;
                    s.next = end;
                    at as *mut u8
                }
                _ => core::ptr::null_mut(),
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        self.with(|s, _, _| {
            if p as usize == s.last && p as usize + l.size() == s.next {
                s.next = s.last;
            }
        });
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let in_place = self.with(|s, _, _| {
            if p as usize == s.last && p as usize + l.size() == s.next {
                match (p as usize).checked_add(new) {
                    Some(end) if end <= s.top => {
                        s.next = end;
                        true
                    }
                    _ => false,
                }
            } else {
                false
            }
        })
        .unwrap_or(false);
        if in_place {
            return p;
        }
        // SAFETY: `new` and `l.align()` form a valid layout whenever the caller's contract holds.
        let q = unsafe { self.alloc(Layout::from_size_align_unchecked(new, l.align())) };
        if !q.is_null() {
            // SAFETY: both blocks are live, distinct and at least min(l.size(), new) long.
            unsafe { core::ptr::copy_nonoverlapping(p, q, l.size().min(new)) };
        }
        q
    }
}
