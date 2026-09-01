// SPDX-License-Identifier: GPL-2.0-only
//! Pure ethtool-style settings conversion and register write planning.
//!
//! Ported from Linux `drivers/net/mii.c` and `include/uapi/linux/ethtool.h`.
//! Copyright Jeff Garzik, Donald Becker, and the Linux networking authors.

use crate::ethtool::*;
use crate::negotiation::{resolve_ethtool_common, resolve_forced_bmcr, Duplex, LinkMode, Speed};
use crate::registers::*;

pub const PORT_MII: u8 = 0x02; // include/uapi/linux/ethtool.h:2266
pub const XCVR_INTERNAL: u8 = 0x00; // include/uapi/linux/ethtool.h:2274
pub const AUTONEG_DISABLE: u8 = 0x00; // include/uapi/linux/ethtool.h:2281
pub const AUTONEG_ENABLE: u8 = 0x01; // include/uapi/linux/ethtool.h:2282
pub const ETH_MDIO_SUPPORTS_C22: u8 = 0x01; // include/uapi/linux/ethtool.h:139
pub const SPEED_UNKNOWN: u32 = u32::MAX; // include/uapi/linux/ethtool.h:2213 (`-1`)

const BASE_SUPPORTED: u32 = ADVERTISED_10BASE_T_HALF
    | ADVERTISED_10BASE_T_FULL
    | ADVERTISED_100BASE_T_HALF
    | ADVERTISED_100BASE_T_FULL
    | ADVERTISED_AUTONEG
    | ADVERTISED_TP
    | ADVERTISED_MII; // drivers/net/mii.c:149-151
