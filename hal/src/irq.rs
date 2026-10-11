// Copied verbatim from CoralOS userland/coral-hal/src/irq.rs (2026-10-11).
//! The [`Irq`] seam — one interrupt line a driver sleeps on instead of polling.
//!
//! Two calls, matching the host's: WAIT (block until the line fires, or a bound passes, so a masked or unrouted line
//! degrades to the driver's polled path instead of hanging it) and ACK (re-arm the line once the device's cause has
//! been serviced). The ack is not optional ceremony: a LEVEL line is masked by the host on delivery — otherwise it
//! would re-fire forever while the device still asserts it — so a driver that does not ack never hears that line
//! again. An EDGE line (MSI/MSI-X, iwlwifi on PCIe) needs no re-arm; its ack succeeds and does nothing.

/// What a wait ended with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqWait {
    /// The line fired (one pending signal consumed).
    Fired,
    /// The bound passed with no interrupt.
    TimedOut,
}

/// Why the host refused. Each names the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrqRefusal {
    /// The task holds no capability for this line.
    NotHeld { line: u32 },
    /// The host refused to re-arm the line.
    ArmRefused { line: u32 },
    /// Any other word the host returned, as itself.
    Host { line: u32, word: u64 },
}

/// One interrupt line.
pub trait Irq {
    /// Block until the line fires, or `timeout_ms` passes (`0` = no bound).
    fn wait(&mut self, timeout_ms: u64) -> Result<IrqWait, IrqRefusal>;
    /// Re-arm the line after the device's cause is serviced (see the module doc: mandatory for a level line).
    fn ack(&mut self) -> Result<(), IrqRefusal>;
}
