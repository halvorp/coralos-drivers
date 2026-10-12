// SPDX-License-Identifier: GPL-2.0-only
//! Increment 1 end to end on a simulated GPU: the identity gate, the forcewake proof of life, the marker lines, and the MMIO write
//! allow-list (the ONLY writes are the three forcewake SET registers, and a CHV / unknown id never touches BAR0).
mod common;
use common::*;
use coralos_hal::Mmio;
use coralos_rt::mmio::{MmioError, MmioRaw, WriteWindow};
use i915_common_core::{I915Error, WriteGuard};
use i915_device::gt::{gate, proof_of_life, ByRef};
use i915_device::identity::PciIdentity;
use i915_device::regio::{Bar, BarMmio, Clock, RegRead, BAR0_NEED_BYTES, BAR0_WRITE_RANGES};
use i915_device::startup::{bringup, BringupError};
use i915_uncore_core::WRITE_ALLOWLIST;

const GET: u32 = 0x0001_0001;
const PUT: u32 = 0x0001_0000;

// ---- a simulated GPU: the display Window plus the Gen9 forcewake domains and the GT fuse registers -----------------------------

#[derive(Clone, Copy, PartialEq)]
enum Ack {
    Immediate,
    Never,
    StuckSet,
    AllOnes,
}

struct Dom {
    set: u32,
    ack: u32,
    beh: Ack,
    state: u32,
    ack_val: u32,
}

struct Chip {
    win: Window,
    doms: Vec<Dom>,
    gt: Vec<(u32, u32)>,
    writes: Vec<(u32, u32)>,
    reads: Vec<u32>,
    deadline_reads: u32,
    armed: u32,
    /// every access in order: ('R' | 'W', register)
    trace: Vec<(char, u32)>,
}

const FUSE2: u32 = 0x9120;

impl Chip {
    fn new(win: Window) -> Chip {
        let d = |set, ack| Dom { set, ack, beh: Ack::Immediate, state: 0, ack_val: 0 };
        Chip {
            win,
            // RENDER, GT, MEDIA
            doms: vec![d(0xa278, 0xd84), d(0xa188, 0x130044), d(0xa270, 0xd88)],
            // FUSE2: 1 slice, subslice 3 disabled; EU_DISABLE0: EU 0 of subslice 1 fused off; RPM_CONFIG0; CTC_MODE shift 2 (12 MHz);
            // RP_STATE_CAP RP0 0x18 / RP1 0x0b / RPN 0x09
            gt: vec![(FUSE2, 0x0280_0000), (0x9134, 0x100), (0x9138, 0), (0x913c, 0), (0xd00, 0x8), (0xa26c, 0x4), (0x145998, 0x0009_0b18)],
            writes: vec![],
            reads: vec![],
            deadline_reads: 50,
            armed: 0,
            trace: vec![],
        }
    }
    fn dom(&mut self, set: u32) -> &mut Dom {
        self.doms.iter_mut().find(|d| d.set == set).unwrap()
    }
    fn set_gt(&mut self, off: u32, v: u32) {
        self.gt.iter_mut().find(|(o, _)| *o == off).unwrap().1 = v;
    }
}

impl Mmio for Chip {
    fn r8(&mut self, _: u32) -> u8 { panic!() }
    fn r16(&mut self, _: u32) -> u16 { panic!() }
    fn r32(&mut self, reg: u32) -> u32 {
        self.reads.push(reg);
        self.trace.push(('R', reg));
        if let Some(d) = self.doms.iter_mut().find(|d| d.ack == reg) {
            self.armed += 1;
            return if d.beh == Ack::AllOnes { u32::MAX } else { d.ack_val };
        }
        if let Some((_, v)) = self.gt.iter().find(|(o, _)| *o == reg) {
            return *v;
        }
        panic!("read of unmodelled register {reg:#x}")
    }
    fn w8(&mut self, r: u32, _: u8) { panic!("8-bit write {r:#x}") }
    fn w16(&mut self, r: u32, _: u16) { panic!("16-bit write {r:#x}") }
    fn w32(&mut self, reg: u32, v: u32) {
        self.writes.push((reg, v));
        self.trace.push(('W', reg));
        let Some(d) = self.doms.iter_mut().find(|d| d.set == reg) else { panic!("write to {reg:#x}, which is not a forcewake set register") };
        let m = v >> 16;
        d.state = (d.state & !m) | (v & m & 0xffff);
        match d.beh {
            Ack::Immediate | Ack::AllOnes => d.ack_val = d.state & 0x8001,
            Ack::Never => {}
            Ack::StuckSet => d.ack_val |= d.state & 0x8001,
        }
    }
    fn arm_deadline(&mut self, _: u64) { self.armed = 0 }
    fn deadline_expired(&mut self) -> bool { self.armed >= self.deadline_reads }
}

