// SPDX-License-Identifier: GPL-2.0-only
//! The MEDIA SEAT: a session opened over the MEDIA listen endpoint (a capability only the media service's row, and the maintenance
//! witness row, hold SEND on) is the one session the video engines are routed to. Bound by the transport, never by a request;
//! one holder while it lives; the next media session binds once the holder is gone.
#![cfg(feature = "p3-exec")]
mod common;
mod exec_common;
mod net_common;
use coralos_hal::fake::FakeDma;
use exec_common::{built, kbl, Chip, Mem};
use i915_device::live::{ConnId, Live, NoRegions, Step};
use i915_device::uapi::{bring_up_render, Hw, RenderConfig};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_gt_core::emit::FlushQuirks;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::msg::session::*;
use i915_wire::msg::Body;
use i915_wire::Opcode;
use net_common::*;

type L = Live<FakeT, NoRegions, Chip, FakeDma, Mem, exec_common::Lines>;

fn world() -> (L, FakeT) {
    let (_chip, _mem, mut ex, mut gm) = built();
    let mut log = exec_common::Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 << 30).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let cfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: 1000, keep: None };
    let svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).unwrap();
    let t = FakeT::default();
    t.0.borrow_mut().now = 1;
    t.0.borrow_mut().auto = true;
    (Live::new(t.clone(), NoRegions, svc, ex, gm, log).unwrap(), t)
}

fn run(live: &mut L) {
    while live.step() != Step::Closed {}
}

fn open(live: &mut L, t: &FakeT, id: ConnId) -> u64 {
    let mut p = vec![0u8; SessionOpenReq::SIZE.max(64)];
    let n = SessionOpenReq::default().to_payload(&mut p).unwrap();
    t.send_frame(id, &frame(Opcode::SessionOpen, 7, 0, &p[..n]));
    run(live);
    let mut f = t.drain_frames(id);
    assert_eq!(f.len(), 1);
    parse(&f.remove(0)).body::<SessionOpenRep>().incarnation
}

fn close(live: &mut L, t: &FakeT, id: ConnId, inc: u64) {
    t.send_frame(id, &frame(Opcode::SessionClose, 8, inc, &[]));
    run(live);
    let _ = t.drain_frames(id);
}

#[test]
fn only_a_session_opened_over_the_media_endpoint_holds_the_seat_and_it_is_held_once() {
    let (mut live, t) = world();
    // an ordinary client's session: no seat, whatever it asks
    let plain = t.connect();
    run(&mut live);
    let p = open(&mut live, &t, plain);
    assert_eq!(live.svc.media_seat(), None, "a session on the client listen endpoint never holds the seat");
    // the media service's connection binds the seat, by the endpoint it arrived on
    let m1 = t.connect_media();
    run(&mut live);
    let i1 = open(&mut live, &t, m1);
    assert_eq!(live.svc.media_seat(), Some(i1));
    assert!(live.log.has(&format!("[I915-MEDIA seat bound inc={}]", i1)), "{:?}", live.log.0);
    assert_ne!(Some(p), live.svc.media_seat());
    // a second media connection while the first lives is refused the seat, by name; the holder keeps it
    let m2 = t.connect_media();
    run(&mut live);
    let i2 = open(&mut live, &t, m2);
    assert_eq!(live.svc.media_seat(), Some(i1));
    assert!(live.log.has(&format!("[I915-MEDIA seat NOT bound inc={} name=MEDIA-SEAT-HELD]", i2)), "{:?}", live.log.0);
    // the holder's session ends: the seat ends with it, and the next media session binds
    close(&mut live, &t, m1, i1);
    assert_eq!(live.svc.media_seat(), None);
    assert!(live.log.has(&format!("[I915-MEDIA seat ended inc={}]", i1)), "{:?}", live.log.0);
    let m3 = t.connect_media();
    run(&mut live);
    let i3 = open(&mut live, &t, m3);
    assert_eq!(live.svc.media_seat(), Some(i3));
}
