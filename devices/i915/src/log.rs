// SPDX-License-Identifier: GPL-2.0-only
//! The device's task-log lines. Every marker starts `[I915-KBL` so a boot gate (or a grep of a Linux-recon comparison)
//! needs one prefix. One line per fact; a line that would overflow is cut, never dropped.
use core::fmt::{self, Write};

/// Where a finished line goes: the task log in the device, a vector in a test.
pub trait Log {
    fn line(&mut self, s: &str);
}

/// A fixed buffer `core::fmt` can write a line into.
pub struct Line {
    buf: [u8; 320],
    n: usize,
}
impl Line {
    pub const fn new() -> Self {
        Line { buf: [0; 320], n: 0 }
    }
    /// End the line: the newline goes in the byte `write_str` kept back for it, so a cut line still ends in one.
    pub fn finish(&mut self) {
        self.buf[self.n] = b'\n';
        self.n += 1;
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.n]).unwrap_or("[I915-KBL LOG-NOT-UTF8]\n")
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
    l.finish();
    log.line(l.as_str());
}
