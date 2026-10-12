//! Published with the owner's approval, 2026-10-11 ("yes, publish rt/ to the drivers repo"). MIT OR Apache-2.0.
//!
//! coralos-rt: the CoralOS x86_64 kernel ABI a ring-3 `.device` driver needs, and nothing else. See Cargo.toml.
//!
//! CALLING CONVENTION. `syscall` with the number in RAX and arguments in RDI, RSI, RDX, R10; the result in RAX; RCX and
//! R11 clobbered. Every call below is capability-checked by the kernel against the calling task: holding a number
//! (an endpoint, a slot, a line) is never authority.
//!
//! WHAT IS HERE (every fact this crate makes public):
//!   numbers  WRITE 0 · EXIT 1 · SEND 3 · RECV 4 · MMIO_MAP 8 · IRQ_WAIT 10 · TRY_RECV 20 · CONNECT 21 · TICKS 24 ·
//!            MMAP 25 · PORT_IO 52 · WAIT_ANY_IRQ 80 · MMIO_LEN 90 · DMA_ALLOC_TYPED 103 · DMA_QUOTA 105 · IRQ_LINE 109 · IRQ_ARM 110
//!   words    SYSCALL_ERR / SEND_DENIED / WAIT_TIMEOUT = u64::MAX · WAIT_IRQ_FIRED = MAX-1 · WAIT_IRQ_DENIED = MAX-2 ·
//!            PEER_DEAD = MAX-6 · SEND_OK 0 · SEND_RING_FULL 1 · DMA_ALLOC_TYPED refusals MAX-120..=MAX-127 (8, named)
//!   layout   the DMA out-record `{ uva: u64, phys: u64 }` (repr C, 16 bytes) · DMA cache argument: low byte WB 0 / WC 1 /
//!            UC 2, bit 8 = whole run below 4 GiB
//!   port I/O (2026-10-11, Astra's ruling; portio.rs) PORT_IO ops IN8/OUT8/IN16/OUT16/IN32/OUT32 = 0..5 and QUERY = 6;
//!            refusal u64::MAX; QUERY(slot) = `base | len << 16 | rights << 32`; rights IN 1 · OUT 2 · W8 4 · W16 8 · W32 16;
//!            4 slots. A driver holds an opaque `PortRange`, never a raw port.
//!   read-only MMIO (2026-10-12; mmio.rs) `ReadWindow`: the loader's window in a slot, MMIO_MAP / MMIO_LEN above; read8/16/32 at an
//!            OFFSET checked against the granted length and the width; no write is exposed. No new system call or number.
//!   write-allow-listed MMIO (2026-10-12; mmio.rs) `WriteWindow`: a window readable everywhere and writable ONLY at the `&'static`
//!            `WriteRange` list given at open; one 32-bit `write32` at an OFFSET checked against the length, alignment and the list
//!            (a write outside it is `MMIO-WRITE-NOT-ALLOWED` and touches nothing); no 8/16-bit write. No new system call or number.
//!   key wire (2026-10-11; keywire.rs) the physical key event frame `[0x90, HID usage, state]` on endpoint 12 (KEY_EP);
//!            state RELEASE 0 · PRESS 1 · REPEAT 2 · ALL_UP 3
//!   heap     (2026-10-12; heap.rs) `LazyHeap`: a `GlobalAlloc` over ONE demand-zero `MMAP` reservation made on first use; no new call.
//!   entry    `_start(role: u64)` in RDI, no return (EXIT); the image is a static ELF linked at 0x40_0000 with no page
//!            holding both code and writable data (the loader refuses W∧X)
//! WHAT IS NOT: process spawning, the capability DAG, the shared-memory and display ABIs, the Firmament config page —
//! a driver needs none of them.

#![cfg_attr(not(test), no_std)]
#[cfg(test)]
extern crate std;

use coralos_hal::dma::{check_grant, check_request};
use coralos_hal::{Coherence, Dma, DmaRefusal, DmaRegion, Irq, IrqRefusal, IrqWait};

pub mod heap;
pub mod keywire;
pub mod mmio;
pub mod portio;

/// The syscall numbers.
pub mod nr {
    pub const WRITE: u64 = 0;
    pub const EXIT: u64 = 1;
    pub const SEND: u64 = 3;
    pub const RECV: u64 = 4;
    pub const MMIO_MAP: u64 = 8;
    pub const IRQ_WAIT: u64 = 10;
    pub const TRY_RECV: u64 = 20;
    pub const CONNECT: u64 = 21;
    pub const TICKS: u64 = 24;
    pub const MMAP: u64 = 25;
    pub const PORT_IO: u64 = 52;
    pub const WAIT_ANY_IRQ: u64 = 80;
    pub const MMIO_LEN: u64 = 90;
    pub const DMA_ALLOC_TYPED: u64 = 103;
    pub const DMA_QUOTA: u64 = 105;
    pub const IRQ_LINE: u64 = 109;
    pub const IRQ_ARM: u64 = 110;
}

