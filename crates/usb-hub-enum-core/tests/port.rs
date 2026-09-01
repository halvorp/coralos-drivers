// SPDX-License-Identifier: GPL-2.0-only
//! Port-word vectors from hub.c/ch11.h. Original copyright: Linus Torvalds, Johannes Erdfelt,
//! Gregory P. Smith, Brad Hards, and the Linux USB core authors.

use usb_hub_enum_core::port::{change, speed_from_status, status, warm_reset_required, PortStatus,
                              SpeedError, UsbSpeed, CHANGE_BITS};

/// ch11.h:123-:134 — every named USB2 status literal is pinned.
#[test]
fn all_eleven_usb2_status_names_and_values_are_pinned() {
    let got = [
        ("connection", status::CONNECTION),
        ("enable", status::ENABLE),
        ("suspend", status::SUSPEND),
        ("over-current", status::OVERCURRENT),
        ("reset", status::RESET),
        ("l1", status::L1),
        ("power", status::POWER),
        ("low-speed", status::LOW_SPEED),
        ("high-speed", status::HIGH_SPEED),
        ("test", status::TEST),
        ("indicator", status::INDICATOR),
    ];
    assert_eq!(got.len(), 11);
    assert_eq!(got, [
        ("connection", 0x0001), ("enable", 0x0002), ("suspend", 0x0004),
        ("over-current", 0x0008), ("reset", 0x0010), ("l1", 0x0020), ("power", 0x0100),
        ("low-speed", 0x0200), ("high-speed", 0x0400), ("test", 0x0800),
        ("indicator", 0x1000),
    ]);
}

/// ch11.h:174-:186 defines nine semantic names (USB2 C_L1 and USB3 C_BH_RESET alias 0x20).
#[test]
fn every_change_semantic_has_a_frozen_name_and_literal() {
    assert_eq!(CHANGE_BITS.len(), 9);
    let got: Vec<(&str, u16)> = CHANGE_BITS.iter().map(|b| (b.name, b.mask)).collect();
    assert_eq!(got, vec![
        ("connection", 0x0001), ("enable", 0x0002), ("suspend", 0x0004),
        ("over-current", 0x0008), ("reset", 0x0010), ("l1", 0x0020),
        ("warm-reset", 0x0020),
        ("link-state", 0x0040), ("config-error", 0x0080),
    ]);
    assert_eq!(change::BH_RESET, change::L1, "protocol-specific aliases share bit five");
}

/// hub.c:3237-:3245 — USB2 power is 0x100 while SuperSpeed power is 0x200.
#[test]
fn status_predicates_keep_usb2_and_usb3_power_distinct() {
    let s = PortStatus { raw: 0x0013 };
    assert!(s.connected());
    assert!(s.enabled());
    assert!(s.resetting());
    assert!(PortStatus { raw: 0x0100 }.powered(false));
    assert!(!PortStatus { raw: 0x0100 }.powered(true));
    assert!(PortStatus { raw: 0x0200 }.powered(true));
    assert_eq!(status::SS_POWER, 0x0200); // ch11.h:142
}

/// hub.c:3035-:3045 — SSP, SS, high, low, then full precedence.
#[test]
fn speed_selection_matches_linux_precedence() {
    let ready = 0x0001 | 0x0002;
    assert_eq!(speed_from_status(ready, false, false), Ok(UsbSpeed::Full));
    assert_eq!(speed_from_status(ready | 0x0200, false, false), Ok(UsbSpeed::Low));
    assert_eq!(speed_from_status(ready | 0x0400, false, false), Ok(UsbSpeed::High));
    assert_eq!(speed_from_status(ready | 0x0600, false, false), Ok(UsbSpeed::High));
    assert_eq!(speed_from_status(ready, true, false), Ok(UsbSpeed::Super));
    assert_eq!(speed_from_status(ready, true, true), Ok(UsbSpeed::SuperPlus));
    assert_eq!(speed_from_status(0x0002, false, false), Err(SpeedError::PortDisconnected));
    assert_eq!(speed_from_status(0x0001, false, false), Err(SpeedError::PortNotEnabled));
}

/// ch9.h:1201-:1207 and hub.c:2921 rely on ordering.
#[test]
fn all_seven_speed_names_are_ordered_like_linux() {
    let speeds = [UsbSpeed::Unknown, UsbSpeed::Low, UsbSpeed::Full, UsbSpeed::High,
                  UsbSpeed::Wireless, UsbSpeed::Super, UsbSpeed::SuperPlus];
    assert_eq!(speeds.len(), 7);
    for pair in speeds.windows(2) {
        assert!(pair[0] < pair[1]);
    }
}

/// ch11.h:141-:166 defines thirteen SuperSpeed masks/values used to interpret this word.
#[test]
fn all_superspeed_status_and_link_state_literals_are_pinned() {
    let got = [
        ("link-state-mask", status::LINK_STATE), ("power", status::SS_POWER),
        ("speed-mask", status::SS_SPEED), ("5gbps", status::SPEED_5GBPS),
        ("u0", status::SS_U0), ("u1", status::SS_U1), ("u2", status::SS_U2),
        ("u3", status::SS_U3), ("disabled", status::SS_DISABLED),
        ("rx-detect", status::RX_DETECT), ("inactive", status::SS_INACTIVE),
        ("polling", status::POLLING), ("recovery", status::RECOVERY),
        ("hot-reset", status::HOT_RESET), ("compliance", status::COMPLIANCE_MODE),
        ("loopback", status::LOOPBACK),
    ];
    assert_eq!(got.len(), 16);
    assert_eq!(got, [
        ("link-state-mask", 0x01e0), ("power", 0x0200), ("speed-mask", 0x1c00),
        ("5gbps", 0x0000), ("u0", 0x0000), ("u1", 0x0020), ("u2", 0x0040),
        ("u3", 0x0060), ("disabled", 0x0080), ("rx-detect", 0x00a0),
        ("inactive", 0x00c0), ("polling", 0x00e0), ("recovery", 0x0100),
        ("hot-reset", 0x0120), ("compliance", 0x0140), ("loopback", 0x0160),
    ]);
    assert_eq!(status::SS_MASK, 0x001b); // ch11.h:147-:150
}

/// hub.c:2940-:2951; ch11.h:141, :162, :166.
#[test]
fn only_superspeed_inactive_compliance_or_marked_ports_need_warm_reset() {
    assert_eq!(status::LINK_STATE, 0x01e0);
    assert_eq!(status::SS_INACTIVE, 0x00c0);
    assert_eq!(status::COMPLIANCE_MODE, 0x0140);
    assert!(warm_reset_required(true, false, 0x00c0));
    assert!(warm_reset_required(true, false, 0x0140));
    assert!(warm_reset_required(true, true, 0));
    assert!(!warm_reset_required(true, false, 0));
    assert!(!warm_reset_required(false, true, 0x00c0));
}
