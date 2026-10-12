// SPDX-License-Identifier: GPL-2.0-only
//! The one error type of the i915 port family. Every refusal is a variant that says WHAT refused and WHY
//! (PORT_CONVENTIONS.md §5); [`I915Error::name`] is the stable token the device prints in its marker lines.
use crate::reg::Reg;

/// Why an i915 operation refused or failed. Plain data, `Copy`, no allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I915Error {
    /// The PCI device id is not in any row the port carries (i915_pci.c:`pciidlist`, i915_pci.c:829-840). Never aliased.
    UnknownDevice { devid: u16 },
    /// The id IS a row Linux binds, but of a generation this build refuses to drive (the negative-control rows).
    UnsupportedGeneration { devid: u16, gen: u8 },
    /// A register write outside the device's write allow-list was refused and NOT forwarded.
    WriteRefused { offset: u32, width: u8 },
    /// A register wait ran out of its bound. `domain` is the forcewake domain bit that was waited on; `want`/`got` are the
    /// masked ack values; `polls` is the reads made.
    AckTimeout { domain: u32, reg: Reg, mask: u32, want: u32, got: u32, polls: u32 },
    /// The register read back all-ones: the BAR is not answering (intel_uncore.c:194, "MMIO unreliable").
    MmioUnreliable { domain: u32, reg: Reg },
    /// A forcewake domain this platform does not have was requested.
    NoSuchDomain { domain_bit: u32 },
    /// A put without a matching get (Linux: `GEM_BUG_ON(!domain->wake_count)`, intel_uncore.c:775).
    Unbalanced { domain_bit: u32 },
    /// A register read that needs forcewake domains the caller does not hold.
    ForcewakeNotHeld { offset: u32, domains: u32 },
    /// A decoded value is outside what the hardware generation can encode.
    BadValue { what: &'static str, value: u32 },
}

impl I915Error {
    /// The stable name used in markers: `[I915-FORCEWAKE FAIL why=ACK-TIMEOUT ...]`.
    pub const fn name(&self) -> &'static str {
        match self {
            I915Error::UnknownDevice { .. } => "UNKNOWN-DEVICE",
            I915Error::UnsupportedGeneration { .. } => "UNSUPPORTED-GENERATION",
            I915Error::WriteRefused { .. } => "WRITE-NOT-ALLOWED",
            I915Error::AckTimeout { .. } => "ACK-TIMEOUT",
            I915Error::MmioUnreliable { .. } => "MMIO-UNRELIABLE",
            I915Error::NoSuchDomain { .. } => "NO-SUCH-DOMAIN",
            I915Error::Unbalanced { .. } => "UNBALANCED-PUT",
            I915Error::ForcewakeNotHeld { .. } => "FORCEWAKE-NOT-HELD",
            I915Error::BadValue { .. } => "BAD-VALUE",
        }
    }
}