impl RegRead for Chip {
    fn len(&self) -> u64 { 0x40_0000 }
    fn read32(&mut self, off: u32) -> Result<u32, MmioError> {
        if self.gt.iter().any(|(o, _)| *o == off) || self.doms.iter().any(|d| d.ack == off) {
            return Ok(Mmio::r32(self, off));
        }
        self.win.read32(off)
    }
}

impl Bar for Chip {
    fn take_fault(&mut self) -> Option<MmioError> { None }
}

fn ident(device: u16) -> PciIdentity {
    PciIdentity { vendor: 0x8086, device, revision: 0x02, base_class: 3, sub_class: 0, subsystem_vendor: 0x1234, subsystem_device: 0x5678 }
}

fn find<'a>(l: &'a Lines, prefix: &str) -> Vec<&'a String> {
    l.0.iter().filter(|s| s.starts_with(prefix)).collect()
}

// ---- the proof of life -------------------------------------------------------------------------------------------------------

#[test]
fn the_pixelbook_go_proves_life_and_prints_exactly_the_three_markers() {
    let mut chip = Chip::new(bar_vector_a());
    let mut lg = Lines::default();
    let p = proof_of_life(&mut chip, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut lg).unwrap();

    assert_eq!(lg.0.len(), 3, "{:#?}", lg.0);
    assert_eq!(lg.0[0], "[I915-PROOF-OF-LIFE id=591c rev=02 platform=kbl-gt2 gen=9 llc=1 display_ver=9 gt=2]");
    assert_eq!(
        lg.0[1],
        "[I915-GT-FUSE fuse2=0x2800000 eu_dis=0x100/0x0/0x0 slices=1 subslices=3 eu=23 ts_hz=12000000 rp_state_cap=0x90b18 rpm_config0=0x8 ctc_mode=0x4]"
    );
    assert_eq!(lg.0[2], "[I915-FORCEWAKE domains=GT,RENDER,MEDIA ack=ok released=1 writes=6 refused=0]");

    assert_eq!((p.fuse2, p.eu_disable, p.rpm_config0, p.ctc_mode, p.rp_state_cap, p.ts_hz), (0x0280_0000, [0x100, 0, 0], 0x8, 0x4, 0x0009_0b18, 12_000_000));
    assert_eq!((p.sseu.slice_total(), p.sseu.subslice_total(), p.sseu.eu_total), (1, 3, 23));
    assert_eq!(p.writes, 6);
}

#[test]
fn the_only_writes_are_the_forcewake_set_registers_in_get_then_put_order() {
    let mut chip = Chip::new(bar_vector_a());
    proof_of_life(&mut chip, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut Lines::default()).unwrap();
    // GT, RENDER, MEDIA taken one at a time, then released in the same order (the Chip panics on any other register)
    assert_eq!(chip.writes, vec![(0xa188, GET), (0xa278, GET), (0xa270, GET), (0xa188, PUT), (0xa278, PUT), (0xa270, PUT)]);
    // the release was observed: every ack cleared
    assert!(chip.doms.iter().all(|d| d.ack_val & 1 == 0));
    // and every GT register read happened while the domains were held: after the last GET write, before the first PUT write
    let last_get = chip.trace.iter().enumerate().filter(|(_, e)| e.0 == 'W').map(|(i, _)| i).nth(2).unwrap();
    let first_put = chip.trace.iter().enumerate().filter(|(_, e)| e.0 == 'W').map(|(i, _)| i).nth(3).unwrap();
    for r in [FUSE2, 0x9134, 0x9138, 0x913c, 0xa26c, 0xd00, 0x145998] {
        let at = chip.trace.iter().position(|e| *e == ('R', r)).unwrap_or_else(|| panic!("{r:#x} was not read"));
        assert!(at > last_get && at < first_put, "{r:#x} read at {at}, outside the held window {last_get}..{first_put}");
    }
}

