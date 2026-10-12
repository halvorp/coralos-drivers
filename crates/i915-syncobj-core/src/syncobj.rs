// SPDX-License-Identifier: GPL-2.0-only
//! A `struct drm_syncobj`: a container for ONE fence (binary) or a CHAIN of points (timeline), shared by reference between the handle
//! tables that name it.
//!
//! Binary: `drm_syncobj_replace_fence` (drm_syncobj.c:372-400) swaps the fence; NULL is "no fence yet". Timeline: `drm_syncobj_add_point`
//! (drm_syncobj.c:333-366) links a new `dma_fence_chain` node in front of whatever was there, with `dma_fence_chain_init`'s rule for
//! the node's seqno / context (dma-buf/dma-fence-chain.c:240-275): a point that is LATER than the head continues the head's context
//! and records `prev_seqno = head.seqno`; an unordered point starts a new context with `seqno = max(head.seqno, point)`.
//! `dma_fence_chain_find_seqno` (dma-buf/dma-fence-chain.c:90-109) finds the node that signals a point.
//!
//! SIMPLIFICATION, named: Linux's `dma_fence_chain_walk` garbage-collects signalled nodes as it goes. Here a chain is a `Vec` of nodes
//! and signalled nodes are pruned from the FRONT by [`Obj::prune`]; a point at or below the highest pruned seqno (`floor`) reads as the
//! stub fence, which is what its pruned node would have been. A node signals when its payload fence and every earlier payload has.
use crate::error::SyncError;
use crate::fence::{FenceId, FenceStatus, Fences};
use crate::slab::{push, Budget, Slab};
use alloc::vec::Vec;

pub const CREATE_SIGNALED: u32 = 1 << 0; // drm.h:944
pub const WAIT_ALL: u32 = 1 << 0; // drm.h:976
pub const WAIT_FOR_SUBMIT: u32 = 1 << 1; // drm.h:977
pub const WAIT_AVAILABLE: u32 = 1 << 2; // drm.h:978
pub const WAIT_DEADLINE: u32 = 1 << 3; // drm.h:979
pub const QUERY_LAST_SUBMITTED: u32 = 1 << 0; // drm.h:1046
pub const HANDLE_TO_FD_EXPORT_SYNC_FILE: u32 = 1 << 0; // drm.h:955
pub const HANDLE_TO_FD_TIMELINE: u32 = 1 << 1; // drm.h:956
pub const FD_TO_HANDLE_IMPORT_SYNC_FILE: u32 = 1 << 0; // drm.h:953
pub const FD_TO_HANDLE_TIMELINE: u32 = 1 << 1; // drm.h:954

/// A syncobj: `gen << 32 | index` in the device-wide table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncobjId(pub u64);

impl SyncobjId {
    pub(crate) const fn new(idx: u32, gen: u32) -> SyncobjId {
        SyncobjId((gen as u64) << 32 | idx as u64)
    }
    pub(crate) const fn idx(self) -> u32 {
        self.0 as u32
    }
    pub(crate) const fn gen(self) -> u32 {
        (self.0 >> 32) as u32
    }
}

/// One `dma_fence_chain` node.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub seqno: u64,
    pub context: u64,
    pub prev_seqno: u64,
    pub fence: FenceId,
}

#[derive(Debug, Default)]
pub(crate) struct Obj {
    pub refs: u32,
    /// The binary fence, or the non-chain `prev` under the first chain node.
    pub base: Option<FenceId>,
    pub chain: Vec<Node>,
    /// Highest seqno pruned from the front of the chain.
    pub floor: u64,
    next_context: u64,
}

/// Where a point lands.
pub(crate) enum Found {
    /// No fence at all, or a timeline that has not reached the point (`-EINVAL` unless the caller waits for submit).
    Missing,
    /// The syncobj's binary fence, or the stub for a pruned point.
    Fence(FenceId),
    /// Chain node `idx`.
    Node(usize),
    /// Point 0 on a syncobj with no chain: the binary fence as is (or the stub).
    Nothing,
}

impl Obj {
    /// `drm_syncobj_fence_get(syncobj) != NULL`.
    pub(crate) fn has_fence(&self) -> bool {
        self.base.is_some() || !self.chain.is_empty()
    }

    /// `dma_fence_chain_find_seqno` over this object (drm_syncobj.c:436-520 with dma-fence-chain.c:90-109).
    pub(crate) fn find(&self, point: u64) -> Found {
        if !self.has_fence() {
            return Found::Missing;
        }
        if point == 0 {
            return match self.chain.len() {
                0 => Found::Fence(self.base.expect("has_fence")),
                n => Found::Node(n - 1),
            };
        }
        // `!chain || chain->base.seqno < seqno` -> -EINVAL (dma-fence-chain.c:98-100)
        let Some(head) = self.chain.last() else { return Found::Missing };
        if head.seqno < point {
            return Found::Missing;
        }
        if point <= self.floor {
            return Found::Nothing; // pruned: it signalled long ago
        }
        // dma_fence_chain_for_each: stop at the first node of another context, or whose prev_seqno is below the point
        let mut i = self.chain.len() - 1;
        loop {
            let n = &self.chain[i];
            if n.context != head.context || n.prev_seqno < point || i == 0 {
                return Found::Node(i);
            }
            i -= 1;
        }
    }

