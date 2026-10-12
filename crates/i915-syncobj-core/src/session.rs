// SPDX-License-Identifier: GPL-2.0-only
//! The ioctl-level operations of `drm_syncobj.c` over per-session handle tables (`file_private->syncobj_xa`, drm_syncobj.c:797-805).
//! Handles are `index + 1` (`XA_FLAGS_ALLOC1`) and are looked up ONLY in the calling session's table: a handle that another session
//! holds is "no such handle" here exactly as in `drm_syncobj_find` (drm_syncobj.c:248-262). A syncobj is shared between sessions only
//! by an explicit EXPORT token ([`Sync::export`]); the token dies with its owner.
use crate::error::SyncError;
use crate::fence::{FenceId, FenceStatus};
use crate::slab::{push, Budget, Slab};
use crate::syncobj::*;
use alloc::vec::Vec;

/// Handle -> syncobj for one session, plus its sync_file "descriptors".
#[derive(Debug)]
struct SessionTables {
    id: u64,
    handles: Vec<Option<SyncobjId>>,
    files: Slab<FenceId>,
}

#[derive(Clone, Copy, Debug)]
struct Export {
    obj: SyncobjId,
    owner: u64,
}

/// The result of one evaluation of a wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitEval {
    /// The wait is satisfied; `first` is `first_signaled`.
    Done { first: u32 },
    /// Not yet: re-evaluate with `Some(first)` when something changes, and give up on the device's clock.
    Pending { first: u32 },
}

/// `first_signaled` initial value (`uint32_t first = ~0`, drm_syncobj.c:1234).
pub const FIRST_NONE: u32 = u32::MAX;

#[derive(Debug)]
pub struct Sync {
    objs: Objects,
    budget: Budget,
    sessions: Vec<Option<SessionTables>>,
    exports: Slab<Export>,
}

impl Default for Sync {
    fn default() -> Self {
        Sync::new()
    }
}

const FILE_IDX_BITS: u32 = 20;

fn token_of(idx: u32, gen: u32) -> u32 {
    gen << FILE_IDX_BITS | (idx + 1)
}

fn split_token(t: u32) -> Option<(u32, u32)> {
    let i = t & ((1 << FILE_IDX_BITS) - 1);
    (i != 0).then(|| (i - 1, t >> FILE_IDX_BITS))
}

impl Sync {
    pub const fn new() -> Sync {
        Sync { objs: Objects::new(), budget: Budget::unlimited(), sessions: Vec::new(), exports: Slab::new() }
    }

    /// The per-device table quota in NEW entries (`None` = bounded only by memory). Also the allocation-failure injector.
    pub fn set_budget(&mut self, entries: Option<usize>) {
        self.budget.set(entries);
    }
    pub fn budget(&self) -> Option<usize> {
        self.budget.remaining()
    }
    pub fn live_syncobjs(&self) -> usize {
        self.objs.live_syncobjs()
    }
    pub fn live_fences(&self) -> usize {
        self.objs.live_fences()
    }
    pub fn live_exports(&self) -> usize {
        self.exports.len()
    }
    pub fn session_count(&self) -> usize {
        self.sessions.iter().flatten().count()
    }
    /// Handles this session holds.
    pub fn session_handles(&self, session: u64) -> usize {
        self.tables(session).map_or(0, |t| t.handles.iter().flatten().count())
    }
    /// sync_file descriptors this session holds.
    pub fn session_files(&self, session: u64) -> usize {
        self.tables(session).map_or(0, |t| t.files.len())
    }

    // ---- sessions ---------------------------------------------------------------------------------------------------------

    pub fn open_session(&mut self, session: u64) -> Result<(), SyncError> {
        if self.tables(session).is_some() {
            return Ok(());
        }
        let t = SessionTables { id: session, handles: Vec::new(), files: Slab::new() };
        match self.sessions.iter().position(|s| s.is_none()) {
            Some(i) => self.sessions[i] = Some(t),
            None => {
                push(&mut self.sessions, Some(t), &mut self.budget, "sync-sessions")?;
            }
        }
        Ok(())
    }