/// The SHIPPED build: no increment feature. (With `p2-memory` / `p3-exec` the grant widens by design - see tests/exec_allow.rs; the
/// forcewake registers stay on it, which the next test proves.)
#[cfg(not(feature = "p2-memory"))]
#[test]
fn the_write_allow_list_is_the_three_set_registers_in_both_layers() {
    let mut got: Vec<u32> = WRITE_ALLOWLIST.iter().map(|r| r.offset()).collect();
    got.sort();
    assert_eq!(got, vec![0xa188, 0xa270, 0xa278]);
    let mut win: Vec<u64> = BAR0_WRITE_RANGES.iter().map(|r| r.off).collect();
    win.sort();
    assert_eq!(win, vec![0xa188, 0xa270, 0xa278]);
    assert!(BAR0_WRITE_RANGES.iter().all(|r| r.len == 4));
}

#[cfg(feature = "p2-memory")]
#[test]
fn an_increment_build_still_grants_the_three_set_registers_first_in_both_layers() {
    let fw = [0xa188u32, 0xa270, 0xa278];
    let mut got: Vec<u32> = WRITE_ALLOWLIST.iter().map(|r| r.offset()).collect();
    got.sort();
    assert_eq!(got, fw);
    for r in fw {
        assert!(BAR0_WRITE_RANGES.iter().any(|w| w.off == r as u64 && w.len == 4), "{r:#x} missing from the window grant");
    }
}

#[test]
fn any_other_write_is_refused_by_the_guard_and_never_reaches_the_device() {
    let mut chip = Chip::new(bar_vector_a());
    {
        let mut g = WriteGuard::new(ByRef(&mut chip), &WRITE_ALLOWLIST);
        for r in [0x45400u32, 0x70180, 0x46000, 0xa184, 0xa18c, 0xd84, 0x130044, 0x9120, 0x1300b0, 0x64000, 0x6c058] {
            g.w32(r, 0xffff_ffff);
        }
        g.w16(0xa188, 1);
        g.w8(0xa188, 1);
        assert_eq!(g.refusals(), 13);
        assert_eq!(g.writes(), 0);
        assert_eq!(g.check(), Err(I915Error::WriteRefused { offset: 0x45400, width: 4 }));
    }
    assert!(chip.writes.is_empty(), "the Chip would have panicked on a write to a non-forcewake register");
}

// ---- the gate: CHV and unknown ids are refused before BAR0 is touched --------------------------------------------------------

#[test]
fn a_cherry_trail_id_is_refused_by_name_and_bar0_is_never_touched() {
    for id in [0x22b0u16, 0x22b1, 0x22b2, 0x22b3] {
        let mut chip = Chip::new(bar_vector_a());
        let mut cfg = config_page(id, 0x21);
        let mut lg = Lines::default();
        let e = bringup(&mut chip, &mut cfg, &mut lg).err().unwrap();
        assert_eq!(e, BringupError::Gate(I915Error::UnsupportedGeneration { devid: id, gen: 8 }), "{id:#x}");
        assert_eq!(e.name(), "UNSUPPORTED-GENERATION");
        assert_eq!(lg.0, vec![format!("[I915-PROOF-OF-LIFE REFUSED id={id:04x} why=UNSUPPORTED-GENERATION platform=chv gen=8 writes=0]")]);
        assert!(chip.reads.is_empty() && chip.writes.is_empty(), "BAR0 untouched");
        assert!(chip.win.reads.is_empty(), "not even a display register was read");
    }
}

