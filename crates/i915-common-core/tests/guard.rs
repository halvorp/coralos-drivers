// SPDX-License-Identifier: GPL-2.0-only
//! The write guard: only allow-listed 32-bit writes are forwarded; everything else is refused, NAMED, and not forwarded.
use coralos_hal::Mmio;
use i915_common_core::{I915Error, Reg, WriteGuard};
use std::collections::BTreeMap;

#[derive(Default)]
struct Dev {
    regs: BTreeMap<u32, u32>,
    writes: Vec<(u32, u32)>,
    reads: Vec<u32>,
}
impl Mmio for Dev {
    fn r8(&mut self, _: u32) -> u8 { 0 }
    fn r16(&mut self, _: u32) -> u16 { 0 }
    fn r32(&mut self, r: u32) -> u32 { self.reads.push(r); *self.regs.get(&r).unwrap_or(&0) }
    fn w8(&mut self, r: u32, _: u8) { self.writes.push((r, 0xdead)); }
    fn w16(&mut self, r: u32, _: u16) { self.writes.push((r, 0xdead)); }
    fn w32(&mut self, r: u32, v: u32) { self.writes.push((r, v)); self.regs.insert(r, v); }
}

// Literal offsets, not the production list: the three forcewake SET registers (intel_gt_regs.h:864, :909, :910).
static ALLOW: [Reg; 3] = [Reg::new(0xa188), Reg::new(0xa270), Reg::new(0xa278)];

#[test]
fn allowed_write_is_forwarded() {
    let mut g = WriteGuard::new(Dev::default(), &ALLOW);
    for r in [0xa188u32, 0xa270, 0xa278] {
        g.w32(r, 0x0001_0001);
    }
    assert_eq!(g.writes(), 3);
    assert_eq!(g.refusals(), 0);
    assert!(g.check().is_ok());
    assert_eq!(g.into_inner().writes, vec![(0xa188, 0x0001_0001), (0xa270, 0x0001_0001), (0xa278, 0x0001_0001)]);
}

#[test]
fn every_other_offset_is_refused_named_and_not_forwarded() {
    let mut g = WriteGuard::new(Dev::default(), &ALLOW);
    // the neighbours of each allowed register, the ack registers, FUSE2, and the CHV forcewake set register
    for r in [0xa184u32, 0xa18c, 0xa26c, 0xa274, 0xa27c, 0xd84, 0xd88, 0x130044, 0x9120, 0x1300b0, 0x0, 0x45400] {
        g.w32(r, 1);
    }
    assert_eq!(g.writes(), 0);
    assert_eq!(g.refusals(), 12);
    assert_eq!(g.first_refusal().unwrap().offset, 0xa184, "the FIRST refusal is the one named");
    let e = g.check().unwrap_err();
    assert_eq!(e, I915Error::WriteRefused { offset: 0xa184, width: 4 });
    assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
    assert!(g.into_inner().writes.is_empty(), "nothing reached the device");
}

#[test]
fn narrow_writes_to_an_allowed_register_are_refused() {
    let mut g = WriteGuard::new(Dev::default(), &ALLOW);
    g.w8(0xa188, 1);
    g.w16(0xa188, 1);
    assert_eq!(g.refusals(), 2);
    assert_eq!(g.first_refusal().unwrap().width, 1);
    assert_eq!(g.writes(), 0);
    assert!(g.into_inner().writes.is_empty());
}

#[test]
fn reads_pass_through_untouched() {
    let mut d = Dev::default();
    d.regs.insert(0x9120, 0x1234);
    let mut g = WriteGuard::new(d, &ALLOW);
    assert_eq!(g.r32(0x9120), 0x1234);
    assert_eq!(g.refusals(), 0);
    assert_eq!(g.into_inner().reads, vec![0x9120]);
}

#[test]
fn an_empty_allow_list_is_a_read_only_device() {
    static NONE: [Reg; 0] = [];
    let mut g = WriteGuard::new(Dev::default(), &NONE);
    g.w32(0xa188, 1);
    assert_eq!(g.writes(), 0);
    assert_eq!(g.refusals(), 1);
}

#[test]
fn error_names_are_distinct_and_stable() {
    let all = [
        I915Error::UnknownDevice { devid: 0x22b0 },
        I915Error::UnsupportedGeneration { devid: 0x22b0, gen: 8 },
        I915Error::WriteRefused { offset: 0, width: 4 },
        I915Error::AckTimeout { domain: 1, reg: Reg::new(0xd84), mask: 1, want: 1, got: 0, polls: 5 },
        I915Error::MmioUnreliable { domain: 1, reg: Reg::new(0xd84) },
        I915Error::NoSuchDomain { domain_bit: 8 },
        I915Error::Unbalanced { domain_bit: 1 },
        I915Error::ForcewakeNotHeld { offset: 0x9120, domains: 2 },
        I915Error::BadValue { what: "x", value: 0 },
    ];
    let names = ["UNKNOWN-DEVICE", "UNSUPPORTED-GENERATION", "WRITE-NOT-ALLOWED", "ACK-TIMEOUT", "MMIO-UNRELIABLE", "NO-SUCH-DOMAIN", "UNBALANCED-PUT", "FORCEWAKE-NOT-HELD", "BAD-VALUE"];
    assert_eq!(all.len(), names.len());
    for (e, n) in all.iter().zip(names) {
        assert_eq!(e.name(), n);
    }
    assert_eq!(i915_common_core::CONTRACT_ABI, 1);
}