    /// `drm_syncobj_release` (drm_syncobj.c:811-820): every handle, every sync_file and every export token of the session is dropped.
    pub fn close_session(&mut self, session: u64) {
        let Some(i) = self.sessions.iter().position(|s| matches!(s, Some(t) if t.id == session)) else { return };
        let t = self.sessions[i].take().expect("found");
        for h in t.handles.into_iter().flatten() {
            self.objs.put(h);
        }
        let ids: Vec<(u32, u32)> = t.files.live_ids().collect();
        let mut files = t.files;
        for (idx, gen) in ids {
            if let Some(f) = files.remove(idx, gen) {
                self.objs.fences.put(f);
            }
        }
        let owned: Vec<(u32, u32)> = self.exports.live_ids().filter(|(i, g)| matches!(self.exports.get(*i, *g), Some(e) if e.owner == session)).collect();
        for (idx, gen) in owned {
            if let Some(e) = self.exports.remove(idx, gen) {
                self.objs.put(e.obj);
            }
        }
    }

    fn tables(&self, session: u64) -> Option<&SessionTables> {
        self.sessions.iter().flatten().find(|t| t.id == session)
    }
    fn tables_mut(&mut self, session: u64) -> Result<&mut SessionTables, SyncError> {
        self.sessions.iter_mut().flatten().find(|t| t.id == session).ok_or(SyncError::NoSuchSession { session })
    }

    /// `drm_syncobj_find` (drm_syncobj.c:248-262): the caller's own table only.
    pub fn lookup(&self, session: u64, handle: u32) -> Result<SyncobjId, SyncError> {
        let t = self.tables(session).ok_or(SyncError::NoSuchSession { session })?;
        (handle as usize)
            .checked_sub(1)
            .and_then(|i| t.handles.get(i).copied().flatten())
            .ok_or(SyncError::NoSuchSyncobj { handle })
    }

    /// `drm_syncobj_get_handle` (drm_syncobj.c:596-611): the lowest free handle, holding one reference.
    fn install(&mut self, session: u64, obj: SyncobjId) -> Result<u32, SyncError> {
        let Sync { sessions, budget, objs, .. } = self;
        let t = sessions.iter_mut().flatten().find(|t| t.id == session).ok_or(SyncError::NoSuchSession { session })?;
        let i = match t.handles.iter().position(|h| h.is_none()) {
            Some(i) => {
                t.handles[i] = Some(obj);
                i
            }
            None => push(&mut t.handles, Some(obj), budget, "syncobj-handles")?,
        };
        objs.get(obj);
        Ok(i as u32 + 1)
    }

    // ---- create / destroy -------------------------------------------------------------------------------------------------

    /// `drm_syncobj_create_ioctl` (drm_syncobj.c:822-837) over `drm_syncobj_create_as_handle` (drm_syncobj.c:613-626).
    pub fn create(&mut self, session: u64, flags: u32) -> Result<u32, SyncError> {
        self.tables_mut(session)?;
        let obj = self.objs.create(flags, &mut self.budget)?;
        let r = self.install(session, obj);
        self.objs.put(obj); // the handle table holds its own reference now (or nothing does and the object dies)
        r
    }

    /// `drm_syncobj_destroy` (drm_syncobj.c:628-639): a handle that is not there is `-EINVAL`.
    pub fn destroy(&mut self, session: u64, handle: u32) -> Result<(), SyncError> {
        let t = self.tables_mut(session)?;
        let slot = (handle as usize).checked_sub(1).and_then(|i| t.handles.get_mut(i)).and_then(|s| s.take());
        match slot {
            Some(o) => {
                self.objs.put(o);
                Ok(())
            }
            None => Err(SyncError::DestroyMissing { handle }),
        }
    }