#[test]
fn an_unknown_id_is_refused_by_name_and_bar0_is_never_touched() {
    for id in [0x1616u16, 0x3e92, 0x5a85, 0x0000, 0x591f] {
        let mut chip = Chip::new(bar_vector_a());
        let mut cfg = config_page(id, 0x01);
        let mut lg = Lines::default();
        let e = bringup(&mut chip, &mut cfg, &mut lg).err().unwrap();
        assert_eq!(e, BringupError::Gate(I915Error::UnknownDevice { devid: id }), "{id:#x}");
        assert_eq!(lg.0, vec![format!("[I915-PROOF-OF-LIFE REFUSED id={id:04x} why=UNKNOWN-DEVICE writes=0]")]);
        assert!(chip.reads.is_empty() && chip.writes.is_empty() && chip.win.reads.is_empty());
    }
}

#[test]
fn a_non_intel_vendor_with_a_kaby_lake_device_id_is_refused() {
    let mut chip = Chip::new(bar_vector_a());
    let mut cfg = config_full(0x1234, 0x591c, 0x02, 0x03, 0x00);
    let mut lg = Lines::default();
    let e = bringup(&mut chip, &mut cfg, &mut lg).err().unwrap();
    assert_eq!(e, BringupError::Gate(I915Error::UnknownDevice { devid: 0x591c }));
    assert!(chip.reads.is_empty() && chip.writes.is_empty());
    assert!(lg.0[0].starts_with("[I915-PROOF-OF-LIFE REFUSED id=591c why=UNKNOWN-DEVICE vendor-not-intel"));
}

#[test]
fn gate_alone_reads_nothing_and_accepts_kbl_and_skl() {
    let mut lg = Lines::default();
    assert_eq!(gate(&ident(0x591c), &mut lg).unwrap().name(), "kbl-gt2");
    assert_eq!(gate(&ident(0x1916), &mut lg).unwrap().name(), "skl-gt2");
    assert!(lg.0.is_empty(), "an accepted id prints nothing at the gate");
}

// ---- the whole bring-up ------------------------------------------------------------------------------------------------------

#[test]
fn bringup_runs_the_readout_then_the_proof() {
    let mut chip = Chip::new(bar_vector_a());
    let mut cfg = config_page(0x591c, 0x02);
    let mut lg = Lines::default();
    let (ready, proof) = bringup(&mut chip, &mut cfg, &mut lg).expect("a Pixelbook Go");
    assert_eq!(ready.active_pipes, 1);
    assert_eq!(proof.writes, 6);
    let i_ready = lg.0.iter().position(|l| l.starts_with("[I915-KBL-DEV READY")).unwrap();
    let i_pol = lg.0.iter().position(|l| l.starts_with("[I915-PROOF-OF-LIFE id=")).unwrap();
    let i_fw = lg.0.iter().position(|l| l.starts_with("[I915-FORCEWAKE domains=")).unwrap();
    assert!(i_ready < i_pol && i_pol < i_fw, "readout, then proof of life, then the forcewake verdict");
    assert!(lg.0[i_ready].ends_with("writes=0 refusals=0]"), "the readout stage still writes nothing");
    assert_eq!(chip.writes.len(), 6);
}

// ---- failures stop the device and name the evidence --------------------------------------------------------------------------

#[test]
fn an_absent_gt_ack_is_a_named_failure_and_nothing_further_is_written() {
    let mut chip = Chip::new(bar_vector_a());
    chip.dom(0xa188).beh = Ack::Never;
    let mut lg = Lines::default();
    let e = proof_of_life(&mut chip, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut lg).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { domain: 2, .. }), "{e:?}");
    assert_eq!(lg.0.len(), 2);
    assert_eq!(lg.0[1], "[I915-FORCEWAKE FAIL domain=GT why=ACK-TIMEOUT reg=0x130044 got=0x0 want=0x1 polls=50]");
    assert!(chip.writes.iter().all(|w| w.0 == 0xa188), "RENDER and MEDIA were never requested: {:x?}", chip.writes);
    assert!(!chip.writes.iter().any(|w| w.1 == PUT), "no release was attempted");
    assert!(find(&lg, "[I915-GT-FUSE").is_empty() && find(&lg, "[I915-FORCEWAKE domains=").is_empty(), "no success markers");
}

