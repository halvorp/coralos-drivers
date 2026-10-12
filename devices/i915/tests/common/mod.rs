// SPDX-License-Identifier: GPL-2.0-only
//! Simulated windows and the log sink shared by the device tests.
#![allow(dead_code)]
use coralos_rt::mmio::MmioError;
use i915_device::log::Log;
use i915_device::regio::RegRead;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Lines(pub Vec<String>);
impl Log for Lines {
    fn line(&mut self, s: &str) {
        self.0.push(s.trim_end().to_string());
    }
}
impl Lines {
    pub fn has(&self, needle: &str) -> bool {
        self.0.iter().any(|l| l.contains(needle))
    }
    pub fn find(&self, prefix: &str) -> Vec<&String> {
        self.0.iter().filter(|l| l.starts_with(prefix)).collect()
    }
}

/// A strict, read-only register window: reads of unmapped addresses PANIC, `fail_at` makes one address refuse.
pub struct Window {
    pub len: u64,
    pub vals: BTreeMap<u32, u32>,
    pub reads: Vec<u32>,
    pub fail_at: Option<u32>,
}
impl Window {
    pub fn new(len: u64) -> Self {
        Window { len, vals: BTreeMap::new(), reads: vec![], fail_at: None }
    }
    pub fn set(&mut self, a: u32, v: u32) -> &mut Self {
        self.vals.insert(a, v);
        self
    }
}
impl RegRead for Window {
    fn len(&self) -> u64 {
        self.len
    }
    fn read32(&mut self, off: u32) -> Result<u32, MmioError> {
        self.reads.push(off);
        if self.fail_at == Some(off) {
            return Err(MmioError::OutOfRange { off: off as u64, width: 4, len: self.len });
        }
        match self.vals.get(&off) {
            Some(v) => Ok(*v),
            None => panic!("read of unmapped register {off:#x}"),
        }
    }
}

/// The PCI configuration header of device `id`: vendor 8086, revision `rev`, class 03:00:00.
pub fn config_page(device: u16, rev: u8) -> Window {
    config_full(0x8086, device, rev, 0x03, 0x00)
}
pub fn config_full(vendor: u16, device: u16, rev: u8, base: u8, sub: u8) -> Window {
    let mut w = Window::new(256);
    w.set(0x00, (device as u32) << 16 | vendor as u32);
    w.set(0x08, (base as u32) << 24 | (sub as u32) << 16 | rev as u32);
    w.set(0x2c, 0x1234_5678);
    w
}

/// BAR0 as the firmware leaves a Pixelbook Go-like panel: vector A of the core's tests (1080p60 eDP on DPLL0).
pub fn bar_vector_a() -> Window {
    let mut r = Window::new(0x40_0000);
    let mut s = |a: u32, v: u32| {
        r.set(a, v);
    };
    s(0x45400, 0x3000_0003);
    s(0x45404, 0xc000_000c);
    s(0x42000, 0x8e00_0000);
    s(0x45504, 0);
    for a in [0x46014, 0x46040, 0x46060, 0x6c040, 0x6c044, 0x6c048, 0x6c04c, 0x6c050, 0x6c054] {
        s(a, 0);
    }
    for a in [0x64100, 0x64200, 0x64300, 0x64400] {
        s(a, 0);
    }
    s(0x46010, 0xc000_0000);
    s(0x6c058, 0x3);
    s(0x6c05c, 0x1);
    s(0x6c060, 0x1);
    s(0x46000, 0x382);
    s(0x64000, 0x8000_0002);
    for t in [0x60400u32, 0x61400, 0x62400] {
        s(t, 0);
    }
    s(0x6f400, 0x8203_0002);
    s(0x7f008, 0x8000_0000);
    for (i, v) in [0x0897_077f, 0x0897_077f, 0x0803_07d7, 0x0464_0437, 0x0464_0437, 0x0440_043b].iter().enumerate() {
        s(0x6f000 + 4 * i as u32, *v);
    }
    s(0x6001c, 0x077f_0437);
    s(0x6f030, 0x7e69_9999);
    s(0x6f034, 0x0080_0000);
    s(0x6f040, 288_358);
    s(0x6f044, 0x0008_0000);
    for pipe_base in [0x70000u32, 0x71000, 0x72000] {
        for plane in 0..3u32 {
            for off in [0x180u32, 0x188, 0x18c, 0x190, 0x19c, 0x1a4] {
                s(pipe_base + plane * 0x100 + off, 0);
            }
        }
    }
    s(0x70180, 0x8400_0400);
    s(0x70188, 15);
    s(0x70190, 0x0437_077f);
    s(0x7019c, 0x0010_0005);
    for (base, n) in [(0x68000u32, 2u32), (0x68800, 2), (0x69000, 1)] {
        for id in 0..n {
            for off in [0x170u32, 0x174, 0x180] {
                s(base + id * 0x100 + off, 0);
            }
        }
    }
    r
}

