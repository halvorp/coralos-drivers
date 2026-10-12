// SPDX-License-Identifier: GPL-2.0-only
//! [`UapiService`]: the per-client sessions, the dispatch of one frame, deferred (blocking) calls, polling and session death.
//!
//! A SESSION is one client (one `drm_file`). Its incarnation is gem-core's `SessionId` (a counter that never repeats), so a frame
//! naming a dead session is `StaleSession`, never someone else's. Handles, contexts, address spaces, leases, syncobjs and
//! descriptors are all keyed by it; [`UapiService::session_dead`] frees every one. The session table, like every table here, GROWS with
//! memory and refuses by name (`OUT-OF-MEMORY`) before the session exists.
//!
//! BLOCKING CALLS (GEM-WAIT, SYNCOBJ-WAIT, SYNC-FILE-WAIT) never block the device: a call that cannot answer now is PARKED, and
//! [`UapiService::poll`] answers it later - same request id, same incarnation - when the work completes or its timeout runs out.
use super::adapt::ObjTable;
use super::fail::Fail;
use super::frame;
use super::pool::{PagePool, Run};
use crate::exec::dma::{CpuMem, GpuMem, Grant, ObjKind};
use crate::exec::engine::EngineRig;
use crate::exec::Executor;
use crate::log::{emit, Log};
use alloc::vec::Vec;
use coralos_hal::{Dma, Mmio};
use i915_gem_core::{Gem, GemConfig, ObjId, RequestId, SessionCaps, SessionId, VmId};
use i915_gt_core::emit::FlushQuirks;
use i915_gt_core::engine::{engines_for, EngineClass};
use i915_memory_core::probe::GgttGeometry;
use i915_memory_core::{Gtt, VmaId};
use i915_submit_core::EngineConfig;
use i915_syncobj_core::{FenceId, Sync};
use i915_uapi_core::UapiDevice;
use i915_wire::msg::session::{SessionOpenRep, SessionOpenReq, CAPS_KNOWN, CAP_SYS_ADMIN, CAP_SYS_NICE};
use i915_wire::msg::{self, Body};
use i915_wire::{errno, Header, Opcode, Status, WireError, HEADER_BYTES};

/// The hardware for one call: the executor, the granted memory, the log, and the device's clock in milliseconds.
pub struct Hw<'a, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    pub ex: &'a mut Executor<B>,
    pub gm: &'a mut GpuMem<D, M>,
    pub log: &'a mut L,
    pub now_ms: u64,
}

/// What became of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handled {
    /// The reply frame is `out[..n]`.
    Reply(usize),
    /// The call blocks; its reply comes out of [`UapiService::take_ready`] later.
    Parked,
}

/// The engine configuration the render engine is brought up with.
#[derive(Clone, Copy, Debug)]
pub struct RenderConfig {
    pub quirks: FlushQuirks,
    /// The engine's stall bound in milliseconds (the unit of `Hw::now_ms`).
    pub stall_bound_ms: u64,
}

