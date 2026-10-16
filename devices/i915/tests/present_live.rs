// SPDX-License-Identifier: GPL-2.0-only
//! THE PRESENTATION INCREMENT end to end (D18, `p4-present`): the live loop over a fake kernel and a simulated chip whose plane LATCHES
//! (`PLANE_SURFLIVE` follows a `PLANE_SURF` write) - or, for the RED control, never does. A connection on the DISPLAY listen endpoint is
//! the display authority; it acquires the scanout pair, begins, presents and waits; the first latch releases the firmware's framebuffer.
//! The ruling's RED controls through real frames: another session's present, rendering into a COMMITTED / SCANNING_OUT image (as
//! SCANOUT-BEGIN and as an EXECBUFFER2 write), and a suppressed latch that releases nothing.
#![cfg(feature = "p4-present")]
mod common;
mod exec_common;
mod net_common;
use exec_common::{built, kbl, Chip, Mem, GGTT_BASE};
use coralos_hal::fake::FakeDma;
use i915_device::display::Adoption;
use i915_device::display::scanout::ImageState;
use i915_device::live::{ConnId, Live, NoRegions, Step, Wake};
use i915_device::uapi::{bring_up_render, Hw, RenderConfig};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_display_modeset_core::adopt::FirmwareScanout;
use i915_gt_core::emit::FlushQuirks;
use i915_kbl_display_core::types::Pipe;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::msg::display::*;
use i915_wire::msg::exec::{self, ExecObject};
use i915_wire::msg::gem::*;
use i915_wire::msg::session::*;
use i915_wire::msg::Body;
use i915_wire::{errno, Opcode};
use net_common::*;

type L = Live<FakeT, NoRegions, Chip, FakeDma, Mem, exec_common::Lines>;
const PLANE_SURF: u32 = 0x7019c; // skl_universal_plane_regs.h:153
const PLANE_SURFLIVE: u32 = 0x701ac; // skl_universal_plane_regs.h:186

/// The live device on the Pixelbook Go's adopted display. `latches`: the plane shows a written surface on the next read.
fn world(latches: bool) -> (L, FakeT, Chip) {
    let (mut bar, mut cfg, mut lg) = (common::bar_atlas(), common::config_page(0x591c, 0x02), common::Lines::default());
    let ready = i915_device::startup::startup(&mut bar, &mut cfg, &mut lg).unwrap();
    let adoption: Adoption = ready.adoption;
    let fw = FirmwareScanout::from_readout(&ready.state, Pipe::A, 0).unwrap();
    let (chip, _mem, mut ex, mut gm) = built();
    if latches {
        chip.0.borrow_mut().on_write = Some(Box::new(|regs, reg, v| {
            if reg == PLANE_SURF {
                regs.insert(PLANE_SURFLIVE, v & 0xffff_f000);
            }
        }));
    }
    let mut log = exec_common::Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 << 30).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let cfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: 1000, keep: adoption.keep() };
    let mut svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).unwrap();
    svc.set_display(adoption);
    svc.set_firmware_scanout(Some(fw));
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

/// One call on connection `id`; its one reply.
fn call(live: &mut L, t: &FakeT, id: ConnId, op: Opcode, inc: u64, p: &[u8]) -> Resp {
    t.send_frame(id, &frame(op, 7, inc, p));
    run(live);
    let mut f = t.drain_frames(id);
    assert_eq!(f.len(), 1, "{}: exactly one reply", op.name());
    parse(&f.remove(0))
}

fn open(live: &mut L, t: &FakeT, id: ConnId) -> u64 {
    call(live, t, id, Opcode::SessionOpen, 0, &payload(&SessionOpenReq::default())).body::<SessionOpenRep>().incarnation
}

fn begin(live: &mut L, t: &FakeT, id: ConnId, inc: u64, h: u32) -> Resp {
    call(live, t, id, Opcode::ScanoutBegin, inc, &payload(&ScanoutBeginReq { handle: h, ..Default::default() }))
}
fn present(live: &mut L, t: &FakeT, id: ConnId, inc: u64, h: u32) -> Resp {
    call(live, t, id, Opcode::ScanoutPresent, inc, &payload(&ScanoutPresentReq { handle: h, ..Default::default() }))
}
fn wait(live: &mut L, t: &FakeT, id: ConnId, inc: u64, seq: u64) -> Resp {
    call(live, t, id, Opcode::ScanoutWait, inc, &payload(&ScanoutWaitReq { seq, timeout_ns: 0 }))
}

