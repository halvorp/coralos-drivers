// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for PHY access. Expected values are LINUX literals, quoted with their lines.
//!
//! These assert the REGISTER TRAFFIC, in sequence, not just the returned value. The mandatory
//! 20 us settling delay on the legacy path is invisible in a return value — a test that checked
//! only `Ok(v)` would pass against a driver that dropped it, and that driver works until it is
//! driven quickly.

use r8169_core::init::Bus;
use r8169_core::mdio::{self, MdioError};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Op {
    R32(u32, u32),
    W32(u32, u32),
    Delay(u32),
}

/// A scripted PHY. `settles_after` is how many polls still show the busy/ready flag in its initial
/// state before the device flips it — so one fake expresses "instant", "slow" and "never".
struct Fake {
    log: Vec<Op>,
    polls: u32,
    settles_after: u32,
    /// The value the device presents in the low 16 bits once ready.
    data: u16,
    /// PHYAR and GPHY_OCP both carry the flag in bit 31, but with opposite senses per operation:
    /// a write waits for it to CLEAR, a read waits for it to SET.
    flag_when_settled: bool,
}

impl Fake {
    fn new(settles_after: u32, data: u16, flag_when_settled: bool) -> Self {
        Fake { log: Vec::new(), polls: 0, settles_after, data, flag_when_settled }
    }
    fn ops(&self) -> &[Op] { &self.log }
    fn delays(&self) -> Vec<u32> {
        self.log.iter().filter_map(|o| match o { Op::Delay(us) => Some(*us), _ => None }).collect()
    }
}

impl Bus for Fake {
    fn r8(&mut self, _reg: u32) -> u8 { 0 }
    fn w8(&mut self, _reg: u32, _val: u8) {}
    fn r16(&mut self, _reg: u32) -> u16 { 0 }
    fn w16(&mut self, _reg: u32, _val: u16) {}
    fn r32(&mut self, reg: u32) -> u32 {
        self.polls += 1;
        let settled = self.polls > self.settles_after;
        let flag = if settled == self.flag_when_settled { mdio::OCPAR_FLAG } else { 0 };
        let v = flag | self.data as u32;
        self.log.push(Op::R32(reg, v));
        v
    }
    fn w32(&mut self, reg: u32, val: u32) { self.log.push(Op::W32(reg, val)); }
    fn delay_us(&mut self, us: u32) { self.log.push(Op::Delay(us)); }
}

/// r8169_main.c:296 PHYAR = 0x60 · :415 GPHY_OCP = 0xb8 · :412 OCPAR_FLAG · :80 OCP_STD_PHY_BASE.
#[test]
fn the_register_offsets_and_flag_match_linux() {
    assert_eq!(mdio::PHYAR, 0x60);
    assert_eq!(mdio::GPHY_OCP, 0xb8);
    assert_eq!(mdio::OCPAR_FLAG, 0x8000_0000);
    assert_eq!(mdio::OCP_STD_PHY_BASE, 0xa400);
}

/// The two paths poll at DIFFERENT intervals: legacy 25x20us (:1295, :1309), OCP 25x10us
/// (:1118, :1133). Ten, not twenty — copying one interval to both halves or doubles a timeout.
#[test]
fn the_two_paths_have_different_poll_intervals() {
    assert_eq!((mdio::LEGACY_POLL_N, mdio::LEGACY_POLL_US), (25, 20));
    assert_eq!((mdio::OCP_POLL_N, mdio::OCP_POLL_US), (25, 10));
    assert_eq!(mdio::LEGACY_POST_US, 20);
}

/// :1293 — `PHYAR = 0x80000000 | (reg & 0x1f) << 16 | (value & 0xffff)`.
/// :1305 — a read is the same word with bit 31 CLEAR.
#[test]
fn the_phyar_command_words_match_linux() {
    assert_eq!(mdio::phyar_write_word(0x1f, 0xabcd), 0x8000_0000 | 0x1f << 16 | 0xabcd);
    assert_eq!(mdio::phyar_read_word(0x1f), 0x1f << 16);
    // `reg & 0x1f`: a register number past 5 bits must not bleed into the value or the flag.
    assert_eq!(mdio::phyar_write_word(0xff, 0), 0x8000_0000 | 0x1f << 16);
}

/// :1116 — `GPHY_OCP = OCPAR_FLAG | (reg << 15) | data`. FIFTEEN, not sixteen: `reg` is an even
/// BYTE address, so `reg << 15` is the register INDEX shifted into place.
#[test]
fn the_ocp_shift_is_fifteen_not_sixteen() {
    assert_eq!(mdio::ocp_write_word(0xa400, 0x1234), 0x8000_0000 | (0xa400 << 15) | 0x1234);
    assert_eq!(mdio::ocp_read_word(0xa400), 0xa400 << 15);
    // The distinction is not academic: <<16 would be a different word entirely.
    assert_ne!(mdio::ocp_read_word(0xa400), 0xa400 << 16);
}