    fn find_all(&self, session: u64, handles: &[u32]) -> Result<Vec<SyncobjId>, SyncError> {
        let mut v: Vec<SyncobjId> = Vec::new();
        v.try_reserve_exact(handles.len()).map_err(|_| SyncError::NoMem { table: "syncobj-array" })?;
        for h in handles {
            v.push(self.lookup(session, *h)?); // drm_syncobj_array_find: any missing handle is -ENOENT (drm_syncobj.c:1291-1295)
        }
        Ok(v)
    }

    // ---- reset / signal / timeline signal / query / transfer ---------------------------------------------------------------

    /// `drm_syncobj_reset_ioctl` (drm_syncobj.c:1511-1542): every syncobj loses its fence.
    pub fn reset(&mut self, session: u64, handles: &[u32]) -> Result<(), SyncError> {
        if handles.is_empty() {
            return Err(SyncError::CountZero); // drm_syncobj.c:1523-1524
        }
        let ids = self.find_all(session, handles)?;
        for id in ids {
            self.objs.replace_fence(id, None);
        }
        Ok(())
    }

    /// `drm_syncobj_signal_ioctl` (drm_syncobj.c:1544-1578): the stub (already signalled) fence replaces whatever was there.
    pub fn signal(&mut self, session: u64, handles: &[u32]) -> Result<(), SyncError> {
        if handles.is_empty() {
            return Err(SyncError::CountZero); // drm_syncobj.c:1556-1557
        }
        let ids = self.find_all(session, handles)?;
        for id in ids {
            let stub = self.objs.fences.stub(&mut self.budget)?; // drm_syncobj_assign_null_handle, drm_syncobj.c:407-419
            self.objs.replace_fence(id, Some(stub));
            self.objs.fences.put(stub);
        }
        Ok(())
    }

    /// `drm_syncobj_timeline_signal_ioctl` (drm_syncobj.c:1580-1650): a signalled stub payload at each point. Every chain node is
    /// reserved before the first is linked (the C code allocates all chains first), so a refusal leaves every syncobj as it was.
    pub fn timeline_signal(&mut self, session: u64, handles: &[u32], points: &[u64], flags: u32) -> Result<(), SyncError> {
        if flags != 0 {
            return Err(SyncError::BadFlags { flags }); // drm_syncobj.c:1592-1593
        }
        if handles.is_empty() {
            return Err(SyncError::CountZero); // drm_syncobj.c:1595-1596
        }
        if points.len() != handles.len() {
            return Err(SyncError::ArrayMismatch { handles: handles.len(), points: points.len() });
        }
        let ids = self.find_all(session, handles)?;
        let stub = self.objs.fences.stub(&mut self.budget)?;
        let mut r = Ok(());
        for (id, p) in ids.iter().zip(points) {
            r = self.objs.add_point(*id, stub, *p, &mut self.budget);
            if r.is_err() {
                break;
            }
        }
        self.objs.fences.put(stub);
        r
    }

    /// `drm_syncobj_query_ioctl` (drm_syncobj.c:1652-1738).
    pub fn query(&self, session: u64, handles: &[u32], flags: u32, out: &mut [u64]) -> Result<(), SyncError> {
        if flags & !QUERY_LAST_SUBMITTED != 0 {
            return Err(SyncError::BadFlags { flags }); // drm_syncobj.c:1668-1669
        }
        if handles.is_empty() {
            return Err(SyncError::CountZero); // drm_syncobj.c:1672-1673
        }
        if out.len() < handles.len() {
            return Err(SyncError::ArrayMismatch { handles: handles.len(), points: out.len() });
        }
        for (i, h) in handles.iter().enumerate() {
            let id = self.lookup(session, *h)?;
            out[i] = self.objs.query(id, flags & QUERY_LAST_SUBMITTED != 0)?;
        }
        Ok(())
    }

