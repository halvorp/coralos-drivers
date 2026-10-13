// SPDX-License-Identifier: GPL-2.0-only
//! GEM CPU mapping through the kernel's DMA lease: when a lease is minted for a client session, when it is revoked, and when the pages
//! behind it may be reused.
//!
//! ONE KERNEL LEASE PER OBJECT, over the object's whole page run, for the session that mapped it. Kernel leases are page-exclusive (a page is
//! under at most one), so overlapping GEM mmaps of one object (Mesa maps and unmaps the same BO again and again) share it: GEM-MMAP-OPEN
//! finds the object's lease or mints it, the reply names it (`region_cap`) and the offset to map (`region_offset`). Closing the GEM mapping
//! does not end the kernel lease: it lives with the OBJECT.
//!
//! REVOCATION, in the only orders that are safe:
//!   * the object is FREED (`Effect::FreePages`: closed, no request in flight - the GPU retired everything that touched it - and unbound from
//!     every address space): the lease is revoked first, and the pages go back to the pool only if the kernel said the lease ended. If it will not
//!     say so, the pages are QUARANTINED (never reused, counted, logged) - a leak is safe, a page under a live CPU mapping handed to another client
//!     is not.
//!   * the session ends: its leases are revoked before anything else (its CPU mappings are already gone, the kernel removed them with the
//!     session; this ends the kernel's record). The pages still wait for the object's own free.
//!   * the planner revokes the object's mappings (`Effect::RevokeMmap`: a purged object, an exporter's last prime hold): revoked now.
//!   * another session maps an object whose previous session holds no GEM mapping of it any more: the old lease is revoked and replaced.
//! GEM_CLOSE alone revokes nothing (Linux: the mapping holds the object).
use super::fail::Fail;
use super::kernel_lease::{LeaseFail, LeaseSpec, LeaseSys, Revoked};
use super::pool::Run;
use super::UapiService;
use crate::conn::Peer;
use crate::exec::dma::BO_COHERENCE;
use crate::log::{emit, Log};
use i915_gem_core::ObjId;
use i915_wire::errno;

/// Where a kernel lease stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KState {
    Live,
    /// A revoke the kernel would not confirm: the pages behind it must never be reused.
    Unproven,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct KLease {
    pub obj: ObjId,
    pub inc: u64,
    pub handle: u64,
    pub state: KState,
}

fn fail_of(e: LeaseFail) -> Fail {
    match e {
        LeaseFail::Unavailable => Fail::new(errno::ENODEV, "MMAP-LEASE-UNAVAILABLE"),
        LeaseFail::NoMemory => Fail::oom("kernel-leases"),
        LeaseFail::Refused { name: "LEASE-PAGE-ALREADY-LEASED", .. } => Fail::new(errno::EBUSY, "LEASE-PAGE-ALREADY-LEASED"),
        LeaseFail::Refused { name, .. } => Fail::new(errno::EIO, name),
    }
}

impl UapiService {
    /// Install the kernel binding (the default refuses every GEM mmap by name).
    pub fn set_lease_sys(&mut self, sys: alloc::boxed::Box<dyn LeaseSys>) {
        self.lease_sys = sys;
    }

    /// Record who a session's client is, as the connect announce named it: the RECIPIENT of its mapping leases.
    pub fn set_session_peer(&mut self, inc: u64, peer: Peer) {
        if let Some(i) = self.session_idx(inc) {
            if let Some(s) = self.sessions[i].as_mut() {
                s.peer = Some(peer);
            }
        }
    }

    /// Kernel leases the device holds (live or unproven).
    pub fn live_kernel_leases(&self) -> usize {
        self.kleases.len()
    }

    /// Pages that will never be reused because the kernel would not confirm their lease ended.
    pub fn pages_quarantined(&self) -> usize {
        self.quarantined
    }

