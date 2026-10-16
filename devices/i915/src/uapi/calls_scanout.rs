// SPDX-License-Identifier: GPL-2.0-only
//! THE PRESENTATION INCREMENT (D18, feature `p4-present`): the display authority's scanout pair, served. The RULES are
//! `crate::display::scanout::ScanoutPair`'s (FREE / RENDERING / COMMITTED / SCANNING_OUT, tested there); this file is the glue that gives
//! them memory, a GGTT address, a register write and a clock:
//!
//!  * SCANOUT-ACQUIRE: two GEM objects of the adopted layout in the authority's session, uncached for the display
//!    (`i915_gem_object_pin_to_display_plane`, gem/i915_gem_domain.c:414-481: `set_cache_level(NONE)` on a part without WT, then a
//!    GGTT pin at `skl_plane_min_alignment`, mappable first), each held by the device (a `prime_export` hold: closing the handle never
//!    frees pages the plane may read) and marked a framebuffer. A later authority (the first one's session ended) takes the SAME pair
//!    over with handles of its own - nothing on the glass moves.
//!  * SCANOUT-BEGIN / SCANOUT-PRESENT: the pair's rules; a present's flip is ARMED in [`UapiService::advance_scanout`] once the object is
//!    idle (its GPU work completed, whichever session submitted it), by executing the planner's one `PLANE_SURF` write through the
//!    executor's write guard; then every advance reads `PLANE_SURFLIVE` for the latch (a 1 ms timer while a flip is in flight, never at
//!    idle).
//!  * SCANOUT-WAIT: answered at once when the present is latched (or its latch timed out: `-EIO`), else parked like GEM-WAIT.
//!  * EXECBUFFER2 that WRITES an image the authority has not begun is `-EBUSY SCANOUT-REUSE-VIOLATION` ([`UapiService::check_scanout_writes`]).
use super::fail::Fail;
use super::service::{Done, Hw, UapiService, WaitKind};
use crate::display::scanout::{ImageState, LatchStep, Released, ScanoutError, ScanoutPair, SeqState};
use crate::exec::dma::CpuMem;
use crate::log::{emit, Log};
use alloc::vec::Vec;
use coralos_hal::{Dma, Mmio};
use i915_display_modeset_core::adopt::surf_alignment;
use i915_gem_core::{ObjId, SessionId};
use i915_memory_core::{BindRequest, CacheLevel, Gtt, Mode, NoTablePages, Placement, VmaId};
use i915_wire::msg::display::{ScanoutAcquireRep, ScanoutBeginReq, ScanoutPresentRep, ScanoutPresentReq, ScanoutStatusRep, ScanoutWaitReq};
use i915_wire::msg::{self, Body};
use i915_wire::{errno, Header, Opcode, Status};

/// `I915_CACHING_NONE` (i915_drm.h:1714): what a scanout object is set to on a part without `HAS_WT` (every Gen9 part).
const CACHING_NONE: u32 = 0;
/// `I915_GEM_DOMAIN_WC` (i915_drm.h:1650): the CPU domain the pages are first pinned in.
const DOMAIN_WC: u32 = 0x80;

/// The device's record of the pair: the rules, and what the rules do not hold - the objects and their size. The pair is never torn
/// down (an image may be on the glass): a new authority takes it over, so its GGTT pins live as long as the device.
pub(crate) struct PairRec {
    pub pair: ScanoutPair,
    pub objs: [ObjId; 2],
    pub size: u64,
}

fn scan_fail(e: ScanoutError) -> Fail {
    Fail::new(e.errno(), e.name())
}

/// The `SCANOUT-WAIT` / status body of a pair.
pub(crate) fn status_rep(p: &ScanoutPair) -> ScanoutStatusRep {
    let s = p.status();
    ScanoutStatusRep { latched_seq: s.latched_seq, scanning: s.scanning, free_mask: s.free_mask, latch_timeouts: s.latch_timeouts, ..Default::default() }
}