/// The compositor's connection (on the DISPLAY endpoint) with its session open, and the pair acquired.
fn authority(live: &mut L, t: &FakeT) -> (ConnId, u64, ScanoutAcquireRep) {
    let id = t.connect_display();
    run(live);
    let inc = open(live, t, id);
    let a = call(live, t, id, Opcode::ScanoutAcquire, inc, &[]).body::<ScanoutAcquireRep>();
    (id, inc, a)
}

#[test]
fn a_display_connection_is_the_authority_and_its_first_latched_present_replaces_the_firmware_framebuffer() {
    let (mut live, t, chip) = world(true);
    let (id, inc, a) = authority(&mut live, &t);
    assert!(live.log.has(&format!("[I915-DISPLAY authority bound inc={}]", inc)), "bound by the transport, nothing in a request");
    assert_eq!((a.width, a.height, a.stride, a.modifier, a.size), (1920, 1080, 7680, 0, 0x7e_9000));
    let imgs = live.svc.scanout_images().unwrap();
    for (_, st, ggtt) in imgs {
        assert_eq!(st, ImageState::Free);
        assert_eq!(ggtt % (256 * 1024), 0, "skl_plane_min_alignment for linear");
        assert!(ggtt as u64 >= 0x7e_9000, "never over the adopted framebuffer: {ggtt:#x}");
        assert_ne!(chip.ggtt_entry(GGTT_BASE + 8 * (ggtt / 4096)), 0, "the image's first page is mapped");
    }
    assert!(live.log.has("[I915-SCANOUT pair acquired"));
    assert!(begin(&mut live, &t, id, inc, a.handle0).errno == 0);
    let seq = present(&mut live, &t, id, inc, a.handle0).body::<ScanoutPresentRep>().seq;
    assert_eq!(seq, 1);
    assert_eq!(chip.writes_to(PLANE_SURF), [imgs[0].2], "ONE register written: PLANE_SURF = image 0's GGTT address");
    assert!(live.log.has("[I915-PRESENT latched seq=1 released=firmware"), "{:?}", live.log.0);
    assert!(live.log.has("[I915-SCANOUT-LIVE]"));
    let s = wait(&mut live, &t, id, inc, 1).body::<ScanoutStatusRep>();
    assert_eq!((s.latched_seq, s.scanning, s.free_mask, s.latch_timeouts), (1, a.handle0, 0b10, 0));
    // the second frame: image 1, and image 0 released only by ITS latch
    assert!(begin(&mut live, &t, id, inc, a.handle1).errno == 0);
    assert_eq!(present(&mut live, &t, id, inc, a.handle1).body::<ScanoutPresentRep>().seq, 2);
    let s = wait(&mut live, &t, id, inc, 2).body::<ScanoutStatusRep>();
    assert_eq!((s.latched_seq, s.scanning, s.free_mask), (2, a.handle1, 0b01));
    assert_eq!(chip.writes_to(PLANE_SURF), [imgs[0].2, imgs[1].2]);
}

/// RED: a session that did NOT connect on the display endpoint is not the authority, whatever it asks; and the authority cannot present
/// an image of a session that is not its own.
#[test]
fn a_plain_session_is_refused_and_another_sessions_present_never_reaches_the_plane() {
    let (mut live, t, chip) = world(true);
    let (_id, _inc, a) = authority(&mut live, &t);
    let other = t.connect();
    run(&mut live);
    let oinc = open(&mut live, &t, other);
    let r = call(&mut live, &t, other, Opcode::ScanoutPresent, oinc, &payload(&ScanoutPresentReq { handle: a.handle0, ..Default::default() }));
    assert!(r.is_err(errno::EACCES), "DISPLAY-NOT-AUTHORIZED");
    assert!(call(&mut live, &t, other, Opcode::ScanoutAcquire, oinc, &[]).is_err(errno::EACCES));
    assert!(chip.writes_to(PLANE_SURF).is_empty(), "nothing reached the plane");
}