const GIGABIT: u32 = ADVERTISED_1000BASE_T_HALF | ADVERTISED_1000BASE_T_FULL; // mii.c:153-154
const SPEED_MODES: u32 = ADVERTISED_10BASE_T_HALF
    | ADVERTISED_10BASE_T_FULL
    | ADVERTISED_100BASE_T_HALF
    | ADVERTISED_100BASE_T_FULL
    | ADVERTISED_1000BASE_T_HALF
    | ADVERTISED_1000BASE_T_FULL; // drivers/net/mii.c:362-367

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterSnapshot {
    pub bmcr: u16,
    pub bmsr: u16,
    pub advertise: u16,
    pub lpa: u16,
    pub ctrl1000: u16,
    pub stat1000: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkSettings {
    pub supported: u32,
    pub advertising: u32,
    pub lp_advertising: u32,
    pub speed: u32,
    pub duplex: Duplex,
    pub autoneg: u8,
    pub port: u8,
    pub transceiver: u8,
    pub phy_address: u8,
    pub mdio_support: u8,
}

/// Convert supplied register words to ethtool-style settings
/// (`drivers/net/mii.c:149-226`). No register access occurs here.
pub const fn get_link_settings(
    phy_address: u8,
    supports_gmii: bool,
    regs: RegisterSnapshot,
) -> LinkSettings {
    let mut supported = BASE_SUPPORTED;
    if supports_gmii {
        supported |= GIGABIT;
    }

    let mut advertising = ADVERTISED_TP | ADVERTISED_MII;
    advertising |= mii_lpa_to_ethtool_lpa(regs.advertise);
    if supports_gmii {
        advertising |= mii_ctrl1000_to_ethtool_adv(regs.ctrl1000);
    }

    let (mut mode, autoneg, lp_advertising) = if regs.bmcr & BMCR_ANENABLE != 0 {
        advertising |= ADVERTISED_AUTONEG;
        let lp = if regs.bmsr & BMSR_ANEGCOMPLETE != 0 {
            mii_lpa_to_ethtool_lpa(regs.lpa)
                | if supports_gmii { mii_stat1000_to_ethtool_lpa(regs.stat1000) } else { 0 }
        } else {
            0
        };
        (resolve_ethtool_common(advertising & lp), AUTONEG_ENABLE, lp)
    } else {
        (resolve_forced_bmcr(regs.bmcr), AUTONEG_DISABLE, 0)
    };

    if regs.bmsr & BMSR_LSTATUS == 0 {
        mode.speed = Speed::Unknown;
    }

    LinkSettings {
        supported,
        advertising,
        lp_advertising,
        speed: speed_number(mode.speed),
        duplex: mode.duplex,
        autoneg,
        port: PORT_MII,
        transceiver: XCVR_INTERNAL,
        phy_address,
        mdio_support: ETH_MDIO_SUPPORTS_C22,
    }
}

const fn speed_number(speed: Speed) -> u32 {
    match speed {
        Speed::Mbps10 => 10, // include/uapi/linux/ethtool.h:2194
        Speed::Mbps100 => 100, // include/uapi/linux/ethtool.h:2195
        Speed::Mbps1000 => 1000, // include/uapi/linux/ethtool.h:2196
        Speed::Unknown => SPEED_UNKNOWN,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestedSettings {
    pub speed: u32,
    pub duplex: Duplex,
    pub port: u8,
    pub transceiver: u8,
    pub phy_address: u8,
    pub autoneg: u8,
    pub advertising: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterWrite {
    pub register: u8,
    pub value: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsPlan {
    /// In Linux order: ADVERTISE, CTRL1000, then BMCR. Unchanged optional writes are absent;
    /// the autoneg BMCR restart write is always present (`drivers/net/mii.c:286-296`).
    pub writes: [Option<RegisterWrite>; 3],
    pub advertising_cache: Option<u16>,
    /// `mii->full_duplex` update. Autonegotiated settings do not update it in Linux.
    pub full_duplex_update: Option<bool>,
    pub force_media: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRefusal {
    UnsupportedSpeed { requested_mbps: u32, allowed_mbps: [u32; 3] },
    WrongPort { requested: u8, required: u8 },
    WrongTransceiver { requested: u8, required: u8 },
    WrongPhyAddress { requested: u8, required: u8 },
    InvalidAutoneg { requested: u8, disable: u8, enable: u8 },
    GigabitUnsupported { requested_mbps: u32 },
    NoAdvertisedSpeed { advertising: u32, required_mask: u32 },
}

/// Validate ethtool-style settings and return Linux's register update plan
/// (`drivers/net/mii.c:243-320`). The caller performs any writes.
pub const fn plan_settings(
    request: RequestedSettings,
    configured_phy_address: u8,
    supports_gmii: bool,
    regs: RegisterSnapshot,
) -> Result<SettingsPlan, SettingsRefusal> {
    if request.speed != 10 && request.speed != 100 && request.speed != 1000 {
        return Err(SettingsRefusal::UnsupportedSpeed {
            requested_mbps: request.speed,
            allowed_mbps: [10, 100, 1000],
        });
    }
    if request.port != PORT_MII {
        return Err(SettingsRefusal::WrongPort { requested: request.port, required: PORT_MII });
    }
    if request.transceiver != XCVR_INTERNAL {
        return Err(SettingsRefusal::WrongTransceiver {
            requested: request.transceiver,
            required: XCVR_INTERNAL,
        });
    }
    if request.phy_address != configured_phy_address {
        return Err(SettingsRefusal::WrongPhyAddress {
            requested: request.phy_address,
            required: configured_phy_address,
        });
    }
    if request.autoneg != AUTONEG_DISABLE && request.autoneg != AUTONEG_ENABLE {
        return Err(SettingsRefusal::InvalidAutoneg {
            requested: request.autoneg,
            disable: AUTONEG_DISABLE,
            enable: AUTONEG_ENABLE,
        });
    }
    if request.speed == 1000 && !supports_gmii {
        return Err(SettingsRefusal::GigabitUnsupported { requested_mbps: request.speed });
    }

    if request.autoneg == AUTONEG_ENABLE {
        if request.advertising & SPEED_MODES == 0 {
            return Err(SettingsRefusal::NoAdvertisedSpeed {
                advertising: request.advertising,
                required_mask: SPEED_MODES,
            });
        }
        let advertise = (regs.advertise & !(ADVERTISE_ALL | ADVERTISE_100BASE4))
            | ethtool_adv_to_mii_adv(request.advertising);
        let ctrl1000 = if supports_gmii {
            (regs.ctrl1000 & !(ADVERTISE_1000HALF | ADVERTISE_1000FULL))
                | ethtool_adv_to_mii_ctrl1000(request.advertising)
        } else {
            regs.ctrl1000
        };
        Ok(SettingsPlan {
            writes: [
                if regs.advertise != advertise {
                    Some(RegisterWrite { register: MII_ADVERTISE, value: advertise })
                } else {
                    None
                },
                if supports_gmii && regs.ctrl1000 != ctrl1000 {
                    Some(RegisterWrite { register: MII_CTRL1000, value: ctrl1000 })
                } else {
                    None
                },
                Some(RegisterWrite {
                    register: MII_BMCR,
                    value: regs.bmcr | BMCR_ANENABLE | BMCR_ANRESTART,
                }),
            ],
            advertising_cache: if regs.advertise != advertise { Some(advertise) } else { None },
            full_duplex_update: None,
            force_media: false,
        })
    } else {
        let mut bmcr = regs.bmcr & !(BMCR_ANENABLE | BMCR_SPEED100 | BMCR_SPEED1000 | BMCR_FULLDPLX);
        if request.speed == 1000 {
            bmcr |= BMCR_SPEED1000;
        } else if request.speed == 100 {
            bmcr |= BMCR_SPEED100;
        }
        if matches!(request.duplex, Duplex::Full) {
            bmcr |= BMCR_FULLDPLX;
        }
        Ok(SettingsPlan {
            writes: [
                if regs.bmcr != bmcr {
                    Some(RegisterWrite { register: MII_BMCR, value: bmcr })
                } else {
                    None
                },
                None,
                None,
            ],
            advertising_cache: None,
            full_duplex_update: Some(matches!(request.duplex, Duplex::Full)),
            force_media: true,
        })
    }
}

/// Build a compact mode value for callers that do not need the full settings object.
pub const fn current_mode(settings: LinkSettings) -> LinkMode {
    let speed = match settings.speed {
        10 => Speed::Mbps10,
        100 => Speed::Mbps100,
        1000 => Speed::Mbps1000,
        _ => Speed::Unknown,
    };
    LinkMode { speed, duplex: settings.duplex }
}
