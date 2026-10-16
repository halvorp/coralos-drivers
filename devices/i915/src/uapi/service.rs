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

/// Milliseconds between completion checks while the engine has submitted work in flight and no engine interrupt is armed. It is a timer
/// for work in flight only - an idle device schedules nothing - and it exists because the interrupt-enable registers are not on the
/// write allow-list until the owner-attended silicon increment; with an interrupt armed `next_deadline_ms` never uses it.
pub const COMPLETION_POLL_MS: u64 = 4;

/// What became of a decoded call ([`UapiService::handle_payload`]): the reply header and payload length, or parked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dispatched {
    /// The reply payload is `out[HEADER_BYTES..HEADER_BYTES + n]`; frame it under this header.
    Reply(Header, usize),
    Parked,
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
    /// The GGTT range that is LIVE before bring-up - the framebuffer the panel is showing (`crate::display::Adoption::keep`): its
    /// entries are never written and no bind lands on it (`Gtt::new_ggtt_preserving`). `None` = nothing adopted.
    pub keep: Option<i915_memory_core::Preallocated>,
}

#[derive(Debug)]
pub(crate) struct ClientSession {
    pub inc: u64,
    pub caps: u32,
    /// Who the client is (the connect announce): the recipient of its mapping leases. None until the loop records it.
    pub peer: Option<crate::conn::Peer>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LeaseRec {
    pub lease: u32,
    pub inc: u64,
    pub obj: ObjId,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PrimeRec {
    pub token: u64,
    pub obj: ObjId,
    pub owner: u64,
    /// The one session that may redeem it (ABI 1.4). `PRIME-HANDLE-TO-FD` addresses it to the exporter itself.
    pub recipient: u64,
}

/// An offer of one fence to ONE named session (`SYNC-FILE-GRANT`, ABI 1.4). It holds a reference on the fence until it is redeemed,
/// withdrawn, or either party's session ends.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FenceGrant {
    pub grant: u64,
    pub fence: i915_syncobj_core::FenceId,
    pub owner: u64,
    pub recipient: u64,
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
    /// This context's own timeline page (Linux `hwsp_alloc`): its pool run, its GGTT vma and where the GPU writes the seqno.
    pub tl: Run,
    pub tl_vma: VmaId,
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
    /// A SCANOUT-WAIT (D18): until present `seq` is latched.
    #[cfg(feature = "p4-present")]
    Scanout { seq: u64 },
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
    pub(crate) fence_grants: Vec<FenceGrant>,
    pub(crate) next_fence_grant: u64,
    pub(crate) hwctx: Vec<HwCtx>,
    pub(crate) vms: Vec<VmRec>,
    pub(crate) inflight: Vec<Inflight>,
    pub(crate) gates: Vec<Gate>,
    pub(crate) waiters: Vec<Waiter>,
    pub(crate) ready: Vec<Vec<u8>>,
    /// Fences imported onto objects with DMA_BUF_IOCTL_IMPORT_SYNC_FILE: `(object, fence, importing session)`. Work that uses the object
    /// waits for them.
    pub(crate) implicit: Vec<(ObjId, FenceId, u64)>,
    /// The kernel's DMA leases this device holds over object pages (one per object; see `mmap_lease`).
    pub(crate) kleases: Vec<super::mmap_lease::KLease>,
    pub(crate) lease_sys: alloc::boxed::Box<dyn super::kernel_lease::LeaseSys>,
    /// Pages never reused because the kernel would not confirm their lease ended.
    pub(crate) quarantined: usize,
    /// The display this device adopted from the firmware (D18), and which session, if any, is its AUTHORITY.
    pub(crate) display: crate::display::Adoption,
    pub(crate) authority: Option<u64>,
    /// The firmware's scanout as the readout found it: where the scanout pair's flips start (D18).
    pub(crate) firmware: Option<i915_display_modeset_core::adopt::FirmwareScanout>,
    /// The display authority's scanout pair (D18, feature `p4-present`).
    #[cfg(feature = "p4-present")]
    pub(crate) scanout: Option<super::calls_scanout::PairRec>,
}

/// Bring up the render engine and the GGTT and return the service: the window, the scratch page, the GGTT, the status page, the
/// engine rig with execlists enabled. The geometry (`GgttGeometry::probe`) comes from the PCI GMCH control word and the BAR sizes.
pub fn bring_up_render<B: Mmio, D: Dma, M: CpuMem, L: Log>(hw: &mut Hw<'_, B, D, M, L>, dev: UapiDevice, geom: &GgttGeometry, cfg: RenderConfig) -> Result<UapiService, Fail> {
    let info = *engines_for(&dev.desc.info).map_err(|_| Fail::new(errno::ENODEV, "NO-ENGINES"))?.as_slice().iter().find(|e| e.class == EngineClass::Render).ok_or(Fail::new(errno::ENODEV, "NO-RENDER-ENGINE"))?;
    let window = crate::exec::allow::GgttWindow::for_table(geom.pte_base, geom.entries).ok_or(Fail::new(errno::EIO, "GGTT-WINDOW"))?;
    hw.ex.set_ggtt_window(window)?;
    let scratch = hw.gm.alloc.alloc(ObjKind::Data, crate::exec::dma::PAGE)?;
    // D18: the adopted framebuffer's GGTT entries are kept exactly as the firmware left them - the panel is showing them NOW
    let (mut ggtt, create) = Gtt::new_ggtt_preserving(&dev.desc, geom, scratch.device_addr, cfg.keep.as_slice()).map_err(|e| Fail::new(errno::EIO, e.name()))?;
    if let Some(k) = cfg.keep {
        emit(hw.log, format_args!("[I915-GGTT kept={} start={:#x} size={:#x} - not written, never bound over]", k.what, k.start, k.size));
    }
    hw.ex.run_bind_step(hw.log, hw.gm, &create, true, &mut |_| true)?;
    let hwsp = hw.gm.alloc.alloc_hwsp()?;
    let hwsp_ggtt = bind_ggtt_grant(hw, &mut ggtt, &hwsp)?.0;
    let ecfg = EngineConfig { status_page_ggtt: hwsp_ggtt, quirks: cfg.quirks, wa_ctx: None, stall_bound: cfg.stall_bound_ms };
    let mut rig = EngineRig::new(&info, dev.gen(), ecfg, hwsp, hwsp_ggtt)?;
    rig.bring_up(hw.ex, hw.gm, hw.log)?;
    let mut d = dev;
    d.route_engines(1 << i915_devinfo_core::engine::RCS0); // GPU-ENGINES-ADVERTISED-ARE-ROUTED: only the render engine has a submit row
    let gem = Gem::new(d, GemConfig { pat_enabled: true });
    emit(hw.log, format_args!("[I915-UAPI READY engines=rcs0 timelines=page-each wire={}.{}]", i915_wire::ABI_MAJOR, i915_wire::ABI_MINOR));
    Ok(UapiService::from_parts(gem, ggtt, rig))
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
    pub fn from_parts(gem: Gem, ggtt: Gtt, rig: EngineRig) -> UapiService {
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
            fence_grants: Vec::new(),
            next_fence_grant: 1,
            hwctx: Vec::new(),
            vms: Vec::new(),
            inflight: Vec::new(),
            gates: Vec::new(),
            waiters: Vec::new(),
            ready: Vec::new(),
            implicit: Vec::new(),
            kleases: Vec::new(),
            lease_sys: alloc::boxed::Box::new(super::kernel_lease::NoLeaseSys),
            quarantined: 0,
            display: crate::display::Adoption::NoReadout,
            authority: None,
            firmware: None,
            #[cfg(feature = "p4-present")]
            scanout: None,
        }
    }

