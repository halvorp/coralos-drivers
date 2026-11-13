// SPDX-License-Identifier: GPL-2.0-only
//! The VIDEO engine (VCS0) as the device's second execlists engine, and the reset coordinator that keeps the engines' faults apart.
//! Linux: one `intel_context` per (GEM context, engine) (gt/intel_context.c), engine resets by their own GRDOM bit on Gen8/9
//! (gt/intel_reset.c `gen8_reset_engines`, GEN6_GRDOM_MEDIA = bit 2, intel_gt_regs.h:666) and a full GT reset only as the escalation.
#![cfg(feature = "p5-video")]
mod exec_common;
#[macro_use]
mod uapi_world;
use i915_device::exec::dma::ObjKind;
use i915_device::uapi::Hw;
use i915_gt_core::emit::FlushQuirks;
use i915_wire::msg::exec::{self, ExecObject};
use i915_wire::msg::sync::{self, *};
use i915_wire::{errno, Opcode};
use uapi_world::*;

const GDRST: u32 = 0x941c; // GEN6_GDRST, intel_gt_regs.h:663
const GRDOM_FULL: u32 = 1 << 0; // intel_gt_regs.h:664
const GRDOM_RENDER: u32 = 1 << 1; // :665
const GRDOM_MEDIA: u32 = 1 << 2; // :666
const BSD: u64 = 2; // I915_EXEC_BSD, i915_drm.h:1434

/// The device with the video engine up, a media-seat session and a plain one.
fn video_world() -> (W, u64, u64) {
    let mut w = world();
    w.svc.bring_up_video(&mut hw!(w), FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, 1000).unwrap();
    assert!(w.log.has("[I915-UAPI READY engines=rcs0,vcs0"), "{:?}", w.log.0.iter().rev().take(3).collect::<Vec<_>>());
    let media = open(&mut w);
    let mut lg = exec_common::Lines::default();
    w.svc.bind_media_seat(&mut lg, media).unwrap();
    let plain = open(&mut w);
    (w, media, plain)
}

/// An EXECBUFFER2 of one store batch on `ring` (0 = the render default, BSD = the video engine), with an out syncobj.
fn job_on(w: &mut W, inc: u64, ring: u64, out: u32) -> Result<exec::ExecRepHead, i32> {
    let (batch, result) = (create(w, inc, 4096), create(w, inc, 4096));
    let bl = lease(w, inc, batch, 4096);
    write_batch(w, &bl);
    let objs = [
        ExecObject { handle: result, offset: RESULT_VA, flags: PINNED | SUPPORTS_48B | WRITE, ..Default::default() },
        ExecObject { handle: batch, offset: BATCH_VA, flags: PINNED | SUPPORTS_48B, ..Default::default() },
    ];
    let fa = [exec::Fence { handle: out, flags: FENCE_SIGNAL }];
    let hdr = exec::ExecHeader { batch_len: 24, flags: exec::flags::NO_RELOC | exec::flags::FENCE_ARRAY | ring, ..Default::default() };
    let mut buf = vec![0u8; 4096];
    let n = exec::encode_request(&mut buf, hdr, &objs, &[], &fa, &[]).unwrap();
    let (r, head) = submit(w, inc, &buf[..n]);
    head.ok_or(r.errno)
}

/// The engine status pages: [0] the render engine's, [1] the video engine's (allocated in bring-up order, before any context).
fn engine_hwsp(w: &W, engine: usize) -> u64 {
    w.gm.alloc.grants().filter(|g| g.kind == ObjKind::Hwsp).nth(engine).unwrap().cpu_mapping
}

/// The engine accepts its first submission (CSB promote, entry 0) and its context runs - the timeline is not advanced.
fn accept(w: &mut W, engine: usize) {
    let h = engine_hwsp(w, engine);
    w.mem.put(h + 0x10 * 4, 1);
    w.mem.put(h + 0x11 * 4, 0);
    w.mem.put(h + 0x1f * 4, 0);
}

