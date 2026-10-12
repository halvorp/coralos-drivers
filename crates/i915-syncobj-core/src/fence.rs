// SPDX-License-Identifier: GPL-2.0-only
//! `struct dma_fence` reduced to what syncobj semantics need: a fence is PENDING, or SIGNALLED with an error word (0 = success, a
//! negative errno = the work failed, `dma_fence_set_error`), or an ARRAY of plain fences that signals when all of them have
//! (`dma_fence_array`, the shape `sync_file` merges and `dma_fence_unwrap_merge` produce). An array never contains an array: a merge
//! flattens (dma-buf/dma-fence-unwrap.c does the same), so every walk is two levels deep and nothing recurses.
//!
//! Reference counts are explicit: `get` / `put`. A fence is freed when its last holder puts it; an array puts its members.
use crate::error::SyncError;
use crate::slab::{Budget, Slab};
use alloc::vec::Vec;

/// A fence: `gen << 32 | index`. Never reused while a holder could present it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FenceId(pub u64);

impl FenceId {
    const fn new(idx: u32, gen: u32) -> FenceId {
        FenceId((gen as u64) << 32 | idx as u64)
    }
    const fn idx(self) -> u32 {
        self.0 as u32
    }
    const fn gen(self) -> u32 {
        (self.0 >> 32) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FenceStatus {
    Pending,
    /// Signalled; the error word is 0 or a negative errno.
    Signalled(i32),
}

#[derive(Debug)]
enum State {
    Plain(FenceStatus),
    Array(Vec<FenceId>),
}

#[derive(Debug)]
struct Fence {
    refs: u32,
    state: State,
}

#[derive(Debug)]
pub struct Fences {
    slab: Slab<Fence>,
    stub: Option<FenceId>,
}

impl Fences {
    pub(crate) const fn new() -> Fences {
        Fences { slab: Slab::new(), stub: None }
    }
    /// Live fences (the leak counter the tests read).
    pub fn live(&self) -> usize {
        self.slab.len()
    }

    /// A new PENDING fence holding one reference for the caller.
    pub fn new_pending(&mut self, b: &mut Budget) -> Result<FenceId, SyncError> {
        let (i, g) = self.slab.insert(Fence { refs: 1, state: State::Plain(FenceStatus::Pending) }, b, "fences")?;
        Ok(FenceId::new(i, g))
    }

    /// `dma_fence_get_stub` (dma-buf/dma-fence.c): an already-signalled fence; one shared object, a new reference each call.
    pub fn stub(&mut self, b: &mut Budget) -> Result<FenceId, SyncError> {
        if let Some(s) = self.stub.filter(|s| self.exists(*s)) {
            self.get(s);
            return Ok(s);
        }
        let (i, g) = self.slab.insert(Fence { refs: 1, state: State::Plain(FenceStatus::Signalled(0)) }, b, "fences")?;
        let s = FenceId::new(i, g);
        self.stub = Some(s); // remembered so the next caller shares it; it is freed with its last holder
        Ok(s)
    }

    pub fn get(&mut self, f: FenceId) {
        if let Some(x) = self.slab.get_mut(f.idx(), f.gen()) {
            x.refs += 1;
        }
    }

    /// Drop one reference; the last one frees the fence and puts an array's members.
    pub fn put(&mut self, f: FenceId) {
        let Some(x) = self.slab.get_mut(f.idx(), f.gen()) else { return };
        x.refs -= 1;
        if x.refs != 0 {
            return;
        }
        if let Some(Fence { state: State::Array(members), .. }) = self.slab.remove(f.idx(), f.gen()) {
            for m in members {
                self.put(m); // members are plain: depth two
            }
        }
        if self.stub == Some(f) {
            self.stub = None;
        }
    }

    /// Signal a plain fence once (`dma_fence_signal` on an already signalled fence is a no-op that reports `-EINVAL`; here the FIRST
    /// error word stays). An array cannot be signalled directly.
    pub fn signal(&mut self, f: FenceId, error: i32) {
        if let Some(Fence { state: State::Plain(st @ FenceStatus::Pending), .. }) = self.slab.get_mut(f.idx(), f.gen()) {
            *st = FenceStatus::Signalled(error);
        }
    }

    pub fn exists(&self, f: FenceId) -> bool {
        self.slab.get(f.idx(), f.gen()).is_some()
    }

    /// `dma_fence_is_signaled`, with the error word. An array is signalled when every member is; its error is the first member error.
    /// A freed fence reads as signalled with no error (nothing is left to wait for).
    pub fn status(&self, f: FenceId) -> FenceStatus {
        match self.slab.get(f.idx(), f.gen()) {
            None => FenceStatus::Signalled(0),
            Some(Fence { state: State::Plain(s), .. }) => *s,
            Some(Fence { state: State::Array(ms), .. }) => {
                let mut err = 0;
                for m in ms {
                    match self.status(*m) {
                        FenceStatus::Pending => return FenceStatus::Pending,
                        FenceStatus::Signalled(e) => {
                            if err == 0 {
                                err = e;
                            }
                        }
                    }
                }
                FenceStatus::Signalled(err)
            }
        }
    }

    /// The plain fences behind `f` (the fence itself, or an array's members).
    fn leaves(&self, f: FenceId, out: &mut Vec<FenceId>) -> Result<(), SyncError> {
        match self.slab.get(f.idx(), f.gen()) {
            None => Ok(()),
            Some(Fence { state: State::Plain(_), .. }) => push_unique(out, f),
            Some(Fence { state: State::Array(ms), .. }) => {
                for m in ms {
                    push_unique(out, *m)?;
                }
                Ok(())
            }
        }
    }

    /// `dma_fence_unwrap_merge` / `sync_file` merge (dma-buf/dma-fence-unwrap.c, drivers/dma-buf/sync_file.c): a new fence that signals when
    /// every given fence has. Signalled leaves are dropped; if nothing is left the result is the stub. The caller's references are
    /// untouched; the result carries one reference for the caller.
    pub fn merge(&mut self, parts: &[FenceId], b: &mut Budget) -> Result<FenceId, SyncError> {
        let mut leaves: Vec<FenceId> = Vec::new();
        for p in parts {
            self.leaves(*p, &mut leaves)?;
        }
        leaves.retain(|l| self.status(*l) == FenceStatus::Pending);
        match leaves.len() {
            0 => self.stub(b),
            1 => {
                let l = leaves[0];
                self.get(l);
                Ok(l)
            }
            n => {
                let (i, g) = self.slab.insert(Fence { refs: 1, state: State::Array(leaves) }, b, "fences")?;
                let id = FenceId::new(i, g);
                // the array now exists: it takes a reference on each member (so a refused insert above took none)
                for k in 0..n {
                    let m = match self.slab.get(i, g) {
                        Some(Fence { state: State::Array(ms), .. }) => ms[k],
                        _ => break,
                    };
                    self.get(m);
                }
                Ok(id)
            }
        }
    }
}

/// A scratch list, grown fallibly (no budget charge: it is a temporary, not a table).
fn push_unique(out: &mut Vec<FenceId>, f: FenceId) -> Result<(), SyncError> {
    if out.contains(&f) {
        return Ok(());
    }
    out.try_reserve(1).map_err(|_| SyncError::NoMem { table: "fence-members" })?;
    out.push(f);
    Ok(())
}
