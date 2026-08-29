// Copied verbatim from CoralOS userland/coral-hal/src/lib.rs (2026-08-29).
//! coral-hal — the shared hardware-access seam: the [`Mmio`] register trait + the
//! cooperative-wait hooks every EL0 driver busy-wait must use.
//!
//! One trait, many backings: an EL0 service satisfies it with volatile reads/writes over an
//! `mmio_map`'d BAR; a host test satisfies it with a scripted register fake. Moved here from
//! coral-emmc (which re-exports it, so its consumers are unchanged) — see this crate's
//! Cargo.toml header for the extraction rationale and the wait-hook doctrine.

#![cfg_attr(not(test), no_std)]

/// Byte/16/32-bit MMIO access to a device register file. `&mut self` because some reads are
/// FIFO-consuming (e.g. SDHCI_BUFFER advances the data FIFO) and the host fake needs to mutate.
pub trait Mmio {
    fn r8(&mut self, reg: u32) -> u8;
    fn r16(&mut self, reg: u32) -> u16;
    fn r32(&mut self, reg: u32) -> u32;
    fn w8(&mut self, reg: u32, v: u8);
    fn w16(&mut self, reg: u32, v: u16);
    fn w32(&mut self, reg: u32, v: u32);

    /// Cooperative yield hook, called once per spin of a status-polling busy-wait.
    /// A driver's PIO path waits for hardware by polling registers in tight loops; on a
    /// system where the driver shares a CPU with a latency-sensitive sibling (e.g. the x86
    /// `emmc_srv` continent co-resident with the network lifeline on a single bare-metal ring-3
    /// core), a non-yielding spin monopolizes the core and starves that sibling. An impl that runs
    /// under a cooperative scheduler should override this to relinquish the CPU (e.g. `yield_now`),
    /// ideally rate-limited so a genuine timeout still terminates promptly. The DEFAULT is a no-op,
    /// so bare-register impls (host tests, probes, the aarch64 driver) are unchanged and pay nothing.
    /// (Doctrine: a wait-for-readiness loop in task context must yield, not spin — `feedback_dag_yield_rule`.)
    #[inline]
    fn relax(&mut self) {}

    /// (Re)arm a wall-clock deadline, `_ms` milliseconds from now, for a readiness busy-wait.
    /// A cooperatively-scheduled impl backs this with a monotonic clock so hardware that keeps
    /// answering polls without ever reaching the ready state (e.g. an eMMC in an extended NAND
    /// program) gives up in bounded WALL-CLOCK time instead of the ~2e12 nested polls a pure
    /// iteration bound permits. The DEFAULT is inert (`deadline_expired` never fires), so pure
    /// iteration-bounded impls — host tests, probes — are byte-for-byte unchanged and pay nothing.
    #[inline]
    fn arm_deadline(&mut self, _ms: u64) {}
    /// True once the deadline armed by [`Mmio::arm_deadline`] has elapsed. DEFAULT: never.
    #[inline]
    fn deadline_expired(&mut self) -> bool {
        false
    }

    /// Release the CPU for approximately `_ms` ms inside a wall-clock busy-wait (hardware can
    /// hold a busy line for tens-to-hundreds of ms). A cooperatively-scheduled impl backs this
    /// with a TIMED PARK — the task sleeps and the core runs its siblings (the network lifeline) —
    /// NOT a `yield_now` spin: yielding every N polls cost a full scheduler round-trip (1-5 ms
    /// each) and dominated eMMC write latency (Sol/M3/Qwen 2026-08-28). DEFAULT: a no-op, so
    /// clockless impls (host tests / probes) just re-poll the bounded loop and are unchanged.
    #[inline]
    fn park_ms(&mut self, _ms: u64) {}
}
