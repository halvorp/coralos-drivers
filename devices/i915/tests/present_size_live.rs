// SPDX-License-Identifier: GPL-2.0-only
//! THE LOGICAL SCANOUT SIZE end to end (`p4-present`, `display::size`): the live loop over a fake kernel and a simulated chip whose plane
//! latches, on the Pixelbook Go's adopted display (1920x1080 fitted onto the 3840x2160 eDP panel) with the default policy - panel / 1.5.
//! The authority is offered 2560x1440, its pair is made at that size, its FIRST present writes the whole fastset (pipe source, scaler,
//! plane, watermarks) with PLANE_SURF last, the next presents write PLANE_SURF only, and the authority's end puts the firmware's
//! geometry back with its framebuffer.
#![cfg(feature = "p4-present")]
mod common;
mod exec_common;
mod net_common;
use exec_common::{kbl, Chip, Mem, PHYS_BASE};
use i915_device::exec::dma::{GpuAlloc, GpuMem};
use i915_device::exec::Executor;
use coralos_hal::fake::FakeDma;
use i915_device::display::Adoption;
use i915_device::live::{ConnId, Live, NoRegions, Step, Wake};
use i915_device::regio::RegRead;
use i915_device::uapi::{bring_up_render, Hw, RenderConfig};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_display_modeset_core::adopt::FirmwareScanout;
use i915_display_modeset_core::resize::{read_firmware_geometry, FirmwareGeometry, LogicalSize};
use i915_gt_core::emit::FlushQuirks;
use i915_kbl_display_core::types::Pipe;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::msg::display::*;
use i915_wire::msg::session::*;
use i915_wire::msg::Body;
use i915_wire::Opcode;
use net_common::*;

type L = Live<FakeT, NoRegions, Chip, FakeDma, Mem, exec_common::Lines>;
const PLANE_SURF: u32 = 0x7019c;
const PLANE_SURFLIVE: u32 = 0x701ac;
const PIPESRC_A: u32 = 0x6001c;
const PS_1A_VPHASE: u32 = 0x68188;
const PLANE_SIZE: u32 = 0x70190;
const PLANE_STRIDE: u32 = 0x70188;

/// The live device on the Atlas display, its logical size chosen by `policy` from the geometry the firmware left (a DDB of 860 blocks
/// and enabled watermark levels on top of the readout fixture).
fn world(policy: LogicalSize) -> (L, FakeT, Chip) {
    let (mut bar, mut cfg, mut lg) = (common::bar_atlas(), common::config_page(0x591c, 0x02), common::Lines::default());
    let ready = i915_device::startup::startup(&mut bar, &mut cfg, &mut lg).unwrap();
    let adoption: Adoption = ready.adoption;
    let fw = FirmwareScanout::from_readout(&ready.state, Pipe::A, 0).unwrap();
    for r in (0x70240..=0x7027c).step_by(4).chain([0x68188, 0x68194]) {
        bar.set(r, 0);
    }
    let g = read_firmware_geometry(|off| RegRead::read32(&mut bar, off).unwrap(), &ready.device.caps, &ready.state, adoption.adopted().unwrap()).unwrap();
    let g = FirmwareGeometry { buf_cfg: 859 << 16, wm: [0x8000_0004, 0x8000_4011, 0x8000_4011, 0x8000_8017, 0x8000_c025, 0x8000_c031, 0x8001_003d, 0x8001_4048], ..g };
    // two 2560x1440 images are 2 x 14 MiB: more than the shared fixture's 16 MiB endowment (silicon's is I915_DEVICE_DMA_PAGES, 192 MiB)
    let chip = Chip::new();
    let mut gm = GpuMem::new(GpuAlloc::new(FakeDma::new(PHYS_BASE), &kbl(), 64 << 20), Mem::default());
    let mut ex = Executor::new(chip.clone(), &kbl()).unwrap();
    chip.0.borrow_mut().on_write = Some(Box::new(|regs, reg, v| {
        if reg == PLANE_SURF {
            regs.insert(PLANE_SURFLIVE, v & 0xffff_f000);
        }
    }));
    let mut log = exec_common::Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 << 30).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let rcfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: 1000, keep: adoption.keep() };
    let mut svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, rcfg).unwrap();
    svc.set_display(adoption);
    svc.set_firmware_scanout(Some(fw));
    svc.set_scanout_size(&mut log, &ready.device.caps, Some(&g), policy);
    let t = FakeT::default();
    t.0.borrow_mut().now = 1;
    t.0.borrow_mut().auto = true;
    (Live::new(t.clone(), NoRegions, svc, ex, gm, log).unwrap(), t, chip)
}

