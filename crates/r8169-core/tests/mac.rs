// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the MAC-address cascade and the ERI command word. Expected values are LINUX
//! literals, quoted with their lines, and every arithmetic expectation is worked out by hand here
//! rather than recomputed from the constant under test.

use std::collections::BTreeMap;

use r8169_core::chip::MacVersion;
use r8169_core::eri::{self, EriError};
use r8169_core::init::Bus;
use r8169_core::mac::{self, HwMacSource, MacAddr, MacSource};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Op {
    R8(u32, u8),
    W32(u32, u32),
    R32(u32, u32),
    Delay(u32),
}

/// A scripted ERI interface and register file.
///
/// It PANICS on any byte register nobody scripted and on any 32-bit access outside ERIDR/ERIAR,
/// because a fake that invents a zero for an unexpected offset turns a port's wrong address into a
/// passing test.
struct Fake {
    log: Vec<Op>,
    bytes: BTreeMap<u32, u8>,
    /// Values handed back from ERIDR, in order, one per completed read.
    eridr: Vec<u32>,
    /// How many polls a transaction stays busy before it settles.
    settle_polls: u32,
    /// Never settles — used for the timeout vectors.
    never_settles: bool,
    polls_since_cmd: u32,
    last_cmd_was_write: bool,
}

impl Fake {
    fn new() -> Self {
        Fake {
            log: Vec::new(),
            bytes: BTreeMap::new(),
            eridr: Vec::new(),
            settle_polls: 0,
            never_settles: false,
            polls_since_cmd: 0,
            last_cmd_was_write: false,
        }
    }
    fn with_bytes(mut self, base: u32, vals: &[u8]) -> Self {
        for (i, v) in vals.iter().enumerate() {
            self.bytes.insert(base + i as u32, *v);
        }
        self
    }
    fn with_eridr(mut self, vals: &[u32]) -> Self {
        self.eridr = vals.to_vec();
        self.eridr.reverse();
        self
    }
    fn ops(&self) -> &[Op] {
        &self.log
    }
    fn writes(&self) -> Vec<(u32, u32)> {
        self.log
            .iter()
            .filter_map(|o| match o {
                Op::W32(r, v) => Some((*r, *v)),
                _ => None,
            })
            .collect()
    }
}

impl Bus for Fake {
    fn r8(&mut self, reg: u32) -> u8 {
        let v = *self
            .bytes
            .get(&reg)
            .unwrap_or_else(|| panic!("unscripted byte read at {reg:#x}"));
        self.log.push(Op::R8(reg, v));
        v
    }
    fn w8(&mut self, reg: u32, _val: u8) {
        panic!("unexpected w8 at {reg:#x}")
    }
    fn r16(&mut self, reg: u32) -> u16 {
        panic!("unexpected r16 at {reg:#x}")
    }
    fn w16(&mut self, reg: u32, _val: u16) {
        panic!("unexpected w16 at {reg:#x}")
    }
    fn r32(&mut self, reg: u32) -> u32 {
        let v = match reg {
            eri::ERIAR => {
                self.polls_since_cmd += 1;
                let busy = self.never_settles || self.polls_since_cmd <= self.settle_polls;
                // Busy means the flag is SET for a write (it clears on completion) and CLEAR for a
                // read (it sets on completion) — the opposite senses eri.rs waits on.
                let flag_set = if self.last_cmd_was_write { busy } else { !busy };
                if flag_set {
                    eri::ERIAR_FLAG
                } else {
                    0
                }
            }
            eri::ERIDR => self.eridr.pop().expect("unscripted ERIDR read"),
            other => panic!("unexpected r32 at {other:#x}"),
        };
        self.log.push(Op::R32(reg, v));
        v
    }
    fn w32(&mut self, reg: u32, val: u32) {
        if reg == eri::ERIAR {
            self.polls_since_cmd = 0;
            self.last_cmd_was_write = val & eri::ERIAR_WRITE_CMD != 0;
        }
        self.log.push(Op::W32(reg, val));
    }
    fn delay_us(&mut self, us: u32) {
        self.log.push(Op::Delay(us));
    }
}