    /// The kernel lease handle for `obj` for session `inc`, minted if the object has none. The reply to GEM-MMAP-OPEN carries it.
    pub(crate) fn kernel_lease_for<L: Log>(&mut self, log: &mut L, inc: u64, obj: ObjId) -> Result<u64, Fail> {
        if let Some(i) = self.kleases.iter().position(|k| k.obj == obj) {
            let k = self.kleases[i];
            if k.state == KState::Unproven {
                return Err(Fail::new(errno::EIO, "MMAP-LEASE-UNPROVEN"));
            }
            if k.inc == inc {
                return Ok(k.handle);
            }
            // another session holds the object's lease: it may be replaced only when that session maps nothing of the object any more
            if self.leases.iter().any(|l| l.obj == obj && l.inc == k.inc) {
                return Err(Fail::new(errno::EBUSY, "MMAP-OBJECT-LEASED-TO-ANOTHER-SESSION"));
            }
            if !self.revoke_klease(log, i) {
                return Err(Fail::new(errno::EIO, "MMAP-LEASE-REVOKE-REFUSED"));
            }
        }
        let peer = self.session_idx(inc).and_then(|i| self.sessions[i].as_ref()).and_then(|s| s.peer).ok_or(Fail::new(errno::ENODEV, "MMAP-NO-RECIPIENT"))?;
        let run: Run = self.objs.get(obj).map(|o| o.run).ok_or(Fail::new(errno::EFAULT, "MMAP-OBJECT-NO-PAGES"))?;
        self.kleases.try_reserve(1).map_err(|_| Fail::oom("kernel-leases"))?;
        let spec = LeaseSpec { alloc_va: run.cpu, offset: 0, length: run.bytes() as u64, coherence: BO_COHERENCE, peer };
        let handle = self.lease_sys.create(&spec).map_err(fail_of)?;
        self.kleases.push(KLease { obj, inc, handle, state: KState::Live }); // room reserved above
        emit(log, format_args!("[I915-UAPI mmap lease obj={} inc={} handle={:#x} pages={}]", obj.index(), inc, handle, run.pages));
        Ok(handle)
    }

    /// Revoke record `i`. True = the kernel confirmed (the record is gone); false = it would not (the record stays, `Unproven`).
    fn revoke_klease<L: Log>(&mut self, log: &mut L, i: usize) -> bool {
        let k = self.kleases[i];
        if k.state == KState::Unproven {
            return false;
        }
        match self.lease_sys.revoke(k.handle) {
            Ok(r) => {
                self.kleases.swap_remove(i);
                emit(log, format_args!("[I915-UAPI mmap lease revoked obj={} inc={} handle={:#x} kernel={}]", k.obj.index(), k.inc, k.handle, if r == Revoked::Ended { "ended" } else { "already-gone" }));
                true
            }
            Err(e) => {
                self.kleases[i].state = KState::Unproven;
                emit(log, format_args!("[I915-UAPI mmap lease revoke UNPROVEN obj={} inc={} handle={:#x} why={:?}]", k.obj.index(), k.inc, k.handle, e));
                false
            }
        }
    }

    /// Revoke every kernel lease of `obj`. True = none is left that the kernel has not confirmed ended.
    pub(crate) fn revoke_object_leases<L: Log>(&mut self, log: &mut L, obj: ObjId) -> bool {
        let mut all = true;
        let mut i = 0;
        while i < self.kleases.len() {
            if self.kleases[i].obj != obj {
                i += 1;
            } else if !self.revoke_klease(log, i) {
                all = false;
                i += 1;
            } // a confirmed revoke swap_removes row i: look at the row that took its place
        }
        all
    }

    /// The session ended: revoke every lease it was the recipient of.
    pub(crate) fn revoke_session_leases<L: Log>(&mut self, log: &mut L, inc: u64) {
        let mut i = 0;
        while i < self.kleases.len() {
            if self.kleases[i].inc == inc && self.revoke_klease(log, i) {
                continue;
            }
            i += 1;
        }
    }

    /// The object's pages are about to be freed: may they go back to the pool? Only if every kernel lease over them ended. If not they are
    /// quarantined: dropped from the object table, NOT released, counted.
    pub(crate) fn release_object_pages<L: Log>(&mut self, log: &mut L, obj: ObjId, run: Run) {
        if self.revoke_object_leases(log, obj) {
            self.pool.release(run);
            return;
        }
        self.quarantined += run.pages;
        self.kleases.retain(|k| k.obj != obj); // the pages are gone from this device's use for good; the record would only dangle
        emit(log, format_args!("[I915-UAPI pages QUARANTINED obj={} pages={} total={} why=lease-not-confirmed-ended]", obj.index(), run.pages, self.quarantined));
    }
}