/// The return words.
pub mod word {
    pub const SYSCALL_ERR: u64 = u64::MAX;
    pub const SEND_OK: u64 = 0;
    pub const SEND_RING_FULL: u64 = 1;
    pub const SEND_DENIED: u64 = u64::MAX;
    pub const PEER_DEAD: u64 = u64::MAX - 6;
    pub const WAIT_TIMEOUT: u64 = u64::MAX;
    pub const WAIT_IRQ_FIRED: u64 = u64::MAX - 1;
    pub const WAIT_IRQ_DENIED: u64 = u64::MAX - 2;
    /// DMA_ALLOC_TYPED's refusals, in kernel order: bad cache type, PAT not ready, bad out pointer, over quota, out of
    /// memory, map failed, copy-out failed, above 4 GiB. Refusal `i` (1-based) is `DMA_REFUSAL_BASE - (i - 1)`.
    pub const DMA_REFUSAL_BASE: u64 = u64::MAX - 120;
    pub const DMA_PAT_NOT_READY: u64 = DMA_REFUSAL_BASE - 1;
    pub const DMA_OVER_QUOTA: u64 = DMA_REFUSAL_BASE - 3;
    pub const DMA_OUT_OF_MEMORY: u64 = DMA_REFUSAL_BASE - 4;
}

/// DMA_ALLOC_TYPED's cache argument.
pub mod cache {
    pub const WB: u64 = 0;
    pub const WC: u64 = 1;
    pub const UC: u64 = 2;
    pub const BELOW_4G: u64 = 1 << 8;
}

/// DMA_ALLOC_TYPED's out-record.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DmaInfo {
    pub uva: u64,
    pub phys: u64,
}

#[cfg(target_arch = "x86_64")]
#[inline]
unsafe fn sc(n: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let r: u64;
    core::arch::asm!("syscall", in("rax") n, in("rdi") a0, in("rsi") a1, in("rdx") a2, in("r10") a3,
        lateout("rax") r, out("rcx") _, out("r11") _, options(nostack));
    r
}