// ─────────────────────────── ERI: the command word ───────────────────────────

/// r8169_main.c:387-:403 — every ERIAR define, against its literal.
#[test]
fn eri_defines_are_the_linux_literals() {
    assert_eq!(eri::ERIDR, 0x70);
    assert_eq!(eri::ERIAR, 0x74);
    assert_eq!(eri::ERIAR_FLAG, 0x8000_0000);
    assert_eq!(eri::ERIAR_WRITE_CMD, 0x8000_0000);
    assert_eq!(eri::ERIAR_READ_CMD, 0x0000_0000);
    assert_eq!(eri::ERIAR_ADDR_BYTE_ALIGN, 4);
    assert_eq!(eri::ERIAR_TYPE_SHIFT, 16);
    assert_eq!(eri::ERIAR_EXGMAC, 0x0000_0000);
    assert_eq!(eri::ERIAR_MSIX, 0x0001_0000);
    assert_eq!(eri::ERIAR_MASK_SHIFT, 12);
    assert_eq!(eri::ERIAR_MASK_0001, 0x0000_1000);
    assert_eq!(eri::ERIAR_MASK_0011, 0x0000_3000);
    assert_eq!(eri::ERIAR_MASK_0100, 0x0000_4000);
    assert_eq!(eri::ERIAR_MASK_0101, 0x0000_5000);
    assert_eq!(eri::ERIAR_MASK_1111, 0x0000_f000);
    assert_eq!(eri::ERI_POLL_N, 100);
    assert_eq!(eri::ERI_POLL_US, 100);
}

/// r8169_main.c:396-:397 — ASF and OOB are the SAME encoding under two names.
///
/// This is not pedantry. `adjust_ocp_cmd` tests the numeric type, so it fires for a caller who
/// believes it is asking for ASF. A port that gives the type distinct discriminants would make
/// that vector unwritable and the behaviour a surprise on silicon.
#[test]
fn asf_and_oob_are_one_encoding_under_two_names() {
    assert_eq!(eri::ERIAR_ASF, eri::ERIAR_OOB);
    assert_eq!(eri::ERIAR_OOB, 0x0002_0000);
    assert_eq!(
        eri::adjust_ocp_cmd(0, eri::ERIAR_ASF, eri::ADJUST_OCP_MAC_VER),
        eri::adjust_ocp_cmd(0, eri::ERIAR_OOB, eri::ADJUST_OCP_MAC_VER),
    );
}

/// r8169_main.c:389-:391 — bit 31 is the write command going in and the busy flag coming back, and
/// the read command contributes nothing at all.
#[test]
fn bit31_is_both_the_write_command_and_the_busy_flag() {
    assert_eq!(eri::ERIAR_WRITE_CMD, eri::ERIAR_FLAG);
    assert_eq!(eri::ERIAR_READ_CMD, 0);
    // A read command word therefore carries bit 31 clear; nothing else distinguishes it.
    assert_eq!(eri::read_cmd_word(0xe0, eri::ERIAR_EXGMAC) & eri::ERIAR_FLAG, 0);
    assert_ne!(
        eri::write_cmd_word(0xe0, eri::ERIAR_MASK_1111, eri::ERIAR_EXGMAC) & eri::ERIAR_FLAG,
        0
    );
}

/// r8169_main.c:1050 and :1070 — the two command words, assembled by hand.
#[test]
fn command_words_match_hand_assembly() {
    // write: WRITE_CMD | EXGMAC | MASK_1111 | 0xe0
    //      = 0x80000000 | 0x00000000 | 0x0000f000 | 0x000000e0
    assert_eq!(
        eri::write_cmd_word(0xe0, eri::ERIAR_MASK_1111, eri::ERIAR_EXGMAC),
        0x8000_f0e0
    );
    // read: READ_CMD | EXGMAC | MASK_1111 | 0xe4 = 0x0000f0e4
    assert_eq!(eri::read_cmd_word(0xe4, eri::ERIAR_EXGMAC), 0x0000_f0e4);
    // A non-EXGMAC type lands in bits 16+: MSIX | MASK_0011 | 0x08
    assert_eq!(
        eri::write_cmd_word(0x08, eri::ERIAR_MASK_0011, eri::ERIAR_MSIX),
        0x8001_3008
    );
}

