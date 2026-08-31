// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for link state and the link-change patch. Expected values are LINUX literals, quoted
//! with their lines, and the register/value pairs are written out longhand rather than derived from
//! the table under test.

use r8169_core::chip::MacVersion;
use r8169_core::eri;
use r8169_core::irq;
use r8169_core::link::{self, EriRmw, EriWrite, LinkChgPatch, Speed};
use r8169_core::regs;

/// r8169_main.c:297 and :558-:565.
#[test]
fn the_phy_status_bits_match_linux() {
    assert_eq!(regs::PHY_STATUS, 0x6c);
    assert_eq!(link::TBI_ENABLE, 0x80);
    assert_eq!(link::TX_FLOW_CTRL, 0x40);
    assert_eq!(link::RX_FLOW_CTRL, 0x20);
    assert_eq!(link::GBPS_FULL, 0x10);
    assert_eq!(link::MBPS_100, 0x08);
    assert_eq!(link::MBPS_10, 0x04);
    assert_eq!(link::LINK_STATUS, 0x02);
    assert_eq!(link::FULL_DUP, 0x01);
    assert_eq!(link::SPEED_BITS, 0x1c, "0x10 | 0x08 | 0x04");
}

/// The eight bits PARTITION the byte: they cover 0xff and no two overlap. A coverage assertion, so
/// that adding a ninth name or mistyping one so it collides fails here.
#[test]
fn the_eight_bits_partition_the_byte() {
    const ALL: [u8; 8] = [
        link::TBI_ENABLE,
        link::TX_FLOW_CTRL,
        link::RX_FLOW_CTRL,
        link::GBPS_FULL,
        link::MBPS_100,
        link::MBPS_10,
        link::LINK_STATUS,
        link::FULL_DUP,
    ];
    let mut seen: u8 = 0;
    for b in ALL {
        assert_eq!(b.count_ones(), 1, "{b:#x} is not a single bit");
        assert_eq!(seen & b, 0, "{b:#x} collides with an earlier bit");
        seen |= b;
    }
    assert_eq!(seen, 0xff, "the eight names must cover the whole byte");
}

/// r8169_main.c:560-:562 — one speed bit at a time.
#[test]
fn each_speed_bit_decodes_to_its_own_speed() {
    assert_eq!(link::decode_phy_status(0x10).speed, Speed::G1000);
    assert_eq!(link::decode_phy_status(0x08).speed, Speed::M100);
    assert_eq!(link::decode_phy_status(0x04).speed, Speed::M10);
    assert_eq!(link::decode_phy_status(0x00).speed, Speed::Unresolved);
    // The other five bits do not disturb the speed.
    assert_eq!(link::decode_phy_status(0x10 | 0xe3).speed, Speed::G1000);
}

/// TWO SPEED BITS IS NOT A SPEED. A decoder that tests them in order reports whichever it looked at
/// first; this one names the impossible value instead.
#[test]
fn more_than_one_speed_bit_is_a_named_refusal_not_a_guess() {
    for raw in [0x18u8, 0x14, 0x0c, 0x1c] {
        assert_eq!(
            link::decode_phy_status(raw).speed,
            Speed::Conflicting(raw),
            "raw {raw:#x}"
        );
    }
    // The refusal carries the whole byte, not just the speed field, so the log shows the context.
    assert_eq!(link::decode_phy_status(0x1e).speed, Speed::Conflicting(0x1e));
}

/// r8169_main.c:560 — `_1000bpsF`. The trailing F is the duplex, and it is not `FullDup`.
#[test]
fn gigabit_is_full_duplex_by_its_own_bit_name() {
    // Gigabit with FullDup CLEAR is still full duplex.
    let g = link::decode_phy_status(link::GBPS_FULL | link::LINK_STATUS);
    assert_eq!(g.speed, Speed::G1000);
    assert!(g.full_duplex, "the trailing F in _1000bpsF is the duplex");
    // 100 Mbit takes FullDup, in both directions.
    assert!(!link::decode_phy_status(link::MBPS_100 | link::LINK_STATUS).full_duplex);
    assert!(
        link::decode_phy_status(link::MBPS_100 | link::LINK_STATUS | link::FULL_DUP).full_duplex
    );
    assert!(!link::decode_phy_status(link::MBPS_10 | link::LINK_STATUS).full_duplex);
}

/// The whole decode, including the raw byte kept for a value nobody anticipated.
#[test]
fn a_decoded_status_keeps_the_byte_it_came_from() {
    let s = link::decode_phy_status(0xff);
    assert_eq!(s.raw, 0xff, "the raw byte survives decoding");
    assert!(s.up && s.tbi && s.tx_pause && s.rx_pause);
    assert_eq!(s.speed, Speed::Conflicting(0xff), "0xff sets all three speed bits");

    let down = link::decode_phy_status(0x00);
    assert!(!down.up);
    assert_eq!(down.raw, 0x00);

    // Link down with a speed still reported: the register is decoded as it stands, not corrected.
    let odd = link::decode_phy_status(link::MBPS_100);
    assert!(!odd.up);
    assert_eq!(odd.speed, Speed::M100);
}

