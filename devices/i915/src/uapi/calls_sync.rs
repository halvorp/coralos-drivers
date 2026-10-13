// SPDX-License-Identifier: GPL-2.0-only
//! Syncobj, timeline and sync_file calls, and the parked (blocking) calls. The semantics are `i915-syncobj-core`'s; this file decodes
//! the wire, makes the call, and owns the one thing the pure module cannot: WAITING. A wait that cannot answer now is parked with a
//! deadline on the device's clock, and [`UapiService::poll_waiters`] re-evaluates it whenever anything may have changed.
use super::fail::Fail;
use super::service::{Done, Hw, WaitKind, Waiter};
use super::calls_gem::JIFFY_NS;
use super::UapiService;
use crate::exec::dma::CpuMem;
use crate::log::{emit, Log};
use alloc::vec::Vec;
use coralos_hal::{Dma, Mmio};
use i915_gem_core::{SessionId, Wait};
use i915_syncobj_core::{FenceStatus, SyncError, WaitEval};
use i915_wire::msg::sync::*;
use i915_wire::msg::{self, Body};
use i915_wire::{errno, Header, Opcode, Status, HEADER_BYTES};

fn collect32(t: i915_wire::msg::U32s<'_>) -> Result<Vec<u32>, Fail> {
    let mut v = Vec::new();
    v.try_reserve_exact(t.len()).map_err(|_| Fail::oom("syncobj-array"))?;
    v.extend(t.iter());
    Ok(v)
}
fn collect64(t: i915_wire::msg::U64s<'_>) -> Result<Vec<u64>, Fail> {
    let mut v = Vec::new();
    v.try_reserve_exact(t.len()).map_err(|_| Fail::oom("syncobj-array"))?;
    v.extend(t.iter());
    Ok(v)
}

/// A relative timeout in nanoseconds on the device's millisecond clock: `None` = forever.
fn deadline(now_ms: u64, timeout_ns: i64) -> Option<u64> {
    (timeout_ns >= 0).then(|| now_ms.saturating_add((timeout_ns as u64).div_ceil(1_000_000)))
}

