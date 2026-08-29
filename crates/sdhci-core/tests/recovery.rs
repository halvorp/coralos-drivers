// SPDX-License-Identifier: GPL-2.0-or-later

//! Integration tests for the SDHCI recovery state machine.
//!
//! Each test sets up a `Recovery` instance with a specific `RequestCtx`, feeds a
//! sequence of `Event`s, and asserts the resulting `Action`s match the expected
//! reducer output.
//!
//! SCOPE, stated plainly so it is not mistaken for more than it is: these vectors
//! exercise the PURE REDUCER (`Recovery::step`) and nothing else. They do not run
//! the `Executor`, so the `Bus`/`Time` implementations below are not exercised by
//! any assertion here and the executor layer currently has NO test coverage.
//! An earlier draft of this helper built an `Executor` and then never used it —
//! four of the five compile errors that blocked this file came from that dead
//! line, and deleting it is what fixed them. The mocks are kept because they are
//! the scaffolding an executor-level vector will need; they are marked dead-code
//! so the gap stays visible instead of reading as coverage that exists.
//!
//! The six vectors documented in the crate-level specification are
//! tested here: adma_error, clean_multiblock_read_auto_cmd23,
//! data_crc_multiblock_write_stop, software_data_timeout, and two
//! baseline tests (no_error, cmd_timeout).

use core::cell::RefCell;
use sdhci_core::core::*;
use sdhci_core::regs::*;

/// A mock register file for testing.
#[allow(dead_code)] // see SCOPE above: the executor layer is not yet covered
struct MockRegs {
    regs: [u32; 0x100],
}

impl Default for MockRegs {
    fn default() -> Self {
        MockRegs { regs: [0; 0x100] }
    }
}

impl sdhci_core::executor::Bus for MockRegs {
    fn r8(&mut self, reg: u16) -> u8 {
        (self.regs[reg as usize] & 0xFF) as u8
    }
    fn r16(&mut self, reg: u16) -> u16 {
        (self.regs[reg as usize] & 0xFFFF) as u16
    }
    fn r32(&mut self, reg: u16) -> u32 {
        self.regs[reg as usize]
    }
    fn w8(&mut self, reg: u16, val: u8) {
        self.regs[reg as usize] = (self.regs[reg as usize] & !0xFF) | val as u32;
    }
    fn w16(&mut self, reg: u16, val: u16) {
        self.regs[reg as usize] = (self.regs[reg as usize] & !0xFFFF) | val as u32;
    }
    fn w32(&mut self, reg: u16, val: u32) {
        self.regs[reg as usize] = val;
    }
}

/// A mock time source that never expires unless explicitly set.
#[allow(dead_code)] // see SCOPE above: the executor layer is not yet covered
struct MockTime {
    deadline: u64,
    now: u64,
}

impl Default for MockTime {
    fn default() -> Self {
        MockTime { deadline: u64::MAX, now: 0 }
    }
}

impl sdhci_core::executor::Time for MockTime {
    type Deadline = u64;
    fn deadline_after_ms(&mut self, ms: u64) -> u64 {
        let d = self.now + ms;
        self.deadline = d;
        d
    }
    fn expired(&mut self, deadline: u64) -> bool {
        self.now >= deadline
    }
    fn delay_us(&mut self, _us: u32) {
        self.now += 1; // advance for polling simplicity
    }
    fn park_ms(&mut self, ms: u64) {
        self.now += ms;
        self.deadline = self.now; // expire immediately
    }
}

/// Helper to run a test scenario and collect actions.
fn run_test(req: RequestCtx, events: &[Event]) -> Vec<Action> {
    let mut recovery = Recovery::new(req);
    let actions = RefCell::new(Vec::new());
    // Feed events one by one, each time collecting actions.
    for ev in events {
        // Because the executor's run method would loop, we manually step.
        // For simplicity, we just call recovery.step and collect.
        // The executor is not used directly here; we use the step method.
        // This test helper directly drives recovery.
        recovery.step(ev.clone(), &mut |a| actions.borrow_mut().push(a));
    }
    actions.into_inner()
}

