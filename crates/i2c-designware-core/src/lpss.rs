// SPDX-License-Identifier: GPL-2.0-only
//! The Intel LPSS variant — where the platform, not the driver, knows the answers.
//!
//! Ported from Linux `drivers/i2c/busses/`:
//!   * `i2c_dw_acpi_params` / `i2c_dw_acpi_configure` (i2c-designware-common.c:307-:340)
//!   * `i2c_dw_no_acpi_params`, the DMI blocklist (i2c-designware-common.c:288-:301)
//!   * `i2c_dw_baytrail_probe_lock_support` (i2c-designware-baytrail.c, whole file)
//!
//! Copyright (c) Intel Corporation and the Linux i2c-designware authors.
//!
//! Two things on this silicon are not the controller's business and cannot be computed from it:
//! what the SCL counts should be, and whether anything else is driving the bus.

/// The ACPI methods that supply per-mode SCL counts, in Linux's own order
/// (i2c-designware-common.c:314-:317).
///
/// Each returns a PACKAGE of EXACTLY THREE integers — `[hcnt, lcnt, sda_hold]`. Linux checks both
/// the type and the count and silently ignores anything else (:321), because a malformed method is
/// a firmware bug it cannot fix and a half-read package would be worse than none.
pub const ACPI_COUNT_METHODS: [&str; 4] = ["SSCN", "FMCN", "FPCN", "HSCN"];

/// The package Linux expects back. `hcnt`/`lcnt` are truncated to u16 (:325-:326) because that is
/// the width of the registers they are written to; `sda_hold` is u32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcpiCounts {
    pub hcnt: u16,
    pub lcnt: u16,
    pub sda_hold: u32,
}

/// Parse one `SSCN`/`FMCN`/`FPCN`/`HSCN` package.
///
/// Returns `None` for anything that is not exactly three integers — Linux's own check at :321,
/// `obj->type == ACPI_TYPE_PACKAGE && obj->package.count == 3`. Accepting a shorter package and
/// defaulting the rest would put a firmware typo straight into a timing register.
pub fn parse_acpi_counts(package: &[u64]) -> Option<AcpiCounts> {
    if package.len() != 3 {
        return None;
    }
    Some(AcpiCounts {
        hcnt: package[0] as u16,
        lcnt: package[1] as u16,
        sda_hold: package[2] as u32,
    })
}

/// Whether the driver must compute the SCL counts itself.
///
/// THIS IS THE FIRST-CLASS PATH, NOT A FALLBACK, AND THE DIRECTION MATTERS. Linux computes counts
/// only `if (!dev->ss_hcnt || !dev->ss_lcnt)` (i2c-designware-master.c:59) — that is, ONLY when
/// ACPI supplied nothing. Its comment at common.c:288 says why: "The HCNT/LCNT information coming
/// from ACPI should be the most accurate for given platform."
///
/// It is also not academic on this silicon. At a 100 MHz input clock, standard mode needs about
/// 430000 ticks — far past the 16-bit register — so on such a platform the computed path cannot
/// produce a programmable value at all and the ACPI numbers are the only ones that work.
pub fn must_compute(supplied: Option<AcpiCounts>) -> bool {
    match supplied {
        None => true,
        // Linux's predicate is per-field and treats ZERO as absent, not as a valid count of zero.
        Some(c) => c.hcnt == 0 || c.lcnt == 0,
    }
}

/// One entry of the DMI blocklist (i2c-designware-common.c:292-:300): a machine whose ACPI counts
/// are known wrong, where computing from the input clock gives better results.
///
/// Linux keeps exactly ONE entry. It is carried rather than dropped because the list is the record
/// of a real failure — and because an empty list in a port would read as "no such machines exist"
/// rather than "one is known".
pub const ACPI_PARAMS_BLOCKLIST: [(&str, &str); 1] = [("Dell Inc.", "Inspiron 7348")];

/// Whether this machine's ACPI counts should be ignored.
pub fn acpi_params_blocked(sys_vendor: &str, product_name: &str) -> bool {
    ACPI_PARAMS_BLOCKLIST
        .iter()
        .any(|(v, p)| *v == sys_vendor && *p == product_name)
}

/// What `_SEM` says about who owns the bus.
///
/// From `i2c_dw_baytrail_probe_lock_support` (i2c-designware-baytrail.c). On Baytrail and Cherry
/// Trail the PMIC I2C bus is SHARED WITH THE PUNIT — the power-management unit drives it too. A
/// port that ignores this lets firmware and OS issue transactions concurrently on the same bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PunitLock {
    /// No `_SEM`, or `_SEM` returned zero: the bus is ours alone. Linux returns -ENODEV, which here
    /// means "no lock needed" rather than "something failed" — the same value carries both meanings
    /// in the C and only the call site distinguishes them.
    NotShared,
    /// Shared, and the IOSF mailbox is available: every transfer must be bracketed by
    /// block/unblock of PUNIT access.
    Required,
    /// Shared, but the IOSF mailbox is not up yet. Linux returns -EPROBE_DEFER: the correct answer
    /// is to RETRY LATER, not to proceed unlocked. Proceeding is the dangerous option, because the
    /// bus works right up until the PUNIT touches it.
    DeferUntilMailbox,
}

/// Decide the locking regime from `_SEM` and mailbox availability.
pub fn punit_lock(sem: Option<u64>, iosf_mbi_available: bool) -> PunitLock {
    match sem {
        None | Some(0) => PunitLock::NotShared,
        Some(_) if iosf_mbi_available => PunitLock::Required,
        Some(_) => PunitLock::DeferUntilMailbox,
    }
}