impl UapiService {
    pub(crate) fn call_scanout<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        match h.opcode {
            Opcode::ScanoutAcquire => {
                msg::Empty::from_payload(p)?;
                let rep = self.scanout_acquire(hw, inc)?;
                Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&rep))?))
            }
            Opcode::ScanoutBegin => {
                let r = ScanoutBeginReq::from_payload(p)?;
                let rec = self.scanout.as_mut().ok_or(Fail::new(errno::ENOENT, "SCANOUT-NOT-ACQUIRED"))?;
                rec.pair.begin(inc, r.handle).map_err(scan_fail)?;
                Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?))
            }
            Opcode::ScanoutPresent => {
                let r = ScanoutPresentReq::from_payload(p)?;
                let rec = self.scanout.as_mut().ok_or(Fail::new(errno::ENOENT, "SCANOUT-NOT-ACQUIRED"))?;
                let seq = rec.pair.present(inc, r.handle).map_err(scan_fail)?;
                self.advance_scanout(hw); // arm at once when the GPU is already done
                Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&ScanoutPresentRep { seq }))?))
            }
            Opcode::ScanoutWait => {
                let r = ScanoutWaitReq::from_payload(p)?;
                let rec = self.scanout.as_ref().ok_or(Fail::new(errno::ENOENT, "SCANOUT-NOT-ACQUIRED"))?;
                if rec.pair.owner() != inc {
                    return Err(Fail::new(errno::EACCES, "SCANOUT-NOT-YOURS"));
                }
                match rec.pair.seq_state(r.seq) {
                    SeqState::Latched => Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&status_rep(&rec.pair)))?)),
                    SeqState::LatchUnconfirmed => Err(Fail::new(errno::EIO, "SCANOUT-LATCH-UNCONFIRMED")),
                    SeqState::Unknown => Err(Fail::new(errno::EINVAL, "SCANOUT-UNKNOWN-SEQ")),
                    SeqState::InFlight if r.timeout_ns == 0 => Err(Fail::new(errno::ETIME, "SCANOUT-WAIT-TIMEOUT")),
                    SeqState::InFlight => {
                        let deadline = (r.timeout_ns > 0).then(|| hw.now_ms.saturating_add((r.timeout_ns as u64).div_ceil(1_000_000)));
                        self.park(*h, WaitKind::Scanout { seq: r.seq }, hw.now_ms, deadline)?;
                        Ok(Done::Parked)
                    }
                }
            }
            _ => Err(Fail::new(errno::EINVAL, "UNROUTED")),
        }
    }

    /// The pair for session `inc` (the authority): made once, or the existing one taken over.
    fn scanout_acquire<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, inc: u64) -> Result<ScanoutAcquireRep, Fail> {
        let a = *self.display.adopted().ok_or(Fail::new(errno::ENODEV, "DISPLAY-NOT-ADOPTED"))?;
        let sid = SessionId(inc);
        if let Some(rec) = self.scanout.as_mut() {
            if rec.pair.owner() != inc {
                // the previous authority's session ended: the same two images, new handles - nothing on the glass moves
                let mut handles = [0u32; 2];
                for (i, h) in handles.iter_mut().enumerate() {
                    *h = self.gem.prime_import(sid, rec.objs[i])?;
                }
                rec.pair.rebind_owner(inc, handles);
                emit(hw.log, format_args!("[I915-SCANOUT pair taken over inc={} handles={},{}]", inc, handles[0], handles[1]));
            }
            let rec = self.scanout.as_ref().ok_or(Fail::new(errno::EIO, "UNREACHABLE"))?;
            return Ok(acquire_rep(&rec.pair, &a, rec.size));
        }
        let fw = self.firmware.ok_or(Fail::new(errno::ENODEV, "DISPLAY-NO-FIRMWARE-SCANOUT"))?;
        let size = a.fb.size.div_ceil(4096) * 4096;
        let align = surf_alignment(a.fb.tiling) as u64;
        let mut made: Vec<(u32, ObjId, u32, VmaId)> = Vec::new();
        made.try_reserve_exact(2).map_err(|_| Fail::oom("scanout-pair"))?;
        for _ in 0..2 {
            match self.make_scanout_image(hw, sid, size, align) {
                Ok(m) => made.push(m), // reserved above
                Err(f) => {
                    for (h, obj, _, vma) in made {
                        self.undo_scanout_image(hw, inc, h, obj, vma);
                    }
                    emit(hw.log, format_args!("[I915-SCANOUT acquire refused name={} errno={}]", f.name, f.errno));
                    return Err(f);
                }
            }
        }
        let img = |m: &(u32, ObjId, u32, VmaId)| (m.0, m.1 .0, m.2);
        let pair = ScanoutPair::new(inc, fw, &a, [img(&made[0]), img(&made[1])]);
        emit(hw.log, format_args!(
            "[I915-SCANOUT pair acquired inc={} handles={},{} ggtt={:#x},{:#x} size={:#x} {}x{} stride={} align={:#x} caching=none]",
            inc, made[0].0, made[1].0, made[0].2, made[1].2, size, a.fb.width, a.fb.height, a.fb.stride, align));
        let rep = acquire_rep(&pair, &a, size);
        self.scanout = Some(PairRec { pair, objs: [made[0].1, made[1].1], size });
        Ok(rep)
    }

    /// One scanout image: created in `sid`, uncached, its pages pinned, held by the device, a framebuffer, pinned in the GGTT.
    fn make_scanout_image<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, sid: SessionId, size: u64, align: u64) -> Result<(u32, ObjId, u32, VmaId), Fail> {
        let c = self.gem.create(sid, size)?;
        let h = c.handle;
        let step = (|| -> Result<(ObjId, u32, VmaId), Fail> {
            let (r, err) = self.with_sink(hw, sid.0, |gem, sink| gem.set_caching(sid, h, CACHING_NONE, sink));
            r?;
            err.map_or(Ok(()), Err)?;
            self.with_pages(hw, |gem, pages| gem.set_domain(sid, h, DOMAIN_WC, DOMAIN_WC, pages))?;
            let obj = self.gem.lookup_object(sid, h)?;
            let pages: Vec<u64> = {
                let o = self.objs.get(obj).ok_or(Fail::new(errno::EFAULT, "SCANOUT-NO-PAGES"))?;
                let mut v = Vec::new();
                v.try_reserve_exact(o.pages.len()).map_err(|_| Fail::oom("scanout-pages"))?;
                v.extend_from_slice(&o.pages);
                v
            };
            let (off, vma) = bind_ggtt_display(hw, &mut self.ggtt, &pages, size, align)?;
            self.gem.prime_export(sid, h)?; // the device's display hold: the handle may close, the pages stay
            self.gem.set_framebuffer(obj, true)?;
            Ok((obj, off, vma))
        })();
        match step {
            Ok((obj, off, vma)) => Ok((h, obj, off, vma)),
            Err(f) => {
                let _ = self.with_sink(hw, sid.0, |gem, sink| gem.close(sid, h, sink));
                Err(f)
            }
        }
    }

    fn undo_scanout_image<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, inc: u64, h: u32, obj: ObjId, vma: VmaId) {
        if let Ok(p) = self.ggtt.plan_unbind(vma) {
            let _ = hw.ex.run_bind_step(hw.log, hw.gm, &p, true, &mut |_| true);
        }
        let sid = SessionId(inc);
        let _ = self.with_sink(hw, inc, |gem, sink| gem.close(sid, h, sink));
        let _ = self.with_sink(hw, inc, |gem, sink| gem.prime_release(obj, sink));
    }

    /// EXECBUFFER2's rule (D18): an object the request may WRITE that is a scanout image must be one the authority has begun drawing.
    pub(crate) fn check_scanout_writes<L: Log>(&self, log: &mut L, sid: SessionId, objects: impl Iterator<Item = (u32, u64)>) -> Result<(), Fail> {
        let Some(rec) = self.scanout.as_ref() else { return Ok(()) };
        for (handle, flags) in objects {
            if flags & i915_uapi_core::consts::exec_object::EXEC_OBJECT_WRITE == 0 {
                continue;
            }
            let Ok(obj) = self.gem.lookup_object(sid, handle) else { continue };
            if let Err(e) = rec.pair.check_render_write(obj.0) {
                if let ScanoutError::ReuseViolation { handle: img, state } = e {
                    emit(log, format_args!("[SCANOUT-REUSE VIOLATION] session={} handle={} image={} state={} - the write is refused", sid.0, handle, img, state.name()));
                }
                return Err(scan_fail(e));
            }
        }
        Ok(())
    }

    /// Does the pair want the loop to come back (a flip waiting for its GPU work, or for the latch)?
    pub(crate) fn scanout_wants_time(&self) -> bool {
        self.scanout.as_ref().is_some_and(|r| r.pair.awaiting_gpu().is_some() || r.pair.wants_latch_reads() || r.pair.restoring())
    }

    /// Move the pair on: arm a flip whose image is idle, read the latch of an armed one.
    pub(crate) fn advance_scanout<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        let Some(rec) = self.scanout.as_mut() else { return };
        match rec.pair.ready_to_restore() {
            Some(Ok(plan)) => {
                if let Err(e) = hw.ex.run_link_effects(&plan.write) {
                    rec.pair.restore_failed();
                    emit(hw.log, format_args!("[I915-SCANOUT restore refused name={}] - the last image stays on the glass", e.name()));
                    return;
                }
            }
            Some(Err(e)) => emit(hw.log, format_args!("[I915-SCANOUT restore refused name={}] - the last image stays on the glass", e.name())),
            None => {}
        }
        if let Some(img) = rec.pair.awaiting_gpu_image() {
            if !self.gem.object_busy(ObjId(img.key)) {
                if let Some(plan) = rec.pair.ready_to_arm() {
                    if let Err(e) = hw.ex.run_link_effects(&plan.write) {
                        let seq = rec.pair.arm_failed();
                        emit(hw.log, format_args!("[I915-PRESENT arm refused seq={:?} name={}] - nothing on the glass changed", seq, e.name()));
                        return;
                    }
                }
            }
        }
        if !rec.pair.wants_latch_reads() {
            return;
        }
        let Some(reg) = rec.pair.latch_reg() else { return };
        let v = match hw.ex.rd(reg) {
            Ok(v) => v,
            Err(e) => {
                emit(hw.log, format_args!("[I915-PRESENT latch read refused name={}]", e.name()));
                return;
            }
        };
        match rec.pair.observe(v) {
            LatchStep::Latched { seq, released } => {
                let what = match released {
                    Released::Firmware => "firmware",
                    Released::Image { .. } => "image",
                };
                // the first latch is THE witness; then a cadence (powers of two), never a line per frame
                if seq.is_power_of_two() {
                    emit(hw.log, format_args!("[I915-PRESENT latched seq={} released={} scanning={}{}]", seq, what, rec.pair.status().scanning,
                        if seq == 1 { " [I915-SCANOUT-LIVE]" } else { "" }));
                }
            }
            LatchStep::TimedOut { seq, got } => {
                emit(hw.log, format_args!("[I915-PRESENT latch_timeout seq={} surflive={:#x} timeouts={}] - nothing released", seq, got, rec.pair.status().latch_timeouts));
            }
            LatchStep::Restored => {
                emit(hw.log, format_args!("[I915-SCANOUT restored firmware-fb surf={:#x}] - the display authority is gone; the boot framebuffer is on the glass [I915-SCANOUT-RESTORED]", rec.pair.firmware_surf()));
            }
            LatchStep::StillWaiting | LatchStep::Idle => {}
        }
    }

    /// The authority's session ended: the pair stays (its images may be on the glass, and held), orphaned until the next authority takes
    /// it over.
    pub(crate) fn scanout_owner_dead(&mut self, inc: u64) {
        if let Some(rec) = self.scanout.as_mut() {
            if rec.pair.owner() == inc {
                rec.pair.orphan(); // and the firmware's framebuffer goes back on the glass (advance_scanout)
            }
        }
    }

    /// A parked SCANOUT-WAIT: decided once its present is latched (or its latch timed out), or its deadline passed.
    pub(crate) fn eval_scanout_wait(&self, seq: u64, late: bool) -> Option<(Status, i32, ([u8; 24], usize))> {
        let rec = self.scanout.as_ref()?;
        match rec.pair.seq_state(seq) {
            SeqState::Latched => {
                let mut b = [0u8; 24];
                let n = status_rep(&rec.pair).to_payload(&mut b).ok()?;
                Some((Status::Ok, 0, (b, n)))
            }
            SeqState::LatchUnconfirmed => Some((Status::Ok, errno::EIO, ([0u8; 24], ScanoutStatusRep::SIZE))),
            SeqState::Unknown => Some((Status::Ok, errno::EINVAL, ([0u8; 24], ScanoutStatusRep::SIZE))),
            SeqState::InFlight => late.then_some((Status::Ok, errno::ETIME, ([0u8; 24], ScanoutStatusRep::SIZE))),
        }
    }

    /// The pair's images, for the tests and the markers.
    pub fn scanout_images(&self) -> Option<[(u32, ImageState, u32); 2]> {
        self.scanout.as_ref().map(|r| {
            let im = r.pair.images();
            [(im[0].handle, im[0].state, im[0].ggtt), (im[1].handle, im[1].state, im[1].ggtt)]
        })
    }
}