    /// The firmware's scanout as startup read it (the scanout pair's first flip replaces it).
    pub fn set_firmware_scanout(&mut self, f: Option<i915_display_modeset_core::adopt::FirmwareScanout>) {
        self.firmware = f;
    }

    /// The display startup adopted (or refused): `DISPLAY-QUERY` answers from it, the authority calls act on it.
    pub fn set_display(&mut self, a: crate::display::Adoption) {
        self.display = a;
    }

    /// Bind session `inc` as the DISPLAY AUTHORITY. The caller is the transport, on the strength of a capability only the
    /// compositor holds - never anything a request says. A second binding replaces the first only when the first session is gone.
    pub fn bind_display_authority<L: Log>(&mut self, log: &mut L, inc: u64) -> Result<(), Fail> {
        if self.session_idx(inc).is_none() {
            return Err(Fail::new(errno::ENOENT, "DISPLAY-AUTHORITY-NO-SESSION"));
        }
        match self.authority {
            Some(cur) if cur != inc && self.session_idx(cur).is_some() => return Err(Fail::new(errno::EBUSY, "DISPLAY-AUTHORITY-HELD")),
            _ => {}
        }
        self.authority = Some(inc);
        emit(log, format_args!("[I915-DISPLAY authority bound inc={}]", inc));
        Ok(())
    }