    /// Does chain node `idx` signal? Its payload and every earlier payload (and the base) must have.
    pub(crate) fn node_status(&self, fences: &Fences, idx: usize) -> FenceStatus {
        let mut err = 0;
        let mut check = |f: FenceId| -> bool {
            match fences.status(f) {
                FenceStatus::Pending => false,
                FenceStatus::Signalled(e) => {
                    if err == 0 {
                        err = e;
                    }
                    true
                }
            }
        };
        if let Some(b) = self.base {
            if !check(b) {
                return FenceStatus::Pending;
            }
        }
        for n in &self.chain[..=idx] {
            if !check(n.fence) {
                return FenceStatus::Pending;
            }
        }
        FenceStatus::Signalled(err)
    }

    /// Drop signalled, error-free nodes from the front, remembering the highest seqno dropped. Leaves at least the head.
    pub(crate) fn prune(&mut self, fences: &mut Fences) {
        if let Some(b) = self.base {
            if fences.status(b) == FenceStatus::Signalled(0) && !self.chain.is_empty() {
                fences.put(b);
                self.base = None;
            }
        }
        if self.base.is_some() {
            return;
        }
        let mut drop = 0;
        while drop + 1 < self.chain.len() && fences.status(self.chain[drop].fence) == FenceStatus::Signalled(0) {
            drop += 1;
        }
        for n in self.chain.drain(..drop) {
            self.floor = self.floor.max(n.seqno);
            fences.put(n.fence);
        }
    }
}

/// The device-wide set of syncobjs and fences.
#[derive(Debug)]
pub struct Objects {
    pub(crate) fences: Fences,
    objs: Slab<Obj>,
}

impl Objects {
    pub(crate) const fn new() -> Objects {
        Objects { fences: Fences::new(), objs: Slab::new() }
    }
    pub fn live_syncobjs(&self) -> usize {
        self.objs.len()
    }
    pub fn live_fences(&self) -> usize {
        self.fences.live()
    }
    pub(crate) fn obj(&self, id: SyncobjId) -> Option<&Obj> {
        self.objs.get(id.idx(), id.gen())
    }
    fn obj_mut(&mut self, id: SyncobjId) -> Option<&mut Obj> {
        self.objs.get_mut(id.idx(), id.gen())
    }

    /// `drm_syncobj_create` (drm_syncobj.c:554-585): one reference for the caller; SIGNALED starts with the stub fence.
    pub(crate) fn create(&mut self, flags: u32, b: &mut Budget) -> Result<SyncobjId, SyncError> {
        if flags & !CREATE_SIGNALED != 0 {
            return Err(SyncError::BadFlags { flags }); // drm_syncobj.c:831-833
        }
        let base = if flags & CREATE_SIGNALED != 0 { Some(self.fences.stub(b)?) } else { None };
        match self.objs.insert(Obj { refs: 1, base, chain: Vec::new(), floor: 0, next_context: 1 }, b, "syncobjs") {
            Ok((i, g)) => Ok(SyncobjId::new(i, g)),
            Err(e) => {
                if let Some(f) = base {
                    self.fences.put(f);
                }
                Err(e)
            }
        }
    }

    pub(crate) fn get(&mut self, id: SyncobjId) {
        if let Some(o) = self.obj_mut(id) {
            o.refs += 1;
        }
    }

    /// `drm_syncobj_put` -> `drm_syncobj_free` (drm_syncobj.c:526-540): the last reference drops the fences.
    pub(crate) fn put(&mut self, id: SyncobjId) {
        let Some(o) = self.obj_mut(id) else { return };
        o.refs -= 1;
        if o.refs != 0 {
            return;
        }
        if let Some(o) = self.objs.remove(id.idx(), id.gen()) {
            if let Some(b) = o.base {
                self.fences.put(b);
            }
            for n in o.chain {
                self.fences.put(n.fence);
            }
        }
    }

    /// `drm_syncobj_replace_fence` (drm_syncobj.c:372-400): the syncobj becomes BINARY holding `fence` (a new reference is taken), or empty.
    pub fn replace_fence(&mut self, id: SyncobjId, fence: Option<FenceId>) {
        if let Some(f) = fence {
            self.fences.get(f);
        }
        let Some(o) = self.objs.get_mut(id.idx(), id.gen()) else {
            if let Some(f) = fence {
                self.fences.put(f);
            }
            return;
        };
        let old_base = core::mem::replace(&mut o.base, fence);
        let old_chain = core::mem::take(&mut o.chain);
        o.floor = 0;
        if let Some(b) = old_base {
            self.fences.put(b);
        }
        for n in old_chain {
            self.fences.put(n.fence);
        }
    }