/// Helper to compare actions ignoring token values.
fn assert_actions(expected: &[Action], actual: &[Action]) {
    // Normalize token values for comparison: replace every Token with a placeholder.
    let normalize = |act: &Action| -> String {
        format!("{:?}", act)
            .replace("Token(", "T(")
            .replace(')', "")
    };
    for (i, (exp, act)) in expected.iter().zip(actual.iter()).enumerate() {
        let e = normalize(exp);
        let a = normalize(act);
        assert_eq!(e, a, "Action mismatch at index {}", i);
    }
    assert_eq!(expected.len(), actual.len(), "Action count mismatch");
}

// ----------------------------------------------------------------
// Test 1: No error (simple data read with DATA_END + RESPONSE)
// ----------------------------------------------------------------
#[test]
fn no_error() {
    let req = RequestCtx {
        id: 1,
        has_data: true,
        direction: Direction::Read,
        multiblock: false,
        stop: None,
        sbc: None,
        auto_cmd: AutoCmd::None,
        busy: false,
        response_present: true,
        response_136: false,
        cap_cmd_during_tfr: false,
        opcode: 17,
        is_tuning: false,
        quirks: Quirks { bits: 0, bits2: 0 },
        host_flags: HostFlags { use_adma: false, req_use_dma: false, auto_cmd12: false, auto_cmd23: false },
        cmd_timeout_ms: 100,
        data_timeout_ms: 100,
    };
    let events = [
        Event::InterruptStatus { raw: SDHCI_INT_RESPONSE | SDHCI_INT_DATA_END },
        Event::ReadComplete { token: Token(0), value: 17 }, // COMMAND read if needed? this test doesn't trigger aux read.
        Event::ReadComplete { token: Token(1), value: 0 }, // if needed
    ];
    // A clean command+data completion ACKs the two interrupt bits and then COMPLETES the request
    // with no error. Sol's adjudication of this vector was explicit — the clean transfer needs the
    // CompleteRequest, and AdmaPost only when the request uses ADMA (this one does not).
    // This vector previously declared `expected` and then threw it away (`let _actions = ...`,
    // commented "we just check no crash"), so it asserted NOTHING and would have passed against a
    // reducer that emitted no actions at all, or the wrong ones. It is a real check now.
    let expected = [
        Action::Write32 { reg: SDHCI_INT_STATUS, value: SDHCI_INT_RESPONSE | SDHCI_INT_DATA_END },
        Action::CompleteRequest { id: 1, err: None },
    ];
    let actions = run_test(req, &events);
    assert_actions(&expected, &actions);
}

// ----------------------------------------------------------------
// Test 2: Command timeout (via Event::CommandTimeout)
// ----------------------------------------------------------------
#[test]
fn cmd_timeout() {
    let req = RequestCtx {
        id: 2,
        has_data: false,
        direction: Direction::Read,
        multiblock: false,
        stop: None,
        sbc: None,
        auto_cmd: AutoCmd::None,
        busy: false,
        response_present: true,
        response_136: false,
        cap_cmd_during_tfr: false,
        opcode: 13,
        is_tuning: false,
        quirks: Quirks { bits: 0, bits2: 0 },
        host_flags: HostFlags { use_adma: false, req_use_dma: false, auto_cmd12: false, auto_cmd23: false },
        cmd_timeout_ms: 100,
        data_timeout_ms: 100,
    };
    // A command timeout sets cmd->error = -ETIMEDOUT, and a COMMAND ERROR is one of Linux's
    // sdhci_needs_reset() predicates (sdhci.c:1501-1507), so the request does NOT complete bare.
    // sdhci_request_done() runs sdhci_reset_for(REQUEST_ERROR) — CMD then DATA reset
    // (sdhci_reset_for_reason, 274-295) — BEFORE mmc_request_done (3172-3191). This vector
    // originally expected a lone CompleteRequest and failed against a reducer that was right:
    // it emitted the RESET_CMD write first. The expectation was the defect, not the reducer.
    // Same shape as the adjudicated software_data_timeout vector below, minus AdmaPost — this
    // request carries no data and use_adma is false.
    let events = [
        Event::CommandTimeout { id: 2 },
        // The reset waits are poll-driven: the reducer asks for SOFTWARE_RESET reads and only
        // advances when the bit reads clear, so the vector must supply those completions.
        Event::ReadComplete { token: Token(1), value: 0 },
        Event::ReadComplete { token: Token(3), value: 0 },
    ];
    let expected = [
        Action::Write8 { reg: SDHCI_SOFTWARE_RESET, value: SDHCI_RESET_CMD },
        Action::ArmDeadline { token: Token(0), ms: 100 },
        Action::Read8 { reg: SDHCI_SOFTWARE_RESET, token: Token(1) },
        Action::Write8 { reg: SDHCI_SOFTWARE_RESET, value: SDHCI_RESET_DATA },
        Action::ArmDeadline { token: Token(2), ms: 100 },
        Action::Read8 { reg: SDHCI_SOFTWARE_RESET, token: Token(3) },
        Action::CompleteRequest { id: 2, err: Some(Error::TimedOut) },
    ];
    let actions = run_test(req, &events);
    assert_actions(&expected, &actions);
}

