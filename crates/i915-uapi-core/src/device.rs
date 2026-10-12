// SPDX-License-Identifier: GPL-2.0-only
//! The device facts every uAPI answer is derived from. The STATIC ones come from `i915-devinfo-core`'s [`PlatformDescriptor`]
//! (generation, engine mask, PPGTT type/size, LLC, coherent GGTT, reset-engine, GuC capability); the RUNTIME ones are measured by
//! the device (fuse decode, CTC clock, eDRAM probe, golden-context capture, RAM size) and handed in as values. Nothing here is a
//! Gen9 constant typed from memory: a row that is not Gen9 is refused by [`UapiDevice::new`].
use crate::consts::misc as m;
use crate::types::engine_class;
use i915_devinfo_core::engine as eng;
use i915_devinfo_core::sseu::SseuInfo;
use i915_devinfo_core::{I915Error, PlatformDescriptor, PpgttType};

/// One user-visible engine (`engine->uabi_class` / `uabi_instance`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UabiEngine {
    pub class: u16,
    pub instance: u16,
}

/// The engines in `uabi_engines` order: sorted by uabi class then instance (`engine_cmp`, gt/intel_engine_user.c:57-76).
#[derive(Clone, Copy, Debug)]
pub struct UabiEngines {
    list: [UabiEngine; MAX_UABI_ENGINES],
    len: usize,
}

/// KEPT AS VALIDATION (D10 / RUNTIME-TABLES-HAVE-NO-CAP): the engine set is fused into the part - hardware, not a client population.
/// RCS0 + BCS0 + VCS0..1 + VECS0 are all the engines a carried Gen9 row can have.
pub const MAX_UABI_ENGINES: usize = 5;

impl UabiEngines {
    pub fn as_slice(&self) -> &[UabiEngine] {
        &self.list[..self.len]
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// `intel_engine_lookup_user(i915, class, instance)` (gt/intel_engine_user.c:19-39).
    pub fn lookup(&self, class: u16, instance: u16) -> Option<UabiEngine> {
        self.as_slice().iter().copied().find(|e| e.class == class && e.instance == instance)
    }
    /// `i915->engine_uabi_class_count[class]` (gt/intel_engine_user.c:233).
    pub fn class_count(&self, class: u16) -> usize {
        self.as_slice().iter().filter(|e| e.class == class).count()
    }
}

/// Everything GETPARAM / I915_QUERY / EXECBUFFER2 need to know about the device.
#[derive(Clone, Copy, Debug)]
pub struct UapiDevice {
    pub desc: PlatformDescriptor,
    /// `pdev->revision` (PCI config 0x08).
    pub pci_revision: u8,
    /// `to_gt(i915)->info.sseu`, decoded by `i915_devinfo_core::sseu::gen9_sseu_info` from the fuse registers.
    pub sseu: SseuInfo,
    /// `to_gt(i915)->clock_frequency`, from `i915_devinfo_core::clock` (Hz).
    pub cs_timestamp_hz: u32,
    /// `i915->edram_size_mb` (zero when the part has no eDRAM; `HAS_EDRAM`, i915_drv.h:557 via `HAS_WT`).
    pub edram_mb: u32,
    /// Bit `1 << uabi_class` for every class whose engines captured a default context state (`engine->default_state`,
    /// `intel_engines_has_context_isolation`, gt/intel_engine_user.c:317-328). Set by GT init once the golden context is recorded.
    pub default_state_classes: u32,
    /// `totalram_pages() << PAGE_SHIFT`: the SYSTEM memory region's size (gem/i915_gem_shmem.c:699).
    pub system_memory_bytes: u64,
    /// What `intel_memory_region_avail` would report to a `perfmon_capable()` caller.
    pub system_available_bytes: u64,
    /// `perfmon_capable()` of the calling session (i915_query.c:511-519).
    pub perfmon_capable: bool,
    /// `i915_ggtt_has_aperture(ggtt)`: the mappable GGTT aperture exists (gt/intel_ggtt_fencing.c:853-854).
    pub ggtt_has_aperture: bool,
    /// `i915.enable_hangcheck` (default true, i915_params.h:63) and `i915.reset` (default 3, i915_params.h:57).
    pub enable_hangcheck: bool,
    pub reset_param: u8,
    /// POLICY, not hardware: the `enum intel_engine_id` bits this service routes THE ASKING SESSION's batches to
    /// (GPU-ENGINES-ADVERTISED-ARE-ROUTED). [`UapiDevice::engines`] is `platform_engine_mask & routed_engine_mask`, so
    /// GETPARAM HAS_BSD/BLT/VEBOX/BSD2, QUERY_ENGINE_INFO and EXECBUFFER2 ring selection all agree and a client is never
    /// told of (nor allowed to submit to) an engine it cannot reach. [`UapiDevice::new`] starts at the whole row (what
    /// Linux advertises); the device MUST narrow it per session with [`UapiDevice::route_engines`] until every engine has
    /// a submit row.
    pub routed_engine_mask: u32,
}

impl UapiDevice {
    /// Build the device facts for a Gen9 row. A non-Gen9 row (the Cherryview negative control) is refused by name, exactly as the
    /// rest of the port does (`PlatformDescriptor::require_gen9`).
    pub fn new(
        desc: PlatformDescriptor,
        pci_revision: u8,
        sseu: SseuInfo,
        cs_timestamp_hz: u32,
        system_memory_bytes: u64,
    ) -> Result<UapiDevice, I915Error> {
        desc.require_gen9()?;
        let mut d = UapiDevice {
            desc,
            pci_revision,
            sseu,
            cs_timestamp_hz,
            edram_mb: 0,
            default_state_classes: 0,
            system_memory_bytes,
            system_available_bytes: system_memory_bytes,
            perfmon_capable: false,
            ggtt_has_aperture: true,
            enable_hangcheck: true, // i915_params.h:63
            reset_param: 3,         // i915_params.h:57
            routed_engine_mask: desc.info.engine_mask,
        };
        d.default_state_classes = d.all_classes_mask();
        Ok(d)
    }

