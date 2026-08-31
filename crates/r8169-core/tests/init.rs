// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the ported reset and config-unlock sequences.
//!
//! These assert the REGISTER TRAFFIC — which registers, in what order, with what values — not just
//! the return code. sdhci-core in this repo had to be rewritten for exactly that reason: its first
//! recovery vectors checked outcomes and so passed against a driver that wrote nothing.

use r8169_core::init::{self, Bus, ResetError};
use r8169_core::regs::{self, cfg9346, chip_cmd};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Op {
    R8(u32, u8),
    W8(u32, u8),
    R16(u32, u16),
    W16(u32, u16),
    Delay(u32),
}

/// A scripted register file. `reset_clears_after` says how many reads of ChipCmd still show the
/// reset bit set before the chip drops it — that is how a real self-clearing bit behaves, and it
/// lets one fake express "instant", "slow", and "never".
struct Fake {
    log: Vec<Op>,
    chipcmd_reads: u32,
    reset_clears_after: u32,
}

impl Fake {
    fn new(reset_clears_after: u32) -> Self {
        Fake { log: Vec::new(), chipcmd_reads: 0, reset_clears_after }
    }
    fn writes_to(&self, reg: u32) -> Vec<u8> {
        self.log.iter().filter_map(|o| match o {
            Op::W8(r, v) if *r == reg => Some(*v),
            _ => None,
        }).collect()
    }
}

impl Bus for Fake {
    fn r8(&mut self, reg: u32) -> u8 {
        let v = if reg == regs::CHIP_CMD {
            self.chipcmd_reads += 1;
            if self.chipcmd_reads > self.reset_clears_after { 0 } else { chip_cmd::RESET }
        } else {
            0
        };
        self.log.push(Op::R8(reg, v));
        v
    }
    fn w8(&mut self, reg: u32, val: u8) {
        self.log.push(Op::W8(reg, val));
    }
    fn r16(&mut self, reg: u32) -> u16 {
        self.log.push(Op::R16(reg, 0));
        0
    }
    fn w16(&mut self, reg: u32, val: u16) {
        self.log.push(Op::W16(reg, val));
    }
    // This fake drives no 32-bit register. Panicking rather than returning 0 keeps a
    // future test that reaches one from passing against an answer nobody wrote.
    fn r32(&mut self, reg: u32) -> u32 { panic!("unexpected 32-bit read of {reg:#x}") }
    fn w32(&mut self, reg: u32, val: u32) { panic!("unexpected 32-bit write {val:#x} -> {reg:#x}") }
    fn delay_us(&mut self, us: u32) {
        self.log.push(Op::Delay(us));
    }
}

/// rtl_hw_reset (r8169_main.c:2671-2676): write CmdReset to ChipCmd, then poll it low.
#[test]
fn reset_writes_cmdreset_then_polls_chipcmd_low() {
    let mut f = Fake::new(0); // clears on the first read
    let polls = init::hw_reset(&mut f).expect("reset must succeed");
    assert_eq!(polls, 1);
    assert_eq!(
        f.log[0],
        Op::W8(regs::CHIP_CMD, chip_cmd::RESET),
        "the FIRST action must be writing CmdReset to ChipCmd"
    );
    assert_eq!(f.log[1], Op::R8(regs::CHIP_CMD, 0), "then it must READ ChipCmd back");
    assert_eq!(f.writes_to(regs::CHIP_CMD), vec![chip_cmd::RESET], "exactly one write");
}

/// A chip that takes its time is normal. The poll count is returned so a caller can tell a healthy
/// slow reset from an instant one.
#[test]
fn a_slow_reset_is_waited_out_and_the_poll_count_is_reported() {
    let mut f = Fake::new(4); // still set for four reads
    let polls = init::hw_reset(&mut f).expect("a slow reset must still succeed");
    assert_eq!(polls, 5);
    let delays = f.log.iter().filter(|o| matches!(o, Op::Delay(_))).count();
    assert_eq!(delays, 4, "one delay between each pair of polls, none after the last");
    assert!(f.log.contains(&Op::Delay(regs::RESET_POLL_INTERVAL_US)));
}

/// A chip that never clears must TIME OUT, not spin forever, and must not report success.
#[test]
fn a_reset_that_never_completes_times_out_within_the_linux_budget() {
    let mut f = Fake::new(u32::MAX); // never clears
    assert_eq!(init::hw_reset(&mut f), Err(ResetError::Timeout));
    let reads = f.log.iter().filter(|o| matches!(o, Op::R8(r, _) if *r == regs::CHIP_CMD)).count();
    // Against the LINUX LITERAL, not against our own constant. Comparing the observed count to
    // `regs::RESET_POLL_MAX` would be self-referential — change the constant and both sides move,
    // so the check could never catch a wrong budget. Caught by a mutation that flipped it to 101
    // and left every test passing.
    assert_eq!(
        reads, 100,
        "must poll exactly the Linux budget: rtl_loop_wait_low(tp, &rtl_chipcmd_cond, 100, 100), \
         r8169_main.c:2675 — no more, no fewer"
    );
    assert_eq!(regs::RESET_POLL_MAX, 100, "the ported constant must BE the Linux budget");
    assert_eq!(regs::RESET_POLL_INTERVAL_US, 100, "…and the interval, the second 100 in that call");
}

/// Cfg9346 (r8169_main.c:817, :822): unlock before config writes, lock after. The ORDER is the
/// whole point — a config write outside the unlocked window is silently discarded.
#[test]
fn config_writes_are_bracketed_by_unlock_and_lock_in_that_order() {
    let mut f = Fake::new(0);
    init::with_config_unlocked(&mut f, |b| b.w8(regs::CONFIG1, 0x5a));
    let expected = vec![
        Op::W8(regs::CFG_9346, cfg9346::UNLOCK),
        Op::W8(regs::CONFIG1, 0x5a),
        Op::W8(regs::CFG_9346, cfg9346::LOCK),
    ];
    assert_eq!(f.log, expected, "unlock, then the config write, then lock — in that order");
}

/// The lock must be restored even when the body writes nothing, so the registers are never left
/// open by a caller that decided it had nothing to do.
#[test]
fn the_lock_is_restored_even_if_the_body_writes_nothing() {
    let mut f = Fake::new(0);
    init::with_config_unlocked(&mut f, |_| {});
    assert_eq!(f.writes_to(regs::CFG_9346), vec![cfg9346::UNLOCK, cfg9346::LOCK]);
}

/// The unlock and lock VALUES are RealTek's, not ours. (r8169_main.c:486-487)
#[test]
fn the_cfg9346_values_match_linux() {
    assert_eq!(cfg9346::UNLOCK, 0xc0);
    assert_eq!(cfg9346::LOCK, 0x00);
    assert_eq!(chip_cmd::RESET, 0x10);
    assert_eq!(chip_cmd::RX_ENB, 0x08);
    assert_eq!(chip_cmd::TX_ENB, 0x04);
    assert_eq!(chip_cmd::STOP_REQ, 0x80);
}