fn acquire_rep(p: &ScanoutPair, a: &i915_display_modeset_core::initial::AdoptedDisplay, size: u64) -> ScanoutAcquireRep {
    let im = p.images();
    ScanoutAcquireRep { handle0: im[0].handle, handle1: im[1].handle, width: a.fb.width, height: a.fb.height, stride: a.fb.stride, fourcc: a.fb.fourcc, modifier: a.fb.modifier, size }
}

/// Pin `pages` in the GGTT for the display: `size` bytes at `align`, inside the mappable aperture when it fits (PIN_MAPPABLE first,
/// gem/i915_gem_domain.c:452-459), uncached PTEs; the entries are read back from BAR0 before the address is used.
fn bind_ggtt_display<B: Mmio, D: Dma, M: CpuMem, L: Log>(hw: &mut Hw<'_, B, D, M, L>, ggtt: &mut Gtt, pages: &[u64], size: u64, align: u64) -> Result<(u32, VmaId), Fail> {
    let mut place = Placement::with_mode(Mode::Best);
    place.mappable = true;
    let req = |place| BindRequest { pages, size, alignment: align, guard: 0, cache: CacheLevel::None, flags: 0, place };
    let plan = match ggtt.plan_bind(&mut NoTablePages, &req(place)) {
        Ok(p) => p,
        Err(_) => ggtt.plan_bind(&mut NoTablePages, &req(Placement::best())).map_err(|e| Fail::new(errno::ENOSPC, e.name()))?,
    };
    let (off, vma) = (plan.offset() as u32, plan.vma());
    hw.ex.run_bind_step(hw.log, hw.gm, &plan, true, &mut |_| true)?;
    if hw.ex.verify_ggtt(&plan).is_some() {
        return Err(Fail::new(errno::EIO, "GGTT-READBACK"));
    }
    Ok((off, vma))
}