    /// `drm_syncobj_add_point` (drm_syncobj.c:333-366). `fence` is the payload (a reference is taken).
    pub fn add_point(&mut self, id: SyncobjId, fence: FenceId, point: u64, b: &mut Budget) -> Result<(), SyncError> {
        let Some(o) = self.objs.get_mut(id.idx(), id.gen()) else { return Err(SyncError::NoSuchSyncobj { handle: id.0 as u32 }) };
        // dma_fence_chain_init (dma-fence-chain.c:240-275)
        let last = o.chain.last().map(|p| (p.seqno, p.context));
        let (seqno, context, prev_seqno) = match last {
            Some((ps, pc)) if point > ps => (point, pc, ps), // __dma_fence_is_later: continue the context
            Some((ps, _)) => {
                o.next_context += 1;
                (point.max(ps), o.next_context, 0) // a new context; "always have a valid sequence number"
            }
            None => {
                o.next_context += 1;
                (point, o.next_context, 0) // prev is not a chain (or absent): a new context
            }
        };
        push(&mut o.chain, Node { seqno, context, prev_seqno, fence }, b, "timeline-points")?;
        self.fences.get(fence);
        if let Some(o) = self.objs.get_mut(id.idx(), id.gen()) {
            o.prune(&mut self.fences);
        }
        Ok(())
    }

    /// `drm_syncobj_find_fence` WITHOUT the wait (drm_syncobj.c:436-520): a new reference to the fence that signals `point`, or
    /// [`SyncError::NoFence`] where Linux returns `-EINVAL`. The caller that waits for submit retries when the syncobj changes.
    pub fn find_fence(&mut self, id: SyncobjId, point: u64, b: &mut Budget) -> Result<FenceId, SyncError> {
        let handle = id.0 as u32;
        let Some(o) = self.objs.get(id.idx(), id.gen()) else { return Err(SyncError::NoSuchSyncobj { handle }) };
        match o.find(point) {
            Found::Missing => Err(SyncError::NoFence { handle, point }),
            Found::Nothing => self.fences.stub(b), // drm_syncobj.c:480-487: an already signalled point gets the stub
            Found::Fence(f) => {
                self.fences.get(f);
                Ok(f)
            }
            Found::Node(i) => {
                let mut parts: Vec<FenceId> = Vec::new();
                parts.try_reserve(i + 2).map_err(|_| SyncError::NoMem { table: "fence-members" })?;
                if let Some(base) = o.base {
                    parts.push(base);
                }
                for n in &o.chain[..=i] {
                    parts.push(n.fence);
                }
                self.fences.merge(&parts, b)
            }
        }
    }

    /// The status of the fence that signals `point` without allocating: `None` where [`Objects::find_fence`] would say NoFence.
    pub fn point_status(&self, id: SyncobjId, point: u64) -> Result<Option<FenceStatus>, SyncError> {
        let Some(o) = self.obj(id) else { return Err(SyncError::NoSuchSyncobj { handle: id.0 as u32 }) };
        Ok(match o.find(point) {
            Found::Missing => None,
            Found::Nothing => Some(FenceStatus::Signalled(0)),
            Found::Fence(f) => Some(self.fences.status(f)),
            Found::Node(i) => Some(o.node_status(&self.fences, i)),
        })
    }

    /// `drm_syncobj_query_ioctl` for one handle (drm_syncobj.c:1652-1738): 0 for a binary syncobj; for a timeline the last SUBMITTED point
    /// (the head's seqno), or the last SIGNALED one (the highest point below the first unsignalled node of the head's context).
    pub fn query(&self, id: SyncobjId, last_submitted: bool) -> Result<u64, SyncError> {
        let Some(o) = self.obj(id) else { return Err(SyncError::NoSuchSyncobj { handle: id.0 as u32 }) };
        let Some(head) = o.chain.last() else { return Ok(0) };
        if last_submitted {
            return Ok(head.seqno); // drm_syncobj.c:1696-1698
        }
        // drm_syncobj.c:1699-1711: walk the head's context; the answer is the first unsignalled node's prev_seqno, else the head's seqno
        let mut first_pending = None;
        for (i, n) in o.chain.iter().enumerate().rev() {
            if n.context != head.context {
                break;
            }
            if self.fences.status(n.fence) == FenceStatus::Pending {
                first_pending = Some(i);
            }
        }
        Ok(match first_pending {
            Some(i) => o.chain[i].prev_seqno,
            None => head.seqno,
        })
    }
}