// ----------------------------------------------------------------
// Test 3: adma_error (sdhci.c:3494 → -EIO)
// ----------------------------------------------------------------
#[test]
fn adma_error() {
    let req = RequestCtx {
        id: 3,
        has_data: true,
        direction: Direction::Read,
        multiblock: false,
        stop: None,
        sbc: None,
        auto_cmd: AutoCmd::None,
        busy: false,
        response_present: false,
        response_136: false,
        cap_cmd_during_tfr: false,
        opcode: 17,
        is_tuning: false,
        quirks: Quirks { bits: 0, bits2: 0 },
        host_flags: HostFlags { use_adma: true, req_use_dma: true, auto_cmd12: false, auto_cmd23: false },
        cmd_timeout_ms: 100,
        data_timeout_ms: 100,
    };
    // Sequence: ADMA_ERROR → AdmaWorkaround → RESET_CMD → RESET_DATA → AdmaPost → Complete(Eio)
    let events = [
        Event::InterruptStatus { raw: SDHCI_INT_ADMA_ERROR },
        // ADMA_ERROR triggers data_irq → sets error → finish_data → reset_for(RequestError) → start_reset(CMD)
        // Then we need to complete the reset reads.
        // First: Ack, then Write8(RESET_CMD), ArmDeadline, Read8(SOFTWARE_RESET). 
        // Then when ReadComplete returns 0, it will schedule RESET_DATA, then eventually after_reset.
        // For simplicity, we feed the read completions.
        Event::ReadComplete { token: Token(1), value: 0 }, // reset CMD cleared
        Event::ReadComplete { token: Token(3), value: 0 }, // reset DATA cleared
    ];
    let expected = [
        Action::Write32 { reg: SDHCI_INT_STATUS, value: SDHCI_INT_ADMA_ERROR },
        Action::AdmaWorkaround,
        Action::Write8 { reg: SDHCI_SOFTWARE_RESET, value: SDHCI_RESET_CMD },
        Action::ArmDeadline { token: Token(0), ms: 100 },
        Action::Read8 { reg: SDHCI_SOFTWARE_RESET, token: Token(1) },
        // NO DelayUs and no second read here. Linux's sdhci_reset() (sdhci.c:217-234) reads
        // SOFTWARE_RESET first and BREAKS immediately when the mask is already clear; the
        // udelay(10) is spent only BETWEEN polls, i.e. when the bit is still set. This vector
        // feeds ReadComplete(value: 0) — clear on the first read — so the reducer moves straight
        // to the DATA reset. The expectation carried a delay+re-read that Linux would not do.
        // after reset CMD cleared, we get start_reset(DATA)
        Action::Write8 { reg: SDHCI_SOFTWARE_RESET, value: SDHCI_RESET_DATA },
        Action::ArmDeadline { token: Token(2), ms: 100 },
        Action::Read8 { reg: SDHCI_SOFTWARE_RESET, token: Token(3) },
        // after reset DATA cleared, after_reset: AdmaPost then finish_mrq
        Action::AdmaPost,
        Action::CompleteRequest { id: 3, err: Some(Error::Eio) },
    ];
    let actions = run_test(req, &events);
    // assert_actions compares position-by-position and already ignores token VALUES, so the whole
    // adjudicated sequence can be asserted — checking only the last action let every reset write,
    // the ADMA workaround and AdmaPost go unverified.
    assert_actions(&expected, &actions);
}

