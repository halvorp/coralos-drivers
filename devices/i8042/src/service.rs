// SPDX-License-Identifier: GPL-2.0-only
//! The device's entry: the real ports, IRQ 1, the keyboard endpoint and the task log, wired to the generic pieces.
use crate::init::init;
use crate::io::{Delay, KbPort};
use crate::log::{self, Log};
use crate::pacer::{Mode, Pacer};
use crate::run::{drain, Outbox, Sink};
use coralos_hal::{Irq, IrqWait};
use coralos_rt::keywire::KEY_EP;
use coralos_rt::portio::Kernel;
use coralos_rt::{send, tsc, ticks, word, write, RtIrq};
use i8042_core::bringup::Options;

/// The keyboard controller's interrupt line (ISA IRQ 1).
pub const IRQ_LINE: u32 = 1;

struct TaskLog;
impl Log for TaskLog {
    fn line(&mut self, s: &str) {
        write(s.as_bytes());
    }
}

/// `udelay` on the TSC, calibrated once against the millisecond tick.
struct TscDelay {
    per_us: u64,
}
impl TscDelay {
    fn calibrate() -> Self {
        // Wait for a tick edge, count TSC over the next 2 ms. Bounded: a clock that does not advance leaves a
        // conservative 1000 cycles per microsecond (a delay too long, never too short, on anything under 1 THz).
        let mut spin = 0u32;
        let t0 = ticks();
        while ticks() == t0 && spin < 50_000_000 {
            spin += 1;
            core::hint::spin_loop();
        }
        let (a, ta) = (tsc(), ticks());
        while ticks() < ta + 2 && spin < 100_000_000 {
            spin += 1;
            core::hint::spin_loop();
        }
        let (b, tb) = (tsc(), ticks());
        let ms = tb.saturating_sub(ta);
        let per_us = if ms == 0 { 1000 } else { ((b - a) / (ms * 1000)).max(1) };
        TscDelay { per_us }
    }
}
impl Delay for TscDelay {
    fn us(&mut self, us: u32) {
        let end = tsc().wrapping_add(self.per_us * us as u64);
        while (tsc().wrapping_sub(end) as i64) < 0 {
            core::hint::spin_loop();
        }
    }
}

/// The keyboard endpoint. A full ring is retried briefly (the reader drains it each pass) before the frame is called lost.
struct KeySink {
    delay: TscDelay,
}
impl Sink for KeySink {
    fn send(&mut self, frame: [u8; 3]) -> bool {
        for _ in 0..4 {
            match send(KEY_EP as u64, &frame) {
                word::SEND_OK => return true,
                word::SEND_RING_FULL => self.delay.us(300),
                _ => return false, // denied or the reader is dead: retrying changes nothing
            }
        }
        false
    }
}

/// A panic: say so and end the task, so a supervisor sees it die rather than hold a half-driven controller.
pub fn on_panic() -> ! {
    write(b"[I8042-DEV PANIC] - device exiting [I8042-DEV FAIL step=PANIC]\n");
    coralos_rt::exit(2)
}

/// The device.
pub fn service() -> ! {
    let mut lg = TaskLog;
    log::emit(&mut lg, format_args!("[I8042-DEV START] ports 0x60/0x64 irq {} -> endpoint {}", IRQ_LINE, KEY_EP));
    let delay = TscDelay::calibrate();
    let port = match KbPort::open(Kernel, delay) {
        Ok(p) => p,
        Err(e) => {
            log::emit(&mut lg, format_args!("[I8042-DEV FAIL step=PORTS why={}]", e.name()));
            coralos_rt::exit(1)
        }
    };
    let mut dev = match init(port, Options::LINUX_X86, &mut lg) {
        Ok(d) => d,
        Err(_) => coralos_rt::exit(1), // init already printed [I8042-DEV FAIL step=…]
    };
    log::emit(
        &mut lg,
        format_args!("[I8042-DEV READY] ports 0x60/0x64 refusals={} degraded={}", dev.ctl.io.refusals, dev.degraded as u8),
    );
    let mut irq = RtIrq { line: IRQ_LINE, level: false };
    let mut sink = KeySink { delay: TscDelay::calibrate() };
    let (mut out, mut pacer) = (Outbox::new(), Pacer::new());
    loop {
        let by_irq = match irq.wait(pacer.timeout_ms()) {
            Ok(IrqWait::Fired) => true,
            Ok(IrqWait::TimedOut) => false,
            Err(e) => {
                if pacer.irq_unavailable().is_some() {
                    log::emit(&mut lg, format_args!("[I8042-DEV IRQ-UNAVAILABLE {:?} - polling the controller]", e));
                }
                // No wait primitive to park on: pace the poll with a bounded timed wait on nothing.
                coralos_rt::wait_any_irq(0, 0, pacer.timeout_ms(), 0);
                false
            }
        };
        let n = drain(&mut dev, &mut out, &mut sink, &mut lg);
        if let Some(m) = pacer.observe(by_irq, n) {
            match m {
                Mode::Polling => log::emit(&mut lg, format_args!("[I8042-DEV IRQ-DEAD - bytes arrived without IRQ 1; polling every {} ms]", pacer.timeout_ms())),
                Mode::Irq => log::emit(&mut lg, format_args!("[I8042-DEV IRQ-ALIVE - IRQ 1 delivers; back to waiting on it]")),
            }
        }
    }
}
