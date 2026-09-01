// SPDX-License-Identifier: GPL-2.0-only
//! Settings vectors from Linux `drivers/net/mii.c`.
//! Copyright Jeff Garzik, Donald Becker, and the Linux networking authors.

use mii_phy_core::negotiation::{Duplex, LinkMode, Speed};
use mii_phy_core::settings::*;

const EMPTY: RegisterSnapshot = RegisterSnapshot {
    bmcr: 0,
    bmsr: 0,
    advertise: 0,
    lpa: 0,
    ctrl1000: 0,
    stat1000: 0,
};

#[test]
fn get_settings_resolves_completed_gigabit_autonegotiation() {
    // drivers/net/mii.c:149-226. Registers: AN enabled, link+AN complete, all local modes,
    // partner ACK + 100-full, and partner 1000-full.
    let got = get_link_settings(7, true, RegisterSnapshot {
        bmcr: 0x1000,
        bmsr: 0x0024,
        advertise: 0x0de0,
        lpa: 0x4160,
        ctrl1000: 0x0300,
        stat1000: 0x0800,
    });
    assert_eq!(got, LinkSettings {
        supported: 0x02ff,
        advertising: 0x62ff,
        lp_advertising: 0x006b,
        speed: 1000,
        duplex: Duplex::Full,
        autoneg: 0x01,
        port: 0x02,
        transceiver: 0x00,
        phy_address: 7,
        mdio_support: 0x01,
    });
    assert_eq!(current_mode(got), LinkMode { speed: Speed::Mbps1000, duplex: Duplex::Full });
}

#[test]
fn get_settings_reports_unknown_speed_when_link_is_down() {
    // drivers/net/mii.c:202-217; SPEED_UNKNOWN is -1 at include/uapi/linux/ethtool.h:2213.
    let got = get_link_settings(1, false, RegisterSnapshot {
        bmcr: 0x2100,
        bmsr: 0x0000,
        advertise: 0x01e0,
        ..EMPTY
    });
    assert_eq!(got.speed, 0xffff_ffff);
    assert_eq!(got.duplex, Duplex::Full);
    assert_eq!(got.autoneg, 0x00);
    assert_eq!(got.lp_advertising, 0);
    assert_eq!(current_mode(got), LinkMode { speed: Speed::Unknown, duplex: Duplex::Full });
}

#[test]
fn get_settings_hides_partner_modes_until_autoneg_is_complete() {
    // drivers/net/mii.c:176-186.
    let got = get_link_settings(0, true, RegisterSnapshot {
        bmcr: 0x1000,
        bmsr: 0x0004,
        advertise: 0x01e0,
        lpa: 0x4160,
        ctrl1000: 0x0300,
        stat1000: 0x0800,
    });
    assert_eq!(got.lp_advertising, 0);
    assert_eq!(got.speed, 10);
    assert_eq!(got.duplex, Duplex::Half);
}

fn request() -> RequestedSettings {
    RequestedSettings {
        speed: 100,
        duplex: Duplex::Full,
        port: 0x02,
        transceiver: 0x00,
        phy_address: 3,
        autoneg: 0x01,
        advertising: 0x002a,
    }
}

#[test]
fn autoneg_settings_plan_preserves_unrelated_bits_and_restarts() {
    // drivers/net/mii.c:274-298. 0x8001 and 0x4000 are unrelated bits and must survive.
    let plan = plan_settings(request(), 3, true, RegisterSnapshot {
        bmcr: 0x0100,
        advertise: 0x83e1,
        ctrl1000: 0x4300,
        ..EMPTY
    }).unwrap();
    assert_eq!(plan, SettingsPlan {
        writes: [
            Some(RegisterWrite { register: 0x04, value: 0x8141 }),
            Some(RegisterWrite { register: 0x09, value: 0x4200 }),
            Some(RegisterWrite { register: 0x00, value: 0x1300 }),
        ],
        advertising_cache: Some(0x8141),
        full_duplex_update: None,
        force_media: false,
    });
}

#[test]
fn forced_settings_plan_clears_autoneg_and_selects_speed_duplex() {
    // drivers/net/mii.c:302-318; preserve RESET (0x8000), select SPEED100 (0x2000), FULL (0x0100).
    let mut req = request();
    req.autoneg = 0x00;
    let plan = plan_settings(req, 3, true, RegisterSnapshot { bmcr: 0x9340, ..EMPTY }).unwrap();
    assert_eq!(plan, SettingsPlan {
        writes: [Some(RegisterWrite { register: 0x00, value: 0xa300 }), None, None],
        advertising_cache: None,
        full_duplex_update: Some(true),
        force_media: true,
    });
}

#[test]
fn unchanged_forced_bmcr_produces_no_write() {
    // drivers/net/mii.c:315-316.
    let mut req = request();
    req.autoneg = 0;
    let plan = plan_settings(req, 3, true, RegisterSnapshot { bmcr: 0x2100, ..EMPTY }).unwrap();
    assert_eq!(plan.writes, [None, None, None]);
}

#[test]
fn each_invalid_setting_names_what_refused_and_why() {
    let mut req = request();
    req.speed = 2500;
    assert_eq!(plan_settings(req, 3, true, EMPTY), Err(SettingsRefusal::UnsupportedSpeed {
        requested_mbps: 2500,
        allowed_mbps: [10, 100, 1000], // drivers/net/mii.c:243-246
    }));

    req = request(); req.port = 0;
    assert_eq!(plan_settings(req, 3, true, EMPTY), Err(SettingsRefusal::WrongPort { requested: 0, required: 2 }));
    req = request(); req.transceiver = 1;
    assert_eq!(plan_settings(req, 3, true, EMPTY), Err(SettingsRefusal::WrongTransceiver { requested: 1, required: 0 }));
    req = request(); req.phy_address = 4;
    assert_eq!(plan_settings(req, 3, true, EMPTY), Err(SettingsRefusal::WrongPhyAddress { requested: 4, required: 3 }));
    req = request(); req.autoneg = 2;
    assert_eq!(plan_settings(req, 3, true, EMPTY), Err(SettingsRefusal::InvalidAutoneg { requested: 2, disable: 0, enable: 1 }));
    req = request(); req.speed = 1000;
    assert_eq!(plan_settings(req, 3, false, EMPTY), Err(SettingsRefusal::GigabitUnsupported { requested_mbps: 1000 }));
    req = request(); req.advertising = 0x6000;
    assert_eq!(plan_settings(req, 3, true, EMPTY), Err(SettingsRefusal::NoAdvertisedSpeed {
        advertising: 0x6000,
        required_mask: 0x003f, // drivers/net/mii.c:266-271
    }));
}
