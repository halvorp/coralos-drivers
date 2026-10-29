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
//!  * THE LOGICAL SIZE (`display::size`): with a planned fastset the images are made at the logical size, and the flip off the firmware's
//!    framebuffer (and the restore back onto it) is the whole geometry burst, written once PIPEDSL is outside the vblank-evasion window
//!    ([`evade`], `intel_vblank_evade`) so the pipe source, the scaler and the plane latch on one vblank.
//!  * THE HEALTH READBACK (`scanout_health`): the pipe's FIFO underrun bit is watched from just before the fastset burst; a few frames
//!    after its latch (`ScanoutResize::health_after_ms`) what latched is read back into one `[I915-SCANOUT-HEALTH ...]` line, the
//!    firmware's IMR goes back, and an underrun rolls the firmware's geometry and framebuffer back at once
//!    (`ScanoutPair::roll_back_geometry`, `[I915-SCANOUT-SIZE ROLLBACK why=underrun]`).
use super::fail::Fail;
use super::service::{Done, Hw, UapiService, WaitKind};
use crate::display::scanout::{ImageState, LatchStep, Released, ScanoutError, ScanoutPair, SeqState};
use crate::exec::dma::CpuMem;
use crate::log::{emit, Log};
use alloc::vec::Vec;
use coralos_hal::{Dma, Mmio};
use i915_display_modeset_core::adopt::surf_alignment;
use i915_display_modeset_core::resize::VblankEvade;
use i915_display_modeset_core::scanout_health::{read_health, UnderrunWatch};
use i915_kbl_display_core::regs_write::{gen8_de_pipe_ier, gen8_de_pipe_imr};
use i915_kbl_display_core::types::Pipe;
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
    /// The health readback of the logical-size fastset: armed with its burst, due a few frames after its latch.
    pub health: Option<Health>,
}

