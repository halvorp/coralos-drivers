// SPDX-License-Identifier: GPL-2.0-only
//! Link state, and the per-chip patch that runs when it changes.
//!
//! Ported from Linux `drivers/net/ethernet/realtek/r8169_main.c`:
//!   * the `rtl8169_PHYstatus` bit block (:557-:565) behind `PHYstatus` (:297);
//!   * `rtl_link_chg_patch` (:1671-:1704);
//!   * `rtl_reset_packet_filter` (:1613-:1617) and `rtl_w0w1_eri` (:1084-:1089);
//!   * `r8169_phylink_handler` (:4959-:4972) — which decides WHEN the patch runs;
//!   * the `LinkChg` interrupt (:460, :4865) and the default mask (:5316).
//!
//! Copyright (c) a lot of people. See the Linux source for the full authorship of r8169.
//!
//! *** LINUX NEVER READS `PHYstatus` IN THIS TREE. *** The offset is defined at :297, the bit block
//! is labelled by a comment at :557, and there is no third occurrence: modern r8169 takes speed and
//! duplex from phylib (`tp->phydev->speed`), so those bit names are carried by the enum and
//! exercised by nothing. That matters for CoralOS, which has no phylib and must therefore read the
//! register itself. The decode below is transcribed from the enum and is NOT corroborated by the
//! reference driver's own behaviour — the one place in this port where "Linux does it this way" is
//! not available as evidence, said here rather than left for a reader to discover.

use crate::chip::MacVersion;
use crate::eri;

/// The eight `rtl8169_PHYstatus` bits (r8169_main.c:558-:565). Together they cover all of `0xff`.
pub const TBI_ENABLE: u8 = 0x80;
pub const TX_FLOW_CTRL: u8 = 0x40;
pub const RX_FLOW_CTRL: u8 = 0x20;
/// `_1000bpsF` — the trailing F is FULL DUPLEX. Gigabit on this part is full duplex by the bit's
/// own name, and [`FULL_DUP`] below is the 10/100 duplex indication. Reading duplex from
/// `FULL_DUP` alone reports half duplex on a gigabit link that has no reason to set it.
pub const GBPS_FULL: u8 = 0x10;
pub const MBPS_100: u8 = 0x08;
pub const MBPS_10: u8 = 0x04;
pub const LINK_STATUS: u8 = 0x02;
pub const FULL_DUP: u8 = 0x01;

/// The three speed bits, as a set. Exactly one is expected at a time.
pub const SPEED_BITS: u8 = GBPS_FULL | MBPS_100 | MBPS_10;

/// What the register says the link is running at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Speed {
    /// No speed bit set.
    Unresolved,
    M10,
    M100,
    G1000,
    /// MORE THAN ONE SPEED BIT SET. The register is not supposed to do this, and a decoder that
    /// tests the bits in some order silently reports whichever it happened to check first. Named,
    /// with the offending value, so the caller learns that the PHY said something impossible rather
    /// than that the link is slow.
    Conflicting(u8),
}

/// A decoded `PHYstatus`, with the raw byte kept.
///
/// The raw byte is retained because this register is the one place in the port with no corroborating
/// use in the reference: a value nobody anticipated must reach a log intact rather than be reduced
/// to the fields somebody thought of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PhyStatus {
    pub raw: u8,
    pub up: bool,
    pub speed: Speed,
    pub full_duplex: bool,
    pub tx_pause: bool,
    pub rx_pause: bool,
    pub tbi: bool,
}

/// Decode `PHYstatus` (r8169_main.c:297, bits at :558-:565).
pub fn decode_phy_status(raw: u8) -> PhyStatus {
    let speed = match raw & SPEED_BITS {
        0 => Speed::Unresolved,
        GBPS_FULL => Speed::G1000,
        MBPS_100 => Speed::M100,
        MBPS_10 => Speed::M10,
        _ => Speed::Conflicting(raw),
    };
    PhyStatus {
        raw,
        up: raw & LINK_STATUS != 0,
        speed,
        // Gigabit's bit already means full duplex; FULL_DUP carries the 10/100 case.
        full_duplex: speed == Speed::G1000 || raw & FULL_DUP != 0,
        tx_pause: raw & TX_FLOW_CTRL != 0,
        rx_pause: raw & RX_FLOW_CTRL != 0,
        tbi: raw & TBI_ENABLE != 0,
    }
}

/// One ERI write of `rtl_link_chg_patch`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EriWrite {
    pub addr: u32,
    pub mask: u32,
    pub val: u32,
}

/// What a link change costs on this part, at this speed.
///
/// `writes` is a FIXED-SIZE array of two options, which makes the three distinct counts the
/// reference actually produces — zero, one and two writes — a compile-time shape rather than a
/// length somebody has to keep in step with a comment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LinkChgPatch {
    pub writes: [Option<EriWrite>; 2],
    pub reset_packet_filter: bool,
}