/// r8169_main.c:1070 — a read ALWAYS asserts all four byte enables; there is no mask parameter.
#[test]
fn a_read_command_always_carries_the_full_mask() {
    for addr in [0x00u32, 0xe0, 0xe4, 0x1bc] {
        let w = eri::read_cmd_word(addr, eri::ERIAR_EXGMAC);
        assert_eq!(
            w & (0xf << eri::ERIAR_MASK_SHIFT),
            eri::ERIAR_MASK_1111,
            "addr {addr:#x}"
        );
    }
}

/// r8169_main.c:1035-:1040 — `*cmd |= 0xf70 << 18`, for OOB type on VER_52 only.
#[test]
fn the_ocp_adjustment_fires_only_for_oob_on_version_52() {
    // 0xf70 << 18 by hand: 0xf70 << 16 = 0x0f70_0000, two more shifts = 0x3dc0_0000.
    assert_eq!(eri::ADJUST_OCP_OR, 0x3dc0_0000);
    assert_eq!(eri::ADJUST_OCP_MAC_VER, 52);

    let base = eri::read_cmd_word(0xe0, eri::ERIAR_OOB);
    assert_eq!(eri::adjust_ocp_cmd(base, eri::ERIAR_OOB, 52), base | 0x3dc0_0000);
    // Wrong version: untouched.
    assert_eq!(eri::adjust_ocp_cmd(base, eri::ERIAR_OOB, 51), base);
    assert_eq!(eri::adjust_ocp_cmd(base, eri::ERIAR_OOB, 53), base);
    // Wrong type on the right version: untouched.
    let ex = eri::read_cmd_word(0xe0, eri::ERIAR_EXGMAC);
    assert_eq!(eri::adjust_ocp_cmd(ex, eri::ERIAR_EXGMAC, 52), ex);
    assert_eq!(eri::adjust_ocp_cmd(ex, eri::ERIAR_MSIX, 52), ex);
}

/// r8169_main.c:1053 — `WARN(addr & 3 || !mask)` then a bare return. Two causes, named apart, and
/// the bus is not touched at all.
#[test]
fn a_write_refusal_names_its_cause_and_touches_nothing() {
    for bad in [0x01u32, 0x02, 0x03, 0xe1, 0xe2, 0xe3] {
        let mut f = Fake::new();
        assert_eq!(
            eri::write(&mut f, bad, eri::ERIAR_MASK_1111, 0, eri::ERIAR_EXGMAC, 40),
            Err(EriError::UnalignedAddr(bad))
        );
        assert!(f.ops().is_empty(), "refused write still drove the bus for {bad:#x}");
    }

    let mut f = Fake::new();
    assert_eq!(
        eri::write(&mut f, 0xe0, 0, 0x1234, eri::ERIAR_EXGMAC, 40),
        Err(EriError::EmptyMask)
    );
    assert!(f.ops().is_empty(), "refused write still drove the bus");

    // And an aligned address with a real mask is NOT refused — the control that keeps the two
    // vectors above from passing against a function that refuses everything.
    let mut f = Fake::new();
    assert_eq!(
        eri::write(&mut f, 0xe0, eri::ERIAR_MASK_1111, 0x1234, eri::ERIAR_EXGMAC, 40),
        Ok(())
    );
    assert!(!f.ops().is_empty());
}