fn run(live: &mut L) {
    while live.step() != Step::Closed {}
}

fn payload<B: Body>(b: &B) -> Vec<u8> {
    let mut v = vec![0u8; B::SIZE.max(64)];
    let n = b.to_payload(&mut v).unwrap();
    v.truncate(n);
    v
}

fn call(live: &mut L, t: &FakeT, id: ConnId, op: Opcode, inc: u64, p: &[u8]) -> Resp {
    t.send_frame(id, &frame(op, 7, inc, p));
    run(live);
    let mut f = t.drain_frames(id);
    assert_eq!(f.len(), 1, "{}: exactly one reply", op.name());
    parse(&f.remove(0))
}

fn authority(live: &mut L, t: &FakeT) -> (ConnId, u64) {
    let id = t.connect_display();
    run(live);
    let inc = call(live, t, id, Opcode::SessionOpen, 0, &payload(&SessionOpenReq::default())).body::<SessionOpenRep>().incarnation;
    (id, inc)
}

fn frame_on(live: &mut L, t: &FakeT, id: ConnId, inc: u64, h: u32) -> u64 {
    assert_eq!(call(live, t, id, Opcode::ScanoutBegin, inc, &payload(&ScanoutBeginReq { handle: h, ..Default::default() })).errno, 0);
    call(live, t, id, Opcode::ScanoutPresent, inc, &payload(&ScanoutPresentReq { handle: h, ..Default::default() })).body::<ScanoutPresentRep>().seq
}

