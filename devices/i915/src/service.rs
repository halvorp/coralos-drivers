// SPDX-License-Identifier: GPL-2.0-only
//! The device's entry: the real windows, the task log, and the parked request loop.
use crate::log::{emit, Log};
use crate::regio::{BarMmio, Clock, BAR0_NEED_BYTES, BAR0_SLOT, BAR0_WRITE_RANGES, CFG_HEADER_BYTES, CFG_SLOT};
use crate::startup::bringup;
use crate::wire::{exit_code, handle, PACE_MS, REPLY_EP, REQUEST_BUF, SERVICE_EP};
use coralos_rt::mmio::{Kernel, ReadWindow, WriteWindow};
use coralos_rt::{exit, recv, send, write};

/// The kernel's `TICKS` (system call 24), read as milliseconds (kernel/src/arch/x86_64/apic.rs documents `TIMER_TICKS` as ms).
struct Ticks;
impl Clock for Ticks {
    fn now_ms(&mut self) -> u64 {
        coralos_rt::ticks()
    }
    /// A timed wait on nothing (the same call the parked loop paces itself with): a poll never spins.
    fn relax(&mut self) {
        coralos_rt::wait_any_irq(0, 0, 1, 0);
    }
    fn park_ms(&mut self, ms: u64) {
        coralos_rt::wait_any_irq(0, 0, ms, 0);
    }
}

struct TaskLog;
impl Log for TaskLog {
    fn line(&mut self, s: &str) {
        write(s.as_bytes());
    }
}

/// A panic: say so and end the task, so a supervisor sees it die rather than hold a half-initialised device.
pub fn on_panic() -> ! {
    write(b"[I915-KBL-DEV PANIC] - device exiting [I915-KBL-DEV FAIL step=PANIC]\n");
    exit(exit_code::PANIC)
}

/// The device. Exit codes are in [`exit_code`]; the log line before an exit names the step.
pub fn service() -> ! {
    let mut lg = TaskLog;
    emit(&mut lg, format_args!("[I915-KBL-DEV LAUNCH] bar0=mmio-slot-{} (writes: forcewake set x{} only) config=mmio-slot-{} (read-only)", BAR0_SLOT, BAR0_WRITE_RANGES.len(), CFG_SLOT));
    emit(&mut lg, format_args!("[I915-CONTRACT abi={} wire={}.{} write_allow={}]", i915_common_core::CONTRACT_ABI, i915_wire::ABI_MAJOR, i915_wire::ABI_MINOR, BAR0_WRITE_RANGES.len()));
    let mut cfg = match ReadWindow::open(&mut Kernel, CFG_SLOT, CFG_HEADER_BYTES) {
        Ok(w) => w,
        Err(e) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=CONFIG-WINDOW why={}]", e.name()));
            exit(exit_code::NO_WINDOW)
        }
    };
    let mut bar = match WriteWindow::open(&mut Kernel, BAR0_SLOT, BAR0_NEED_BYTES, &BAR0_WRITE_RANGES) {
        Ok(w) => BarMmio::new(w, Ticks),
        Err(e) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=BAR0-WINDOW why={}]", e.name()));
            exit(exit_code::NO_WINDOW)
        }
    };
    match bringup(&mut bar, &mut cfg, &mut lg) {
        Ok(_) => {}
        // the failing stage already printed its named line
        Err(crate::startup::BringupError::Proof(_)) | Err(crate::startup::BringupError::Window(_)) => exit(exit_code::PROOF_FAILED),
        Err(_) => exit(exit_code::STARTUP_FAILED),
    }
    emit(&mut lg, format_args!("[I915-KBL-DEV PARKED] every request is answered UNSUPPORTED (HELLO excepted); request ep {} reply ep {}", SERVICE_EP, REPLY_EP));
    let mut buf = [0u8; REQUEST_BUF];
    loop {
        let n = recv(SERVICE_EP as u64, &mut buf);
        // An ABI-1.1 frame (magic "I915"): the uAPI service is not wired to this endpoint yet (src/uapi is driven by its own tests), so
        // every such request gets the named answer - Unsupported, reason INCREMENT-NOT-ENABLED - instead of the old protocol's BAD_VERSION.
        if n >= 4 && n <= buf.len() as u64 && buf[..4] == *b"I915" {
            let mut out = [0u8; 96];
            if let Ok(len) = crate::uapi::frame::answer_without_service(&buf[..n as usize], &mut out) {
                let _ = send(REPLY_EP as u64, &out[..len]);
            }
            continue;
        }
        match handle(n, &buf) {
            Some(reply) => {
                let _ = send(REPLY_EP as u64, &reply);
            }
            // nothing usable arrived: pace the loop with a timed wait on nothing instead of spinning
            None => {
                coralos_rt::wait_any_irq(0, 0, PACE_MS, 0);
            }
        }
    }
}