    pub fn display_authority(&self) -> Option<u64> {
        self.authority
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
    /// The DEVICE address of each live hardware context's timeline dword (where the GPU writes its seqno; a test plays the GPU there).
    pub fn timeline_addrs(&self) -> Vec<u64> {
        self.hwctx.iter().map(|h| h.tl.dev).collect()
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
        match self.handle_payload(hw, &h, payload, out)? {
            Dispatched::Reply(rh, n) => frame::finish(rh, n, out).map(Handled::Reply),
            Dispatched::Parked => Ok(Handled::Parked),
        }
    }

    /// [`UapiService::handle`] after the header is decoded and the payload is in hand - from the frame itself, or copied out of a
    /// payload region (`i915_wire::region`), where it may be larger than an inline frame holds. The reply PAYLOAD is written at
    /// `out[HEADER_BYTES..]` (`out` must be at least `HEADER_BYTES + 64` long) and the header it needs is returned with its length;
    /// the caller frames it (`frame::finish`, which refuses a payload past `MAX_PAYLOAD`) or places it in the client's reply area.
    pub fn handle_payload<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, payload: &[u8], out: &mut [u8]) -> Result<Dispatched, WireError> {
        let h = *h;
        if h.is_reply() {
            let e = WireError::BadValue { what: "request.flags.REPLY", got: 1 };
            return frame::malformed_reply(e, out).map(|n| Dispatched::Reply(Header::request(Opcode::Hello, 0, 0, 0).reply(0), n - HEADER_BYTES));
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
            Ok(Done::Parked) => return Ok(Dispatched::Parked),
            Err(f) => {
                if f.errno != errno::ENOENT || op as u32 >= 0x100 {
                    emit(hw.log, format_args!("[I915-UAPI op={} refused name={} errno={}]", op.name(), f.name, f.errno));
                }
                let status = if f.malformed { Status::Malformed } else if f.name == "STALE-SESSION" { Status::StaleSession } else { Status::Ok };
                write_failure(op, status, f.errno, &mut out[HEADER_BYTES..])?
            }
        };
        Ok(Dispatched::Reply(h, n))
    }

    /// Any live session's incarnation (the first), if there is one: a peer that died takes all of its sessions with it, one at a time.
    pub fn any_session(&self) -> Option<u64> {
        self.sessions.iter().flatten().next().map(|s| s.inc)
    }

    /// When the device next has something to do WITHOUT being asked: the earliest of the deadlines of parked timed waits, and - only
    /// while the engine has submitted work in flight - its stall check and (when no engine interrupt is armed, so completion cannot wake the
    /// loop by itself) a completion poll [`COMPLETION_POLL_MS`] away. `None` = nothing is timed: block until a request or an interrupt.
    /// An idle device has no deadline at all (doctrine: an idle wake-up is a bug); a timer exists only while work is in flight.
    pub fn next_deadline_ms(&self, now: u64, irq_armed: bool) -> Option<u64> {
        let mut next: Option<u64> = None;
        let mut want = |t: u64| next = Some(next.map_or(t, |n| n.min(t)));
        for w in &self.waiters {
            if let Some(d) = w.deadline_ms {
                want(d);
            }
        }
        // a flip in flight (waiting for its GPU work or for the latch) is read for every millisecond until it settles; never at idle
        #[cfg(feature = "p4-present")]
        if self.scanout_wants_time() {
            want(now.saturating_add(1));
        }
        if let Some(stall) = self.rig.next_stall_check(now) {
            want(stall);
            if !irq_armed {
                want(now.saturating_add(COMPLETION_POLL_MS));
            }
        }
        next
    }