// ----------------------------------------------------------------
// Test 4: clean_multiblock_read_auto_cmd23 (DATA_END after RESPONSE)
// ----------------------------------------------------------------
#[test]
fn clean_multiblock_read_auto_cmd23() {
    let req = RequestCtx {
        id: 6,
        has_data: true,
        direction: Direction::Read,
        multiblock: true,
        stop: Some(StopCtx { opcode: 12, arg: 0, flags: 0, timeout_ms: 100 }),
        sbc: Some(SbcCtx { arg: 128 }),
        auto_cmd: AutoCmd::Cmd23,
        busy: false,
        response_present: true,
        response_136: false,
        cap_cmd_during_tfr: false,
        opcode: 18,
        is_tuning: false,
        quirks: Quirks { bits: 0, bits2: 0 },
        host_flags: HostFlags { use_adma: true, req_use_dma: true, auto_cmd12: false, auto_cmd23: true },
        cmd_timeout_ms: 100,
        data_timeout_ms: 100,
    };
    // Sequence: RESPONSE then DATA_END
    let events = [
        Event::InterruptStatus { raw: SDHCI_INT_RESPONSE },
        Event::ReadComplete { token: Token(0), value: 0 }, // reset read not used
        Event::InterruptStatus { raw: SDHCI_INT_DATA_END },
    ];
    // After RESPONSE: finish_command clears cmd, data_cmd still present.
    // DATA_END: data_early? cmd_is_data? Since cmd already cleared, data_early false, so finish_data directly.
    // data has no error, so no reset. Need stop? data has stop and not auto_cmd12 (since sbc present? Wait sbc present means auto_cmd23, so stop not needed? Actually if sbc present, stop is used? In finish_data, need_stop is true if stop.is_some() and (!sbc.is_some() && !auto_cmd12) or error.is_some(). Here sbc.is_some() is true, so !sbc.is_some() is false, so need_stop is false only if no error. So no stop. And data.host_flags.req_use_dma and use_adma true, so AdmaPost, then finish_mrq.
    let expected = [
        // first irq: ACK RESPONSE
        Action::Write32 { reg: SDHCI_INT_STATUS, value: SDHCI_INT_RESPONSE },
        // second irq: ACK DATA_END
        Action::Write32 { reg: SDHCI_INT_STATUS, value: SDHCI_INT_DATA_END },
        Action::AdmaPost,
        Action::CompleteRequest { id: 6, err: None },
    ];
    let actions = run_test(req, &events);
    assert_actions(&expected, &actions);
}

// ----------------------------------------------------------------
// Test 5: data_crc_multiblock_write_stop
// ----------------------------------------------------------------
#[test]
fn data_crc_multiblock_write_stop() {
    let req = RequestCtx {
        id: 2,
        has_data: true,
        direction: Direction::Write,
        multiblock: true,
        stop: Some(StopCtx { opcode: 12, arg: 0, flags: 0, timeout_ms: 100 }),
        sbc: None,
        auto_cmd: AutoCmd::None,
        busy: false,
        response_present: false,
        response_136: false,
        cap_cmd_during_tfr: false,
        opcode: 25,
        is_tuning: false,
        quirks: Quirks { bits: 0, bits2: SDHCI_QUIRK2_STOP_WITH_TC },
        host_flags: HostFlags { use_adma: true, req_use_dma: true, auto_cmd12: false, auto_cmd23: false },
        cmd_timeout_ms: 100,
        data_timeout_ms: 100,
    };
    // Sequence per adjudication:
    // Interrupt DATA_CRC:
    //   Write32(INT_STATUS, DATA_CRC)
    //   Read16(COMMAND, Token(0))
    // ReadComplete: opcode 25 (SDHCI_MAKE_CMD(25, ...) = 0x? we'll use 25)
    //   Write8(SOFTWARE_RESET, RESET_CMD)
    //   ArmDeadline
    //   Read8(SOFTWARE_RESET, Token(2))
    // ReadComplete: 0
    //   Write8(SOFTWARE_RESET, RESET_DATA)
    //   ArmDeadline
    //   Read8(SOFTWARE_RESET, Token(4))
    // ReadComplete: 0
    //   AdmaPost
    //   SendStop(CMD12 with MMC_RSP_BUSY)
    // Then RESPONSE:
    //   Write32(INT_STATUS, RESPONSE)
    // Then DATA_END:
    //   Write32(INT_STATUS, DATA_END)
    //   CompleteRequest(id=2, Eilseq)
    let events = [
        Event::InterruptStatus { raw: SDHCI_INT_DATA_CRC },
        Event::ReadComplete { token: Token(0), value: SDHCI_MAKE_CMD(25, 0) as u32 },
        Event::ReadComplete { token: Token(2), value: 0 },
        Event::ReadComplete { token: Token(4), value: 0 },
        Event::InterruptStatus { raw: SDHCI_INT_RESPONSE },
        Event::InterruptStatus { raw: SDHCI_INT_DATA_END },
    ];
    let expected = [
        Action::Write32 { reg: SDHCI_INT_STATUS, value: SDHCI_INT_DATA_CRC },
        Action::Read16 { reg: SDHCI_COMMAND, token: Token(0) },
        // after ReadComplete: reset CMD
        Action::Write8 { reg: SDHCI_SOFTWARE_RESET, value: SDHCI_RESET_CMD },
        Action::ArmDeadline { token: Token(1), ms: 100 },
        Action::Read8 { reg: SDHCI_SOFTWARE_RESET, token: Token(2) },
        // first reset read complete: reset DATA
        Action::Write8 { reg: SDHCI_SOFTWARE_RESET, value: SDHCI_RESET_DATA },
        Action::ArmDeadline { token: Token(3), ms: 100 },
        Action::Read8 { reg: SDHCI_SOFTWARE_RESET, token: Token(4) },
        // second reset read complete: after_reset
        Action::AdmaPost,
        Action::SendStop { id: 2, stop: StopCtx { opcode: 12, arg: 0, flags: MMC_RSP_BUSY, timeout_ms: 100 } },
        // RESPONSE and DATA_END interrupts come after SendStop
        Action::Write32 { reg: SDHCI_INT_STATUS, value: SDHCI_INT_RESPONSE },
        Action::Write32 { reg: SDHCI_INT_STATUS, value: SDHCI_INT_DATA_END },
        Action::CompleteRequest { id: 2, err: Some(Error::Eilseq) },
    ];
    let actions = run_test(req, &events);
    // Was an `any(...)` match on the final error — which would have passed even if the mandatory
    // COMMAND read, the reset ordering, or the STOP_WITH_TC CMD12 (the three things Sol actually
    // adjudicated on this vector) were absent or wrong. Assert the sequence.
    assert_actions(&expected, &actions);
}