#[test]
fn a_media_failure_leaves_gt_and_render_as_taken_and_stops() {
    let mut chip = Chip::new(bar_vector_a());
    chip.dom(0xa270).beh = Ack::Never;
    let mut lg = Lines::default();
    let e = proof_of_life(&mut chip, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut lg).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { domain: 4, .. }));
    assert!(lg.0[1].starts_with("[I915-FORCEWAKE FAIL domain=MEDIA why=ACK-TIMEOUT reg=0xd88 got=0x0 want=0x1 polls="));
    assert!(!chip.writes.iter().any(|w| w.1 == PUT), "the device stopped: it did not clean up with further writes");
    assert!(!chip.reads.contains(&FUSE2), "no GT register was read after the failure");
}

#[test]
fn a_dead_bar_is_mmio_unreliable_before_any_write() {
    let mut chip = Chip::new(bar_vector_a());
    chip.dom(0xa188).beh = Ack::AllOnes;
    let mut lg = Lines::default();
    let e = proof_of_life(&mut chip, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut lg).unwrap_err();
    assert_eq!(e.name(), "MMIO-UNRELIABLE");
    assert_eq!(lg.0[1], "[I915-FORCEWAKE FAIL domain=GT why=MMIO-UNRELIABLE reg=0x130044 got=0xffffffff]");
    assert!(chip.writes.is_empty());
}

#[test]
fn an_all_ones_fuse_register_is_not_a_fuse_value() {
    let mut chip = Chip::new(bar_vector_a());
    chip.set_gt(FUSE2, u32::MAX);
    let mut lg = Lines::default();
    let e = proof_of_life(&mut chip, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut lg).unwrap_err();
    assert_eq!(e.name(), "MMIO-UNRELIABLE");
    assert!(find(&lg, "[I915-GT-FUSE").is_empty());
    assert!(find(&lg, "[I915-FORCEWAKE FAIL").len() == 1);
}

#[test]
fn a_release_that_never_shows_in_the_ack_is_reported_released_zero() {
    let mut chip = Chip::new(bar_vector_a());
    chip.dom(0xa270).beh = Ack::StuckSet;
    let mut lg = Lines::default();
    let e = proof_of_life(&mut chip, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut lg).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { domain: 4, want: 0, got: 1, .. }), "{e:?}");
    assert!(find(&lg, "[I915-GT-FUSE").len() == 1, "the reads had already succeeded");
    assert!(lg.0.iter().any(|l| l == "[I915-FORCEWAKE domains=GT,RENDER,MEDIA ack=ok released=0]"));
    assert!(find(&lg, "[I915-FORCEWAKE FAIL domain=MEDIA why=ACK-TIMEOUT").len() == 1);
}

#[test]
fn the_divide_logic_clock_reads_the_timestamp_override_register() {
    let mut chip = Chip::new(bar_vector_a());
    chip.set_gt(0xa26c, 0x1); // CTC_SOURCE_DIVIDE_LOGIC
    chip.gt.push((0x44074, 23 | (1 << 12))); // divider 23 -> 24 MHz, denominator 1 -> +500 kHz
    let mut lg = Lines::default();
    let p = proof_of_life(&mut chip, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut lg).unwrap();
    assert_eq!(p.ts_hz, 24_500_000);
    assert!(lg.0[1].contains("ts_hz=24500000"));
}

// ---- the window layer: the real WriteWindow + BarMmio --------------------------------------------------------------------------

struct Heap {
    mem: Vec<u32>,
}
impl MmioRaw for Heap {
    fn map(&mut self, slot: u32) -> u64 { if slot == 0 { self.mem.as_mut_ptr() as u64 } else { 0 } }
    fn len(&mut self, slot: u32) -> u64 { if slot == 0 { self.mem.len() as u64 * 4 } else { 0 } }
}
struct FakeClock(std::rc::Rc<std::cell::Cell<u64>>);
impl Clock for FakeClock {
    fn now_ms(&mut self) -> u64 { self.0.get() }
}