/// Write `b` to the task's log.
pub fn write(b: &[u8]) {
    unsafe { sc(nr::WRITE, b.as_ptr() as u64, b.len() as u64, 0, 0) };
}
/// End the task.
pub fn exit(code: u64) -> ! {
    unsafe { sc(nr::EXIT, code, 0, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}
/// Send `msg` on endpoint `ep`: a SEND_* / PEER_DEAD word.
pub fn send(ep: u64, msg: &[u8]) -> u64 {
    unsafe { sc(nr::SEND, ep, msg.as_ptr() as u64, msg.len() as u64, 0) }
}
/// Receive on `ep` (blocking): the byte count.
pub fn recv(ep: u64, buf: &mut [u8]) -> u64 {
    unsafe { sc(nr::RECV, ep, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }
}
/// Receive on `ep` without parking: the byte count, 0 when empty.
pub fn try_recv(ep: u64, buf: &mut [u8]) -> u64 {
    unsafe { sc(nr::TRY_RECV, ep, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }
}
/// Open a session on a listen endpoint: `Some((req, evt))`, or None (one undifferentiated refusal).
pub fn connect(listen_ep: u16) -> Option<(u16, u16)> {
    let w = unsafe { sc(nr::CONNECT, listen_ep as u64, 0, 0, 0) };
    (w != word::SYSCALL_ERR).then_some(((w >> 16) as u16, w as u16))
}
/// Monotonic milliseconds since boot.
pub fn ticks() -> u64 {
    unsafe { sc(nr::TICKS, 0, 0, 0, 0) }
}
/// Anonymous zeroed memory (prot: 1 read, 2 write): the base, or SYSCALL_ERR.
pub fn mmap(len: u64, prot: u64) -> u64 {
    unsafe { sc(nr::MMAP, len, prot, 0, 0) }
}
/// Wait for any endpoint in the 128-bit set, or `irq`, or the bound (0 = none): a ready ep or a WAIT_* word.
pub fn wait_any_irq(eps_lo: u64, eps_hi: u64, timeout_ms: u64, irq: u32) -> u64 {
    unsafe { sc(nr::WAIT_ANY_IRQ, eps_lo, eps_hi, timeout_ms, irq as u64) }
}
/// Map the device window granted in `slot`: its VA, or 0.
pub fn mmio_map(slot: u32) -> u64 {
    unsafe { sc(nr::MMIO_MAP, slot as u64, 0, 0, 0) }
}
/// The exact length of the window granted in `slot` (0 = none).
pub fn mmio_len(slot: u32) -> u64 {
    unsafe { sc(nr::MMIO_LEN, slot as u64, 0, 0, 0) }
}
/// The task's remaining DMA budget in 4 KiB pages.
pub fn dma_quota() -> u64 {
    unsafe { sc(nr::DMA_QUOTA, 0, 0, 0, 0) }
}
/// The line held in IRQ slot `slot`, or None.
pub fn irq_line(slot: u32) -> Option<u32> {
    let w = unsafe { sc(nr::IRQ_LINE, slot as u64, 0, 0, 0) };
    (w != word::SYSCALL_ERR && w != 0).then_some(w as u32)
}

/// The cache rule: a snooping device gets write-back, a non-snooping one write-combining.
pub const fn cache_for(c: Coherence) -> u64 {
    match c {
        Coherence::Snooped => cache::WB,
        Coherence::NonSnooped => cache::WC,
    }
}
/// DMA_ALLOC_TYPED's word, as the seam names it.
pub fn dma_refusal(w: u64, len: usize) -> DmaRefusal {
    match w {
        word::DMA_OVER_QUOTA => DmaRefusal::OverQuota { len },
        word::DMA_OUT_OF_MEMORY => DmaRefusal::OutOfMemory { len },
        word::DMA_PAT_NOT_READY => DmaRefusal::CacheTypeUnavailable,
        w => DmaRefusal::Host(w),
    }
}
/// IRQ_WAIT's word, as the seam names it.
pub fn wait_outcome(w: u64, line: u32) -> Result<IrqWait, IrqRefusal> {
    match w as i64 {
        0 => Ok(IrqWait::Fired),
        1 => Ok(IrqWait::TimedOut),
        -1 => Err(IrqRefusal::NotHeld { line }),
        _ => Err(IrqRefusal::Host { line, word: w }),
    }
}

/// The CPU's time-stamp counter (`rdtsc`). A CPU fact, not a kernel one: a driver calibrates it against [`ticks`] to build
/// the microsecond delays a controller's protocol is written in.
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn tsc() -> u64 {
    let (lo, hi): (u32, u32);
    unsafe { core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags)) };
    (hi as u64) << 32 | lo as u64
}

/// The task's DMA window.
pub struct RtDma;
impl Dma for RtDma {
    fn alloc_coherent(&mut self, len: usize, align: usize, c: Coherence) -> Result<DmaRegion, DmaRefusal> {
        check_request(len, align)?;
        let mut info = DmaInfo::default();
        let w = unsafe { sc(nr::DMA_ALLOC_TYPED, len as u64, &mut info as *mut DmaInfo as u64, cache_for(c), 0) };
        if w != 0 {
            return Err(dma_refusal(w, len));
        }
        let info = unsafe { core::ptr::read_volatile(&info) };
        check_grant(info.phys, align)?;
        Ok(DmaRegion { virt: info.uva, phys: info.phys, len })
    }
}

/// One interrupt line: a platform (level) line re-armed by IRQ_ARM, or an MSI vector (edge, nothing to re-arm).
pub struct RtIrq {
    pub line: u32,
    pub level: bool,
}
impl Irq for RtIrq {
    fn wait(&mut self, timeout_ms: u64) -> Result<IrqWait, IrqRefusal> {
        wait_outcome(unsafe { sc(nr::IRQ_WAIT, self.line as u64, timeout_ms, 0, 0) }, self.line)
    }
    fn ack(&mut self) -> Result<(), IrqRefusal> {
        if !self.level || unsafe { sc(nr::IRQ_ARM, self.line as u64, 0, 0, 0) } == 0 {
            Ok(())
        } else {
            Err(IrqRefusal::ArmRefused { line: self.line })
        }
    }
}

#[cfg(test)]
mod mmio_tests;
#[cfg(test)]
mod portio_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_out_record_is_two_u64s() {
        assert_eq!(core::mem::size_of::<DmaInfo>(), 16);
        assert_eq!(core::mem::offset_of!(DmaInfo, phys), 8);
    }

    #[test]
    fn the_words_and_their_names() {
        assert_eq!(word::DMA_REFUSAL_BASE, u64::MAX - 120);
        assert_eq!(dma_refusal(u64::MAX - 123, 4096), DmaRefusal::OverQuota { len: 4096 });
        assert_eq!(dma_refusal(u64::MAX - 124, 4096), DmaRefusal::OutOfMemory { len: 4096 });
        assert_eq!(dma_refusal(u64::MAX - 121, 4096), DmaRefusal::CacheTypeUnavailable);
        assert_eq!(dma_refusal(u64::MAX - 127, 1), DmaRefusal::Host(u64::MAX - 127));
        assert_eq!(cache_for(Coherence::Snooped), 0);
        assert_eq!(cache_for(Coherence::NonSnooped), 1);
        assert_eq!(wait_outcome(0, 9), Ok(IrqWait::Fired));
        assert_eq!(wait_outcome(1, 9), Ok(IrqWait::TimedOut));
        assert_eq!(wait_outcome(u64::MAX, 9), Err(IrqRefusal::NotHeld { line: 9 }));
        assert_eq!(wait_outcome(2, 9), Err(IrqRefusal::Host { line: 9, word: 2 }));
    }
}