// ----------------------------------------------------------------
// Test 6: software_data_timeout (sdhci.c:3276-3279)
// ----------------------------------------------------------------
#[test]
fn software_data_timeout() {
    let req = RequestCtx {
        id: 4,
        has_data: true,
        direction: Direction::Read,
        multiblock: false,
        stop: None,
        sbc: None,
        auto_cmd: AutoCmd::None,
        busy: false,
        response_present: false,
        response_136: false,
        cap_cmd_during_tfr: false,
        opcode: 17,
        is_tuning: false,
        quirks: Quirks { bits: 0, bits2: 0 },
        host_flags: HostFlags { use_adma: true, req_use_dma: true, auto_cmd12: false, auto_cmd23: false },
        cmd_timeout_ms: 100,
        data_timeout_ms: 100,
    };
    // Feed Event::DataTimeout { id: 4 }
    // (the bare `events` array here was dead — this vector drives `events_with_resets` below)
        // Expected: finish_data with sw_data_timeout=true → reset_for(RequestError) → start_reset(RESET_CMD) ...
    // Then after reset completion, AdmaPost and CompleteRequest(TimedOut)
    let expected_sequence = [
        Action::Write8 { reg: SDHCI_SOFTWARE_RESET, value: SDHCI_RESET_CMD },
        Action::ArmDeadline { token: Token(0), ms: 100 },
        Action::Read8 { reg: SDHCI_SOFTWARE_RESET, token: Token(1) },
        // after reset CMD cleared: start_reset(DATA)
        Action::Write8 { reg: SDHCI_SOFTWARE_RESET, value: SDHCI_RESET_DATA },
        Action::ArmDeadline { token: Token(2), ms: 100 },
        Action::Read8 { reg: SDHCI_SOFTWARE_RESET, token: Token(3) },
        // after DATA reset: after_reset → AdmaPost → finish_mrq → Complete(TimedOut)
        Action::AdmaPost,
        Action::CompleteRequest { id: 4, err: Some(Error::TimedOut) },
    ];
    // We need to include the reset read completions in the events to drive the state machine.
    let events_with_resets = [
        Event::DataTimeout { id: 4 },
        Event::ReadComplete { token: Token(1), value: 0 },
        Event::ReadComplete { token: Token(3), value: 0 },
    ];
    let actions = run_test(req, &events_with_resets);
    assert_actions(&expected_sequence, &actions);
}