#[derive(Debug)]
pub(crate) struct ClientSession {
    pub inc: u64,
    pub caps: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LeaseRec {
    pub lease: u32,
    pub inc: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PrimeRec {
    pub token: u64,
    pub obj: ObjId,
    pub owner: u64,
}

/// A hardware (execlist) context behind one GEM context.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HwCtx {
    pub slot: u32,
    pub ectx: u32,
    pub image: Run,
    pub ring: Run,
    pub image_vma: VmaId,
    pub ring_vma: VmaId,
    pub tl_slot: usize,
}

/// The pages behind one address space: the PML4 and the four scratch pages, and every page-table page planned for it.
#[derive(Debug)]
pub(crate) struct VmRec {
    pub vm: VmId,
    pub base: Run,
    pub tables: Vec<Run>,
}

#[derive(Debug)]
pub(crate) struct Inflight {
    pub req: RequestId,
    pub ectx: u32,
    pub seqno: u32,
    pub fence: FenceId,
}

/// A queued request waits for these fences before the engine may start it.
#[derive(Debug)]
pub(crate) struct Gate {
    pub fences: Vec<FenceId>,
}

#[derive(Debug)]
pub(crate) enum WaitKind {
    Gem { handle: u32, timeout_ns: i64 },
    Syncobj { handles: Vec<u32>, points: Option<Vec<u64>>, flags: u32, timeline: bool, first: u32 },
    SyncFile { token: u32 },
}

/// A parked call.
#[derive(Debug)]
pub(crate) struct Waiter {
    pub hdr: Header,
    pub kind: WaitKind,
    pub start_ms: u64,
    pub deadline_ms: Option<u64>,
}

pub struct UapiService {
    pub(crate) gem: Gem,
    pub(crate) sync: Sync,
    pub(crate) rig: EngineRig,
    pub(crate) ggtt: Gtt,
    pub(crate) pool: PagePool,
    pub(crate) objs: ObjTable,
    pub(crate) sessions: Vec<Option<ClientSession>>,
    pub(crate) leases: Vec<LeaseRec>,
    pub(crate) primes: Vec<PrimeRec>,
    pub(crate) next_prime: u64,
    pub(crate) hwctx: Vec<HwCtx>,
    /// Timeline slots in the status page: `HWSP_TIMELINE_SLOTS` entries, one per live hardware context (HARDWARE-bounded: one 4 KiB
    /// page; slot `n` is `hwsp_ggtt + HWSP_TIMELINE_BASE + n * HWSP_TIMELINE_STRIDE`).
    pub(crate) tl_used: [bool; HWSP_TIMELINE_SLOTS],
    pub(crate) vms: Vec<VmRec>,
    pub(crate) inflight: Vec<Inflight>,
    pub(crate) gates: Vec<Gate>,
    pub(crate) waiters: Vec<Waiter>,
    pub(crate) ready: Vec<Vec<u8>>,
    /// Fences imported onto objects with DMA_BUF_IOCTL_IMPORT_SYNC_FILE: `(object, fence, importing session)`. Work that uses the object
    /// waits for them.
    pub(crate) implicit: Vec<(ObjId, FenceId, u64)>,
    pub(crate) hwsp_ggtt: u32,
}

/// First timeline dword of the status page: past the CSB (`I915_HWS_CSB_BUF0_INDEX` and its write index sit below 0x100).
pub const HWSP_TIMELINE_BASE: u32 = 0x100;
/// Bytes between two timelines' seqno dwords (a cache line).
pub const HWSP_TIMELINE_STRIDE: u32 = 0x40;
/// Timelines one status page holds: `(4096 - 0x100) / 0x40`. HARDWARE-bounded (one 4 KiB page); named refusal beyond it.
pub const HWSP_TIMELINE_SLOTS: usize = ((4096 - HWSP_TIMELINE_BASE) / HWSP_TIMELINE_STRIDE) as usize;

/// Bring up the render engine and the GGTT and return the service: the window, the scratch page, the GGTT, the status page, the
/// engine rig with execlists enabled. The geometry (`GgttGeometry::probe`) comes from the PCI GMCH control word and the BAR sizes.
pub fn bring_up_render<B: Mmio, D: Dma, M: CpuMem, L: Log>(hw: &mut Hw<'_, B, D, M, L>, dev: UapiDevice, geom: &GgttGeometry, cfg: RenderConfig) -> Result<UapiService, Fail> {
    let info = *engines_for(&dev.desc.info).map_err(|_| Fail::new(errno::ENODEV, "NO-ENGINES"))?.as_slice().iter().find(|e| e.class == EngineClass::Render).ok_or(Fail::new(errno::ENODEV, "NO-RENDER-ENGINE"))?;
    let window = crate::exec::allow::GgttWindow::for_table(geom.pte_base, geom.entries).ok_or(Fail::new(errno::EIO, "GGTT-WINDOW"))?;
    hw.ex.set_ggtt_window(window)?;
    let scratch = hw.gm.alloc.alloc(ObjKind::Data, crate::exec::dma::PAGE)?;
    let (mut ggtt, create) = Gtt::new_ggtt(&dev.desc, geom, scratch.device_addr).map_err(|e| Fail::new(errno::EIO, e.name()))?;
    hw.ex.run_bind_step(hw.log, hw.gm, &create, true, &mut |_| true)?;
    let hwsp = hw.gm.alloc.alloc_hwsp()?;
    let hwsp_ggtt = bind_ggtt_grant(hw, &mut ggtt, &hwsp)?.0;
    let ecfg = EngineConfig { status_page_ggtt: hwsp_ggtt, quirks: cfg.quirks, wa_ctx: None, stall_bound: cfg.stall_bound_ms };
    let mut rig = EngineRig::new(&info, dev.gen(), ecfg, hwsp, hwsp_ggtt)?;
    rig.bring_up(hw.ex, hw.gm, hw.log)?;
    let mut d = dev;
    d.route_engines(1 << i915_devinfo_core::engine::RCS0); // GPU-ENGINES-ADVERTISED-ARE-ROUTED: only the render engine has a submit row
    let gem = Gem::new(d, GemConfig { pat_enabled: true });
    emit(hw.log, format_args!("[I915-UAPI READY engines=rcs0 timeline_slots={} wire={}.{}]", HWSP_TIMELINE_SLOTS, i915_wire::ABI_MAJOR, i915_wire::ABI_MINOR));
    Ok(UapiService::from_parts(gem, ggtt, rig, hwsp_ggtt))
}

/// Bind a granted run into the GGTT (`ggtt.plan_bind` over its pages, then execute and read back): returns the GGTT offset and the VMA.
pub(crate) fn bind_ggtt_grant<B: Mmio, D: Dma, M: CpuMem, L: Log>(hw: &mut Hw<'_, B, D, M, L>, ggtt: &mut Gtt, g: &Grant) -> Result<(u32, VmaId), Fail> {
    let pages: Vec<u64> = {
        let mut v = Vec::new();
        v.try_reserve_exact(g.pages()).map_err(|_| Fail::oom("ggtt-pages"))?;
        for i in 0..g.pages() {
            v.push(g.page(i));
        }
        v
    };
    bind_ggtt_pages(hw, ggtt, &pages)
}

pub(crate) fn bind_ggtt_pages<B: Mmio, D: Dma, M: CpuMem, L: Log>(hw: &mut Hw<'_, B, D, M, L>, ggtt: &mut Gtt, pages: &[u64]) -> Result<(u32, VmaId), Fail> {
    use i915_memory_core::{BindRequest, CacheLevel, NoTablePages, Placement};
    let req = BindRequest { pages, size: pages.len() as u64 * 4096, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place: Placement::best() };
    let plan = ggtt.plan_bind(&mut NoTablePages, &req).map_err(|e| Fail::new(errno::ENOSPC, e.name()))?;
    let (off, vma) = (plan.offset() as u32, plan.vma());
    hw.ex.run_bind_step(hw.log, hw.gm, &plan, true, &mut |_| true)?;
    if hw.ex.verify_ggtt(&plan).is_some() {
        return Err(Fail::new(errno::EIO, "GGTT-READBACK"));
    }
    Ok((off, vma))
}

impl UapiService {
    pub fn from_parts(gem: Gem, ggtt: Gtt, rig: EngineRig, hwsp_ggtt: u32) -> UapiService {
        UapiService {
            gem,
            sync: Sync::new(),
            rig,
            ggtt,
            pool: PagePool::new(),
            objs: ObjTable::default(),
            sessions: Vec::new(),
            leases: Vec::new(),
            primes: Vec::new(),
            next_prime: 1,
            hwctx: Vec::new(),
            tl_used: [false; HWSP_TIMELINE_SLOTS],
            vms: Vec::new(),
            inflight: Vec::new(),
            gates: Vec::new(),
            waiters: Vec::new(),
            ready: Vec::new(),
            implicit: Vec::new(),
            hwsp_ggtt,
        }
    }