#[test]
fn the_default_policy_offers_2560x1440_and_the_first_present_writes_the_fastset() {
    let (mut live, t, chip) = world(LogicalSize::default());
    assert!(live.log.has("[I915-SCANOUT-SIZE planned logical=2560x1440 panel=3840x2160 scaler=ps1 ratio=1.500 how=fastset"), "{:?}", live.log.0);
    let (id, inc) = authority(&mut live, &t);
    let q = call(&mut live, &t, id, Opcode::DisplayQuery, inc, &[]).body::<DisplayQueryRep>();
    assert_eq!((q.width, q.height, q.stride, q.active_width, q.active_height, q.scaled), (2560, 1440, 10240, 3840, 2160, 1));
    let set = payload(&DisplayModeReq { width: 2560, height: 1440, refresh_millihz: q.refresh_millihz, ..Default::default() });
    assert_eq!(call(&mut live, &t, id, Opcode::DisplaySetMode, inc, &set).errno, 0, "the offered mode is the logical one");
    let a = call(&mut live, &t, id, Opcode::ScanoutAcquire, inc, &[]).body::<ScanoutAcquireRep>();
    assert_eq!((a.width, a.height, a.stride, a.size), (2560, 1440, 10240, 10240 * 1440));
    assert!(chip.all_writes().iter().all(|(r, _)| *r != PIPESRC_A), "nothing is written before the first present");
    assert_eq!(frame_on(&mut live, &t, id, inc, a.handle0), 1);
    let imgs = live.svc.scanout_images().unwrap();
    assert_eq!(chip.writes_to(PIPESRC_A), [(2559 << 16) | 1439]);
    assert_eq!(chip.writes_to(PS_1A_VPHASE), [0x3554]);
    assert_eq!(chip.writes_to(PLANE_STRIDE), [160]);
    assert_eq!(chip.writes_to(PLANE_SIZE), [(1439 << 16) | 2559]);
    assert_eq!(chip.writes_to(PLANE_SURF), [imgs[0].2]);
    let w = chip.all_writes();
    let at = |reg| w.iter().position(|(r, _)| *r == reg).unwrap();
    assert!(at(PIPESRC_A) < at(PLANE_SIZE) && at(PLANE_SIZE) < at(PLANE_SURF), "pipe, then plane, then PLANE_SURF arms it");
    assert!(live.log.has("[I915-SCANOUT-SIZE logical=2560x1440 panel=3840x2160 scaler=ps1 ratio=1.500 how=fastset ok]"), "{:?}", live.log.0);
    // the next frame is the one-register flip
    assert_eq!(frame_on(&mut live, &t, id, inc, a.handle1), 2);
    assert_eq!(chip.writes_to(PIPESRC_A).len(), 1, "the geometry is written once");
    assert_eq!(chip.writes_to(PLANE_SURF), [imgs[0].2, imgs[1].2]);
    // the authority's end: the firmware's geometry and framebuffer go back
    t.kill(id);
    for ms in 0..4u64 {
        t.0.borrow_mut().script.push_back(Ev::At(Wake::Deadline, 5 + ms));
    }
    run(&mut live);
    assert_eq!(chip.writes_to(PIPESRC_A), [(2559 << 16) | 1439, 0x077f_0437], "PIPESRC back to 1920x1080");
    assert_eq!(chip.writes_to(PLANE_SIZE), [(1439 << 16) | 2559, 0x0437_077f]);
    assert_eq!(chip.writes_to(PLANE_STRIDE), [160, 120]);
    assert_eq!(chip.writes_to(PLANE_SURF), [imgs[0].2, imgs[1].2, 0]);
    assert!(live.log.has("[I915-SCANOUT-RESTORED]"), "{:?}", live.log.0);
    // the next authority's first present applies the fastset again
    let (id2, inc2) = authority(&mut live, &t);
    let b = call(&mut live, &t, id2, Opcode::ScanoutAcquire, inc2, &[]).body::<ScanoutAcquireRep>();
    assert_eq!((b.width, b.height), (2560, 1440));
    frame_on(&mut live, &t, id2, inc2, b.handle0);
    assert_eq!(chip.writes_to(PIPESRC_A).len(), 3);
}

#[test]
fn the_adopted_policy_keeps_the_firmware_size_and_writes_only_plane_surf() {
    let (mut live, t, chip) = world(LogicalSize::Adopted);
    let (id, inc) = authority(&mut live, &t);
    let a = call(&mut live, &t, id, Opcode::ScanoutAcquire, inc, &[]).body::<ScanoutAcquireRep>();
    assert_eq!((a.width, a.height, a.stride), (1920, 1080, 7680));
    frame_on(&mut live, &t, id, inc, a.handle0);
    assert!(chip.writes_to(PIPESRC_A).is_empty());
    assert_eq!(chip.writes_to(PLANE_SURF).len(), 1);
}

#[test]
fn a_refused_size_is_named_and_the_firmware_size_is_offered() {
    let (mut live, t, _chip) = world(LogicalSize::Exact { width: 4096, height: 2304 });
    assert!(live.log.has("[I915-SCANOUT-SIZE REFUSED why=RESIZE-DOWNSCALE-NOT-FASTSET logical=4096x2304 panel=3840x2160"), "{:?}", live.log.0);
    let (id, inc) = authority(&mut live, &t);
    let q = call(&mut live, &t, id, Opcode::DisplayQuery, inc, &[]).body::<DisplayQueryRep>();
    assert_eq!((q.width, q.height, q.stride), (1920, 1080, 7680));
}
