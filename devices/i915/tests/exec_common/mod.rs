// SPDX-License-Identifier: GPL-2.0-only
//! A simulated Gen9 GPU for the executor tests: a register file with the forcewake domains, the RESET_CTL / GDRST handshakes, a dense
//! GGTT window, a write log - and a sparse CPU memory for DMA grants. Both are `Rc<RefCell<..>>` handles so a test keeps its own view
//! after the chip moves into the `Executor`.
//!
//! EVERY unbounded loop a bug could create is turned into a FAILURE here: `relax` panics after `RELAX_LIMIT` calls, so a poll whose
//! `max_polls` check is removed fails the test instead of hanging it (forcewake's `max_polls` lesson, 2026-10-12).
#![allow(dead_code)]
use coralos_hal::Mmio;
use i915_device::exec::dma::CpuMem;
use i915_device::log::Log;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const RELAX_LIMIT: u32 = 2_000_000;
pub const GGTT_BASE: u32 = 8 << 20;
pub const BAR0_LEN: u32 = 16 << 20;

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

#[derive(Clone, Copy)]
pub struct Dom {
    pub set: u32,
    pub ack: u32,
    pub state: u32,
    pub ack_val: u32,
}

pub struct ChipState {
    pub regs: HashMap<u32, u32>,
    pub ggtt: Vec<u32>,
    /// Every register write that is not a forcewake set and not a GGTT store, in order.
    pub writes: Vec<(u32, u32)>,
    /// Forcewake set writes.
    pub fw_writes: Vec<(u32, u32)>,
    /// GGTT dword stores (offset, value), counted not logged beyond this cap.
    pub ggtt_writes: u32,
    pub ggtt_write_offsets: Vec<u32>,
    pub relax_calls: u32,
    pub park_ms: Vec<u64>,
    pub doms: [Dom; 3],
    /// RESET_CTL registers that model the ready-to-reset handshake: base + 0xd0.
    pub reset_ctls: Vec<u32>,
    pub ready_never: bool,
    pub gdrst_stuck: bool,
    /// The deadline expires after this many reads since `arm_deadline` (u32::MAX = never: a host with no clock).
    pub deadline_after_reads: u32,
    pub armed_reads: u32,
    pub reads: Vec<u32>,
    /// Called after every logged register write (not forcewake, not GGTT): models a device that answers a write.
    pub on_write: Option<Box<dyn FnMut(&mut HashMap<u32, u32>, u32, u32)>>,
}

#[derive(Clone)]
pub struct Chip(pub Rc<RefCell<ChipState>>);

impl Chip {
    pub fn new() -> Chip {
        let d = |set, ack| Dom { set, ack, state: 0, ack_val: 0 };
        Chip(Rc::new(RefCell::new(ChipState {
            regs: HashMap::new(),
            ggtt: vec![0; ((BAR0_LEN - GGTT_BASE) / 4) as usize],
            writes: vec![],
            fw_writes: vec![],
            ggtt_writes: 0,
            ggtt_write_offsets: vec![],
            relax_calls: 0,
            park_ms: vec![],
            // RENDER, GT, MEDIA (intel_uncore.c:2210-2219)
            doms: [d(0xa278, 0xd84), d(0xa188, 0x130044), d(0xa270, 0xd88)],
            reset_ctls: vec![0x2000 + 0xd0, 0x22000 + 0xd0, 0x12000 + 0xd0, 0x1a000 + 0xd0],
            ready_never: false,
            gdrst_stuck: false,
            deadline_after_reads: u32::MAX,
            armed_reads: 0,
            reads: vec![],
            on_write: None,
        })))
    }
    pub fn set(&self, reg: u32, v: u32) {
        self.0.borrow_mut().regs.insert(reg, v);
    }
    pub fn writes_to(&self, reg: u32) -> Vec<u32> {
        self.0.borrow().writes.iter().filter(|(r, _)| *r == reg).map(|(_, v)| *v).collect()
    }
    pub fn all_writes(&self) -> Vec<(u32, u32)> {
        self.0.borrow().writes.clone()
    }
    pub fn ggtt_entry(&self, off: u32) -> u64 {
        let s = self.0.borrow();
        let i = ((off - GGTT_BASE) / 4) as usize;
        (s.ggtt[i + 1] as u64) << 32 | s.ggtt[i] as u64
    }
}