    // ---- observation (the tests' and the markers' view) ----------------------------------------------------------------------
    pub fn gem(&self) -> &Gem {
        &self.gem
    }
    pub fn sync(&self) -> &Sync {
        &self.sync
    }
    pub fn pool(&self) -> &PagePool {
        &self.pool
    }
    pub fn rig(&self) -> &EngineRig {
        &self.rig
    }
    pub fn live_sessions(&self) -> usize {
        self.sessions.iter().flatten().count()
    }
    pub fn live_leases(&self) -> usize {
        self.leases.len()
    }
    pub fn live_primes(&self) -> usize {
        self.primes.len()
    }
    pub fn live_hw_contexts(&self) -> usize {
        self.hwctx.len()
    }
    pub fn live_vm_records(&self) -> usize {
        self.vms.len()
    }
    /// The status-page byte offset of each live hardware context's timeline dword (where the GPU writes its seqno).
    pub fn timeline_offsets(&self) -> Vec<u32> {
        self.hwctx.iter().map(|h| HWSP_TIMELINE_BASE + h.tl_slot as u32 * HWSP_TIMELINE_STRIDE).collect()
    }
    pub fn live_object_pages(&self) -> usize {
        self.objs.live()
    }
    pub fn requests_in_flight(&self) -> usize {
        self.inflight.len()
    }
    pub fn parked_calls(&self) -> usize {
        self.waiters.len()
    }
    pub fn gated_requests(&self) -> usize {
        self.gates.len()
    }
    /// Set the per-device table quota (entries) on every growable table this service owns directly, and on the planners'.
    pub fn set_table_budget(&mut self, entries: Option<usize>) {
        self.gem.set_table_budget(entries);
        self.sync.set_budget(entries);
    }