/// ... then completes it: the CSB retires (entry 1) and the context's own timeline reaches `seqno`.
fn complete(w: &mut W, engine: usize, timeline_dev: u64, seqno: u32) {
    let h = engine_hwsp(w, engine);
    w.mem.put(h + 0x12 * 4, 1 << 3);
    w.mem.put(h + 0x13 * 4, 0);
    w.mem.put(h + 0x1f * 4, 1);
    let tl = w.gm.alloc.cpu_of(timeline_dev, 4).unwrap();
    w.mem.put(tl, seqno);
}

fn fence_error(w: &mut W, inc: u64, out: u32) -> i32 {
    let sf = call(w, inc, Opcode::SyncobjHandleToFd, &SyncobjHandleToFdReq { handle: out, flags: sync::flags::HANDLE_TO_FD_EXPORT_SYNC_FILE, point: 0 })
        .body::<SyncobjTokenRep>()
        .token as u32;
    call(w, inc, Opcode::SyncFileWait, &SyncFileWaitReq { token: sf, timeout_ns: 0, ..Default::default() }).body::<SyncFileWaitRep>().fence_error
}

#[test]
fn a_bsd_batch_of_the_media_seat_runs_on_the_video_engine_and_nobody_elses_can() {
    let (mut w, media, plain) = video_world();
    // the plain session is never routed to the video engine: refused before anything is built
    let o = syncobj(&mut w, plain, 0);
    assert_eq!(job_on(&mut w, plain, BSD, o).unwrap_err(), -errno::EINVAL);
    assert!(w.svc.rigs()[1].engine.requests().is_empty());
    // the seat's batch is built and submitted on VCS0, not on the render engine
    let o = syncobj(&mut w, media, 0);
    let head = job_on(&mut w, media, BSD, o).unwrap();
    assert_eq!(w.svc.rigs()[1].engine.requests().len(), 1, "on the video engine");
    assert!(w.svc.rigs()[0].engine.requests().is_empty(), "nothing on the render engine");
    assert!(w.chip.writes_to(0x12230).len() >= 4, "the ELSP at GEN6_BSD_RING_BASE + 0x230 was written");
    // its hardware context is the video engine's: the 2-page xcs image (GEN8_LR_CONTEXT_OTHER_SIZE, intel_engine_cs.c:50)
    let tl = *w.svc.timeline_addrs().last().unwrap();
    accept(&mut w, 1);
    poll(&mut w);
    complete(&mut w, 1, tl, head.seqno);
    poll(&mut w);
    assert!(w.svc.rigs()[1].engine.requests().is_empty(), "retired on its own engine");
    assert_eq!(fence_error(&mut w, media, o), 0, "the fence signalled without error");
}

