// SPDX-License-Identifier: GPL-2.0-only
//! The device's task-log lines. Every marker starts `[I8042-DEV` so a boot gate greps one prefix.
use core::fmt::{self, Write};
use i8042_core::bringup::{Step, Verdict};

/// Where a finished line goes: the task log in the device, a vector in a test.
pub trait Log {
    fn line(&mut self, s: &str);
}

/// A fixed buffer `core::fmt` can write a line into; a line that overflows is cut, never lost.
pub struct Line {
    buf: [u8; 192],
    n: usize,
}
impl Line {
    pub const fn new() -> Self {
        Line { buf: [0; 192], n: 0 }
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.n]).unwrap_or("[I8042-DEV LOG-NOT-UTF8]\n")
    }
}
impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}
impl Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let room = self.buf.len() - 1 - self.n; // one byte kept for the newline
        let take = s.len().min(room);
        self.buf[self.n..self.n + take].copy_from_slice(&s.as_bytes()[..take]);
        self.n += take;
        Ok(())
    }
}

/// Format and emit one line (a newline is added).
pub fn emit<L: Log>(log: &mut L, args: fmt::Arguments<'_>) {
    let mut l = Line::new();
    let _ = l.write_fmt(args);
    let _ = l.write_str("\n");
    log.line(l.as_str());
}

/// One bring-up step's line: `[I8042-DEV STEP <NAME> OK detail=…]`, `… TOLERATED …`, `… SKIPPED`, or the FAIL marker.
pub fn step<L: Log>(log: &mut L, s: Step, v: Verdict) {
    match v {
        Verdict::Ok { detail } => emit(log, format_args!("[I8042-DEV STEP {} OK detail={:#x}]", s.name(), detail)),
        Verdict::Tolerated { answer, why } => {
            emit(log, format_args!("[I8042-DEV STEP {} TOLERATED answer={:#04x} why={}]", s.name(), answer, why))
        }
        Verdict::Skipped => emit(log, format_args!("[I8042-DEV STEP {} SKIPPED]", s.name())),
        Verdict::Failed { why } => emit(log, format_args!("[I8042-DEV FAIL step={} why={}]", s.name(), why)),
    }
}