impl UapiService {
    pub(crate) fn call_sync<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, h: &Header, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        macro_rules! ok {
            ($body:expr) => {
                Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&$body))?))
            };
        }
        macro_rules! ok_empty {
            () => {{
                self.poll_waiters(hw);
                Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?))
            }};
        }
        match h.opcode {
            Opcode::SyncobjCreate => {
                let r = SyncobjCreateReq::from_payload(p)?;
                ok!(SyncobjHandleRep { handle: self.sync.create(inc, r.flags)?, ..Default::default() })
            }
            Opcode::SyncobjDestroy => {
                let r = SyncobjDestroyReq::from_payload(p)?;
                if r.pad != 0 {
                    return Err(Fail::new(errno::EINVAL, "SYNCOBJ-DESTROY-PAD")); // drm_syncobj.c:846-848
                }
                self.sync.destroy(inc, r.handle)?;
                ok_empty!()
            }
            Opcode::SyncobjWait | Opcode::SyncobjTimelineWait => {
                let timeline = h.opcode == Opcode::SyncobjTimelineWait;
                let r = WaitReq::decode(p, timeline)?;
                let handles = collect32(r.handles)?;
                let points = match r.points {
                    Some(t) => Some(collect64(t)?),
                    None => None,
                };
                match self.sync.wait_eval(inc, &handles, points.as_deref(), r.head.flags, timeline, None)? {
                    WaitEval::Done { first } => ok!(SyncobjWaitRep { first_signaled: first, ..Default::default() }),
                    WaitEval::Pending { first } => {
                        if r.head.timeout_ns == 0 {
                            return Err(Fail::from(SyncError::Timeout)); // abs 0 = poll: -ETIME (drm_syncobj.c:1181-1183)
                        }
                        let dl = deadline(hw.now_ms, r.head.timeout_ns);
                        self.park(*h, WaitKind::Syncobj { handles, points, flags: r.head.flags, timeline, first }, hw.now_ms, dl)?;
                        Ok(Done::Parked)
                    }
                }
            }
            Opcode::SyncobjSignal | Opcode::SyncobjReset => {
                let r = ArrayReq::decode(p, false)?;
                if r.head.flags != 0 {
                    return Err(Fail::from(SyncError::BadFlags { flags: r.head.flags })); // drm_syncobj_array.pad (drm_syncobj.c:1520-1521)
                }
                let handles = collect32(r.handles)?;
                if h.opcode == Opcode::SyncobjSignal {
                    self.sync.signal(inc, &handles)?;
                } else {
                    self.sync.reset(inc, &handles)?;
                }
                ok_empty!()
            }
            Opcode::SyncobjTimelineSignal => {
                let r = ArrayReq::decode(p, true)?;
                let handles = collect32(r.handles)?;
                let points = collect64(r.points.ok_or(Fail::new(errno::EINVAL, "POINTS-MISSING"))?)?;
                self.sync.timeline_signal(inc, &handles, &points, r.head.flags)?;
                ok_empty!()
            }
            Opcode::SyncobjQuery => {
                let r = ArrayReq::decode(p, false)?;
                let handles = collect32(r.handles)?;
                let mut pts: Vec<u64> = Vec::new();
                pts.try_reserve_exact(handles.len()).map_err(|_| Fail::oom("syncobj-array"))?;
                pts.resize(handles.len(), 0);
                self.sync.query(inc, &handles, r.head.flags, &mut pts)?;
                Ok(Done::Len(encode_query_reply(out, 0, &pts)?))
            }
            Opcode::SyncobjTransfer => {
                let r = SyncobjTransferReq::from_payload(p)?;
                self.sync.transfer(inc, r.src_handle, r.src_point, r.dst_handle, r.dst_point, r.flags)?;
                ok_empty!()
            }
            Opcode::SyncobjHandleToFd => {
                let r = SyncobjHandleToFdReq::from_payload(p)?;
                if r.flags & !(flags::HANDLE_TO_FD_EXPORT_SYNC_FILE | flags::HANDLE_TO_FD_TIMELINE) != 0 {
                    return Err(Fail::new(errno::EINVAL, "SYNCOBJ-BAD-FLAGS")); // drm_syncobj.c:866-867
                }
                let point = if r.flags & flags::HANDLE_TO_FD_TIMELINE != 0 { r.point } else { 0 };
                if r.flags & flags::HANDLE_TO_FD_EXPORT_SYNC_FILE != 0 {
                    ok!(SyncobjTokenRep { token: self.sync.export_sync_file(inc, r.handle, point)? as u64 })
                } else {
                    if r.point != 0 {
                        return Err(Fail::new(errno::EINVAL, "SYNCOBJ-POINT-WITHOUT-TIMELINE")); // drm_syncobj.c:876-877
                    }
                    ok!(SyncobjTokenRep { token: self.sync.export(inc, r.handle)? })
                }
            }
            Opcode::SyncobjFdToHandle => {
                let r = SyncobjFdToHandleReq::from_payload(p)?;
                if r.flags & !(flags::FD_TO_HANDLE_IMPORT_SYNC_FILE | flags::FD_TO_HANDLE_TIMELINE) != 0 {
                    return Err(Fail::new(errno::EINVAL, "SYNCOBJ-BAD-FLAGS")); // drm_syncobj.c:898-899
                }
                let point = if r.flags & flags::FD_TO_HANDLE_TIMELINE != 0 { r.point } else { 0 };
                if r.flags & flags::FD_TO_HANDLE_IMPORT_SYNC_FILE != 0 {
                    let token = u32::try_from(r.token).map_err(|_| Fail::from(SyncError::NoSuchSyncFile { token: u32::MAX }))?;
                    self.sync.import_sync_file(inc, r.handle, token, point)?;
                    self.poll_waiters(hw);
                    ok!(SyncobjHandleRep { handle: r.handle, ..Default::default() })
                } else {
                    if r.point != 0 {
                        return Err(Fail::new(errno::EINVAL, "SYNCOBJ-POINT-WITHOUT-TIMELINE")); // drm_syncobj.c:913-914
                    }
                    ok!(SyncobjHandleRep { handle: self.sync.import(inc, r.token)?, ..Default::default() })
                }
            }
            Opcode::FdClose => {
                let r = FdCloseReq::from_payload(p)?;
                match r.kind {
                    fd_kind::SYNCOBJ_EXPORT => self.sync.release_export(inc, r.token)?,
                    fd_kind::SYNC_FILE => {
                        let t = u32::try_from(r.token).map_err(|_| Fail::from(SyncError::NoSuchSyncFile { token: u32::MAX }))?;
                        self.sync.file_close(inc, t)?;
                    }
                    fd_kind::PRIME => {
                        let i = self.primes.iter().position(|g| g.token == r.token && g.owner == inc).ok_or(Fail::new(errno::EINVAL, "PRIME-NO-SUCH-TOKEN"))?;
                        let obj = self.primes.swap_remove(i).obj;
                        let (_, err) = self.with_sink(hw, inc, |gem, sink| gem.prime_release(obj, sink));
                        err.map_or(Ok(()), Err)?;
                    }
                    fd_kind::SYNC_FILE_GRANT => {
                        let i = self.fence_grants.iter().position(|g| g.grant == r.token && g.owner == inc).ok_or(Fail::new(errno::EINVAL, "SYNC-FILE-NO-SUCH-GRANT"))?;
                        let g = self.fence_grants.swap_remove(i);
                        self.sync.put_fence(g.fence);
                    }
                    other => return Err(Fail { errno: errno::EINVAL, name: "FD-CLOSE-BAD-KIND", malformed: other > 3 }),
                }
                ok_empty!()
            }
            Opcode::SyncFileGrant => {
                let r = SyncFileGrantReq::from_payload(p)?;
                if r.recipient == 0 || self.session_idx(r.recipient).is_none() {
                    return Err(Fail::new(errno::EINVAL, "SYNC-FILE-NO-SUCH-RECIPIENT"));
                }
                self.fence_grants.try_reserve(1).map_err(|_| Fail::oom("fence-grants"))?;
                let fence = self.sync.file_fence(inc, r.token)?;
                self.sync.get_fence(fence); // the grant's own reference: the exporter may close its sync_file at once
                let grant = self.next_fence_grant;
                self.next_fence_grant += 1;
                self.fence_grants.push(super::service::FenceGrant { grant, fence, owner: inc, recipient: r.recipient }); // reserved above
                ok!(SyncFileGrantRep { grant })
            }
            Opcode::SyncFileRedeem => {
                let r = SyncFileRedeemReq::from_payload(p)?;
                let i = match self.fence_grants.iter().position(|g| g.grant == r.grant) {
                    Some(i) if self.fence_grants[i].recipient == inc => i,
                    Some(_) => return Err(Fail::new(errno::EINVAL, "SYNC-FILE-NOT-RECIPIENT")), // same errno as an unknown grant: no oracle
                    None => return Err(Fail::new(errno::EINVAL, "SYNC-FILE-NO-SUCH-GRANT")),
                };
                let token = self.sync.file_from_fence(inc, self.fence_grants[i].fence)?; // a refusal leaves the grant for a retry
                let g = self.fence_grants.swap_remove(i);
                self.sync.put_fence(g.fence); // the recipient's sync_file holds its own reference now
                ok!(SyncFileTokenRep { token, ..Default::default() })
            }
            Opcode::SyncFileMerge => {
                let r = SyncFileMergeReq::from_payload(p)?;
                ok!(SyncFileTokenRep { token: self.sync.file_merge(inc, r.a, r.b)?, ..Default::default() })
            }
            Opcode::SyncFileWait => {
                let r = SyncFileWaitReq::from_payload(p)?;
                match self.sync.file_status(inc, r.token)? {
                    FenceStatus::Signalled(e) => ok!(SyncFileWaitRep { fence_error: e, ..Default::default() }),
                    FenceStatus::Pending => {
                        if r.timeout_ns == 0 {
                            return Err(Fail::from(SyncError::Timeout));
                        }
                        self.park(*h, WaitKind::SyncFile { token: r.token }, hw.now_ms, deadline(hw.now_ms, r.timeout_ns))?;
                        Ok(Done::Parked)
                    }
                }
            }
            // routed by `route_session` only; an opcode that reaches here is a routing bug, refused by name rather than taken for a wait
            _ => Err(Fail::new(errno::EINVAL, "UNROUTED")),
        }
    }

    /// Re-evaluate every parked call; answer the ones that can be answered (success, error, or timeout) into the ready queue.
    pub(crate) fn poll_waiters<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        let mut i = 0;
        while i < self.waiters.len() {
            let w = self.waiters.remove(i);
            match self.eval_waiter(&w, hw.now_ms) {
                None => {
                    self.waiters.insert(i, w); // there is room: one was just removed
                    i += 1;
                }
                Some((status, errno, body)) => {
                    let mut payload = [0u8; 32];
                    let mut wr = i915_wire::codec::Writer::new(&mut payload);
                    let n = msg::encode_reply_head(&mut wr, status, -errno).and_then(|()| wr.bytes(&body.0[..body.1])).map(|()| wr.pos());
                    if let Ok(n) = n {
                        if !self.push_ready(&w.hdr, &payload[..n]) {
                            emit(hw.log, format_args!("[I915-UAPI parked reply dropped name=OUT-OF-MEMORY op={}]", w.hdr.opcode.name()));
                        }
                    }
                }
            }
        }
    }

    fn push_ready(&mut self, req: &Header, payload: &[u8]) -> bool {
        let mut frame: Vec<u8> = Vec::new();
        if frame.try_reserve_exact(HEADER_BYTES + payload.len()).is_err() || self.ready.try_reserve(1).is_err() {
            return false;
        }
        frame.resize(HEADER_BYTES + payload.len(), 0);
        if req.reply(payload.len() as u32).encode(&mut frame).is_err() {
            return false;
        }
        frame[HEADER_BYTES..].copy_from_slice(payload);
        self.ready.push(frame);
        true
    }

    /// `Some((status, positive errno, body))` once the parked call is decided.
    fn eval_waiter(&mut self, w: &Waiter, now_ms: u64) -> Option<(Status, i32, ([u8; 16], usize))> {
        let inc = w.hdr.session_incarnation;
        let late = w.deadline_ms.is_some_and(|d| now_ms >= d);
        let fail = |e: i32| Some((Status::Ok, e, ([0u8; 16], 0)));
        let body8 = |v: u32| {
            let mut b = [0u8; 16];
            b[..4].copy_from_slice(&v.to_le_bytes());
            (b, 8)
        };
        match &w.kind {
            WaitKind::Gem { handle, timeout_ns } => {
                let elapsed = now_ms.saturating_sub(w.start_ms).saturating_mul(1_000_000);
                let mut b = [0u8; 16];
                let mut finish = |busy: bool| {
                    let (left, res) = i915_gem_core::wait_finish(*timeout_ns, elapsed, busy, JIFFY_NS);
                    b[..8].copy_from_slice(&left.to_le_bytes());
                    (res, (b, 8usize))
                };
                match self.gem.wait_plan(SessionId(inc), *handle, 0, 0) {
                    Ok(Wait::Idle) => {
                        let (_, body) = finish(false);
                        Some((Status::Ok, 0, body))
                    }
                    Err(i915_gem_core::GemError::WaitTimeout) => {
                        if !late {
                            return None;
                        }
                        let (res, body) = finish(true);
                        Some((Status::Ok, res.err().map_or(errno::ETIME, |e| e.errno()), body))
                    }
                    Ok(Wait::Blocked { .. }) => None,
                    Err(e) => fail(e.errno()),
                }
            }
            WaitKind::Syncobj { handles, points, flags, timeline, first } => match self.sync.wait_eval(inc, handles, points.as_deref(), *flags, *timeline, Some(*first)) {
                Ok(WaitEval::Done { first }) => Some((Status::Ok, 0, body8(first))),
                Ok(WaitEval::Pending { .. }) => late.then(|| (Status::Ok, errno::ETIME, ([0u8; 16], 8))),
                Err(e) => fail(e.errno()),
            },
            WaitKind::SyncFile { token } => match self.sync.file_status(inc, *token) {
                Ok(FenceStatus::Signalled(e)) => Some((Status::Ok, 0, body8(e as u32))),
                Ok(FenceStatus::Pending) => late.then(|| (Status::Ok, errno::ETIME, ([0u8; 16], 8))),
                Err(e) => fail(e.errno()),
            },
        }
    }
}