/// r8169_main.c:1056-:1060 — ERIDR is loaded BEFORE the command word reaches ERIAR.
#[test]
fn a_write_loads_the_data_register_before_the_command() {
    let mut f = Fake::new();
    f.settle_polls = 2;
    eri::write(&mut f, 0xf4, eri::ERIAR_MASK_1111, 0xdead_beef, eri::ERIAR_EXGMAC, 34).unwrap();
    let w = f.writes();
    assert_eq!(w[0], (eri::ERIDR, 0xdead_beef), "ERIDR must be loaded first");
    assert_eq!(w[1], (eri::ERIAR, 0x8000_f0f4), "then the command word");
    assert_eq!(w.len(), 2, "no further writes");
    // Two busy polls, each followed by the 100 us delay, then the settled poll.
    let delays: Vec<u32> = f
        .ops()
        .iter()
        .filter_map(|o| match o {
            Op::Delay(us) => Some(*us),
            _ => None,
        })
        .collect();
    assert_eq!(delays, vec![100, 100]);
}

/// r8169_main.c:1072-:1077 — the command word goes out first and ERIDR is read only after the flag
/// has SET.
#[test]
fn a_read_takes_the_data_register_only_after_the_flag_sets() {
    let mut f = Fake::new().with_eridr(&[0x1122_3344]);
    f.settle_polls = 1;
    let v = eri::read_exgmac(&mut f, 0xe0, 40).unwrap();
    assert_eq!(v, 0x1122_3344);

    let ops = f.ops();
    assert_eq!(ops[0], Op::W32(eri::ERIAR, 0x0000_f0e0));
    assert_eq!(ops[1], Op::R32(eri::ERIAR, 0), "first poll: not ready");
    assert_eq!(ops[2], Op::Delay(100));
    assert_eq!(ops[3], Op::R32(eri::ERIAR, eri::ERIAR_FLAG), "second poll: ready");
    assert_eq!(ops[4], Op::R32(eri::ERIDR, 0x1122_3344), "ERIDR read LAST");
    assert_eq!(ops.len(), 5);
}

/// r8169_main.c:1076-:1077 — the timeout, and the sentinel Linux returns in its place.
#[test]
fn a_read_timeout_is_named_and_its_linux_sentinel_is_caught_downstream() {
    let mut f = Fake::new();
    f.never_settles = true;
    assert_eq!(eri::read_exgmac(&mut f, 0xe0, 40), Err(EriError::Timeout));
    let polls = f
        .ops()
        .iter()
        .filter(|o| matches!(o, Op::R32(r, _) if *r == eri::ERIAR))
        .count();
    assert_eq!(polls as u32, eri::ERI_POLL_N, "the full budget, no more and no less");

    // Linux returns ~0 instead of an error. Six bytes of it is ff:ff:ff:ff:ff:ff, which the address
    // validator rejects — but only because broadcast has the multicast bit set, not because anyone
    // checked for the sentinel.
    assert_eq!(eri::LINUX_READ_TIMEOUT_SENTINEL, 0xffff_ffff);
    let poisoned = mac::mac_from_eri_words(
        eri::LINUX_READ_TIMEOUT_SENTINEL,
        eri::LINUX_READ_TIMEOUT_SENTINEL,
    );
    assert_eq!(poisoned, [0xff; 6]);
    assert!(mac::is_multicast(&poisoned), "the multicast bit is what catches it");
    assert!(!mac::is_valid(&poisoned));
}

// ─────────────────────────── the address cascade ───────────────────────────

/// r8169_main.c:866-:871 — three conditions, and 39 is a HOLE inside the range.
#[test]
fn is_8168evl_up_has_a_hole_at_39_not_a_boundary() {
    for v in 0u8..=70 {
        let expect = v >= 34 && v != 39 && v <= 52;
        assert_eq!(mac::is_8168evl_up(MacVersion(v)), expect, "ver {v}");
    }
    assert!(mac::is_8168evl_up(MacVersion(38)));
    assert!(!mac::is_8168evl_up(MacVersion(39)), "39 is excluded from inside the range");
    assert!(mac::is_8168evl_up(MacVersion(40)));
}

/// r8169_main.c:861-:864.
#[test]
fn is_8125_is_a_plain_lower_bound_at_61() {
    for v in 0u8..=70 {
        assert_eq!(mac::is_8125(MacVersion(v)), v >= 61, "ver {v}");
    }
}

