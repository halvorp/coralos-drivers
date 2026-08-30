// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the LPSS specifics. Expected values are LINUX literals with file and line.

use i2c_designware_core::lpss::{
    acpi_params_blocked, must_compute, parse_acpi_counts, punit_lock, AcpiCounts, PunitLock,
    ACPI_COUNT_METHODS, ACPI_PARAMS_BLOCKLIST,
};

/// i2c-designware-common.c:314-:317 — four methods, in this order, one per speed mode.
#[test]
fn the_acpi_method_names_match_linux() {
    assert_eq!(ACPI_COUNT_METHODS, ["SSCN", "FMCN", "FPCN", "HSCN"]);
}

/// :321 — `obj->type == ACPI_TYPE_PACKAGE && obj->package.count == 3`. Anything else is ignored.
/// Accepting a short package and defaulting the rest puts a firmware typo into a timing register.
#[test]
fn only_a_three_element_package_is_accepted() {
    assert_eq!(
        parse_acpi_counts(&[0x40, 0x80, 0x10]),
        Some(AcpiCounts { hcnt: 0x40, lcnt: 0x80, sda_hold: 0x10 })
    );
    assert_eq!(parse_acpi_counts(&[]), None);
    assert_eq!(parse_acpi_counts(&[0x40, 0x80]), None, "two is not three");
    assert_eq!(parse_acpi_counts(&[0x40, 0x80, 0x10, 0x20]), None, "four is not three either");
}

/// :325-:326 — hcnt and lcnt are cast to u16 because that is the width of the registers they are
/// written to. A firmware value wider than the register must truncate the same way Linux's cast
/// does, not silently select a different behaviour.
#[test]
fn the_counts_truncate_to_the_register_width() {
    let c = parse_acpi_counts(&[0x1_0001, 0x1_0002, 0xdead_beef]).unwrap();
    assert_eq!(c.hcnt, 1, "u16 truncation, as the C cast does");
    assert_eq!(c.lcnt, 2);
    assert_eq!(c.sda_hold, 0xdead_beef, "sda_hold is u32, not truncated");
}

/// THE DIRECTION IS THE POINT. i2c-designware-master.c:59 computes counts only
/// `if (!dev->ss_hcnt || !dev->ss_lcnt)` — ACPI WINS, and computing is the fallback. Linux's
/// comment (common.c:288): "The HCNT/LCNT information coming from ACPI should be the most accurate
/// for given platform."
#[test]
fn acpi_counts_win_and_computing_is_the_fallback() {
    assert!(must_compute(None), "nothing supplied: compute");
    assert!(!must_compute(Some(AcpiCounts { hcnt: 0x40, lcnt: 0x80, sda_hold: 0 })),
            "ACPI supplied both counts: do NOT overwrite them with computed ones");
    // Linux's predicate is PER-FIELD and treats zero as absent, not as a valid count of zero.
    assert!(must_compute(Some(AcpiCounts { hcnt: 0, lcnt: 0x80, sda_hold: 0 })));
    assert!(must_compute(Some(AcpiCounts { hcnt: 0x40, lcnt: 0, sda_hold: 0 })));
}

/// common.c:292-:300 — exactly one machine is known to report wrong counts. The list is carried
/// rather than dropped: an empty list in a port reads as "no such machines exist".
#[test]
fn the_dmi_blocklist_is_carried_not_dropped() {
    assert_eq!(ACPI_PARAMS_BLOCKLIST.len(), 1);
    assert!(acpi_params_blocked("Dell Inc.", "Inspiron 7348"));
    assert!(!acpi_params_blocked("Dell Inc.", "Inspiron 7349"), "the match is exact");
    assert!(!acpi_params_blocked("Intel", "Inspiron 7348"), "both fields must match");
}

/// i2c-designware-baytrail.c — on Baytrail and Cherry Trail the PMIC I2C bus is SHARED WITH THE
/// PUNIT. Absent or zero `_SEM` means the bus is ours; a non-zero `_SEM` means every transfer must
/// be bracketed by block/unblock of PUNIT access.
#[test]
fn the_sem_method_decides_whether_the_punit_shares_the_bus() {
    assert_eq!(punit_lock(None, true), PunitLock::NotShared, "no _SEM: ours alone");
    assert_eq!(punit_lock(Some(0), true), PunitLock::NotShared, "_SEM == 0: ours alone");
    assert_eq!(punit_lock(Some(1), true), PunitLock::Required);
}

/// The mailbox case is the dangerous one. Linux returns -EPROBE_DEFER, i.e. RETRY LATER — it does
/// NOT proceed unlocked. Proceeding is what looks like it works: the bus behaves perfectly until
/// the PUNIT touches it.
#[test]
fn a_shared_bus_without_the_mailbox_defers_rather_than_proceeding_unlocked() {
    assert_eq!(punit_lock(Some(1), false), PunitLock::DeferUntilMailbox);
    assert_ne!(punit_lock(Some(1), false), PunitLock::NotShared,
               "an unavailable mailbox must never be read as 'no lock needed'");
    // And a bus that is not shared does not care about the mailbox at all.
    assert_eq!(punit_lock(Some(0), false), PunitLock::NotShared);
}