/// One pending health readback.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Health {
    /// The underrun watch, `None` when it could not be armed (named when it was refused).
    pub watch: Option<UnderrunWatch>,
    /// When the fastset was proven latched; `None` until then.
    pub latched_ms: Option<u64>,
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
        let resize = self.resize.clone();
        let size = resize.as_ref().map_or(a.fb.size, |r| r.fb_size).div_ceil(4096) * 4096;
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
        let pair = ScanoutPair::with_resize(inc, fw, &a, [img(&made[0]), img(&made[1])], resize);
        let (w, h, stride) = geometry_of(&pair, &a);
        emit(hw.log, format_args!(
            "[I915-SCANOUT pair acquired inc={} handles={},{} ggtt={:#x},{:#x} size={:#x} {}x{} stride={} align={:#x} caching=none]",
            inc, made[0].0, made[1].0, made[0].2, made[1].2, size, w, h, stride, align));
        let rep = acquire_rep(&pair, &a, size);
        self.scanout = Some(PairRec { pair, objs: [made[0].1, made[1].1], size, health: None });
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
        self.scanout.as_ref().is_some_and(|r| {
            r.pair.awaiting_gpu().is_some() || r.pair.wants_latch_reads() || r.pair.restoring() || r.health.is_some_and(|h| h.latched_ms.is_some())
        })
    }

    /// Move the pair on: arm a flip whose image is idle, read the latch of an armed one.
    pub(crate) fn advance_scanout<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        self.advance_health(hw);
        let Some(rec) = self.scanout.as_mut() else { return };
        match rec.pair.ready_to_restore() {
            Some(Ok(plan)) => {
                if let Some(e) = rec.pair.geometry_evade() {
                    evade(hw, &e, rec.pair.resize().map_or(Pipe::A, |r| r.pipe));
                }
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
                    let geometry = rec.pair.flip_is_geometry();
                    if geometry {
                        // the underrun watch is armed before the burst, so an underrun the new size causes is latched in IIR
                        rec.health = Some(Health { watch: arm_watch(hw, rec.pair.resize().map_or(Pipe::A, |r| r.pipe)), latched_ms: None });
                    }
                    if let Some(e) = rec.pair.geometry_evade() {
                        evade(hw, &e, rec.pair.resize().map_or(Pipe::A, |r| r.pipe));
                    }
                    if let Err(e) = hw.ex.run_link_effects(&plan.write) {
                        if let Some(Health { watch: Some(w), .. }) = rec.health.take() {
                            let _ = hw.ex.run_link_effects(&w.disarm_plan());
                        }
                        if geometry {
                            // part of the burst may have landed: put every register of it back (the firmware's framebuffer is still live)
                            let back = hw.ex.run_link_effects(&plan.rollback).map_or_else(|b| b.name(), |()| "ok");
                            emit(hw.log, format_args!("[I915-SCANOUT-SIZE REFUSED why={} step=arm rollback={}] - the firmware's geometry is kept", e.name(), back));
                        }
                        let seq = rec.pair.arm_failed();
                        emit(hw.log, format_args!("[I915-PRESENT arm refused seq={:?} name={}] - nothing on the glass changed", seq, e.name()));
                        return;
                    }
                    // the arm of the FIRST flip is the line that says the register write went through; later ones are the cadence's business
                    if rec.pair.status().latched_seq == 0 {
                        emit(hw.log, format_args!("[I915-PRESENT armed first flip image={}] - the latch is read next", img.key));
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
        let geometry = rec.pair.flip_is_geometry();
        match rec.pair.observe(v) {
            LatchStep::Latched { seq, released } => {
                if geometry {
                    if let Some(r) = rec.pair.resize() {
                        crate::display::size::report_ok(hw.log, r);
                    }
                    if let Some(h) = rec.health.as_mut() {
                        h.latched_ms = Some(hw.now_ms);
                    }
                }
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
            LatchStep::Restored if rec.pair.rolled_back() => {
                emit(hw.log, format_args!("[I915-SCANOUT-SIZE ROLLBACK latched firmware-fb surf={:#x}] - the firmware's geometry is on the glass; the logical-size images are not presented again [I915-SCANOUT-RESTORED]", rec.pair.firmware_surf()));
            }
            LatchStep::Restored => {
                emit(hw.log, format_args!("[I915-SCANOUT restored firmware-fb surf={:#x}] - the display authority is gone; the boot framebuffer is on the glass [I915-SCANOUT-RESTORED]", rec.pair.firmware_surf()));
            }
            LatchStep::StillWaiting | LatchStep::Idle => {}
        }
    }

    /// The health readback, once it is due and no flip is in flight: one line, the watch disarmed, and on an underrun the rollback.
    fn advance_health<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        let Some(rec) = self.scanout.as_mut() else { return };
        let Some(h) = rec.health else { return };
        let Some(latched) = h.latched_ms else { return };
        let Some(r) = rec.pair.resize() else { return };
        let (pipe, plane_id, scaler_id, due) = (r.pipe, r.plane_id, r.scaler_id, latched.saturating_add(r.health_after_ms()));
        if rec.pair.firmware_live() {
            // the authority ended and the firmware's geometry is back already: nothing left to judge
            rec.health = None;
            if let Some(w) = h.watch {
                let _ = hw.ex.run_link_effects(&w.disarm_plan());
            }
            return;
        }
        if hw.now_ms < due || rec.pair.flip_in_flight() {
            return;
        }
        rec.health = None;
        let mut refused = None;
        let ex = &mut *hw.ex;
        let mut health = read_health(
            |off| ex.rd(i915_common_core::Reg::new(off)).unwrap_or_else(|e| {
                refused.get_or_insert(e.name());
                0
            }),
            pipe, plane_id, scaler_id, h.watch.as_ref(), hw.now_ms - latched,
        );
        if let Some(name) = refused {
            emit(hw.log, format_args!("[I915-SCANOUT-HEALTH read refused name={}] - the underrun bit is not judged", name));
            health.iir = None;
        }
        crate::display::size::report_health(hw.log, &health);
        if let Some(w) = h.watch {
            if let Err(e) = hw.ex.run_link_effects(&w.disarm_plan()) {
                emit(hw.log, format_args!("[I915-SCANOUT-HEALTH disarm refused name={}]", e.name()));
            }
        }
        if health.underrun() != Some(true) {
            return;
        }
        let Some(plan) = rec.pair.roll_back_geometry() else { return };
        if let Some(r) = rec.pair.resize() {
            crate::display::size::report_rollback(hw.log, r);
        }
        if let Some(e) = rec.pair.geometry_evade() {
            evade(hw, &e, pipe);
        }
        if let Err(e) = hw.ex.run_link_effects(&plan.write) {
            rec.pair.restore_failed();
            emit(hw.log, format_args!("[I915-SCANOUT-SIZE ROLLBACK refused name={}] - the logical size stays on the glass", e.name()));
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

/// The pair's image geometry: the logical size when it was made for one, else the adopted framebuffer's.
fn geometry_of(p: &ScanoutPair, a: &i915_display_modeset_core::initial::AdoptedDisplay) -> (u32, u32, u32) {
    p.resize().map_or((a.fb.width, a.fb.height, a.fb.stride), |r| (r.logical_w, r.logical_h, r.stride))
}

fn acquire_rep(p: &ScanoutPair, a: &i915_display_modeset_core::initial::AdoptedDisplay, size: u64) -> ScanoutAcquireRep {
    let im = p.images();
    let (width, height, stride) = geometry_of(p, a);
    ScanoutAcquireRep { handle0: im[0].handle, handle1: im[1].handle, width, height, stride, fourcc: a.fb.fourcc, modifier: a.fb.modifier, size }
}

/// Arm the underrun watch of `pipe` from its IMR / IER (`scanout_health`); `None`, named, when it cannot be.
fn arm_watch<B: Mmio, D: Dma, M: CpuMem, L: Log>(hw: &mut Hw<'_, B, D, M, L>, pipe: Pipe) -> Option<UnderrunWatch> {
    let rd = |hw: &mut Hw<'_, B, D, M, L>, off| hw.ex.rd(i915_common_core::Reg::new(off));
    let (imr, ier) = match (rd(hw, gen8_de_pipe_imr(pipe)), rd(hw, gen8_de_pipe_ier(pipe))) {
        (Ok(m), Ok(e)) => (m, e),
        (Err(e), _) | (_, Err(e)) => {
            emit(hw.log, format_args!("[I915-SCANOUT-HEALTH watch refused name={}] - underrun=unobserved", e.name()));
            return None;
        }
    };
    match UnderrunWatch::arm(pipe, imr, ier) {
        Ok((w, plan)) => match hw.ex.run_link_effects(&plan) {
            Ok(()) => Some(w),
            Err(e) => {
                let _ = hw.ex.run_link_effects(&w.disarm_plan());
                emit(hw.log, format_args!("[I915-SCANOUT-HEALTH watch refused name={}] - underrun=unobserved", e.name()));
                None
            }
        },
        Err(u) => {
            emit(hw.log, format_args!("[I915-SCANOUT-HEALTH watch refused name={} imr={:#x} ier={:#x}] - underrun=unobserved", u.name(), imr, ier));
            None
        }
    }
}

/// Bound of the PIPEDSL reads one evasion makes: the window is `resize::EVADE_US` long, so a read loop leaves it long before this;
/// a pipe whose counter never moves (or a refused read) gives up, and - as `intel_vblank_evade` does on its timeout - writes anyway.
const EVADE_MAX_READS: u32 = 100_000;

/// `intel_vblank_evade()`: read PIPEDSL until the scanline is outside the evasion window, so the burst that follows latches on ONE
/// vblank. Named when it gives up.
fn evade<B: Mmio, D: Dma, M: CpuMem, L: Log>(hw: &mut Hw<'_, B, D, M, L>, e: &VblankEvade, pipe: Pipe) {
    let reg = i915_common_core::Reg::new(i915_kbl_display_core::regs_write::pipedsl(pipe));
    let mut last = 0;
    for _ in 0..EVADE_MAX_READS {
        match hw.ex.rd(reg) {
            Ok(v) if e.clear(v) => return,
            Ok(v) => last = v,
            Err(err) => {
                emit(hw.log, format_args!("[I915-SCANOUT-SIZE evade read refused name={}] - written without evasion", err.name()));
                return;
            }
        }
    }
    emit(hw.log, format_args!("[I915-SCANOUT-SIZE evade gave up reads={} scanline={} window={}..{}] - written anyway (intel_vblank_evade's timeout)", EVADE_MAX_READS, e.scanline(last), e.min, e.max));
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