    /// `drm_syncobj_transfer_ioctl` (drm_syncobj.c:982-1000): the fence of `(src, src_point)` becomes `dst`'s fence (binary
    /// destination when `dst_point == 0`, a new chain point otherwise). `flags` is the find-fence flags: only WAIT_FOR_SUBMIT is
    /// legal (drm_syncobj.c:444-445). DEVIATION: a missing source fence is `-EINVAL` even with WAIT_FOR_SUBMIT; the device does not
    /// park a transfer.
    pub fn transfer(&mut self, session: u64, src: u32, src_point: u64, dst: u32, dst_point: u64, flags: u32) -> Result<(), SyncError> {
        if flags & !WAIT_FOR_SUBMIT != 0 {
            return Err(SyncError::BadFlags { flags });
        }
        let dst_id = self.lookup(session, dst)?; // drm_syncobj.c:930-933 / :968-970
        let src_id = self.lookup(session, src)?;
        let found = self.objs.find_fence(src_id, src_point, &mut self.budget)?;
        let r = if dst_point != 0 {
            // dma_fence_unwrap_merge(tmp) is the flattened merge find_fence already produced
            self.objs.add_point(dst_id, found, dst_point, &mut self.budget)
        } else {
            self.objs.replace_fence(dst_id, Some(found));
            Ok(())
        };
        self.objs.fences.put(found);
        r
    }

    // ---- wait ------------------------------------------------------------------------------------------------------------

    /// One evaluation of `drm_syncobj_array_wait_timeout` (drm_syncobj.c:1033-1225) for `handles` (and `points` when it is a timeline
    /// wait). `init` is `None` on the first evaluation, where a missing fence without WAIT_FOR_SUBMIT / WAIT_AVAILABLE is `-EINVAL`
    /// (drm_syncobj.c:1072-1080), and `Some(first)` — the value the first evaluation returned — afterwards. The caller owns the timeout.
    pub fn wait_eval(&mut self, session: u64, handles: &[u32], points: Option<&[u64]>, flags: u32, timeline: bool, init: Option<u32>) -> Result<WaitEval, SyncError> {
        let possible = WAIT_ALL | WAIT_FOR_SUBMIT | WAIT_DEADLINE | if timeline { WAIT_AVAILABLE } else { 0 };
        if flags & !possible != 0 {
            return Err(SyncError::BadFlags { flags }); // drm_syncobj.c:1332-1333, :1384-1385
        }
        if handles.is_empty() {
            return Ok(WaitEval::Done { first: 0 }); // drm_syncobj.c:1335-1336
        }
        if let Some(p) = points {
            if p.len() != handles.len() {
                return Err(SyncError::ArrayMismatch { handles: handles.len(), points: p.len() });
            }
        }
        let ids = self.find_all(session, handles)?;
        let lenient = flags & (WAIT_FOR_SUBMIT | WAIT_AVAILABLE) != 0;
        let all = flags & WAIT_ALL != 0;
        let mut signalled = 0usize;
        let mut first = init.unwrap_or(FIRST_NONE);
        for (i, id) in ids.iter().enumerate() {
            let point = points.map_or(0, |p| p[i]);
            let st = self.objs.point_status(*id, point)?;
            let is_signalled = match st {
                None if init.is_none() && !lenient => return Err(SyncError::NoFence { handle: handles[i], point }), // drm_syncobj.c:1072-1080
                None => false,
                Some(_) if flags & WAIT_AVAILABLE != 0 => true, // drm_syncobj.c:1092-1094
                Some(FenceStatus::Signalled(_)) => true,
                Some(FenceStatus::Pending) => false,
            };
            if is_signalled {
                if init.is_none() && signalled == 0 {
                    first = i as u32; // drm_syncobj.c:1096-1097
                }
                if init.is_some() && !all {
                    return Ok(WaitEval::Done { first: i as u32 }); // drm_syncobj.c:1163-1166
                }
                signalled += 1;
            }
        }
        let done = signalled == ids.len() || (signalled > 0 && !all);
        Ok(if done { WaitEval::Done { first } } else { WaitEval::Pending { first } })
    }

    // ---- sync_file / export tokens ----------------------------------------------------------------------------------------