/// RED: rendering into a COMMITTED or SCANNING_OUT image - by SCANOUT-BEGIN, or by an EXECBUFFER2 that WRITES it - is refused.
#[test]
fn rendering_into_an_image_the_display_holds_is_refused_as_begin_and_as_a_gpu_write() {
    let (mut live, t, _chip) = world(true);
    let (id, inc, a) = authority(&mut live, &t);
    begin(&mut live, &t, id, inc, a.handle0);
    present(&mut live, &t, id, inc, a.handle0); // latched at once: SCANNING_OUT
    assert!(begin(&mut live, &t, id, inc, a.handle0).is_err(errno::EBUSY), "SCANOUT-REUSE-VIOLATION");
    // an EXECBUFFER2 writing image 1, never begun, and image 0, on the glass
    let batch = call(&mut live, &t, id, Opcode::GemCreate, inc, &payload(&GemCreateReq { size: 4096 })).body::<GemCreateRep>().handle;
    for target in [a.handle1, a.handle0] {
        let objs = [
            ExecObject { handle: target, flags: 1 << 2 /* EXEC_OBJECT_WRITE, i915_drm.h:1275 */, ..Default::default() },
            ExecObject { handle: batch, ..Default::default() },
        ];
        let mut out = vec![0u8; 1024];
        let n = exec::encode_request(&mut out, exec::ExecHeader { batch_len: 8, ..Default::default() }, &objs, &[], &[], &[]).unwrap();
        let r = call(&mut live, &t, id, Opcode::Execbuffer2, inc, &out[..n]);
        assert!(r.is_err(errno::EBUSY), "a GPU write into image {target} is refused");
    }
    assert!(live.log.has("[SCANOUT-REUSE VIOLATION]"));
    // the control: a begun image may be written (the request then fails later for its own reasons, never as a reuse violation)
    begin(&mut live, &t, id, inc, a.handle1);
    assert_eq!(live.svc.scanout_images().unwrap()[1].1, ImageState::Rendering);
}

/// RED: the plane never latches. Nothing is released - the firmware's framebuffer stays on the glass, the image stays COMMITTED, the wait
/// says -EIO, and no further present is accepted.
#[test]
fn a_suppressed_latch_releases_nothing() {
    let (mut live, t, chip) = world(false);
    let (id, inc, a) = authority(&mut live, &t);
    begin(&mut live, &t, id, inc, a.handle0);
    let seq = present(&mut live, &t, id, inc, a.handle0).body::<ScanoutPresentRep>().seq;
    assert_eq!(chip.writes_to(PLANE_SURF).len(), 1, "the flip was written");
    // the device's own 1 ms timer while the flip is in flight: 400 deadline wakes, far past the planner's 201-read bound
    for ms in 0..400u64 {
        t.0.borrow_mut().script.push_back(Ev::At(Wake::Deadline, 2 + ms));
    }
    run(&mut live);
    assert!(live.log.has(&format!("[I915-PRESENT latch_timeout seq={seq}")), "{:?}", live.log.0);
    assert!(wait(&mut live, &t, id, inc, seq).is_err(errno::EIO), "SCANOUT-LATCH-UNCONFIRMED");
    let imgs = live.svc.scanout_images().unwrap();
    assert_eq!(imgs[0].1, ImageState::Committed { seq });
    begin(&mut live, &t, id, inc, a.handle1);
    assert!(present(&mut live, &t, id, inc, a.handle1).is_err(errno::EBUSY), "SCANOUT-FLIP-IN-FLIGHT");
    assert!(!live.log.has("released=firmware"));
}

#[test]
fn a_new_authority_takes_the_same_pair_over_and_the_glass_does_not_move() {
    let (mut live, t, chip) = world(true);
    let (id, inc, a) = authority(&mut live, &t);
    begin(&mut live, &t, id, inc, a.handle0);
    present(&mut live, &t, id, inc, a.handle0);
    t.kill(id);
    run(&mut live);
    assert!(live.log.has(&format!("[I915-DISPLAY authority ended inc={}]", inc)));
    let (id2, inc2, b) = authority(&mut live, &t);
    assert!(live.log.has("[I915-SCANOUT pair taken over"));
    assert_eq!(b.size, a.size);
    let s = live.svc.scanout_images().unwrap();
    assert_eq!(s[0].1, ImageState::ScanningOut { seq: 1 }, "still on the glass");
    assert_eq!(chip.writes_to(PLANE_SURF).len(), 1, "taking over wrote nothing");
    assert!(begin(&mut live, &t, id2, inc2, b.handle1).errno == 0);
    assert_eq!(present(&mut live, &t, id2, inc2, b.handle1).body::<ScanoutPresentRep>().seq, 2);
}