#[test]
fn a_hung_video_engine_is_reset_alone_and_the_render_engines_work_survives() {
    let (mut w, media, plain) = video_world();
    let ov = syncobj(&mut w, media, 0);
    job_on(&mut w, media, BSD, ov).unwrap();
    accept(&mut w, 1);
    poll(&mut w);
    // 900 ms later the compositor submits on the render engine; the video request never completes
    w.now += 900;
    let or = syncobj(&mut w, plain, 0);
    let rh = job_on(&mut w, plain, 0, or).unwrap();
    let rtl = *w.svc.timeline_addrs().last().unwrap();
    accept(&mut w, 0);
    let rcs_writes_before = w.chip.all_writes().iter().filter(|(r, _)| (0x2000..0x3000).contains(r)).count();
    w.now += 200; // the video request is 1100 ms old (past the 1000 ms bound), the render one 200 ms
    w.svc.poll(&mut hw!(w));
    assert!(w.log.has("[I915-UAPI HANG name=ENGINE-HUNG engine=1]"), "{:?}", w.log.0.iter().rev().take(6).collect::<Vec<_>>());
    assert!(w.log.has("[I915-UAPI RESET engine=1 scope=engine]"));
    // gen6_hw_domain_reset writes the mask twice (`loops = 2`, intel_reset.c:288): two writes, both the media domain alone
    assert_eq!(w.chip.writes_to(GDRST), vec![GRDOM_MEDIA, GRDOM_MEDIA], "the video engine's reset domain alone: never FULL, never RENDER");
    assert_eq!(w.chip.all_writes().iter().filter(|(r, _)| (0x2000..0x3000).contains(r)).count(), rcs_writes_before, "nothing written at the render engine's base");
    assert_eq!((w.svc.rigs()[1].epoch, w.svc.rigs()[0].epoch), (1, 0), "only the video engine's epoch moved");
    assert_eq!(fence_error(&mut w, media, ov), -errno::EIO, "the lost video request fails");
    // the render request completes as if nothing happened
    complete(&mut w, 0, rtl, rh.seqno);
    poll(&mut w);
    assert_eq!(fence_error(&mut w, plain, or), 0, "the compositor's request was never touched");
    // and the video engine takes new work after its reset
    let o2 = syncobj(&mut w, media, 0);
    assert!(job_on(&mut w, media, BSD, o2).is_ok());
}

#[test]
fn an_engine_reset_that_fails_escalates_to_the_whole_gt_and_every_engines_work_is_lost() {
    let (mut w, media, plain) = video_world();
    let ov = syncobj(&mut w, media, 0);
    job_on(&mut w, media, BSD, ov).unwrap();
    accept(&mut w, 1);
    let or = syncobj(&mut w, plain, 0);
    job_on(&mut w, plain, 0, or).unwrap();
    poll(&mut w);
    // the video engine never becomes ready to reset (RING_RESET_CTL READY never sets): the engine reset fails, the GT is reset
    w.chip.0.borrow_mut().reset_ctls.retain(|r| *r != 0x120d0);
    w.chip.0.borrow_mut().ready_never = true;
    w.chip.0.borrow_mut().deadline_after_reads = 50;
    w.now += 5_000;
    w.svc.poll(&mut hw!(w));
    w.chip.0.borrow_mut().ready_never = false;
    assert!(w.log.has("engine reset failed"), "{:?}", w.log.0.iter().rev().take(8).collect::<Vec<_>>());
    assert!(w.chip.writes_to(GDRST).contains(&GRDOM_FULL), "escalated to GEN6_GRDOM_FULL");
    assert!(!w.chip.writes_to(GDRST).contains(&GRDOM_RENDER));
    assert_eq!(fence_error(&mut w, media, ov), -errno::EIO);
    assert_eq!(fence_error(&mut w, plain, or), -errno::EIO, "a full reset loses every engine's work");
}

#[test]
fn a_gt_that_does_not_come_back_is_wedged_and_refuses_every_later_submission() {
    let (mut w, media, _plain) = video_world();
    let ov = syncobj(&mut w, media, 0);
    job_on(&mut w, media, BSD, ov).unwrap();
    accept(&mut w, 1);
    poll(&mut w);
    w.chip.0.borrow_mut().gdrst_stuck = true; // GDRST never self-clears: neither the engine nor the full reset completes
    w.chip.0.borrow_mut().deadline_after_reads = 50;
    w.now += 5_000;
    w.svc.poll(&mut hw!(w));
    assert!(w.log.has("[I915-UAPI WEDGED"), "{:?}", w.log.0.iter().rev().take(8).collect::<Vec<_>>());
    assert_eq!(fence_error(&mut w, media, ov), -errno::EIO);
    let o2 = syncobj(&mut w, media, 0);
    assert_eq!(job_on(&mut w, media, BSD, o2).unwrap_err(), -errno::EIO, "GT-WEDGED");
    assert!(refused(&w, "GT-WEDGED") || w.log.0.iter().any(|l| l.contains("GT-WEDGED")));
}
