// SPDX-License-Identifier: GPL-2.0-only
//! Negotiation vectors from Linux `include/linux/mii.h` and `drivers/net/mii.c`.
//! Copyright Jeff Garzik, Donald Becker, David S. Miller, and the Linux networking authors.

use mii_phy_core::negotiation::*;

#[test]
fn common_capabilities_are_the_literal_word_intersection() {
    // drivers/net/mii.c:553-554.
    assert_eq!(common_capabilities(0x01e1, 0x00e1), 0x00e1);
}

#[test]
fn nway_priority_is_100full_t4_100half_10full_10half() {
    // include/linux/mii.h:74-85; literals from include/uapi/linux/mii.h:97-105.
    assert_eq!(nway_result(0x03e0), 0x0100);
    assert_eq!(nway_result(0x02e0), 0x0200, "Linux's documented T4 exception");
    assert_eq!(nway_result(0x00e0), 0x0080);
    assert_eq!(nway_result(0x0060), 0x0040);
    assert_eq!(nway_result(0x0020), 0x0020);
    assert_eq!(nway_result(0x0000), 0x0020, "Linux falls back to 10-half");
}

#[test]
fn duplex_lock_and_negotiated_full_bits_match_linux() {
    // include/linux/mii.h:98-105.
    assert_eq!(negotiated_duplex(true, 0x0020), Duplex::Full);
    assert_eq!(negotiated_duplex(false, 0x0100), Duplex::Full);
    assert_eq!(negotiated_duplex(false, 0x0080), Duplex::Half);
}

#[test]
fn ethtool_common_resolution_prefers_speed_then_full_duplex() {
    // drivers/net/mii.c:188-201; ethtool masks from include/uapi/linux/ethtool.h:1964-1969.
    assert_eq!(resolve_ethtool_common(0x003f), LinkMode { speed: Speed::Mbps1000, duplex: Duplex::Full });
    assert_eq!(resolve_ethtool_common(0x001f), LinkMode { speed: Speed::Mbps1000, duplex: Duplex::Half });
    assert_eq!(resolve_ethtool_common(0x000f), LinkMode { speed: Speed::Mbps100, duplex: Duplex::Full });
    assert_eq!(resolve_ethtool_common(0x0005), LinkMode { speed: Speed::Mbps100, duplex: Duplex::Half });
    assert_eq!(resolve_ethtool_common(0x0002), LinkMode { speed: Speed::Mbps10, duplex: Duplex::Full });
    assert_eq!(resolve_ethtool_common(0x0000), LinkMode { speed: Speed::Mbps10, duplex: Duplex::Half });
}

#[test]
fn forced_bmcr_resolution_preserves_linuxs_conflicting_speed_rule() {
    // drivers/net/mii.c:205-211; BMCR masks from include/uapi/linux/mii.h:42,44,49.
    assert_eq!(resolve_forced_bmcr(0x0140), LinkMode { speed: Speed::Mbps1000, duplex: Duplex::Full });
    assert_eq!(resolve_forced_bmcr(0x2100), LinkMode { speed: Speed::Mbps100, duplex: Duplex::Full });
    assert_eq!(resolve_forced_bmcr(0x2040), LinkMode { speed: Speed::Mbps100, duplex: Duplex::Half });
    assert_eq!(resolve_forced_bmcr(0x0000), LinkMode { speed: Speed::Mbps10, duplex: Duplex::Half });
}

#[test]
fn media_resolution_uses_stat1000_for_gigabit_and_full_duplex() {
    // drivers/net/mii.c:553-563.
    assert_eq!(resolve_media(0x01e0, 0x01e0, 0x0000),
        LinkMode { speed: Speed::Mbps100, duplex: Duplex::Full });
    assert_eq!(resolve_media(0x0020, 0x0020, 0x0400),
        LinkMode { speed: Speed::Mbps1000, duplex: Duplex::Half });
    assert_eq!(resolve_media(0x0020, 0x0020, 0x0800),
        LinkMode { speed: Speed::Mbps1000, duplex: Duplex::Full });
}

#[test]
fn gmii_support_requires_extended_status_and_a_1000t_bit() {
    // drivers/net/mii.c:429-436; BMSR_ESTATEN=0x0100, ESTATUS T bits=0x3000.
    assert!(supports_gmii(0x0100, 0x2000));
    assert!(supports_gmii(0x0100, 0x1000));
    assert!(!supports_gmii(0x0000, 0x3000));
    assert!(!supports_gmii(0x0100, 0xc000), "1000Base-X is not 1000Base-T");
}

#[test]
fn second_bmsr_read_is_the_link_status_read() {
    // drivers/net/mii.c:447-451; BMSR_LSTATUS=0x0004.
    assert!(link_ok(0x0000, 0x0004));
    assert!(!link_ok(0x0004, 0x0000), "the first read only latches status");
}

#[test]
fn nway_restart_names_the_disabled_autoneg_refusal() {
    // drivers/net/mii.c:465-474; BMCR_ANENABLE=0x1000, BMCR_ANRESTART=0x0200.
    assert_eq!(nway_restart_word(0x1100), Ok(0x1300));
    assert_eq!(nway_restart_word(0x0100), Err(RestartRefusal::AutonegotiationDisabled {
        bmcr: 0x0100,
        required_mask: 0x1000,
    }));
}