    /// A sync_file for `fence` in this session (a reference is taken). The token is the descriptor.
    pub fn file_from_fence(&mut self, session: u64, fence: FenceId) -> Result<u32, SyncError> {
        let Sync { sessions, budget, objs, .. } = self;
        let t = sessions.iter_mut().flatten().find(|t| t.id == session).ok_or(SyncError::NoSuchSession { session })?;
        let (i, g) = t.files.insert(fence, budget, "sync-files")?;
        objs.fences.get(fence);
        Ok(token_of(i, g))
    }

    /// The fence behind a sync_file token of THIS session (no reference taken): `sync_file_get_fence`.
    pub fn file_fence(&self, session: u64, token: u32) -> Result<FenceId, SyncError> {
        let t = self.tables(session).ok_or(SyncError::NoSuchSession { session })?;
        split_token(token).and_then(|(i, g)| t.files.get(i, g).copied()).ok_or(SyncError::NoSuchSyncFile { token })
    }

    /// Is `token` a live sync_file of this session? (`ExecEnv::sync_file`).
    pub fn file_exists(&self, session: u64, token: u32) -> bool {
        self.file_fence(session, token).is_ok()
    }

    /// `close(fd)` of a sync_file.
    pub fn file_close(&mut self, session: u64, token: u32) -> Result<(), SyncError> {
        let t = self.tables_mut(session)?;
        let f = split_token(token).and_then(|(i, g)| t.files.remove(i, g)).ok_or(SyncError::NoSuchSyncFile { token })?;
        self.objs.fences.put(f);
        Ok(())
    }

    /// `SYNC_IOC_MERGE` (drivers/dma-buf/sync_file.c): a new sync_file that signals when both do.
    pub fn file_merge(&mut self, session: u64, a: u32, b: u32) -> Result<u32, SyncError> {
        let (fa, fb) = (self.file_fence(session, a)?, self.file_fence(session, b)?);
        let m = self.objs.fences.merge(&[fa, fb], &mut self.budget)?;
        let r = self.file_from_fence(session, m);
        self.objs.fences.put(m);
        r
    }

    /// The status of a sync_file (what `poll(2)` on it reports).
    pub fn file_status(&self, session: u64, token: u32) -> Result<FenceStatus, SyncError> {
        Ok(self.objs.fences.status(self.file_fence(session, token)?))
    }

    /// `drm_syncobj_export_sync_file` (drm_syncobj.c:757-795): a sync_file for `(handle, point)`'s fence.
    pub fn export_sync_file(&mut self, session: u64, handle: u32, point: u64) -> Result<u32, SyncError> {
        let id = self.lookup(session, handle)?;
        let f = self.objs.find_fence(id, point, &mut self.budget)?;
        let r = self.file_from_fence(session, f);
        self.objs.fences.put(f);
        r
    }

    /// `drm_syncobj_import_sync_file_fence` (drm_syncobj.c:726-755): the sync_file's fence into an EXISTING syncobj, as a point when
    /// `point != 0`.
    pub fn import_sync_file(&mut self, session: u64, handle: u32, token: u32, point: u64) -> Result<(), SyncError> {
        let f = self.file_fence(session, token)?; // -EINVAL if the descriptor is not a sync_file, drm_syncobj.c:731-732
        let id = self.lookup(session, handle)?; // -ENOENT, drm_syncobj.c:735-739
        if point != 0 {
            self.objs.add_point(id, f, point, &mut self.budget)
        } else {
            self.objs.replace_fence(id, Some(f));
            Ok(())
        }
    }

    /// `drm_syncobj_handle_to_fd` (drm_syncobj.c:662-698) for a cross-session grant: an export token holding one reference.
    pub fn export(&mut self, session: u64, handle: u32) -> Result<u64, SyncError> {
        let id = self.lookup(session, handle)?;
        let (i, g) = self.exports.insert(Export { obj: id, owner: session }, &mut self.budget, "syncobj-exports")?;
        self.objs.get(id);
        Ok((g as u64) << 32 | (i as u64 + 1))
    }