#[test]
fn bar_mmio_over_the_real_write_window_forwards_only_the_allow_list_and_latches_the_rest() {
    let mut heap = Heap { mem: vec![0xabcd_0000; (BAR0_NEED_BYTES / 4) as usize] };
    let win = WriteWindow::open(&mut heap, 0, BAR0_NEED_BYTES, &BAR0_WRITE_RANGES).unwrap();
    let now = std::rc::Rc::new(std::cell::Cell::new(1000u64));
    let mut bar = BarMmio::new(win, FakeClock(now.clone()));

    bar.w32(0xa188, 0x0001_0001);
    assert_eq!(bar.take_fault(), None);
    assert_eq!(bar.r32(0xa188), 0x0001_0001, "an allowed write landed");
    bar.w32(0x45400, 0xdead_beef);
    assert_eq!(bar.take_fault(), Some(MmioError::WriteNotAllowed { off: 0x45400, width: 4 }));
    assert_eq!(bar.r32(0x45400), 0xabcd_0000, "a refused write changed nothing");
    bar.w8(0xa188, 1);
    assert_eq!(bar.take_fault(), Some(MmioError::WriteNotAllowed { off: 0xa188, width: 1 }));
    bar.w16(0xa188, 1);
    assert_eq!(bar.take_fault(), Some(MmioError::WriteNotAllowed { off: 0xa188, width: 2 }));
    assert_eq!(bar.r32(0xa188), 0x0001_0001);

    // a read past the window returns all-ones (a dead BAR to the uncore) and latches the first fault only
    assert_eq!(bar.r32(BAR0_NEED_BYTES as u32), u32::MAX);
    bar.w32(0x1, 1);
    assert_eq!(bar.take_fault(), Some(MmioError::OutOfRange { off: BAR0_NEED_BYTES, width: 4, len: BAR0_NEED_BYTES }));
    assert_eq!(bar.take_fault(), None, "taking clears the latch");

    // the wall-clock deadline
    assert!(!bar.deadline_expired(), "none armed");
    bar.arm_deadline(50);
    assert!(!bar.deadline_expired());
    now.set(1049);
    assert!(!bar.deadline_expired());
    now.set(1050);
    assert!(bar.deadline_expired());
    assert_eq!(RegRead::len(&bar), BAR0_NEED_BYTES);
}

#[test]
fn the_whole_proof_runs_over_the_real_window_against_a_register_image_and_stops_on_the_missing_ack() {
    // A static register image has no forcewake dynamics, so the GT ack never rises: the proof must stop with the named timeout,
    // having written only 0xa188 (the real window refuses everything else, and the guard above it).
    let mut heap = Heap { mem: vec![0; (BAR0_NEED_BYTES / 4) as usize] };
    heap.mem[0x9120 / 4] = 0x0280_0000;
    let win = WriteWindow::open(&mut heap, 0, BAR0_NEED_BYTES, &BAR0_WRITE_RANGES).unwrap();
    let now = std::rc::Rc::new(std::cell::Cell::new(0u64));
    // a clock that advances 1 ms per call, so the 50 ms deadline fires after ~50 polls
    struct Tick(std::rc::Rc<std::cell::Cell<u64>>);
    impl Clock for Tick {
        fn now_ms(&mut self) -> u64 {
            self.0.set(self.0.get() + 1);
            self.0.get()
        }
    }
    let mut bar = BarMmio::new(win, Tick(now));
    let mut lg = Lines::default();
    let e = proof_of_life(&mut bar, &i915_devinfo_core::lookup(0x591c).unwrap(), &ident(0x591c), &mut lg).unwrap_err();
    assert!(matches!(e, I915Error::AckTimeout { domain: 2, .. }), "{e:?}");
    assert_eq!(bar.take_fault(), None, "the proof wrote only what the window allows");
}