/// r8169_main.c:5346 — the FOURTH condition. Version 34 passes `rtl_is_8168evl_up` and is then
/// excluded by name, so it has no hardware source at all.
#[test]
fn version_34_is_excluded_from_the_eri_source_it_otherwise_qualifies_for() {
    assert!(
        mac::is_8168evl_up(MacVersion(34)),
        "34 is the predicate's own lower bound"
    );
    assert_eq!(
        mac::hw_mac_source(MacVersion(34)),
        HwMacSource::None,
        "and is excluded from ERI by the extra clause at :5346"
    );
}

/// r8169_main.c:5343-:5356 — the whole source table, every version.
#[test]
fn the_hardware_source_table_covers_every_version() {
    for v in 0u8..=70 {
        let expect = if v >= 34 && v != 39 && v <= 52 && v != 34 {
            HwMacSource::Eri
        } else if v >= 61 {
            HwMacSource::Mac0Bkp
        } else {
            HwMacSource::None
        };
        assert_eq!(mac::hw_mac_source(MacVersion(v)), expect, "ver {v}");
    }
    // Named spot checks, so a wholesale change to the expression above cannot make this vacuous.
    assert_eq!(mac::hw_mac_source(MacVersion(35)), HwMacSource::Eri);
    assert_eq!(mac::hw_mac_source(MacVersion(39)), HwMacSource::None);
    assert_eq!(mac::hw_mac_source(MacVersion(52)), HwMacSource::Eri);
    assert_eq!(mac::hw_mac_source(MacVersion(53)), HwMacSource::None);
    assert_eq!(mac::hw_mac_source(MacVersion(61)), HwMacSource::Mac0Bkp);
}

/// r8169_main.c:5348-:5351 — le32 at 0xe0, le SIXTEEN at 0xe4.
#[test]
fn the_high_eri_word_contributes_only_two_bytes() {
    // 0xe0 = 0x33221100, 0xe4 = 0xbbaa5544 -> 00:11:22:33:44:55, the 0xbbaa discarded.
    assert_eq!(
        mac::mac_from_eri_words(0x3322_1100, 0xbbaa_5544),
        [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]
    );
    // Changing only the discarded half changes nothing.
    assert_eq!(
        mac::mac_from_eri_words(0x3322_1100, 0x0000_5544),
        mac::mac_from_eri_words(0x3322_1100, 0xffff_5544)
    );
}

