// SPDX-License-Identifier: GPL-2.0-only
//! A simulated GT register file: the three Gen9 forcewake domains with scriptable ack behaviour, plus plain readable registers.
//! Offsets here are written as LITERALS (not imported from the production crates), so a wrong production offset disagrees with the
//! simulation instead of agreeing with itself.
#![allow(dead_code)]
use coralos_hal::Mmio;
use std::collections::BTreeMap;

pub const KERNEL: u32 = 0x1;
pub const FALLBACK: u32 = 0x8000;

#[derive(Clone, Copy, Debug)]
pub enum Beh {
    /// The ack follows the set register at once.
    Immediate,
    /// The ack follows after this many reads of the ack register.
    Delayed(u32),
    /// The ack never changes from 0.
    Never,
    /// The KERNEL ack sets on request but never clears.
    StuckSet,
    /// The KERNEL ack follows only when a FALLBACK toggle is seen (HSDES #1604254524 behaviour).
    NeedsFallback,
    /// Every read of the ack register returns all-ones.
    AllOnes,
}

#[derive(Debug)]
pub struct Dom {
    pub set: u32,
    pub ack: u32,
    pub beh: Beh,
    pub set_state: u32,
    pub ack_val: u32,
    pending: Option<(u32, u32)>, // (target, reads left)
    fb_seen: bool,
}

pub struct Sim {
    pub doms: Vec<Dom>,
    pub plain: BTreeMap<u32, u32>,
    pub writes: Vec<(u32, u32)>,
    pub reads: Vec<u32>,
    /// A wait's wall-clock deadline expires after this many ack-register reads since `arm_deadline` (None = never).
    pub deadline_after: Option<u32>,
    armed_reads: u32,
}

impl Sim {
    pub fn gen9(beh: Beh) -> Sim {
        let mk = |set, ack| Dom { set, ack, beh, set_state: 0, ack_val: 0, pending: None, fb_seen: false };
        Sim {
            // RENDER, GT, MEDIA: sets 0xa278 / 0xa188 / 0xa270, acks 0xd84 / 0x130044 / 0xd88 (intel_gt_regs.h)
            doms: vec![mk(0xa278, 0xd84), mk(0xa188, 0x130044), mk(0xa270, 0xd88)],
            plain: BTreeMap::new(),
            writes: vec![],
            reads: vec![],
            deadline_after: None,
            armed_reads: 0,
        }
    }
    pub fn dom_by_set(&mut self, set: u32) -> &mut Dom {
        self.doms.iter_mut().find(|d| d.set == set).expect("no such set register")
    }
    pub fn writes_to(&self, off: u32) -> Vec<u32> {
        self.writes.iter().filter(|w| w.0 == off).map(|w| w.1).collect()
    }
    fn follow(d: &mut Dom, target: u32) {
        d.ack_val = target;
    }
}

impl Mmio for Sim {
    fn r8(&mut self, _: u32) -> u8 { panic!("8-bit read") }
    fn r16(&mut self, _: u32) -> u16 { panic!("16-bit read") }
    fn r32(&mut self, reg: u32) -> u32 {
        self.reads.push(reg);
        if let Some(d) = self.doms.iter_mut().find(|d| d.ack == reg) {
            self.armed_reads += 1;
            if let Beh::AllOnes = d.beh {
                return u32::MAX;
            }
            if let Some((t, n)) = d.pending {
                if n <= 1 {
                    d.ack_val = t;
                    d.pending = None;
                } else {
                    d.pending = Some((t, n - 1));
                }
            }
            return d.ack_val;
        }
        self.plain.get(&reg).copied().unwrap_or(0)
    }
    fn w8(&mut self, r: u32, _: u8) { panic!("8-bit write {r:#x}") }
    fn w16(&mut self, r: u32, _: u16) { panic!("16-bit write {r:#x}") }
    fn w32(&mut self, reg: u32, v: u32) {
        self.writes.push((reg, v));
        if let Some(d) = self.doms.iter_mut().find(|d| d.set == reg) {
            // masked register: the high half selects which low bits change (i915_reg_defs.h:108)
            let mask = v >> 16;
            d.set_state = (d.set_state & !mask) | (v & mask & 0xffff);
            let target = d.set_state & (KERNEL | FALLBACK);
            match d.beh {
                Beh::Immediate | Beh::AllOnes => Sim::follow(d, target),
                Beh::Delayed(n) => d.pending = Some((target, n)),
                Beh::Never => {}
                Beh::StuckSet => {
                    let keep = d.ack_val & KERNEL;
                    Sim::follow(d, (target & FALLBACK) | (target & KERNEL) | keep);
                }
                Beh::NeedsFallback => {
                    if d.set_state & FALLBACK != 0 {
                        d.fb_seen = true;
                    }
                    let kernel = if d.fb_seen { d.set_state & KERNEL } else { d.ack_val & KERNEL };
                    Sim::follow(d, (d.set_state & FALLBACK) | kernel);
                }
            }
        }
        // writes to any other register are simply recorded; tests assert on them (or on a WriteGuard's refusal count)
    }
    fn arm_deadline(&mut self, _ms: u64) {
        self.armed_reads = 0;
    }
    fn deadline_expired(&mut self) -> bool {
        matches!(self.deadline_after, Some(n) if self.armed_reads >= n)
    }
}