    fn export_ids(token: u64) -> Option<(u32, u32)> {
        let i = token as u32;
        (i != 0).then(|| (i - 1, (token >> 32) as u32))
    }

    /// `drm_syncobj_fd_to_handle` (drm_syncobj.c:701-724): a NEW handle in this session for the exported syncobj.
    pub fn import(&mut self, session: u64, token: u64) -> Result<u32, SyncError> {
        let e = Self::export_ids(token).and_then(|(i, g)| self.exports.get(i, g).copied()).ok_or(SyncError::NoSuchExport { token })?;
        self.install(session, e.obj)
    }

    /// `close(fd)` of the exported descriptor. Only its owner may.
    pub fn release_export(&mut self, session: u64, token: u64) -> Result<(), SyncError> {
        let (i, g) = Self::export_ids(token).ok_or(SyncError::NoSuchExport { token })?;
        match self.exports.get(i, g) {
            Some(e) if e.owner == session => {}
            _ => return Err(SyncError::NoSuchExport { token }),
        }
        let e = self.exports.remove(i, g).expect("checked");
        self.objs.put(e.obj);
        Ok(())
    }

    // ---- the executor's side: request fences ------------------------------------------------------------------------------

    /// A new PENDING fence for a request, one reference owned by the caller.
    pub fn new_fence(&mut self) -> Result<FenceId, SyncError> {
        self.objs.fences.new_pending(&mut self.budget)
    }
    pub fn signal_fence(&mut self, f: FenceId, error: i32) {
        self.objs.fences.signal(f, error);
    }
    pub fn put_fence(&mut self, f: FenceId) {
        self.objs.fences.put(f);
    }
    pub fn get_fence(&mut self, f: FenceId) {
        self.objs.fences.get(f);
    }
    pub fn fence_status(&self, f: FenceId) -> FenceStatus {
        self.objs.fences.status(f)
    }
    pub fn merge_fences(&mut self, parts: &[FenceId]) -> Result<FenceId, SyncError> {
        self.objs.fences.merge(parts, &mut self.budget)
    }

    /// `signal_fence_array` (gem/i915_gem_execbuffer.c:3018-3045): `fence` becomes the syncobj's fence (binary) or its point.
    pub fn attach_out(&mut self, session: u64, handle: u32, point: u64, fence: FenceId) -> Result<(), SyncError> {
        let id = self.lookup(session, handle)?;
        if point != 0 {
            self.objs.add_point(id, fence, point, &mut self.budget)
        } else {
            self.objs.replace_fence(id, Some(fence));
            Ok(())
        }
    }

    /// `await_fence_array` (gem/i915_gem_execbuffer.c:3000-3016): a new reference to the fence a submission must wait for.
    pub fn await_fence(&mut self, session: u64, handle: u32, point: u64) -> Result<FenceId, SyncError> {
        let id = self.lookup(session, handle)?;
        self.objs.find_fence(id, point, &mut self.budget)
    }

    /// `i915_gem_execbuffer`'s `ExecEnv::syncobj`: does the handle exist, does it hold a fence, is it a timeline, and does a point exist.
    pub fn info(&self, session: u64, handle: u32) -> Option<(bool, bool)> {
        let id = self.lookup(session, handle).ok()?;
        let o = self.objs.obj(id)?;
        Some((o.has_fence(), !o.chain.is_empty()))
    }
    /// Chain nodes the syncobj holds (the garbage-collection observable); 0 for a binary one.
    pub fn timeline_points(&self, session: u64, handle: u32) -> usize {
        self.lookup(session, handle).ok().and_then(|id| self.objs.obj(id)).map_or(0, |o| o.chain.len())
    }
    /// Does `point` exist on the syncobj (its timeline has reached it)?
    pub fn has_point(&self, session: u64, handle: u32, point: u64) -> bool {
        self.lookup(session, handle).ok().and_then(|id| self.objs.point_status(id, point).ok()).flatten().is_some()
    }
}