    pub(crate) fn session_idx(&self, inc: u64) -> Option<usize> {
        self.sessions.iter().position(|s| matches!(s, Some(s) if s.inc == inc))
    }

    // ---- dispatch -------------------------------------------------------------------------------------------------------------

    /// Handle one request frame. A reply frame is written to `out` (`Handled::Reply(len)`), or the call is parked
    /// (`Handled::Parked`, answered later through [`UapiService::take_ready`]). `Err` only when `out` cannot hold a reply head.
    pub fn handle<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, frame_in: &[u8], out: &mut [u8]) -> Result<Handled, WireError> {
        let (h, payload) = match Header::decode(frame_in) {
            Ok(x) => x,
            Err(e) => {
                emit(hw.log, format_args!("[I915-UAPI frame refused name={} errno={}]", e.name(), e.errno()));
                return frame::malformed_reply(e, out).map(Handled::Reply);
            }
        };
        if h.is_reply() {
            let e = WireError::BadValue { what: "request.flags.REPLY", got: 1 };
            return frame::malformed_reply(e, out).map(Handled::Reply);
        }
        if out.len() < HEADER_BYTES + 64 {
            return Err(WireError::OutputTooSmall { need: HEADER_BYTES + 64, have: out.len() });
        }
        let op = h.opcode;
        let result = {
            let area = &mut out[HEADER_BYTES..];
            self.route(hw, &h, payload, area)
        };
        let mut h = h;
        let n = match result {
            Ok(Done::Len(n)) => n,
            Ok(Done::Opened(n, inc)) => {
                h.session_incarnation = inc;
                n
            }
            Ok(Done::Parked) => return Ok(Handled::Parked),
            Err(f) => {
                if f.errno != errno::ENOENT || op as u32 >= 0x100 {
                    emit(hw.log, format_args!("[I915-UAPI op={} refused name={} errno={}]", op.name(), f.name, f.errno));
                }
                let status = if f.malformed { Status::Malformed } else if f.name == "STALE-SESSION" { Status::StaleSession } else { Status::Ok };
                write_failure(op, status, f.errno, &mut out[HEADER_BYTES..])?
            }
        };
        frame::finish(h, n, out).map(Handled::Reply)
    }