    fn route<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        use Opcode::*;
        match h.opcode {
            Hello => Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?)),
            // describes, changes nothing: answered with or without a session (D18)
            DisplayQuery => {
                msg::Empty::from_payload(p)?;
                Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&self.display.query_reply()))?))
            }
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
            | GemMadvise | GemSetCaching | GemGetCaching | PrimeHandleToFd | PrimeFdToHandle | PrimeGrant | DmabufExportSyncFile | DmabufImportSyncFile => self.call_gem(hw, h, inc, p, out),
            ContextCreate | ContextCreateExt | ContextDestroy | ContextGetParam | ContextSetParam | VmCreate | VmDestroy | GetResetStats => self.call_ctx(hw, h.opcode, inc, p, out),
            Execbuffer2 => self.call_exec(hw, inc, p, out),
            SyncobjCreate | SyncobjDestroy | SyncobjWait | SyncobjTimelineWait | SyncobjSignal | SyncobjReset | SyncobjTimelineSignal | SyncobjQuery | SyncobjTransfer
            | SyncobjHandleToFd | SyncobjFdToHandle | FdClose | SyncFileMerge | SyncFileWait | SyncFileGrant | SyncFileRedeem => self.call_sync(hw, h, inc, p, out),
            DisplaySetMode | ScanoutAcquire | ScanoutBegin | ScanoutPresent | ScanoutWait => self.call_display(hw, h, inc, p, out),
            // already routed above
            Hello | GetReadout | SetMode | PageFlip | AllocScanout | SubmitBatch | SessionOpen | DisplayQuery => Err(Fail::new(errno::EINVAL, "UNROUTED")),
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
            Some(i) => self.sessions[i] = Some(ClientSession { inc, caps: req.caps, peer: None }),
            None => self.sessions.push(Some(ClientSession { inc, caps: req.caps, peer: None })), // reserved above
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
        // the kernel leases first: this session's CPU access ends before anything it held is freed
        self.revoke_session_leases(hw.log, inc);
        let mine: Vec<u32> = self.leases.iter().filter(|l| l.inc == inc).map(|l| l.lease).collect();
        for lease in mine {
            let _ = self.with_sink(hw, inc, |gem, sink| gem.mmap_close(lease, sink));
        }
        self.leases.retain(|l| l.inc != inc);
        // grants this session made, and grants addressed to it (nobody is left to redeem those): their references end. Handles an importer
        // already redeemed hold their own references and are untouched.
        let grants: Vec<ObjId> = self.primes.iter().filter(|p| p.owner == inc || p.recipient == inc).map(|p| p.obj).collect();
        for obj in grants {
            let _ = self.with_sink(hw, inc, |gem, sink| gem.prime_release(obj, sink));
        }
        self.primes.retain(|p| p.owner != inc && p.recipient != inc);
        let mut k = 0;
        while k < self.fence_grants.len() {
            if self.fence_grants[k].owner == inc || self.fence_grants[k].recipient == inc {
                let g = self.fence_grants.swap_remove(k);
                self.sync.put_fence(g.fence);
            } else {
                k += 1;
            }
        }
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
        if self.authority == Some(inc) {
            // the display keeps scanning out what it last latched; the next authority binds afresh and takes the same pair over
            #[cfg(feature = "p4-present")]
            self.scanout_owner_dead(inc);
            self.authority = None;
            emit(hw.log, format_args!("[I915-DISPLAY authority ended inc={}]", inc));
        }
        self.refresh_gate_state();
        self.trim_pool(hw);
        emit(hw.log, format_args!("[I915-UAPI session dead inc={}]", inc));
    }

    /// Give idle memory back to the host when the pool holds more than the live set justifies ([`PagePool::trim`]); logs what it returned.
    pub(crate) fn trim_pool<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        if !self.pool.wants_trim() {
            return;
        }
        let n = self.pool.trim(hw.gm);
        if n != 0 {
            emit(hw.log, format_args!("[I915-UAPI pool returned pages={} live={} held_grants={}]", n, self.pool.pages_out(), self.pool.grants_held()));
        }
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