/// r8169_main.c:460, :4865, :5316 — the interrupt that brings this work.
#[test]
fn the_link_change_interrupt_is_in_the_default_mask() {
    assert_eq!(irq::events::LINK_CHG, 0x0020);
}

// ─────────────────────── rtl_link_chg_patch ───────────────────────

const M1111: u32 = eri::ERIAR_MASK_1111;
const M0011: u32 = eri::ERIAR_MASK_0011;

fn ew(addr: u32, mask: u32, val: u32) -> Option<EriWrite> {
    Some(EriWrite { addr, mask, val })
}

/// r8169_main.c:1675-:1687 — versions 34 and 38, THREE branches, each ending in a filter reset.
#[test]
fn versions_34_and_38_have_three_speed_branches_and_reset_the_filter() {
    for v in [34u8, 38] {
        assert_eq!(
            link::link_chg_patch(MacVersion(v), Speed::G1000),
            LinkChgPatch {
                writes: [ew(0x1bc, M1111, 0x11), ew(0x1dc, M1111, 0x05)],
                reset_packet_filter: true
            },
            "ver {v} at 1000"
        );
        assert_eq!(
            link::link_chg_patch(MacVersion(v), Speed::M100),
            LinkChgPatch {
                writes: [ew(0x1bc, M1111, 0x1f), ew(0x1dc, M1111, 0x05)],
                reset_packet_filter: true
            },
            "ver {v} at 100"
        );
        for slow in [Speed::M10, Speed::Unresolved] {
            assert_eq!(
                link::link_chg_patch(MacVersion(v), slow),
                LinkChgPatch {
                    writes: [ew(0x1bc, M1111, 0x1f), ew(0x1dc, M1111, 0x3f)],
                    reset_packet_filter: true
                },
                "ver {v} at {slow:?}"
            );
        }
    }
}

/// r8169_main.c:1688-:1697 — versions 35 and 36, TWO branches, no filter reset.
#[test]
fn versions_35_and_36_have_two_branches_and_no_filter_reset() {
    for v in [35u8, 36] {
        assert_eq!(
            link::link_chg_patch(MacVersion(v), Speed::G1000),
            LinkChgPatch {
                writes: [ew(0x1bc, M1111, 0x11), ew(0x1dc, M1111, 0x05)],
                reset_packet_filter: false
            },
            "ver {v} at 1000"
        );
        for slow in [Speed::M100, Speed::M10, Speed::Unresolved] {
            assert_eq!(
                link::link_chg_patch(MacVersion(v), slow),
                LinkChgPatch {
                    writes: [ew(0x1bc, M1111, 0x1f), ew(0x1dc, M1111, 0x3f)],
                    reset_packet_filter: false
                },
                "ver {v} at {slow:?}"
            );
        }
    }
}

/// THE VECTOR THE MERGE FAILS. The two groups' gigabit arms are identical and their fallback arms
/// are identical, which is what makes them look like one case. At 100 Mbit they are NOT, and one of
/// them resets the packet filter.
#[test]
fn the_two_groups_that_look_alike_differ_at_100_megabit_and_on_the_filter() {
    let a = link::link_chg_patch(MacVersion(34), Speed::M100);
    let b = link::link_chg_patch(MacVersion(35), Speed::M100);
    assert_ne!(a, b, "34 and 35 must not agree at 100 Mbit");
    assert_eq!(a.writes[1], ew(0x1dc, M1111, 0x05), "34 has its own 100 Mbit branch");
    assert_eq!(b.writes[1], ew(0x1dc, M1111, 0x3f), "35 falls through to the fallback");
    assert!(a.reset_packet_filter);
    assert!(!b.reset_packet_filter);

    // And they DO agree where the reference makes them agree — the control that stops the vector
    // above from passing against a table that simply differs everywhere.
    assert_eq!(
        link::link_chg_patch(MacVersion(34), Speed::G1000).writes,
        link::link_chg_patch(MacVersion(35), Speed::G1000).writes
    );
    assert_eq!(
        link::link_chg_patch(MacVersion(34), Speed::M10).writes,
        link::link_chg_patch(MacVersion(35), Speed::M10).writes
    );
}

/// r8169_main.c:1698-:1703 — version 37: a narrower mask, different registers, and a fallback that
/// writes ONE register rather than two.
#[test]
fn version_37_uses_a_narrow_mask_and_writes_one_register_on_the_fallback() {
    assert_eq!(
        link::link_chg_patch(MacVersion(37), Speed::M10),
        LinkChgPatch {
            writes: [ew(0x1d0, M0011, 0x4d02), ew(0x1dc, M0011, 0x0060a)],
            reset_packet_filter: false
        }
    );
    for fast in [Speed::M100, Speed::G1000, Speed::Unresolved] {
        let p = link::link_chg_patch(MacVersion(37), fast);
        assert_eq!(
            p,
            LinkChgPatch { writes: [ew(0x1d0, M0011, 0x0000), None], reset_packet_filter: false },
            "ver 37 at {fast:?}"
        );
        assert_eq!(p.len(), 1, "the fallback leaves 0x1dc alone");
    }
    // The mask is 0011, not 1111 — a two-byte access, not four.
    assert_eq!(M0011, 0x3000);
    assert_ne!(M0011, M1111);
}

