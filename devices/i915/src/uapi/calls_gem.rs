// SPDX-License-Identifier: GPL-2.0-only
//! Device queries (GETPARAM, QUERY, REG-READ) and the GEM object calls, PRIME and the dma-buf sync bridge: decode the wire message,
//! make the call on gem-core / uapi-core, encode the reply.
use super::adapt::Pages;
use super::fail::Fail;
use super::service::{Done, Hw, LeaseRec, PrimeRec, WaitKind};
use super::UapiService;
use crate::exec::dma::{CpuMem, BO_COHERENCE};
use crate::log::Log;
use alloc::vec::Vec;
use core::cell::RefCell;
use coralos_hal::{Dma, Mmio};
use i915_common_core::Reg;
use i915_gem_core::{CreateExt, Wait, WaitTimeout, SessionId};
use i915_wire::msg::device::*;
use i915_wire::msg::gem::*;
use i915_wire::msg::prime::*;
use i915_wire::msg::session::CAP_PERFMON;
use i915_wire::msg::{self, ctx, exec, sync, Body};
use i915_wire::{errno, Header, Opcode, Status};

/// The size of the zero body a FAILED call's reply carries: the same size as its success body, so a client decodes both the same way.
pub(crate) fn error_body_len(op: Opcode) -> usize {
    use Opcode::*;
    match op {
        GetParam => GetParamRep::SIZE,
        RegRead => RegReadRep::SIZE,
        Query => 8,
        SessionOpen => i915_wire::msg::session::SessionOpenRep::SIZE,
        GemCreate | GemCreateExt => GemCreateRep::SIZE,
        GemMmapOffset => GemMmapOffsetRep::SIZE,
        GemMmapOpen | GemMmapLegacy => GemMmapRep::SIZE,
        GemSetTiling | GemGetTiling => TilingRep::SIZE,
        GemBusy => GemBusyRep::SIZE,
        GemWait => GemWaitRep::SIZE,
        GemMadvise => GemMadviseRep::SIZE,
        GemGetCaching => GemCachingRep::SIZE,
        ContextCreate | ContextCreateExt => ctx::ContextRep::SIZE,
        ContextGetParam => ctx::ContextGetParamRep::SIZE,
        VmCreate => ctx::VmCreateRep::SIZE,
        GetResetStats => ctx::GetResetStatsRep::SIZE,
        Execbuffer2 => exec::ExecRepHead::SIZE,
        SyncobjCreate => sync::SyncobjHandleRep::SIZE,
        SyncobjWait | SyncobjTimelineWait => sync::SyncobjWaitRep::SIZE,
        SyncobjQuery => sync::SyncobjQueryRepHead::SIZE,
        SyncobjHandleToFd => sync::SyncobjTokenRep::SIZE,
        SyncobjFdToHandle => sync::SyncobjHandleRep::SIZE,
        SyncFileMerge | SyncFileRedeem => sync::SyncFileTokenRep::SIZE,
        SyncFileGrant => sync::SyncFileGrantRep::SIZE,
        SyncFileWait => sync::SyncFileWaitRep::SIZE,
        PrimeHandleToFd | PrimeGrant => PrimeTokenRep::SIZE,
        PrimeFdToHandle => PrimeHandleRep::SIZE,
        DmabufExportSyncFile => DmabufSyncFileRep::SIZE,
        DisplayQuery | ScanoutAcquire | ScanoutPresent | ScanoutWait => crate::display::answer::reply_body_len(op),
        _ => 0,
    }
}

/// The cache attribute every object page carries (`exec::dma::BO_COHERENCE`): what a mapping lease IS, whatever mmap type was asked for.
fn cache_of_coherence() -> u32 {
    match BO_COHERENCE {
        coralos_hal::Coherence::Snooped => cache::WRITE_BACK,
        coralos_hal::Coherence::NonSnooped => cache::WRITE_COMBINING,
    }
}

/// `RING_TIMESTAMP(RENDER_RING_BASE)` (gt/intel_engine_regs.h:207, i915_reg.h:243): the one register `i915_reg_read_ioctl` allows
/// (i915_ioctl.c:30-38).
const RCS_TIMESTAMP: u32 = 0x2000 + 0x358;
/// `RING_TIMESTAMP_UDW`: the upper half, four bytes on (gt/intel_engine_regs.h:208).
const RCS_TIMESTAMP_UDW: u32 = RCS_TIMESTAMP + 4;
/// `I915_REG_READ_8B_WA` (i915_drm.h:2654).
const REG_READ_8B_WA: u64 = 1;
/// `intel_uncore_read64_2x32` retries while the upper half changed under the read; the bound keeps a broken counter from spinning.
const READ64_TRIES: u32 = 8;
/// The generations the whitelist entry covers (`min_graphics_ver`/`max_graphics_ver`, i915_ioctl.c:34-35).
const REG_READ_GEN: core::ops::RangeInclusive<u8> = 4..=12;