impl Mmio for Chip {
    fn r8(&mut self, _: u32) -> u8 {
        panic!("8-bit read")
    }
    fn r16(&mut self, _: u32) -> u16 {
        panic!("16-bit read")
    }
    fn r32(&mut self, reg: u32) -> u32 {
        let mut s = self.0.borrow_mut();
        s.reads.push(reg);
        s.armed_reads = s.armed_reads.saturating_add(1);
        if reg >= GGTT_BASE {
            assert!(reg < BAR0_LEN, "read past BAR0");
            return s.ggtt[((reg - GGTT_BASE) / 4) as usize];
        }
        if let Some(d) = s.doms.iter().find(|d| d.ack == reg) {
            return d.ack_val;
        }
        let v = s.regs.get(&reg).copied().unwrap_or(0);
        if s.reset_ctls.contains(&reg) {
            // RESET_CTL: REQUEST_RESET (bit 0) set => READY_TO_RESET (bit 1) reads set, unless the engine never gets ready
            return if v & 1 != 0 && !s.ready_never { v | 2 } else { v };
        }
        if reg == 0x941c {
            // GEN6_GDRST: the hardware clears the domain bits when the reset completes
            return if s.gdrst_stuck { v } else { 0 };
        }
        v
    }
    fn w8(&mut self, r: u32, _: u8) {
        panic!("8-bit write {r:#x}")
    }
    fn w16(&mut self, r: u32, _: u16) {
        panic!("16-bit write {r:#x}")
    }
    fn w32(&mut self, reg: u32, v: u32) {
        let mut s = self.0.borrow_mut();
        if reg >= GGTT_BASE {
            assert!(reg < BAR0_LEN, "write past BAR0");
            let i = ((reg - GGTT_BASE) / 4) as usize;
            s.ggtt[i] = v;
            s.ggtt_writes += 1;
            if s.ggtt_write_offsets.len() < 4096 {
                s.ggtt_write_offsets.push(reg);
            }
            return;
        }
        if let Some(d) = s.doms.iter_mut().find(|d| d.set == reg) {
            let m = v >> 16;
            d.state = (d.state & !m) | (v & m & 0xffff);
            d.ack_val = d.state & 0x8001;
            s.fw_writes.push((reg, v));
            return;
        }
        s.writes.push((reg, v));
        if s.reset_ctls.contains(&reg) {
            // a masked register: the high half selects the bits the low half sets
            let old = s.regs.get(&reg).copied().unwrap_or(0);
            let m = v >> 16;
            s.regs.insert(reg, (old & !m) | (v & m & 0xffff));
        } else {
            s.regs.insert(reg, v);
        }
        if let Some(mut h) = s.on_write.take() {
            h(&mut s.regs, reg, v);
            s.on_write = Some(h);
        }
    }
    fn arm_deadline(&mut self, _: u64) {
        self.0.borrow_mut().armed_reads = 0;
    }
    fn deadline_expired(&mut self) -> bool {
        let s = self.0.borrow();
        s.armed_reads >= s.deadline_after_reads
    }
    fn relax(&mut self) {
        let mut s = self.0.borrow_mut();
        s.relax_calls += 1;
        assert!(s.relax_calls < RELAX_LIMIT, "a wait is spinning: no bound stopped it after {RELAX_LIMIT} relax calls");
    }
    fn park_ms(&mut self, ms: u64) {
        self.0.borrow_mut().park_ms.push(ms);
    }
}

/// Sparse CPU memory, dword granular, keyed by CPU address.
#[derive(Clone, Default)]
pub struct Mem(pub Rc<RefCell<HashMap<u64, u32>>>, pub Rc<RefCell<Vec<(u64, u32)>>>);

impl CpuMem for Mem {
    fn load32(&mut self, cpu: u64) -> u32 {
        assert_eq!(cpu % 4, 0, "unaligned load");
        self.0.borrow().get(&cpu).copied().unwrap_or(0)
    }
    fn store32(&mut self, cpu: u64, v: u32) {
        assert_eq!(cpu % 4, 0, "unaligned store");
        self.0.borrow_mut().insert(cpu, v);
    }
    fn flush(&mut self, cpu: u64, bytes: u32) {
        self.1.borrow_mut().push((cpu, bytes));
    }
}

impl Mem {
    pub fn get(&self, cpu: u64) -> u32 {
        self.0.borrow().get(&cpu).copied().unwrap_or(0)
    }
    pub fn put(&self, cpu: u64, v: u32) {
        self.0.borrow_mut().insert(cpu, v);
    }
    pub fn flushes(&self) -> Vec<(u64, u32)> {
        self.1.borrow().clone()
    }
}

// ---- a whole simulated device: chip + executor + granted memory ------------------------------------------------------------------
use coralos_hal::fake::FakeDma;
use i915_device::exec::dma::{GpuAlloc, GpuMem};
use i915_device::exec::Executor;
use i915_devinfo_core::{lookup, PlatformDescriptor};

