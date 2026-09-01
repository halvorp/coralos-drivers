// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for descriptor validation.
//!
//! Ported from Linux `drivers/gpio/gpiolib.c`.
//!
//! Copyright (C) 2013 Intel Corporation and the Linux GPIO subsystem authors.

use gpiolib_desc_core::validation::*;

/// gpiolib.c:377-:388: NULL=optional absent, ERR_PTR=its errno, ordinary pointer=valid.
#[test]
fn descriptor_validation_preserves_all_three_linux_states() {
    assert_eq!(
        validate_descriptor(DescriptorRef::OptionalAbsent),
        Validation::OptionalAbsent
    );
    assert_eq!(
        validate_descriptor(DescriptorRef::Error(-22)),
        Validation::Error(-22)
    );
    assert_eq!(
        validate_descriptor(DescriptorRef::Error(-19)),
        Validation::Error(-19)
    );
    assert_eq!(
        validate_descriptor(DescriptorRef::Valid { id: 7 }),
        Validation::Valid
    );
}

/// gpiolib.c:410-:414. Equality requires two valid references to the same physical descriptor.
#[test]
fn descriptor_equality_rejects_absent_and_error_references() {
    assert!(descriptors_equal(
        DescriptorRef::Valid { id: 7 },
        DescriptorRef::Valid { id: 7 }
    ));
    assert!(!descriptors_equal(
        DescriptorRef::Valid { id: 7 },
        DescriptorRef::Valid { id: 8 }
    ));
    assert!(!descriptors_equal(
        DescriptorRef::OptionalAbsent,
        DescriptorRef::OptionalAbsent
    ));
    assert!(!descriptors_equal(
        DescriptorRef::Error(-22),
        DescriptorRef::Error(-22)
    ));
    assert!(!descriptors_equal(
        DescriptorRef::Valid { id: 7 },
        DescriptorRef::Error(-22)
    ));
}

/// gpiolib.c:207-:211 uses the strict literal condition `hwnum >= ngpio`.
#[test]
fn hardware_offset_must_be_strictly_below_line_count() {
    assert_eq!(validate_hardware_offset(0, 1), Ok(()));
    assert_eq!(validate_hardware_offset(31, 32), Ok(()));

    let equal = validate_hardware_offset(32, 32).unwrap_err();
    assert_eq!(
        equal,
        DescriptorError::HardwareOffsetOutOfRange {
            hwnum: 32,
            ngpio: 32
        }
    );
    assert!(equal.to_string().contains("offset 32 refused"));
    assert!(equal.to_string().contains("below 32"));

    let empty = validate_hardware_offset(0, 0).unwrap_err();
    assert_eq!(
        empty,
        DescriptorError::HardwareOffsetOutOfRange { hwnum: 0, ngpio: 0 }
    );
}

/// gpiolib.c:3041-:3048 refuses output only when BOTH IRQ predicates are true.
#[test]
fn only_an_enabled_irq_line_refuses_output_direction() {
    assert_eq!(validate_output_request(false, false), Ok(()));
    assert_eq!(validate_output_request(true, false), Ok(()));
    assert_eq!(validate_output_request(false, true), Ok(()));

    let error = validate_output_request(true, true).unwrap_err();
    assert_eq!(error, DescriptorError::EnabledIrqCannotBecomeOutput);
    assert_eq!(
        error.to_string(),
        "GPIO output request refused: the line is tied to an enabled IRQ"
    );
}