impl UapiService {
    pub(crate) fn call_device<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, op: Opcode, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        let idx = self.session_idx(inc).ok_or(Fail::new(errno::ENODEV, "STALE-SESSION"))?;
        let caps = self.sessions[idx].as_ref().map_or(0, |s| s.caps);
        let mut dev = *self.gem.device();
        dev.perfmon_capable = caps & CAP_PERFMON != 0;
        match op {
            Opcode::GetParam => Ok(Done::Len(i915_uapi_core::handle_getparam(&dev, p, out)?)),
            Opcode::Query => Ok(Done::Len(i915_uapi_core::handle_query(&dev, p, out)?)),
            _ => {
                let req = RegReadReq::from_payload(p)?;
                let flags = req.offset & 7;
                if !REG_READ_GEN.contains(&dev.gen()) || (req.offset & !7) != RCS_TIMESTAMP as u64 || (flags != 0 && flags != REG_READ_8B_WA) {
                    return Err(Fail::new(errno::EINVAL, "REG-READ-NOT-WHITELISTED")); // i915_ioctl.c:62-63, :95-96
                }
                let (lo, hi) = (Reg::new(RCS_TIMESTAMP), Reg::new(RCS_TIMESTAMP_UDW));
                let d = hw.ex.domains_for([lo, hi]);
                let val = hw.ex.awake(d, |ex| {
                    for _ in 0..READ64_TRIES {
                        let upper = ex.rd(hi)?;
                        let lower = ex.rd(lo)?;
                        if ex.rd(hi)? == upper {
                            return Ok(Some((upper as u64) << 32 | lower as u64));
                        }
                    }
                    Ok(None)
                })?;
                let val = val.ok_or(Fail::new(errno::EIO, "REG-READ-UNSTABLE"))?;
                Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&RegReadRep { val }))?))
            }
        }
    }

    pub(crate) fn call_gem<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        let sid = SessionId(inc);
        macro_rules! ok {
            ($body:expr) => {
                Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&$body))?))
            };
        }
        macro_rules! ok_empty {
            () => {
                Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?))
            };
        }
        match h.opcode {
            Opcode::GemCreate => {
                let r = GemCreateReq::from_payload(p)?;
                let c = self.gem.create(sid, r.size)?;
                ok!(GemCreateRep { handle: c.handle, size: c.size, ..Default::default() })
            }
            Opcode::GemCreateExt => {
                let r = GemCreateExtReq::decode(p)?;
                let mut regions = [(0u16, 0u16); MAX_CREATE_REGIONS as usize];
                for (i, slot) in regions.iter_mut().enumerate().take(r.region_count()) {
                    *slot = r.region(i).unwrap_or((0, 0));
                }
                let ext = CreateExt { regions: &regions[..r.region_count()], protected: r.head.protected == 1, set_pat: (r.head.set_pat_present == 1).then_some(r.head.set_pat) };
                let c = self.gem.create_ext(sid, r.head.size, r.head.flags, &ext)?;
                ok!(GemCreateRep { handle: c.handle, size: c.size, ..Default::default() })
            }
            Opcode::GemClose => {
                let r = GemCloseReq::from_payload(p)?;
                let (res, err) = self.with_sink(hw, inc, |gem, sink| gem.close(sid, r.handle, sink));
                res?;
                err.map_or(Ok(()), Err)?;
                ok_empty!()
            }
            Opcode::GemMmapOffset => {
                let r = GemMmapOffsetReq::from_payload(p)?;
                let g = self.gem.mmap_offset(sid, r.handle, r.flags)?;
                ok!(GemMmapOffsetRep { offset: g.offset, kind: g.kind as u32, cache: cache_of_coherence() })
            }
            Opcode::GemMmapOpen | Opcode::GemMmapLegacy => {
                self.leases.try_reserve(1).map_err(|_| Fail::oom("leases"))?;
                let l = if h.opcode == Opcode::GemMmapOpen {
                    let r = GemMmapOpenReq::from_payload(p)?;
                    self.with_pages(hw, |gem, pages| gem.mmap_open(sid, r.offset, r.len, pages))?
                } else {
                    let r = GemMmapLegacyReq::from_payload(p)?;
                    self.with_pages(hw, |gem, pages| gem.mmap_legacy(sid, r.handle, r.offset, r.size, r.flags, pages))?
                };
                let run = self.objs.get(l.obj).map(|o| o.run).ok_or(Fail::new(errno::EFAULT, "MMAP-OBJECT-NO-PAGES"))?;
                // the CPU maps it through the kernel: mint (or find) the object's lease; if the kernel will not, the GEM mapping is undone
                let region_cap = match self.kernel_lease_for(hw.log, inc, l.obj) {
                    Ok(h) => h,
                    Err(f) => {
                        let _ = self.with_sink(hw, inc, |gem, sink| gem.mmap_close(l.lease, sink));
                        return Err(f);
                    }
                };
                self.leases.push(LeaseRec { lease: l.lease, inc, obj: l.obj });
                ok!(GemMmapRep { lease: l.lease, cache: cache_of_coherence(), grant_addr: run.dev + l.start, len: l.len, region_cap, region_offset: l.start })
            }
            Opcode::GemMmapClose => {
                let r = GemMmapCloseReq::from_payload(p)?;
                let i = self.leases.iter().position(|l| l.lease == r.lease && l.inc == inc).ok_or(Fail::new(errno::ENOENT, "LEASE-UNKNOWN"))?;
                let (res, err) = self.with_sink(hw, inc, |gem, sink| gem.mmap_close(r.lease, sink));
                res?;
                self.leases.swap_remove(i);
                err.map_or(Ok(()), Err)?;
                ok_empty!()
            }
            Opcode::GemSetDomain => {
                let r = GemSetDomainReq::from_payload(p)?;
                self.with_pages(hw, |gem, pages| gem.set_domain(sid, r.handle, r.read_domains, r.write_domain, pages))?;
                ok_empty!()
            }
            Opcode::GemSetTiling => {
                let r = GemSetTilingReq::from_payload(p)?;
                let (res, err) = self.with_sink(hw, inc, |gem, sink| gem.set_tiling(sid, r.handle, r.tiling_mode, r.stride, sink));
                let t = res?;
                err.map_or(Ok(()), Err)?;
                ok!(TilingRep { tiling_mode: t.tiling_mode, stride: t.stride, swizzle_mode: t.swizzle_mode, ..Default::default() })
            }
            Opcode::GemGetTiling => {
                let r = GemGetTilingReq::from_payload(p)?;
                let t = self.gem.get_tiling(sid, r.handle)?;
                ok!(TilingRep { tiling_mode: t.tiling_mode, stride: t.stride, swizzle_mode: t.swizzle_mode, ..Default::default() })
            }
            Opcode::GemBusy => {
                let r = GemBusyReq::from_payload(p)?;
                ok!(GemBusyRep { busy: self.gem.busy(sid, r.handle)?, ..Default::default() })
            }
            Opcode::GemWait => self.gem_wait(hw, h, inc, p, out),
            Opcode::GemMadvise => {
                let r = GemMadviseReq::from_payload(p)?;
                let retained = self.gem.madvise(sid, r.handle, r.madv)?;
                ok!(GemMadviseRep { retained: retained as u32, ..Default::default() })
            }
            Opcode::GemSetCaching => {
                let r = GemSetCachingReq::from_payload(p)?;
                let (res, err) = self.with_sink(hw, inc, |gem, sink| gem.set_caching(sid, r.handle, r.caching, sink));
                res?;
                err.map_or(Ok(()), Err)?;
                ok_empty!()
            }
            Opcode::GemGetCaching => {
                let r = GemGetCachingReq::from_payload(p)?;
                ok!(GemCachingRep { caching: self.gem.get_caching(sid, r.handle)?, ..Default::default() })
            }
            Opcode::PrimeHandleToFd => {
                let r = PrimeHandleToFdReq::from_payload(p)?;
                self.primes.try_reserve(1).map_err(|_| Fail::oom("prime-grants"))?;
                let obj = self.gem.prime_export(sid, r.handle)?;
                let token = self.next_prime;
                self.next_prime += 1;
                self.primes.push(PrimeRec { token, obj, owner: inc, recipient: inc }); // a plain export is redeemable by its exporter only
                ok!(PrimeTokenRep { token })
            }
            Opcode::PrimeGrant => {
                let r = PrimeGrantReq::from_payload(p)?;
                if r.recipient == 0 || self.session_idx(r.recipient).is_none() {
                    return Err(Fail::new(errno::EINVAL, "PRIME-NO-SUCH-RECIPIENT"));
                }
                self.primes.try_reserve(1).map_err(|_| Fail::oom("prime-grants"))?;
                let obj = self.gem.prime_export(sid, r.handle)?;
                let token = self.next_prime;
                self.next_prime += 1;
                self.primes.push(PrimeRec { token, obj, owner: inc, recipient: r.recipient });
                ok!(PrimeTokenRep { token })
            }
            Opcode::PrimeFdToHandle => {
                let r = PrimeFdToHandleReq::from_payload(p)?;
                let g = self.primes.iter().find(|g| g.token == r.token).ok_or(Fail::new(errno::EINVAL, "PRIME-NO-SUCH-TOKEN"))?;
                if g.recipient != inc {
                    return Err(Fail::new(errno::EINVAL, "PRIME-NOT-RECIPIENT")); // same errno as an unknown token: no oracle for which tokens exist
                }
                let obj = g.obj;
                let handle = self.gem.prime_import(sid, obj)?;
                ok!(PrimeHandleRep { handle, ..Default::default() })
            }
            Opcode::DmabufExportSyncFile => {
                let r = DmabufExportSyncFileReq::from_payload(p)?;
                if r.flags & !dma_buf_sync::RW != 0 || r.flags == 0 {
                    return Err(Fail::new(errno::EINVAL, "DMABUF-SYNC-FLAGS")); // dma_buf_export_sync_file: bad flags are -EINVAL
                }
                let obj = self.gem.lookup_object(sid, r.handle)?;
                let token = self.export_object_fences(inc, obj)?;
                ok!(DmabufSyncFileRep { token, ..Default::default() })
            }
            _ => {
                let r = DmabufImportSyncFileReq::from_payload(p)?;
                if r.flags & !dma_buf_sync::RW != 0 || r.flags == 0 {
                    return Err(Fail::new(errno::EINVAL, "DMABUF-SYNC-FLAGS"));
                }
                let obj = self.gem.lookup_object(sid, r.handle)?;
                let f = self.sync.file_fence(inc, r.token)?;
                self.implicit.try_reserve(1).map_err(|_| Fail::oom("implicit-fences"))?;
                self.sync.get_fence(f);
                self.implicit.push((obj, f, inc));
                ok_empty!()
            }
        }
    }

    /// A sync_file that signals when every request still touching `obj` has (the implicit-sync read side of `dma_buf` export).
    fn export_object_fences(&mut self, inc: u64, obj: i915_gem_core::ObjId) -> Result<u32, Fail> {
        let n = self.gem.active_requests();
        let mut ids: Vec<i915_gem_core::RequestId> = Vec::new();
        ids.try_reserve_exact(n).map_err(|_| Fail::oom("export-requests"))?;
        ids.resize(n, i915_gem_core::RequestId(0));
        let got = self.gem.pending_requests(obj, &mut ids);
        let mut parts: Vec<i915_syncobj_core::FenceId> = Vec::new();
        parts.try_reserve_exact(got + self.implicit.len()).map_err(|_| Fail::oom("export-fences"))?;
        for id in &ids[..got] {
            if let Some(f) = self.inflight.iter().find(|i| i.req == *id) {
                parts.push(f.fence);
            }
        }
        for (o, f, _) in &self.implicit {
            if *o == obj {
                parts.push(*f);
            }
        }
        let merged = self.sync.merge_fences(&parts)?;
        let t = self.sync.file_from_fence(inc, merged);
        self.sync.put_fence(merged);
        Ok(t?)
    }

    /// Run a planner call that needs the object-page source.
    pub(crate) fn with_pages<B: Mmio, D: Dma, M: CpuMem, L: Log, R>(&mut self, hw: &mut Hw<'_, B, D, M, L>, f: impl FnOnce(&mut i915_gem_core::Gem, &mut Pages<'_, '_, B, D, M, L>) -> R) -> R {
        let UapiService { gem, pool, objs, .. } = self;
        let hwx = RefCell::new(super::adapt::Hwx { ex: &mut *hw.ex, gm: &mut *hw.gm, log: &mut *hw.log });
        let pool_cell = RefCell::new(&mut *pool);
        let mut pages = Pages { pool: &pool_cell, objs, hw: &hwx };
        f(gem, &mut pages)
    }

    fn gem_wait<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        let r = GemWaitReq::from_payload(p)?;
        let sid = SessionId(inc);
        match self.gem.wait_plan(sid, r.handle, r.flags, r.timeout_ns)? {
            Wait::Idle => {
                let (left, res) = i915_gem_core::wait_finish(r.timeout_ns, 0, false, JIFFY_NS);
                res?;
                Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&GemWaitRep { timeout_ns: left }))?))
            }
            Wait::Blocked { timeout, .. } => {
                let deadline = match timeout {
                    WaitTimeout::Forever => None,
                    WaitTimeout::Ns(ns) => Some(hw.now_ms.saturating_add(ns.div_ceil(1_000_000))),
                };
                self.park(*h, WaitKind::Gem { handle: r.handle, timeout_ns: r.timeout_ns }, hw.now_ms, deadline)?;
                Ok(Done::Parked)
            }
        }
    }
}

/// The kernel tick `wait_finish` rounds sub-tick remainders against: 1 ms (HZ=1000).
pub(crate) const JIFFY_NS: u64 = 1_000_000;