impl LinkChgPatch {
    pub const NONE: LinkChgPatch =
        LinkChgPatch { writes: [None, None], reset_packet_filter: false };

    pub fn len(&self) -> usize {
        self.writes.iter().filter(|w| w.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

const fn w(addr: u32, mask: u32, val: u32) -> Option<EriWrite> {
    Some(EriWrite { addr, mask, val })
}

/// `rtl_link_chg_patch` (r8169_main.c:1671-:1704), as a value.
///
/// FOUR GROUPS OF VERSIONS, AND THE TWO THAT LOOK ALIKE ARE NOT. {34, 38} has THREE speed branches
/// and ends with a packet-filter reset; {35, 36} has TWO and does not. Their gigabit arms are
/// identical and their fallback arms are identical, which is exactly what makes merging them
/// tempting — and the merge is wrong twice over: {34, 38} at 100 Mbit writes `0x1f`/`0x05`, while
/// {35, 36} at 100 Mbit falls into the fallback and writes `0x1f`/`0x3f`, and {35, 36} would
/// acquire a filter reset the reference does not perform.
///
/// {37} is different again: a narrower byte-enable mask, different registers, and — the asymmetry
/// a table-shaped port loses — a fallback branch that writes ONE register rather than two, leaving
/// 0x1dc holding whatever the 10 Mbit branch last put there.
///
/// Every other version returns [`LinkChgPatch::NONE`].
pub fn link_chg_patch(v: MacVersion, speed: Speed) -> LinkChgPatch {
    const M1111: u32 = eri::ERIAR_MASK_1111;
    const M0011: u32 = eri::ERIAR_MASK_0011;
    match v.0 {
        34 | 38 => match speed {
            Speed::G1000 => LinkChgPatch {
                writes: [w(0x1bc, M1111, 0x0000_0011), w(0x1dc, M1111, 0x0000_0005)],
                reset_packet_filter: true,
            },
            Speed::M100 => LinkChgPatch {
                writes: [w(0x1bc, M1111, 0x0000_001f), w(0x1dc, M1111, 0x0000_0005)],
                reset_packet_filter: true,
            },
            _ => LinkChgPatch {
                writes: [w(0x1bc, M1111, 0x0000_001f), w(0x1dc, M1111, 0x0000_003f)],
                reset_packet_filter: true,
            },
        },
        35 | 36 => match speed {
            Speed::G1000 => LinkChgPatch {
                writes: [w(0x1bc, M1111, 0x0000_0011), w(0x1dc, M1111, 0x0000_0005)],
                reset_packet_filter: false,
            },
            _ => LinkChgPatch {
                writes: [w(0x1bc, M1111, 0x0000_001f), w(0x1dc, M1111, 0x0000_003f)],
                reset_packet_filter: false,
            },
        },
        37 => match speed {
            Speed::M10 => LinkChgPatch {
                writes: [w(0x1d0, M0011, 0x4d02), w(0x1dc, M0011, 0x0060a)],
                reset_packet_filter: false,
            },
            _ => LinkChgPatch {
                writes: [w(0x1d0, M0011, 0x0000), None],
                reset_packet_filter: false,
            },
        },
        _ => LinkChgPatch::NONE,
    }
}

/// A read-modify-write through `rtl_w0w1_eri` (r8169_main.c:1084-:1089): `(val & ~m) | p`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EriRmw {
    pub addr: u32,
    /// `p` — bits to set.
    pub set: u32,
    /// `m` — bits to clear.
    pub clear: u32,
}

/// `rtl_w0w1_eri`'s arithmetic (:1086-:1088). SET WINS: a bit named in both `p` and `m` ends up
/// set, because the clear happens first and the set is an OR over the result.
pub fn w0w1(old: u32, set: u32, clear: u32) -> u32 {
    (old & !clear) | set
}

/// `rtl_reset_packet_filter` (r8169_main.c:1613-:1617) — clear bit 0 of ERI 0xdc, then set it.
///
/// *** A PULSE, NOT A VALUE. *** The end state is identical to the start state whenever the bit was
/// already set, so a port that computes the final register contents and writes them once performs
/// no reset at all, and every write still succeeds. Both accesses are the effect; the value is not.
/// This is the same shape as the SWR block's repeated write in `phy.rs`.
pub const PACKET_FILTER_PULSE: [EriRmw; 2] = [
    EriRmw { addr: 0xdc, set: 0, clear: 1 << 0 },
    EriRmw { addr: 0xdc, set: 1 << 0, clear: 0 },
];

/// `r8169_phylink_handler` (r8169_main.c:4959-:4972) — the patch runs only when the carrier is UP.
///
/// A link change is delivered for both directions. Running the patch on the way DOWN would program
/// the ERI registers for a speed that no longer applies, and the reference explicitly does not:
/// carrier-down takes the runtime-idle branch and reaches none of it.
pub fn patch_runs_on_link_change(carrier_ok: bool) -> bool {
    carrier_ok
}
