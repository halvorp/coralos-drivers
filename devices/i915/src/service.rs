// SPDX-License-Identifier: GPL-2.0-only
//! The device's entry: the real windows, the task log, and the parked request loop.
use crate::log::{emit, Log};
use crate::regio::{BAR0_SLOT, CFG_HEADER_BYTES, CFG_SLOT, READOUT_WINDOW_BYTES};
use crate::startup::startup;
use crate::wire::{exit_code, handle, PACE_MS, REPLY_EP, REQUEST_BUF, SERVICE_EP};
use coralos_rt::mmio::{Kernel, ReadWindow};
use coralos_rt::{exit, recv, send, write};

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
    emit(&mut lg, format_args!("[I915-KBL-DEV LAUNCH] bar0=mmio-slot-{} config=mmio-slot-{} (read-only)", BAR0_SLOT, CFG_SLOT));
    let mut cfg = match ReadWindow::open(&mut Kernel, CFG_SLOT, CFG_HEADER_BYTES) {
        Ok(w) => w,
        Err(e) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=CONFIG-WINDOW why={}]", e.name()));
            exit(exit_code::NO_WINDOW)
        }
    };
    let mut bar = match ReadWindow::open(&mut Kernel, BAR0_SLOT, READOUT_WINDOW_BYTES) {
        Ok(w) => w,
        Err(e) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=BAR0-WINDOW why={}]", e.name()));
            exit(exit_code::NO_WINDOW)
        }
    };
    if startup(&mut bar, &mut cfg, &mut lg).is_err() {
        exit(exit_code::STARTUP_FAILED) // startup already printed the named line
    }
    emit(&mut lg, format_args!("[I915-KBL-DEV PARKED] every request is answered UNSUPPORTED (HELLO excepted); request ep {} reply ep {}", SERVICE_EP, REPLY_EP));
    let mut buf = [0u8; REQUEST_BUF];
    loop {
        let n = recv(SERVICE_EP as u64, &mut buf);
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