/// r8169_main.c:1671-:1704 — the whole version space. Only five versions produce any writes.
#[test]
fn every_other_version_gets_no_patch_at_all() {
    for v in 0u8..=70 {
        let patched = matches!(v, 34 | 35 | 36 | 37 | 38);
        for speed in [Speed::G1000, Speed::M100, Speed::M10, Speed::Unresolved] {
            let p = link::link_chg_patch(MacVersion(v), speed);
            assert_eq!(p.is_empty(), !patched, "ver {v} at {speed:?}");
            if !patched {
                assert_eq!(p, LinkChgPatch::NONE, "ver {v}");
            }
        }
    }
    // The counts the reference actually produces: zero, one and two. Nothing else.
    let mut counts = [0usize; 3];
    for v in 0u8..=70 {
        for speed in [Speed::G1000, Speed::M100, Speed::M10, Speed::Unresolved] {
            counts[link::link_chg_patch(MacVersion(v), speed).len()] += 1;
        }
    }
    assert_eq!(counts[1], 3, "only version 37's three fast branches write one register");
    assert!(counts[2] > 0 && counts[0] > 0);
}

/// A conflicting speed reading must not be treated as gigabit. It falls to the same branch as an
/// unresolved one, which is the conservative arm in every group.
#[test]
fn a_conflicting_speed_takes_the_conservative_branch() {
    let c = Speed::Conflicting(0x1c);
    assert_eq!(
        link::link_chg_patch(MacVersion(34), c),
        link::link_chg_patch(MacVersion(34), Speed::M10)
    );
    assert_eq!(
        link::link_chg_patch(MacVersion(37), c),
        link::link_chg_patch(MacVersion(37), Speed::M100)
    );
}

// ─────────────────────── the packet filter pulse ───────────────────────

/// r8169_main.c:1086-:1088 — `(val & ~m) | p`, and SET WINS on a bit named in both.
#[test]
fn the_read_modify_write_lets_the_set_win() {
    assert_eq!(link::w0w1(0b1010, 0b0001, 0b1000), 0b0011);
    assert_eq!(link::w0w1(0xffff_ffff, 0, 0xf), 0xffff_fff0);
    assert_eq!(link::w0w1(0, 0xf, 0), 0xf);
    // The same bit in both p and m: cleared first, then set. It ends up SET.
    assert_eq!(link::w0w1(0, 1, 1), 1, "set wins over clear");
    assert_eq!(link::w0w1(1, 1, 1), 1);
}

/// r8169_main.c:1613-:1617 — clear then set. THE END STATE CAN EQUAL THE START STATE.
#[test]
fn the_packet_filter_reset_is_a_pulse_that_cannot_be_folded_into_one_write() {
    assert_eq!(
        link::PACKET_FILTER_PULSE,
        [
            EriRmw { addr: 0xdc, set: 0, clear: 1 },
            EriRmw { addr: 0xdc, set: 1, clear: 0 },
        ]
    );
    // Both steps hit the same register.
    assert_eq!(link::PACKET_FILTER_PULSE[0].addr, link::PACKET_FILTER_PULSE[1].addr);
    // Order matters and is clear-then-set: the first step must not set the bit.
    assert_eq!(link::PACKET_FILTER_PULSE[0].set, 0, "the first step is the clear");
    assert_eq!(link::PACKET_FILTER_PULSE[1].clear, 0, "the second step is the set");

    // THE POINT: starting from bit 0 already set, the pulse ends exactly where it began. Folding
    // the two into one write of the final value performs no reset and reports success.
    let start = 0x0000_00ff;
    let mid = link::w0w1(start, link::PACKET_FILTER_PULSE[0].set, link::PACKET_FILTER_PULSE[0].clear);
    let end = link::w0w1(mid, link::PACKET_FILTER_PULSE[1].set, link::PACKET_FILTER_PULSE[1].clear);
    assert_ne!(mid, start, "the intermediate state is the whole effect");
    assert_eq!(end, start, "and the final state is indistinguishable from doing nothing");
}

/// r8169_main.c:4963-:4970 — the patch runs on carrier UP only.
#[test]
fn the_patch_runs_only_when_the_carrier_comes_up() {
    assert!(link::patch_runs_on_link_change(true));
    assert!(
        !link::patch_runs_on_link_change(false),
        "carrier-down takes the runtime-idle branch and reaches none of the patch"
    );
}