pub type Gm = GpuMem<FakeDma, Mem>;

pub fn kbl() -> PlatformDescriptor {
    lookup(0x591c).expect("KBL row")
}

/// Pretend physical memory starts here (inside the 39-bit mask).
pub const PHYS_BASE: u64 = 0x10_0000;
/// The device's DMA endowment in the tests.
pub const ENDOWMENT: usize = 16 << 20;

pub fn memory() -> (Mem, Gm) {
    let mem = Mem::default();
    let gm = GpuMem::new(GpuAlloc::new(FakeDma::new(PHYS_BASE), &kbl(), ENDOWMENT), mem.clone());
    (mem, gm)
}

/// The executor with THIS build's grant (three registers unless the increment features are on).
pub fn built() -> (Chip, Mem, Executor<Chip>, Gm) {
    let chip = Chip::new();
    let (mem, gm) = memory();
    let ex = Executor::new(chip.clone(), &kbl()).unwrap();
    (chip, mem, ex, gm)
}

#[cfg(feature = "p3-exec")]
#[allow(unused_imports)]
pub use kernel_fakes::*;
#[cfg(feature = "p3-exec")]
mod kernel_fakes {
    // ---- the kernel's DMA lease, as a fake that enforces what the kernel enforces ---------------------------------------------------

    use i915_device::conn::Peer;
    use std::cell::RefCell;
    use std::rc::Rc;
    use i915_device::uapi::kernel_lease::{LeaseFail, LeaseSpec, LeaseSys, Revoked};

    /// What the fake kernel holds: live leases (handle, spec), the ordered events, and the failures a test can arm.
    #[derive(Default)]
    pub struct KernelLeases {
        pub next: u64,
        pub live: Vec<(u64, LeaseSpec)>,
        /// `create 0x..` / `revoke 0x..`, in order.
        pub events: Vec<String>,
        /// Every revoke is refused (an unproven teardown): the kernel will not say the lease ended.
        pub revoke_fails: bool,
        /// The next create is refused with this.
        pub create_fails: Option<LeaseFail>,
        /// Every spec ever accepted.
        pub created: Vec<LeaseSpec>,
    }

    /// The fake kernel's lease table, shared between the device (as its `LeaseSys`) and the test.
    #[derive(Clone, Default)]
    pub struct FakeLeases(pub Rc<RefCell<KernelLeases>>);

    impl FakeLeases {
        pub fn live_count(&self) -> usize {
            self.0.borrow().live.len()
        }
        pub fn events(&self) -> Vec<String> {
            self.0.borrow().events.clone()
        }
    }

    impl LeaseSys for FakeLeases {
        fn create(&mut self, s: &LeaseSpec) -> Result<u64, LeaseFail> {
            let mut k = self.0.borrow_mut();
            if let Some(f) = k.create_fails.take() {
                return Err(f);
            }
            if s.offset % 4096 != 0 || s.length % 4096 != 0 || s.length == 0 {
                return Err(LeaseFail::Refused { name: "LEASE-NOT-PAGE-EXACT", word: 0 });
            }
            assert_eq!(s.coherence, coralos_hal::Coherence::NonSnooped, "one cache type across every alias: object pages are write-combining");
            let (lo, hi) = (s.alloc_va + s.offset, s.alloc_va + s.offset + s.length);
            // the kernel's page-exclusivity: a page under a live lease cannot be leased again
            if k.live.iter().any(|(_, o)| lo < o.alloc_va + o.offset + o.length && o.alloc_va + o.offset < hi) {
                return Err(LeaseFail::Refused { name: "LEASE-PAGE-ALREADY-LEASED", word: 0 });
            }
            k.next += 1;
            let h = coralos_rt::region::LEASE_CAP_TAG | k.next;
            k.live.push((h, *s));
            k.created.push(*s);
            k.events.push(format!("create {h:#x}"));
            Ok(h)
        }
        fn revoke(&mut self, handle: u64) -> Result<Revoked, LeaseFail> {
            let mut k = self.0.borrow_mut();
            if k.revoke_fails {
                return Err(LeaseFail::Refused { name: "LEASE-UNKNOWN-WORD", word: 1 });
            }
            match k.live.iter().position(|(h, _)| *h == handle) {
                Some(i) => {
                    k.live.remove(i);
                    k.events.push(format!("revoke {handle:#x}"));
                    Ok(Revoked::Ended)
                }
                None => Ok(Revoked::AlreadyGone),
            }
        }
    }

    /// The client every session of the tests is: a request endpoint and a node derived from the incarnation.
    pub fn peer_for(inc: u64) -> Peer {
        Peer { req_ep: 100 + inc as u16, node: 0x7000 + inc }
    }
}
