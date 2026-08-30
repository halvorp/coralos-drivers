// SPDX-License-Identifier: GPL-2.0-or-later

//! Executor-level vectors: do the reducer's Actions actually reach the BUS?
//!
//! tests/recovery.rs asserts the `Action` sequence `Recovery::step` emits. That is the pure
//! reducer, and it was ALL the coverage this crate had — the executor, which turns those Actions
//! into register accesses and feeds results back, had none (card sdhci_executor_layer_untested).
//! The distinction is not academic: a reducer can emit a perfect reset sequence while the executor
//! writes it to the wrong register, at the wrong width, or in the wrong order, and every reducer
//! vector would still pass.
//!
//! These vectors assert REGISTER WRITES, not Actions.

mod common;

use common::{MockTime, RecordingRegs};
use sdhci_core::core::*;
use sdhci_core::executor::Executor;
use sdhci_core::regs::*;

/// A request with no data — the shape the cmd_timeout reducer vector uses.
fn bare_request(id: RequestId) -> RequestCtx {
    RequestCtx {
        id,
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
    }
}

/// A command timeout must reach the SOFTWARE_RESET register as CMD then DATA, as BYTE writes.
///
/// Derivation, not observation: a command error is one of Linux's sdhci_needs_reset() predicates
/// (sdhci.c:1501-1507), so sdhci_request_done() runs sdhci_reset_for(REQUEST_ERROR) before
/// mmc_request_done (3172-3191), and sdhci_reset_for_reason() performs CMD then DATA resets only
/// (274-295). SDHCI_SOFTWARE_RESET is a BYTE register (sdhci.h:158), so an executor that wrote it
/// with w16/w32 would corrupt the neighbouring HOST_CONTROL2 half — which the width field catches.
///
/// The reset polls read back 0 (RecordingRegs starts zeroed), which is Linux's "hw clears the bit
/// when it's done" fast path: read first, break immediately when the mask is clear.
#[test]
fn command_timeout_writes_cmd_then_data_reset_to_the_bus() {
    let bus = RecordingRegs::default();
    let time = MockTime::default();
    let mut ex = Executor::new(bus, time);
    let mut rec = Recovery::new(bare_request(2));

    ex.run(&mut rec, Event::CommandTimeout { id: 2 });

    let bus = ex.bus();
    let resets = bus.writes_to(SDHCI_SOFTWARE_RESET);
    assert_eq!(
        resets,
        vec![SDHCI_RESET_CMD as u32, SDHCI_RESET_DATA as u32],
        "expected CMD then DATA reset to reach SOFTWARE_RESET; observed writes: {:?}",
        bus.writes
    );
    assert!(
        bus.writes.iter().filter(|w| w.reg == SDHCI_SOFTWARE_RESET).all(|w| w.width == 8),
        "SOFTWARE_RESET is a byte register (sdhci.h:158); a wider write would clobber its \
         neighbours. observed: {:?}",
        bus.writes
    );
    assert!(
        bus.reads.iter().any(|&r| r == SDHCI_SOFTWARE_RESET),
        "the executor must READ SOFTWARE_RESET back to see the bit clear, not assume it; \
         observed reads: {:?}",
        bus.reads
    );
}
