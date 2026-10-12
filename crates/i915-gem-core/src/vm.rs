// SPDX-License-Identifier: GPL-2.0-only
//! Address spaces: one `i915_address_space` is a memory-core [`Gtt`] (a full 48-bit PPGTT) plus the table that maps this crate's
//! objects to its VMAs. `GEM_VM_CREATE`/`GEM_VM_DESTROY`, per-context default VMs, and the bind/unbind helpers the execbuffer,
//! shrinker and close paths share. Ported from gem/i915_gem_context.c (gem/i915_gem_context.c:1806-1868, gem/i915_gem_context.c:1603-1640), i915_vma.c (`i915_vma_instance`,
//! `i915_vma_unbind`) and gt/gen8_ppgtt.c via `i915-memory-core` (v7.0-rc6).
use alloc::vec::Vec;
use crate::error::GemError;
use crate::gem::Gem;
use crate::ids::{ObjId, SessionId, VmId};
use crate::gem::place;
use crate::seam::{Effect, EffectSink};
use crate::table;
use i915_memory_core::{Gtt, PpgttInit, VmaId};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Vma {
    pub obj: ObjId,
    pub id: VmaId,
    /// Bytes of address space the data range covers (`i915_vma_size`): the object size, or `pad_to_size` when larger.
    pub size: u64,
}

pub(crate) struct Vm {
    pub gtt: Gtt,
    pub init: PpgttInit,
    /// Holders: user-visible VM ids plus contexts (`kref`, `i915_vm_get`).
    pub refs: u16,
    pub vmas: Vec<Option<Vma>>,
}

impl Vm {
    pub(crate) fn vma_of(&self, obj: ObjId) -> Option<(usize, VmaId)> {
        self.vmas.iter().enumerate().find_map(|(i, v)| match v {
            Some(v) if v.obj == obj => Some((i, v.id)),
            _ => None,
        })
    }
    pub(crate) fn record(&mut self, obj: ObjId, id: VmaId, size: u64) -> Result<(), GemError> {
        // `ensure_record_slot` ran before the plan was made, so a free slot exists and this cannot allocate.
        let slot = self.vmas.iter().position(|v| v.is_none()).ok_or(GemError::Memory(i915_memory_core::MemError::OutOfMemory { what: "vma record" }))?;
        self.vmas[slot] = Some(Vma { obj, id, size });
        Ok(())
    }
    /// Make sure a free record slot exists (growing the record list fallibly), BEFORE a bind is planned, so a refusal leaves no
    /// orphan node in the address space. The list has no compile-time cap (RUNTIME-TABLES-HAVE-NO-CAP, D10).
    pub(crate) fn ensure_record_slot(&mut self) -> Result<(), i915_memory_core::MemError> {
        if self.vmas.iter().any(|v| v.is_none()) {
            return Ok(());
        }
        self.vmas.try_reserve(1).map_err(|_| i915_memory_core::MemError::OutOfMemory { what: "vma record" })?;
        self.vmas.push(None);
        Ok(())
    }
    pub(crate) fn forget_vma(&mut self, id: VmaId) {
        for v in self.vmas.iter_mut() {
            if matches!(v, Some(x) if x.id == id) {
                *v = None;
            }
        }
    }
}

impl Gem {
    /// `i915_ppgtt_create` (gt/intel_ppgtt.c via `gen8_ppgtt_create`): a new address space on the executor's pages. The initial
    /// clear of the new space is emitted as an unbind plan.
    pub(crate) fn create_vm_slot(&mut self, init: PpgttInit, sink: &mut dyn EffectSink) -> Result<u32, GemError> {
        let (gtt, plan) = Gtt::new_ppgtt(&self.dev.desc, &init)?;
        // Grow the table BEFORE the plan is emitted: a refusal leaves no VM and no effect.
        let i = place(&mut self.vms, Vm { gtt, init, refs: 0, vmas: Vec::new() }, &mut self.quota, "vms")?;
        sink.unbind(VmId(i as u32), &plan);
        Ok(i as u32)
    }

    pub(crate) fn vm_ref(&mut self, i: usize) {
        if let Some(v) = self.vms[i].as_mut() {
            v.refs += 1;
        }
    }

    pub(crate) fn vm_unref(&mut self, i: usize) {
        if let Some(v) = self.vms[i].as_mut() {
            v.refs = v.refs.saturating_sub(1);
        }
    }

    /// Free address spaces nobody holds (`i915_vm_release`).
    pub(crate) fn sweep_vms(&mut self, sink: &mut dyn EffectSink) {
        for i in 0..self.vms.len() {
            if matches!(&self.vms[i], Some(v) if v.refs == 0) {
                self.vms[i] = None;
                sink.effect(Effect::VmFreed { vm: VmId(i as u32) });
            }
        }
    }