/// :1101 — `rtl_ocp_reg_failure` is `reg & 0xffff0001`: 16 bits wide AND even.
#[test]
fn an_odd_or_oversized_ocp_register_is_refused_by_name() {
    assert!(!mdio::ocp_reg_is_invalid(0xa400));
    assert!(mdio::ocp_reg_is_invalid(0xa401), "odd registers are a mistake, not an address");
    assert!(mdio::ocp_reg_is_invalid(0x1_0000), "must fit in 16 bits");
    // The refusal NAMES the register, rather than collapsing into a generic timeout.
    let mut f = Fake::new(0, 0, false);
    assert_eq!(mdio::ocp_read(&mut f, 0xa401), Err(MdioError::InvalidOcpReg(0xa401)));
    assert!(f.ops().is_empty(), "a refused register must not touch the bus at all");
}

/// :1291-:1297 — write the word, poll for the flag to CLEAR, then delay 20 us before the next
/// command. Asserted as traffic: the delay is the whole point and is invisible in the return value.
#[test]
fn a_legacy_write_polls_then_settles_for_twenty_microseconds() {
    let mut f = Fake::new(0, 0, false); // flag already clear -> settles on the first poll
    assert_eq!(mdio::legacy_write(&mut f, 0x04, 0x0de1), Ok(()));
    assert_eq!(f.ops()[0], Op::W32(mdio::PHYAR, mdio::phyar_write_word(0x04, 0x0de1)));
    assert!(matches!(f.ops()[1], Op::R32(0x60, _)), "it must poll PHYAR: {:?}", f.ops());
    assert_eq!(f.delays(), vec![20], "exactly the one mandatory post-delay");
}

/// :1303-:1315 — the read waits for the flag to SET, takes the low 16 bits, and STILL delays 20 us.
#[test]
fn a_legacy_read_returns_the_low_word_and_still_settles() {
    let mut f = Fake::new(0, 0x5aa5, true);
    assert_eq!(mdio::legacy_read(&mut f, 0x02), Ok(0x5aa5));
    assert_eq!(f.ops()[0], Op::W32(mdio::PHYAR, mdio::phyar_read_word(0x02)));
    assert_eq!(f.delays(), vec![20], "the post-delay applies to reads too (:1315)");
}

/// :893 `rtl_loop_wait` checks BEFORE sleeping, so a device that is already ready costs no delay.
/// A loop written sleep-first would still return the right value and be slower on every access.
#[test]
fn a_ready_device_is_not_slept_on_first() {
    let mut f = Fake::new(0, 0x1234, true);
    assert_eq!(mdio::ocp_read(&mut f, 0xa400), Ok(0x1234));
    assert!(f.delays().is_empty(), "no poll delay for an already-ready device: {:?}", f.ops());
}

/// The budget is real: a device that never settles times out rather than spinning, and says so.
#[test]
fn a_device_that_never_settles_times_out_within_the_budget() {
    let mut f = Fake::new(u32::MAX, 0, true);
    assert_eq!(mdio::ocp_read(&mut f, 0xa400), Err(MdioError::Timeout));
    assert_eq!(f.delays().len(), mdio::OCP_POLL_N as usize, "exactly the budget, no more");
    assert!(f.delays().iter().all(|&d| d == 10), "the OCP path sleeps 10 us, not 20");
}

/// The OCP path has NO post-delay (:1111-:1119 has no udelay). Adding one would be invented.
#[test]
fn the_ocp_path_has_no_settling_delay() {
    let mut f = Fake::new(0, 0, false);
    assert_eq!(mdio::ocp_write(&mut f, 0xa400, 0x1111), Ok(()));
    assert!(f.delays().is_empty(), "the OCP path must not borrow the legacy path's delay");
}

/// :1250-:1268 — an MII register becomes `ocp_base + reg * 2`, with `reg -= 0x10` applied ONLY when
/// the page base is not the standard one.
#[test]
fn an_mii_register_maps_to_an_ocp_byte_address() {
    assert_eq!(mdio::ocp_addr_for_mii(mdio::OCP_STD_PHY_BASE, 0x04), 0xa400 + 0x08);
    // A non-standard page subtracts 0x10 FIRST — dropping that reads the wrong register.
    assert_eq!(mdio::ocp_addr_for_mii(0xa800, 0x14), 0xa800 + 0x08);
    assert_ne!(mdio::ocp_addr_for_mii(0xa800, 0x14), 0xa800 + 0x28);
}