    fn route<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        use Opcode::*;
        match h.opcode {
            Hello => Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?)),
            // delete-when: the display-write increment (set-mode / page-flip / scanout allocation, plan section 3 P7+) and the pre-uAPI SUBMIT-BATCH
            // are served by their own opcodes; until then these first-generation operations answer Unsupported (2026-10-12 skeleton, still named).
            GetReadout | SetMode | PageFlip | AllocScanout | SubmitBatch => Ok(Done::Len(msg::encode_unsupported(out, msg::unsupported::NOT_IMPLEMENTED)?)),
            SessionOpen => self.session_open(hw, h, p, out),
            _ => {
                let inc = h.session_incarnation;
                if self.session_idx(inc).is_none() {
                    return Err(Fail { errno: errno::ENODEV, name: "STALE-SESSION", malformed: false });
                }
                self.route_session(hw, h, inc, p, out)
            }
        }
    }

    fn route_session<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        use Opcode::*;
        match h.opcode {
            SessionClose => {
                self.session_dead(hw, inc);
                Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?))
            }
            GetParam | Query | RegRead => self.call_device(hw, h.opcode, inc, p, out),
            GemCreate | GemCreateExt | GemClose | GemMmapOffset | GemMmapOpen | GemMmapLegacy | GemMmapClose | GemSetDomain | GemSetTiling | GemGetTiling | GemBusy | GemWait
            | GemMadvise | GemSetCaching | GemGetCaching | PrimeHandleToFd | PrimeFdToHandle | DmabufExportSyncFile | DmabufImportSyncFile => self.call_gem(hw, h, inc, p, out),
            ContextCreate | ContextCreateExt | ContextDestroy | ContextGetParam | ContextSetParam | VmCreate | VmDestroy | GetResetStats => self.call_ctx(hw, h.opcode, inc, p, out),
            Execbuffer2 => self.call_exec(hw, inc, p, out),
            SyncobjCreate | SyncobjDestroy | SyncobjWait | SyncobjTimelineWait | SyncobjSignal | SyncobjReset | SyncobjTimelineSignal | SyncobjQuery | SyncobjTransfer
            | SyncobjHandleToFd | SyncobjFdToHandle | FdClose | SyncFileMerge | SyncFileWait => self.call_sync(hw, h, inc, p, out),
            // already routed above
            Hello | GetReadout | SetMode | PageFlip | AllocScanout | SubmitBatch | SessionOpen => Err(Fail::new(errno::EINVAL, "UNROUTED")),
        }
    }

    // ---- sessions -----------------------------------------------------------------------------------------------------------------

    fn session_open<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        if h.session_incarnation != 0 {
            return Err(Fail { errno: errno::EINVAL, name: "SESSION-OPEN-NOT-ZERO", malformed: true });
        }
        let req = SessionOpenReq::from_payload(p)?;
        if req.caps & !CAPS_KNOWN != 0 {
            return Err(Fail { errno: errno::EINVAL, name: "SESSION-CAPS-UNKNOWN", malformed: true });
        }
        // room for the session record BEFORE the session exists
        self.sessions.try_reserve(1).map_err(|_| Fail::oom("sessions"))?;
        self.vms.try_reserve(1).map_err(|_| Fail::oom("vm-records"))?;
        let base = self.pool.alloc(hw.gm, VM_BASE_PAGES)?;
        let init = vm_init(&base);
        let caps = SessionCaps { sys_admin: req.caps & CAP_SYS_ADMIN != 0, sys_nice: req.caps & CAP_SYS_NICE != 0 };
        let (r, err) = self.with_sink(hw, 0, |gem, sink| gem.open_session(caps, init, sink));
        let id = match (r, err) {
            (Ok(id), None) => id,
            (Ok(id), Some(f)) => {
                // the new space's initial clear failed: the session must not exist
                let _ = self.close_gem_session(hw, id.0);
                self.pool.release(base);
                return Err(f);
            }
            (Err(e), _) => {
                self.pool.release(base);
                return Err(e.into());
            }
        };
        let inc = id.0;
        if let Err(e) = self.sync.open_session(inc) {
            let _ = self.close_gem_session(hw, inc);
            self.pool.release(base);
            return Err(e.into());
        }
        self.record_vm(base, init.pml4);
        match self.sessions.iter().position(|s| s.is_none()) {
            Some(i) => self.sessions[i] = Some(ClientSession { inc, caps: req.caps }),
            None => self.sessions.push(Some(ClientSession { inc, caps: req.caps })), // reserved above
        }
        emit(hw.log, format_args!("[I915-UAPI session open inc={} caps={:#x}]", inc, req.caps));
        let n = msg::encode_reply(out, Status::Ok, 0, Some(&SessionOpenRep { incarnation: inc }))?;
        Ok(Done::Opened(n, inc)) // the reply HEADER carries the new incarnation too
    }

    /// The session died (its peer is gone) or closed itself: free every handle, context, address space, lease, syncobj, descriptor and
    /// PRIME grant it held. Requests it still has in flight finish on the engine; their objects are freed when they retire.
    pub fn session_dead<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, inc: u64) {
        let Some(i) = self.session_idx(inc) else { return };
        // parked calls of this session are dropped silently: nobody is left to read the answer
        self.waiters.retain(|w| w.hdr.session_incarnation != inc);
        let mine: Vec<u32> = self.leases.iter().filter(|l| l.inc == inc).map(|l| l.lease).collect();
        for lease in mine {
            let _ = self.with_sink(hw, inc, |gem, sink| gem.mmap_close(lease, sink));
        }
        self.leases.retain(|l| l.inc != inc);
        let grants: Vec<ObjId> = self.primes.iter().filter(|p| p.owner == inc).map(|p| p.obj).collect();
        for obj in grants {
            let _ = self.with_sink(hw, inc, |gem, sink| gem.prime_release(obj, sink));
        }
        self.primes.retain(|p| p.owner != inc);
        let mut k = 0;
        while k < self.implicit.len() {
            if self.implicit[k].2 == inc {
                let (_, f, _) = self.implicit.swap_remove(k);
                self.sync.put_fence(f);
            } else {
                k += 1;
            }
        }
        self.sync.close_session(inc);
        let _ = self.close_gem_session(hw, inc);
        self.sessions[i] = None;
        self.refresh_gate_state();
        emit(hw.log, format_args!("[I915-UAPI session dead inc={}]", inc));
    }

    pub(crate) fn close_gem_session<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, inc: u64) -> Result<(), Fail> {
        let (r, err) = self.with_sink(hw, inc, |gem, sink| gem.close_session(SessionId(inc), sink));
        r?;
        err.map_or(Ok(()), Err)
    }
}

/// Pages of one address space's base: the PML4 and the four scratch pages (`PpgttInit`).
pub(crate) const VM_BASE_PAGES: usize = 5;

pub(crate) fn vm_init(base: &Run) -> i915_memory_core::PpgttInit {
    i915_memory_core::PpgttInit { pml4: base.page(0), scratch: [base.page(1), base.page(2), base.page(3), base.page(4)] }
}

pub(crate) enum Done {
    Len(usize),
    /// A SESSION-OPEN reply: the payload length and the incarnation the header must carry.
    Opened(usize, u64),
    Parked,
}

/// A failure reply: `status`, `-errno`, and a zero body of the size the opcode's reply has.
fn write_failure(op: Opcode, status: Status, errno: i32, out: &mut [u8]) -> Result<usize, WireError> {
    let body = super::calls_gem::error_body_len(op);
    let mut w = i915_wire::codec::Writer::new(out);
    msg::encode_reply_head(&mut w, status, -errno)?;
    w.zeros(body)?;
    Ok(w.pos())
}
