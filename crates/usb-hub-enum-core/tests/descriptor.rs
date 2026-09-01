// SPDX-License-Identifier: GPL-2.0-only
//! Hub descriptor vectors. Expected values are Linux literals with FILE and LINE.
//! Ported from hub.c and ch11.h; original copyright holders are Linus Torvalds, Johannes Erdfelt,
//! Gregory P. Smith, Brad Hards, and the Linux USB core authors.

use usb_hub_enum_core::descriptor::*;

/// ch11.h:246-:247 and hub.c:431-:433.
#[test]
fn descriptor_lengths_are_linux_literals() {
    assert_eq!(HUB_NONVAR_SIZE, 7);
    assert_eq!(SS_HUB_SIZE, 12);
    assert_eq!(usb2_descriptor_required_len(1), 8);
    assert_eq!(usb2_descriptor_required_len(7), 8);
    assert_eq!(usb2_descriptor_required_len(8), 9);
    assert_eq!(usb2_descriptor_required_len(15), 9);
    assert_eq!(usb2_descriptor_required_len(31), 11);
    assert_eq!(validate_descriptor_length(false, 8, 9), Ok(()));
    assert_eq!(
        validate_descriptor_length(false, 8, 8),
        Err(DescriptorError::MissingDeviceRemovable { received: 8, required: 9 })
    );
    assert_eq!(validate_descriptor_length(true, 15, 12), Ok(()));
    assert_eq!(
        validate_descriptor_length(true, 15, 13),
        Err(DescriptorError::InvalidSuperSpeedLength { received: 13, required: 12 })
    );
    assert_eq!(
        validate_descriptor_length(true, 15, 11),
        Err(DescriptorError::InvalidSuperSpeedLength { received: 11, required: 12 })
    );
}

/// ch11.h:253-:260; hub.c:1645-:1646 says bPwrOn2PwrGood * 2 milliseconds.
#[test]
fn fixed_fields_decode_little_endian_and_power_time_units() {
    let d = HubDescriptor::decode(&[9, 0x29, 4, 0x89, 0x00, 50, 7]).unwrap();
    assert_eq!(d.ports, 4);
    assert_eq!(d.characteristics, 0x0089);
    assert_eq!(d.power_on_to_good_ms, 100);
    assert_eq!(d.controller_current_ma, 7);
    assert_eq!(
        HubDescriptor::decode(&[7, 0x29]),
        Err(DescriptorError::MissingFixedFields { received: 2, required: 7 })
    );
}

/// hub.c:1497-:1504 names both port-count refusals.
#[test]
fn invalid_port_counts_are_named() {
    let zero = HubDescriptor::decode(&[7, 0x29, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(zero.validate_ports(31), Err(DescriptorError::HubHasNoPorts));
    let too_many = HubDescriptor::decode(&[7, 0x29, 32, 0, 0, 0, 0]).unwrap();
    assert_eq!(
        too_many.validate_ports(31),
        Err(DescriptorError::HubHasTooManyPorts { ports: 32, maximum: 31 })
    );
    let valid = HubDescriptor::decode(&[7, 0x29, 31, 0, 0, 0, 0]).unwrap();
    assert_eq!(valid.validate_ports(31), Ok(valid));
}

/// ch11.h:207-:220 and hub.c:1554-:1578, :1640-:1643.
#[test]
fn characteristic_masks_and_all_mode_names_are_pinned() {
    assert_eq!(CHAR_LPSM, 0x0003);
    assert_eq!(CHAR_COMPOUND, 0x0004);
    assert_eq!(CHAR_OCPM, 0x0018);
    assert_eq!(CHAR_TTTT, 0x0060);
    assert_eq!(CHAR_PORT_INDICATORS, 0x0080);

    let descriptor = |characteristics| HubDescriptor {
        ports: 1,
        characteristics,
        power_on_to_good_ms: 0,
        controller_current_ma: 0,
    };
    let power = [
        descriptor(0x0000).power_switching(),
        descriptor(0x0001).power_switching(),
        descriptor(0x0002).power_switching(),
        descriptor(0x0003).power_switching(),
    ];
    assert_eq!(power.len(), 4);
    assert_eq!(power, [PowerSwitching::Ganged, PowerSwitching::Individual,
                       PowerSwitching::None, PowerSwitching::None]);
    let over_current = [
        descriptor(0x0000).over_current(),
        descriptor(0x0008).over_current(),
        descriptor(0x0010).over_current(),
        descriptor(0x0018).over_current(),
    ];
    assert_eq!(over_current.len(), 4);
    assert_eq!(over_current, [OverCurrentProtection::Global,
                              OverCurrentProtection::Individual,
                              OverCurrentProtection::None,
                              OverCurrentProtection::None]);
    assert!(descriptor(0x0004).is_compound());
    assert!(!descriptor(0).is_compound());
    assert!(descriptor(0x0080).has_port_indicators());
    assert!(!descriptor(0).has_port_indicators());
}

/// hub.c:1610-:1637 and ch11.h:300-:303 define exactly four TT values.
#[test]
fn all_four_tt_think_times_are_literal_vectors() {
    let got = [0x00, 0x20, 0x40, 0x60].map(|characteristics| HubDescriptor {
        ports: 1,
        characteristics,
        power_on_to_good_ms: 0,
        controller_current_ma: 0,
    }.tt_think_time_ns());
    assert_eq!(got.len(), 4);
    assert_eq!(got, [666, 1_332, 1_998, 2_664]);
}

/// hub.c:1545-:1548 uses bit port%8 in byte port/8; one means fixed, zero removable.
#[test]
fn removable_bitmap_uses_the_one_based_port_bit() {
    assert_eq!(port_is_removable(&[0b0000_0010], 1), Ok(false));
    assert_eq!(port_is_removable(&[0b0000_0000], 1), Ok(true));
    assert_eq!(port_is_removable(&[0, 0b0000_0001], 8), Ok(false));
    assert_eq!(
        port_is_removable(&[0], 8),
        Err(DescriptorError::MissingDeviceRemovable { received: 1, required: 2 })
    );
}
