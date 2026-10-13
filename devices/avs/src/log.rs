// SPDX-License-Identifier: GPL-2.0-only
//! The device's task-log lines. Every marker starts `[AVS-DEV` so a gate or a grep of a recon comparison needs one prefix. One line per
//! fact; a line that would overflow is cut, never dropped.
use core::fmt::{self, Write};

/// A fixed buffer `core::fmt` can write a line into.
pub struct Line {
    buf: [u8; 240],
    n: usize,
}

impl Line {
    pub const fn new() -> Self {
        Line { buf: [0; 240], n: 0 }
    }
    pub fn finish(&mut self) {
        self.buf[self.n] = b'\n';
        self.n += 1;
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.n]).unwrap_or("[AVS-DEV LOG-NOT-UTF8]\n")
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let room = self.buf.len() - 1 - self.n;
        let take = s.len().min(room);
        self.buf[self.n..self.n + take].copy_from_slice(&s.as_bytes()[..take]);
        self.n += take;
        Ok(())
    }
}

/// Format `args` into one line and hand it to the machine's log.
pub fn emit<M: crate::machine::Machine>(m: &mut M, args: fmt::Arguments<'_>) {
    let mut l = Line::new();
    let _ = l.write_fmt(args);
    l.finish();
    m.log(l.as_str());
}