    /// Narrow the session's routed engines to `mask` (bits of `enum intel_engine_id`). It can only remove: a bit the
    /// platform row lacks stays absent, so routing never invents an engine.
    pub fn route_engines(&mut self, mask: u32) {
        self.routed_engine_mask = mask & self.desc.info.engine_mask;
    }

    pub const fn gen(&self) -> u8 {
        self.desc.info.graphics_ver
    }

    /// The bit mask `1 << class` of every class this device has an engine of.
    fn all_classes_mask(&self) -> u32 {
        let mut mask = 0u32;
        for e in self.engines().as_slice() {
            mask |= 1 << e.class;
        }
        mask
    }

    /// The user-visible engines, derived from `PlatformDescriptor::info.engine_mask` (a bit per `enum intel_engine_id`).
    pub fn engines(&self) -> UabiEngines {
        let mask = self.desc.info.engine_mask & self.routed_engine_mask;
        let mut out = UabiEngines { list: [UabiEngine { class: 0, instance: 0 }; MAX_UABI_ENGINES], len: 0 };
        let mut push = |class: u16, instance: u16| {
            if out.len < MAX_UABI_ENGINES {
                out.list[out.len] = UabiEngine { class, instance };
                out.len += 1;
            }
        };
        if mask & (1 << eng::RCS0) != 0 {
            push(engine_class::RENDER, 0);
        }
        if mask & (1 << eng::BCS0) != 0 {
            push(engine_class::COPY, 0);
        }
        if mask & (1 << eng::VCS0) != 0 {
            push(engine_class::VIDEO, 0);
        }
        if mask & (1 << eng::VCS1) != 0 {
            push(engine_class::VIDEO, 1);
        }
        if mask & (1 << eng::VECS0) != 0 {
            push(engine_class::VIDEO_ENHANCE, 0);
        }
        out
    }

    /// `i915->caps.scheduler` as `set_scheduler_caps` computes it (gt/intel_engine_user.c:96-137) for the execlists backend this
    /// port has: every engine has `sched_engine->schedule` (ENABLED|PRIORITY, intel_execlists_submission.c:3371), always
    /// `SUPPORTS_STATS` (:3468), `HAS_SEMAPHORES` (:3470, no vGPU), and `HAS_PREEMPTION` only when `GRAPHICS_VER > 8` (`can_preempt`,
    /// :3313-3316). No GuC, so no STATIC_PRIORITY_MAP.
    pub fn scheduler_caps(&self) -> u32 {
        let mut caps = m::I915_SCHEDULER_CAP_ENABLED | m::I915_SCHEDULER_CAP_PRIORITY | m::I915_SCHEDULER_CAP_SEMAPHORES
            | m::I915_SCHEDULER_CAP_ENGINE_BUSY_STATS;
        if self.gen() > 8 {
            caps |= m::I915_SCHEDULER_CAP_PREEMPTION;
        }
        caps
    }

    /// `INTEL_PPGTT(i915)` (i915_drv.h:568) as the `I915_GEM_PPGTT_*` value.
    pub fn ppgtt_type(&self) -> u32 {
        match self.desc.info.ppgtt_type {
            PpgttType::None => 0,
            PpgttType::Aliasing => 1,
            PpgttType::Full => 2,
        }
    }

    /// `i915_gem_context_has_full_ppgtt` for a context on the default VM of this device.
    pub fn has_full_ppgtt(&self) -> bool {
        self.desc.info.ppgtt_type == PpgttType::Full
    }
}