    /// `i915_gem_vm_create_ioctl` (gem/i915_gem_context.c:1806-1847). `init` is the executor's PML4 + scratch pages.
    pub fn vm_create(&mut self, session: SessionId, flags: u32, extensions: u64, init: PpgttInit, sink: &mut dyn EffectSink) -> Result<u32, GemError> {
        // HAS_FULL_PPGTT: a carried Gen9 row always has it (UapiDevice::new refuses the rest).
        if flags != 0 {
            return Err(GemError::VmFlags { flags }); // gem/i915_gem_context.c:1819-1820
        }
        if extensions != 0 {
            return Err(GemError::VmFlags { flags: 0 }); // i915_user_extensions with no table: every name is unknown, gem/i915_gem_context.c:1826-1832
        }
        let slot = self.session_slot(session)?;
        // Reserve the user-visible id first (cheap to undo), then make the address space.
        let id = match self.session(slot).vm_ids.iter().position(|v| v.is_none()) {
            Some(i) => i,
            None => {
                let s = self.sessions[slot].as_mut().expect("session slot validated");
                table::push(&mut s.vm_ids, None, &mut self.quota, "vm-ids")?
            }
        };
        let v = self.create_vm_slot(init, sink)?;
        self.vm_ref(v as usize);
        self.session_mut(slot).vm_ids[id] = Some(v);
        Ok(id as u32 + 1) // xa_limit with XA_FLAGS_ALLOC1: ids start at 1 (gem/i915_gem_context.c:1757)
    }

    /// `i915_gem_vm_destroy_ioctl` (gem/i915_gem_context.c:1849-1868).
    pub fn vm_destroy(&mut self, session: SessionId, vm_id: u32, flags: u32, extensions: u64, sink: &mut dyn EffectSink) -> Result<(), GemError> {
        if flags != 0 {
            return Err(GemError::VmFlags { flags }); // gem/i915_gem_context.c:1856-1857
        }
        if extensions != 0 {
            return Err(GemError::VmFlags { flags: 0 }); // gem/i915_gem_context.c:1859-1860
        }
        let slot = self.session_slot(session)?;
        let idx = (vm_id as usize).wrapping_sub(1);
        let v = self.session(slot).vm_ids.get(idx).copied().flatten().ok_or(GemError::VmMissing { id: vm_id })?; // gem/i915_gem_context.c:1862-1864
        self.session_mut(slot).vm_ids[idx] = None;
        self.vm_unref(v as usize);
        self.sweep_vms(sink);
        Ok(())
    }

    /// `i915_gem_vm_lookup` (gem/i915_gem_context.c:352-364): the VM slot behind a session's user-visible VM id.
    pub(crate) fn vm_lookup(&self, slot: usize, vm_id: u32) -> Result<u32, GemError> {
        let idx = (vm_id as usize).wrapping_sub(1);
        self.session(slot).vm_ids.get(idx).copied().flatten().ok_or(GemError::VmMissing { id: vm_id })
    }

    /// `i915_vma_unbind` of `obj` from every address space it is bound in (`i915_gem_object_unbind`): each unbind is a plan with
    /// an `AwaitRetire`, so a busy object can be unbound without waiting here. A pinned VMA (in use by an execbuffer being
    /// planned) is left alone.
    pub(crate) fn unbind_everywhere(&mut self, obj: ObjId, sink: &mut dyn EffectSink) {
        for i in 0..self.vms.len() {
            let Some(vm) = self.vms[i].as_mut() else { continue };
            let Some((_, id)) = vm.vma_of(obj) else { continue };
            if let Ok(plan) = vm.gtt.plan_unbind(id) {
                vm.forget_vma(id);
                sink.unbind(VmId(i as u32), &plan);
            }
        }
    }

    /// Where `obj` is bound in `vm` (the address the GPU uses), if it is.
    pub fn bound_offset(&self, vm: VmId, obj: ObjId) -> Option<u64> {
        let v = self.vms.get(vm.0 as usize)?.as_ref()?;
        let (_, id) = v.vma_of(obj)?;
        v.gtt.offset_of(id).map(|(off, _, _)| off)
    }

    /// Number of VMAs live in `vm`.
    pub fn vma_count(&self, vm: VmId) -> usize {
        self.vms.get(vm.0 as usize).and_then(|v| v.as_ref()).map_or(0, |v| v.vmas.iter().flatten().count())
    }

    /// The PML4 device address of a VM, for `ContextSpec::pml4` (`px_dma(ppgtt->pd)`).
    pub fn vm_pml4(&self, vm: VmId) -> Option<u64> {
        self.vms.get(vm.0 as usize)?.as_ref().map(|v| v.init.pml4)
    }

    /// `vm->total` (`I915_CONTEXT_PARAM_GTT_SIZE`).
    pub fn vm_total(&self, vm: VmId) -> Option<u64> {
        self.vms.get(vm.0 as usize)?.as_ref().map(|v| v.gtt.total())
    }

    /// Live address spaces.
    pub fn vm_count(&self) -> usize {
        self.vms.iter().flatten().count()
    }
}