/// r8169_main.c:5346-:5351 — the ERI read path, as traffic.
#[test]
fn the_eri_source_reads_0xe0_then_0xe4() {
    let mut f = Fake::new().with_eridr(&[0x3322_1100, 0x0000_5544]);
    let a = mac::read_mac_address(&mut f, MacVersion(40)).unwrap();
    assert_eq!(a, [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    let cmds: Vec<u32> = f
        .writes()
        .iter()
        .filter(|(r, _)| *r == eri::ERIAR)
        .map(|(_, v)| *v)
        .collect();
    assert_eq!(cmds, vec![0x0000_f0e0, 0x0000_f0e4], "low word first, then high");
}

/// r8169_main.c:880-:886 — six separate BYTE reads at consecutive offsets, and :5355 points them at
/// MAC0_BKP.
#[test]
fn the_backup_source_reads_six_bytes_from_0x19e0() {
    assert_eq!(mac::MAC0_BKP, 0x19e0);
    let mut f = Fake::new().with_bytes(0x19e0, &[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    let a = mac::read_mac_address(&mut f, MacVersion(61)).unwrap();
    assert_eq!(a, [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    assert_eq!(
        f.ops(),
        &[
            Op::R8(0x19e0, 0x00),
            Op::R8(0x19e1, 0x11),
            Op::R8(0x19e2, 0x22),
            Op::R8(0x19e3, 0x33),
            Op::R8(0x19e4, 0x44),
            Op::R8(0x19e5, 0x55),
        ]
    );
}

/// r8169_main.c:5343-:5356 — the `void` function with no `else`. For a part with no hardware
/// source it touches NOTHING, and the caller's zero-initialised buffer is what carries that fact
/// forward. The fake panics on any unscripted access, so an accidental read fails here loudly.
#[test]
fn a_part_with_no_hardware_source_is_not_consulted_at_all() {
    for v in [1u8, 33, 34, 39, 53, 60] {
        let mut f = Fake::new();
        assert_eq!(mac::read_mac_address(&mut f, MacVersion(v)), None, "ver {v}");
        assert!(f.ops().is_empty(), "ver {v} drove the bus for an absent source");
    }
}

/// r8169_main.c:5559-:5583 — the cascade, as a value, with its length pinned by the array type.
#[test]
fn the_cascade_has_four_sources_in_this_order() {
    assert_eq!(mac::MAC_SOURCE_ORDER.len(), 4);
    assert_eq!(
        mac::MAC_SOURCE_ORDER,
        [
            MacSource::Platform,
            MacSource::Hardware,
            MacSource::Mac0,
            MacSource::Random
        ]
    );
}

const GOOD: MacAddr = [0x02, 0x11, 0x22, 0x33, 0x44, 0x55];
const OTHER: MacAddr = [0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0xee];
const THIRD: MacAddr = [0x02, 0x01, 0x02, 0x03, 0x04, 0x05];

/// r8169_main.c:5565-:5567 — THE PLATFORM ADDRESS IS NOT VALIDATED. `goto done` jumps past both
/// `is_valid_ether_addr` checks.
#[test]
fn a_platform_address_is_accepted_without_validation() {
    // A multicast platform address — invalid by every other source's standard — still wins.
    let multicast: MacAddr = [0x01, 0x00, 0x5e, 0x00, 0x00, 0x01];
    assert!(!mac::is_valid(&multicast));
    assert_eq!(
        mac::select_source(Some(multicast), Some(GOOD), OTHER),
        (MacSource::Platform, Some(multicast)),
        "validating the platform source would reject a board's deliberate override"
    );
    // So does an all-zero one.
    let zero: MacAddr = [0; 6];
    assert_eq!(
        mac::select_source(Some(zero), Some(GOOD), OTHER),
        (MacSource::Platform, Some(zero))
    );
}

/// r8169_main.c:5569-:5580 — the fallthrough, one step at a time.
#[test]
fn the_cascade_falls_through_exactly_as_far_as_it_must() {
    // Hardware wins when the platform has nothing.
    assert_eq!(
        mac::select_source(None, Some(GOOD), OTHER),
        (MacSource::Hardware, Some(GOOD))
    );
    // An absent hardware source falls to MAC0.
    assert_eq!(
        mac::select_source(None, None, OTHER),
        (MacSource::Mac0, Some(OTHER))
    );
    // An INVALID hardware source also falls to MAC0 — the check at :5570 is on the value, not on
    // whether the read happened.
    assert_eq!(
        mac::select_source(None, Some([0; 6]), OTHER),
        (MacSource::Mac0, Some(OTHER))
    );
    assert_eq!(
        mac::select_source(None, Some([0xff; 6]), THIRD),
        (MacSource::Mac0, Some(THIRD))
    );
    // Nothing valid anywhere: random, and no address to hand back.
    assert_eq!(mac::select_source(None, None, [0; 6]), (MacSource::Random, None));
    assert_eq!(
        mac::select_source(None, Some([0; 6]), [0xff; 6]),
        (MacSource::Random, None)
    );
}

/// include/linux/etherdevice.h — the validator, including the case its own comment explains away.
#[test]
fn address_validity_is_not_multicast_and_not_zero() {
    assert!(mac::is_valid(&GOOD));
    assert!(!mac::is_valid(&[0; 6]), "all zero");
    assert!(!mac::is_valid(&[0xff; 6]), "broadcast, caught as multicast");
    assert!(!mac::is_valid(&[0x01, 0, 0, 0, 0, 1]), "multicast bit in octet 0");
    assert!(mac::is_valid(&[0x02, 0, 0, 0, 0, 0]), "locally administered, unicast");
    // The multicast bit is in the FIRST octet only.
    assert!(mac::is_valid(&[0x02, 0x01, 0x01, 0x01, 0x01, 0x01]));
}

// ─────────────────────────── writing the address back ───────────────────────────

/// r8169_main.c:2503-:2509 — the second pair is the SAME address at a two-byte offset.
#[test]
fn the_exgmac_copy_is_shifted_by_two_bytes_not_repeated() {
    let a: MacAddr = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
    // By hand: le32(a[0..4]) = 0x33221100 · le16(a[4..6]) = 0x5544
    //          le16(a[0..2]) << 16 = 0x11000000 · le32(a[2..6]) = 0x55443322
    assert_eq!(
        mac::rar_exgmac_words(&a),
        [
            (0xe0, 0x3322_1100),
            (0xe4, 0x0000_5544),
            (0xf0, 0x1100_0000),
            (0xf4, 0x5544_3322),
        ]
    );
    // The two pairs are NOT equal — the assumption that would make them so is the defect.
    let w = mac::rar_exgmac_words(&a);
    assert_ne!(w[0].1, w[3].1, "0xf4 is not a copy of 0xe0");
    assert_ne!(w[1].1, w[2].1, "0xf0 is not a copy of 0xe4");
    assert_eq!(w[2].1 & 0xffff, 0, "the low half of 0xf0 is written as zero");
}

/// r8169_main.c:2559-:2574 — MAC4 first, each half followed by a ChipCmd read that flushes it.
#[test]
fn setting_the_address_writes_the_high_half_first_and_commits_each() {
    let mut f = Fake::new().with_bytes(0x37, &[0x0c]);
    let a: MacAddr = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
    mac::rar_set(&mut f, &a, MacVersion(40));
    assert_eq!(
        f.ops(),
        &[
            // A 32-bit write of a 16-bit value: the upper two bytes are zeroed deliberately.
            Op::W32(4, 0x0000_5544),
            Op::R8(0x37, 0x0c),
            Op::W32(0, 0x3322_1100),
            Op::R8(0x37, 0x0c),
        ],
        "MAC4 then commit then MAC0 then commit, and nothing else for a non-34 part"
    );
}

/// r8169_main.c:2569-:2570 — version 34, and only version 34, also takes the four ERI writes.
#[test]
fn version_34_additionally_writes_the_address_through_eri() {
    let mut f = Fake::new().with_bytes(0x37, &[0x0c]);
    let a: MacAddr = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
    mac::rar_set(&mut f, &a, MacVersion(34));
    let cmds: Vec<(u32, u32)> = f
        .writes()
        .into_iter()
        .filter(|(r, _)| *r == eri::ERIDR || *r == eri::ERIAR)
        .collect();
    assert_eq!(
        cmds,
        vec![
            (eri::ERIDR, 0x3322_1100),
            (eri::ERIAR, 0x8000_f0e0),
            (eri::ERIDR, 0x0000_5544),
            (eri::ERIAR, 0x8000_f0e4),
            (eri::ERIDR, 0x1100_0000),
            (eri::ERIAR, 0x8000_f0f0),
            (eri::ERIDR, 0x5544_3322),
            (eri::ERIAR, 0x8000_f0f4),
        ],
        "four writes, in order, each ERIDR-then-ERIAR"
    );

    // THE CONTROL: version 34 is the ONLY part that gets them. The neighbours do not.
    for v in [33u8, 35, 40, 52, 61] {
        let mut f = Fake::new().with_bytes(0x37, &[0x0c]);
        mac::rar_set(&mut f, &a, MacVersion(v));
        assert!(
            !f.writes().iter().any(|(r, _)| *r == eri::ERIAR),
            "ver {v} must not take the exgmac path"
        );
    }
}

/// The read side and the write side disagree about version 34 ON PURPOSE, and this pins the pair so
/// that "tidying" one to match the other fails here.
#[test]
fn version_34_reads_no_eri_address_but_writes_one() {
    assert_eq!(mac::hw_mac_source(MacVersion(34)), HwMacSource::None);
    let mut f = Fake::new().with_bytes(0x37, &[0x0c]);
    mac::rar_set(&mut f, &GOOD, MacVersion(34));
    assert!(f.writes().iter().any(|(r, _)| *r == eri::ERIAR));
}
