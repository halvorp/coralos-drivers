// SPDX-License-Identifier: GPL-2.0-only
//! Frozen capability-walk vectors from Linux PCI core rules.
//!
//! Ported from `drivers/pci/pci.c`, `drivers/pci/pci.h`, and
//! `include/uapi/linux/pci_regs.h`. Copyright Drew Eckhardt, Martin Mares, and Linux PCI authors.

use pci_config_core::capability::{
    find_capability, find_next_capability, Capability, CapabilityError, CAP_LIST_ID, CAP_LIST_NEXT,
    FIND_CAP_TTL,
};
use pci_config_core::regs::status;

fn endpoint() -> [u8; 256] {
    let mut c = [0u8; 256];
    c[0x06..0x08].copy_from_slice(&status::CAP_LIST.to_le_bytes());
    c[0x0e] = 0;
    c
}

#[test]
fn constants_and_capability_member_names_are_pinned() {
    assert_eq!(CAP_LIST_ID, 0, "pci_regs.h:221");
    assert_eq!(CAP_LIST_NEXT, 1, "pci_regs.h:244");
    assert_eq!(FIND_CAP_TTL, 48, "pci.h:18");
    // The standard capability header has exactly these two fields before capability-specific data.
    let names = [
        ("CAP_LIST_ID", CAP_LIST_ID),
        ("CAP_LIST_NEXT", CAP_LIST_NEXT),
    ];
    assert_eq!(names.len(), 2);
    assert_eq!(names, [("CAP_LIST_ID", 0), ("CAP_LIST_NEXT", 1)]);
}

#[test]
fn ordinary_walk_aligns_pointers_and_finds_the_requested_id() {
    let mut c = endpoint();
    c[0x34] = 0x43; // pci.h:143 then ALIGN_DOWN(..., 4) at :149 => 0x40
    c[0x40] = 0x01;
    c[0x41] = 0x4c;
    c[0x4c] = 0x05;
    c[0x4d] = 0;
    assert_eq!(
        find_capability(&c, 0x05),
        Ok(Some(Capability {
            id: 0x05,
            offset: 0x4c
        }))
    );
    assert_eq!(
        find_capability(&c, 0x11),
        Ok(None),
        "a zero next pointer terminates the list"
    );
}

#[test]
fn find_next_begins_at_the_current_capability_next_byte() {
    let mut c = endpoint();
    c[0x34] = 0x40;
    c[0x40] = 0x09;
    c[0x41] = 0x48;
    c[0x48] = 0x09;
    c[0x49] = 0;
    assert_eq!(
        find_next_capability(&c, 0x40, 0x09),
        Ok(Some(Capability {
            id: 0x09,
            offset: 0x48
        }))
    );
}

#[test]
fn absent_capability_list_status_terminates_without_following_garbage() {
    let mut c = endpoint();
    c[0x06] = 0;
    c[0x34] = 0x40;
    c[0x40] = 0x05;
    assert_eq!(find_capability(&c, 0x05), Ok(None), "pci.c:441-443");
}

#[test]
fn malformed_list_refusals_are_named() {
    let mut c = endpoint();
    c[0x34] = 0x20;
    assert_eq!(
        find_capability(&c, 0x05),
        Err(CapabilityError::PointerBelowStandardHeader {
            pointer: 0x20,
            minimum: 64
        })
    );

    c[0x34] = 0xfc;
    c[0xfc] = 0xff;
    assert_eq!(
        find_capability(&c, 0x05),
        Err(CapabilityError::InvalidCapabilityId {
            pointer: 0xfc,
            id: 0xff
        })
    );

    let short = [0u8; 8];
    assert_eq!(
        find_capability(&short, 1),
        Err(CapabilityError::ConfigTooShort {
            length: 8,
            required: 16
        })
    );

    let mut strange = endpoint();
    strange[0x0e] = 3;
    assert_eq!(
        find_capability(&strange, 1),
        Err(CapabilityError::UnsupportedHeaderType { header_type: 3 })
    );
}

#[test]
fn self_referential_list_hits_a_named_guard_not_a_hang() {
    let mut c = endpoint();
    c[0x34] = 0x40;
    c[0x40] = 0x01;
    c[0x41] = 0x40; // points to itself
    assert_eq!(
        find_capability(&c, 0x05),
        Err(CapabilityError::LoopGuardExhausted {
            limit: 48,
            pointer: 0x40
        }),
        "pci.h:131,137,145: malformed lists get a 48-step TTL"
    );
}
