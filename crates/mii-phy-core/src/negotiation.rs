// SPDX-License-Identifier: GPL-2.0-only
//! Common-capability negotiation and speed/duplex resolution.
//!
//! Ported from Linux `include/linux/mii.h` and `drivers/net/mii.c`.
//! Copyright Jeff Garzik, Donald Becker, David S. Miller, and the Linux networking authors.

use crate::ethtool::*;
use crate::registers::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Mbps10,
    Mbps100,
    Mbps1000,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duplex {
    Half,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkMode {
    pub speed: Speed,
    pub duplex: Duplex,
}

/// The wire-word intersection Linux passes to `mii_nway_result` (`drivers/net/mii.c:553-555`).
pub const fn common_capabilities(advertise: u16, partner: u16) -> u16 {
    advertise & partner
}

/// IEEE priority with Linux's documented 100BASE-T4 exception (`include/linux/mii.h:70-85`).
///
/// As in Linux, an empty word falls back to `LPA_10HALF` rather than inventing an error.
pub const fn nway_result(negotiated: u16) -> u16 {
    if negotiated & LPA_100FULL != 0 {
        LPA_100FULL
    } else if negotiated & LPA_100BASE4 != 0 {
        LPA_100BASE4
    } else if negotiated & LPA_100HALF != 0 {
        LPA_100HALF
    } else if negotiated & LPA_10FULL != 0 {
        LPA_10FULL
    } else {
        LPA_10HALF
    }
}

/// Linux `mii_duplex` (`include/linux/mii.h:98-105`).
pub const fn negotiated_duplex(duplex_lock: bool, negotiated: u16) -> Duplex {
    if duplex_lock || nway_result(negotiated) & LPA_DUPLEX != 0 {
        Duplex::Full
    } else {
        Duplex::Half
    }
}

/// Resolve legacy ethtool common capabilities with the exact 1000/100/10 priority of
/// `mii_ethtool_get_link_ksettings` (`drivers/net/mii.c:188-201`).
pub const fn resolve_ethtool_common(negotiated: u32) -> LinkMode {
    if negotiated & (ADVERTISED_1000BASE_T_FULL | ADVERTISED_1000BASE_T_HALF) != 0 {
        LinkMode {
            speed: Speed::Mbps1000,
            duplex: if negotiated & ADVERTISED_1000BASE_T_FULL != 0 {
                Duplex::Full
            } else {
                Duplex::Half
            },
        }
    } else if negotiated & (ADVERTISED_100BASE_T_FULL | ADVERTISED_100BASE_T_HALF) != 0 {
        LinkMode {
            speed: Speed::Mbps100,
            duplex: if negotiated & ADVERTISED_100BASE_T_FULL != 0 {
                Duplex::Full
            } else {
                Duplex::Half
            },
        }
    } else {
        LinkMode {
            speed: Speed::Mbps10,
            duplex: if negotiated & ADVERTISED_10BASE_T_FULL != 0 {
                Duplex::Full
            } else {
                Duplex::Half
            },
        }
    }
}

/// Forced speed and duplex from BMCR (`drivers/net/mii.c:205-211`).
///
/// Linux treats both speed bits set as 100 Mbps: the 1000 branch requires SPEED100 to be clear.
pub const fn resolve_forced_bmcr(bmcr: u16) -> LinkMode {
    let speed = if bmcr & BMCR_SPEED1000 != 0 && bmcr & BMCR_SPEED100 == 0 {
        Speed::Mbps1000
    } else if bmcr & BMCR_SPEED100 != 0 {
        Speed::Mbps100
    } else {
        Speed::Mbps10
    };
    LinkMode {
        speed,
        duplex: if bmcr & BMCR_FULLDPLX != 0 { Duplex::Full } else { Duplex::Half },
    }
}

/// Media resolution from ADVERTISE/LPA plus STAT1000 (`drivers/net/mii.c:553-563`).
pub const fn resolve_media(advertise: u16, lpa: u16, stat1000: u16) -> LinkMode {
    let media = nway_result(common_capabilities(advertise, lpa));
    let speed = if stat1000 & (LPA_1000FULL | LPA_1000HALF) != 0 {
        Speed::Mbps1000
    } else if media & (LPA_100FULL | LPA_100HALF) != 0 {
        Speed::Mbps100
    } else {
        Speed::Mbps10
    };
    let duplex = if stat1000 & LPA_1000FULL != 0 || media & LPA_DUPLEX != 0 {
        Duplex::Full
    } else {
        Duplex::Half
    };
    LinkMode { speed, duplex }
}

/// GMII support test (`drivers/net/mii.c:429-436`).
pub const fn supports_gmii(bmsr: u16, estatus: u16) -> bool {
    bmsr & BMSR_ESTATEN != 0 && estatus & (ESTATUS_1000_TFULL | ESTATUS_1000_THALF) != 0
}

/// The second BMSR read decides link state; the first is only a latch read
/// (`drivers/net/mii.c:447-451`).
pub const fn link_ok(_latch_read: u16, status_read: u16) -> bool {
    status_read & BMSR_LSTATUS != 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartRefusal {
    AutonegotiationDisabled { bmcr: u16, required_mask: u16 },
}

/// Return the BMCR write needed to restart NWay, or name why Linux refuses it
/// (`drivers/net/mii.c:465-474`).
pub const fn nway_restart_word(bmcr: u16) -> Result<u16, RestartRefusal> {
    if bmcr & BMCR_ANENABLE == 0 {
        Err(RestartRefusal::AutonegotiationDisabled {
            bmcr,
            required_mask: BMCR_ANENABLE,
        })
    } else {
        Ok(bmcr | BMCR_ANRESTART)
    }
}
