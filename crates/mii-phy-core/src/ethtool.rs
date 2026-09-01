// SPDX-License-Identifier: GPL-2.0-only
//! Legacy ethtool link-mode literals and MII register conversions.
//!
//! Ported from Linux `include/linux/mii.h` and `include/uapi/linux/ethtool.h`.
//! Copyright (C) 1996, 1999, 2001 David S. Miller and the Linux networking authors.

use crate::registers::*;

/// One legacy ethtool link-mode bit used by the MII conversion helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkModeDef {
    pub name: &'static str,
    pub mask: u32,
}

pub const ADVERTISED_10BASE_T_HALF: u32 = 0x0001; // include/uapi/linux/ethtool.h:1964,2149
pub const ADVERTISED_10BASE_T_FULL: u32 = 0x0002; // include/uapi/linux/ethtool.h:1965,2150
pub const ADVERTISED_100BASE_T_HALF: u32 = 0x0004; // include/uapi/linux/ethtool.h:1966,2151
pub const ADVERTISED_100BASE_T_FULL: u32 = 0x0008; // include/uapi/linux/ethtool.h:1967,2152
pub const ADVERTISED_1000BASE_T_HALF: u32 = 0x0010; // include/uapi/linux/ethtool.h:1968,2153
pub const ADVERTISED_1000BASE_T_FULL: u32 = 0x0020; // include/uapi/linux/ethtool.h:1969,2154
pub const ADVERTISED_AUTONEG: u32 = 0x0040; // include/uapi/linux/ethtool.h:1970,2155
pub const ADVERTISED_TP: u32 = 0x0080; // include/uapi/linux/ethtool.h:1971,2156
pub const ADVERTISED_MII: u32 = 0x0200; // include/uapi/linux/ethtool.h:1973,2158
pub const ADVERTISED_PAUSE: u32 = 0x2000; // include/uapi/linux/ethtool.h:1977,2162
pub const ADVERTISED_ASYM_PAUSE: u32 = 0x4000; // include/uapi/linux/ethtool.h:1978,2163

/// All eleven legacy modes consumed or emitted by the MII conversion layer.
pub const MII_LINK_MODES: [LinkModeDef; 11] = [
    LinkModeDef { name: "10baseT_Half", mask: 0x0001 }, // include/uapi/linux/ethtool.h:1964
    LinkModeDef { name: "10baseT_Full", mask: 0x0002 }, // include/uapi/linux/ethtool.h:1965
    LinkModeDef { name: "100baseT_Half", mask: 0x0004 }, // include/uapi/linux/ethtool.h:1966
    LinkModeDef { name: "100baseT_Full", mask: 0x0008 }, // include/uapi/linux/ethtool.h:1967
    LinkModeDef { name: "1000baseT_Half", mask: 0x0010 }, // include/uapi/linux/ethtool.h:1968
    LinkModeDef { name: "1000baseT_Full", mask: 0x0020 }, // include/uapi/linux/ethtool.h:1969
    LinkModeDef { name: "Autoneg", mask: 0x0040 }, // include/uapi/linux/ethtool.h:1970
    LinkModeDef { name: "TP", mask: 0x0080 }, // include/uapi/linux/ethtool.h:1971
    LinkModeDef { name: "MII", mask: 0x0200 }, // include/uapi/linux/ethtool.h:1973
    LinkModeDef { name: "Pause", mask: 0x2000 }, // include/uapi/linux/ethtool.h:1977
    LinkModeDef { name: "Asym_Pause", mask: 0x4000 }, // include/uapi/linux/ethtool.h:1978
];

/// `ethtool_adv_to_mii_adv_t` (`include/linux/mii.h:115-133`).
pub const fn ethtool_adv_to_mii_adv(advertising: u32) -> u16 {
    (if advertising & ADVERTISED_10BASE_T_HALF != 0 { ADVERTISE_10HALF } else { 0 })
        | (if advertising & ADVERTISED_10BASE_T_FULL != 0 { ADVERTISE_10FULL } else { 0 })
        | (if advertising & ADVERTISED_100BASE_T_HALF != 0 { ADVERTISE_100HALF } else { 0 })
        | (if advertising & ADVERTISED_100BASE_T_FULL != 0 { ADVERTISE_100FULL } else { 0 })
        | (if advertising & ADVERTISED_PAUSE != 0 { ADVERTISE_PAUSE_CAP } else { 0 })
        | (if advertising & ADVERTISED_ASYM_PAUSE != 0 { ADVERTISE_PAUSE_ASYM } else { 0 })
}

/// `mii_adv_to_ethtool_adv_t` (`include/linux/mii.h:166-184`).
pub const fn mii_adv_to_ethtool_adv(advertise: u16) -> u32 {
    (if advertise & ADVERTISE_10HALF != 0 { ADVERTISED_10BASE_T_HALF } else { 0 })
        | (if advertise & ADVERTISE_10FULL != 0 { ADVERTISED_10BASE_T_FULL } else { 0 })
        | (if advertise & ADVERTISE_100HALF != 0 { ADVERTISED_100BASE_T_HALF } else { 0 })
        | (if advertise & ADVERTISE_100FULL != 0 { ADVERTISED_100BASE_T_FULL } else { 0 })
        | (if advertise & ADVERTISE_PAUSE_CAP != 0 { ADVERTISED_PAUSE } else { 0 })
        | (if advertise & ADVERTISE_PAUSE_ASYM != 0 { ADVERTISED_ASYM_PAUSE } else { 0 })
}

/// `ethtool_adv_to_mii_ctrl1000_t` (`include/linux/mii.h:198-209`).
pub const fn ethtool_adv_to_mii_ctrl1000(advertising: u32) -> u16 {
    (if advertising & ADVERTISED_1000BASE_T_HALF != 0 { ADVERTISE_1000HALF } else { 0 })
        | (if advertising & ADVERTISED_1000BASE_T_FULL != 0 { ADVERTISE_1000FULL } else { 0 })
}

/// `mii_ctrl1000_to_ethtool_adv_t` (`include/linux/mii.h:241-252`).
pub const fn mii_ctrl1000_to_ethtool_adv(ctrl1000: u16) -> u32 {
    (if ctrl1000 & ADVERTISE_1000HALF != 0 { ADVERTISED_1000BASE_T_HALF } else { 0 })
        | (if ctrl1000 & ADVERTISE_1000FULL != 0 { ADVERTISED_1000BASE_T_FULL } else { 0 })
}

/// `mii_lpa_to_ethtool_lpa_t` (`include/linux/mii.h:261-270`).
pub const fn mii_lpa_to_ethtool_lpa(lpa: u16) -> u32 {
    mii_adv_to_ethtool_adv(lpa)
        | if lpa & LPA_LPACK != 0 { ADVERTISED_AUTONEG } else { 0 }
}

/// `mii_stat1000_to_ethtool_lpa_t` (`include/linux/mii.h:279-290`).
pub const fn mii_stat1000_to_ethtool_lpa(stat1000: u16) -> u32 {
    (if stat1000 & LPA_1000HALF != 0 { ADVERTISED_1000BASE_T_HALF } else { 0 })
        | (if stat1000 & LPA_1000FULL != 0 { ADVERTISED_1000BASE_T_FULL } else { 0 })
}
