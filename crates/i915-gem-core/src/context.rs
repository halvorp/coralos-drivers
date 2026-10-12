// SPDX-License-Identifier: GPL-2.0-only
//! GEM contexts (`struct i915_gem_context`) for Gen9 execlists: the engines map, the address space, the params Mesa uses
//! (BANNABLE, RECOVERABLE, PRIORITY, VM, ENGINES, PERSISTENCE, NO_ERROR_CAPTURE, GTT_SIZE), the pre-creation "proto" parameter
//! set, and the per-engine hardware context ids the executor attaches. Hang and ban accounting is in `hang.rs`.
//! Ported from gem/i915_gem_context.c (v7.0-rc6): `proto_context_create` (gem/i915_gem_context.c:283), `set_proto_ctx_param` (gem/i915_gem_context.c:877-965),
//! `set_proto_ctx_engines` (gem/i915_gem_context.c:733-810), `i915_gem_create_context` (gem/i915_gem_context.c:1603), `ctx_setparam` (gem/i915_gem_context.c:2190), `i915_gem_context_create_ioctl`
//! (gem/i915_gem_context.c:2364), `i915_gem_context_destroy_ioctl` (gem/i915_gem_context.c:2437), `i915_gem_context_getparam_ioctl` (gem/i915_gem_context.c:2527), `..._setparam_ioctl` (gem/i915_gem_context.c:2600).
use crate::error::GemError;
use crate::gem::{Gem, SessionCaps};
use crate::ids::{SessionId, VmId};
use crate::gem::place;
use crate::limits::*;
use crate::table;
use crate::seam::{Effect, EffectSink};
use i915_memory_core::PpgttInit;
use i915_uapi_core::consts::{ctx_param as cp, misc as m};
use i915_uapi_core::ContextInfo;

/// `I915_CONTEXT_MAX_USER_PRIORITY` / `MIN` / `DEFAULT` (i915_drm.h:2009-2011).
pub const CONTEXT_MAX_USER_PRIORITY: i64 = 1023; // i915_drm.h:2009
pub const CONTEXT_DEFAULT_PRIORITY: i64 = 0; // i915_drm.h:2010
pub const CONTEXT_MIN_USER_PRIORITY: i64 = -1023; // i915_drm.h:2011
/// `I915_ENGINE_CLASS_INVALID` / `_INVALID_NONE` (i915_drm.h:222, i915_drm.h:241) as the u16 the wire carries.
pub const ENGINE_CLASS_INVALID: u16 = 0xffff; // i915_drm.h:222
pub const ENGINE_INSTANCE_INVALID_NONE: u16 = 0xffff; // i915_drm.h:241

/// One slot of a context's engine map (`struct i915_gem_proto_engine`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserEngine {
    /// `INVALID / INVALID_NONE`: the slot exists but runs nothing (gem/i915_gem_context.c:777-779).
    Invalid,
    Physical { class: u16, instance: u16 },
}

/// One `drm_i915_gem_context_param` (CONTEXT_CREATE_EXT_SETPARAM entry, or a SETPARAM ioctl), with the engine array of
/// `PARAM_ENGINES` already flattened out of the user pointer.
#[derive(Clone, Copy, Debug)]
pub struct ContextParam<'a> {
    pub param: u64,
    pub size: u32,
    pub value: u64,
    /// `PARAM_ENGINES`: `(engine_class, engine_instance)` per slot. Empty otherwise.
    pub engines: &'a [(u16, u16)],
}

impl<'a> ContextParam<'a> {
    pub const fn new(param: u64, value: u64) -> ContextParam<'a> {
        ContextParam { param, size: 0, value, engines: &[] }
    }
}

/// The reply of GETPARAM: `args->size` and `args->value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamValue {
    pub size: u32,
    pub value: u64,
}

/// `struct i915_gem_proto_context` (gem/i915_gem_context.c:283-309 defaults).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProtoContext {
    pub vm_id: Option<u32>,
    pub bannable: bool,
    pub recoverable: bool,
    pub persistence: bool,
    pub no_error_capture: bool,
    pub priority: i32,
    pub engines: Option<([UserEngine; MAX_USER_ENGINES], u8)>,
}

impl Default for ProtoContext {
    fn default() -> Self {
        // BANNABLE | RECOVERABLE, PERSISTENCE iff hangcheck (set by the caller), priority NORMAL (= 0), no engine map (gem/i915_gem_context.c:293-298).
        ProtoContext { vm_id: None, bannable: true, recoverable: true, persistence: true, no_error_capture: false, priority: 0, engines: None }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Context {
    pub session: u32,
    pub vm: u32,
    pub closed: bool,
    pub banned: bool,
    pub bannable: bool,
    pub recoverable: bool,
    pub persistence: bool,
    pub no_error_capture: bool,
    pub priority: i32,
    pub engines: Option<([UserEngine; MAX_USER_ENGINES], u8)>,
    /// `atomic_read(&ctx->guilty_count)` / `active_count` (gem/i915_gem_context_types.h; read by reset_stats, gem/i915_gem_context.c:2660-2661).
    pub guilty: u32,
    pub active: u32,
    /// `ctx->hang_timestamp[2]`, in the caller's millisecond clock (None = never).
    pub hang_ts: [Option<u64>; HANG_HISTORY],
    /// The executor's `i915_submit_core::Engine` context id per physical engine (`intel_context`), once created and pinned.
    pub engine_ctx: [Option<u32>; MAX_ENGINES],
}

/// `sizeof(struct drm_i915_gem_context_param_sseu)` (i915_drm.h): the SSEU param is a 32-byte struct.
pub const SSEU_PARAM_BYTES: u32 = 32;

/// `validate_priority` (gem/i915_gem_context.c:173-193).
fn validate_priority(sched_caps: u32, caps: SessionCaps, size: u32, value: u64) -> Result<i32, GemError> {
    let priority = value as i64;
    if size != 0 {
        return Err(GemError::ParamSize { param: cp::I915_CONTEXT_PARAM_PRIORITY, size }); // gem/i915_gem_context.c:178-179
    }
    if sched_caps & m::I915_SCHEDULER_CAP_PRIORITY == 0 {
        return Err(GemError::ParamUnsupported { param: cp::I915_CONTEXT_PARAM_PRIORITY }); // gem/i915_gem_context.c:181-182, -ENODEV
    }
    if !(CONTEXT_MIN_USER_PRIORITY..=CONTEXT_MAX_USER_PRIORITY).contains(&priority) {
        return Err(GemError::PriorityRange { priority }); // gem/i915_gem_context.c:184-186
    }
    if priority > CONTEXT_DEFAULT_PRIORITY && !caps.sys_nice {
        return Err(GemError::ParamPermission { param: cp::I915_CONTEXT_PARAM_PRIORITY }); // gem/i915_gem_context.c:188-190
    }
    Ok(priority as i32)
}

impl Gem {
    /// `set_proto_ctx_engines` (gem/i915_gem_context.c:733-810) for the engines of `PARAM_ENGINES`.
    fn parse_engines(&self, engines: &[(u16, u16)]) -> Result<([UserEngine; MAX_USER_ENGINES], u8), GemError> {
        if engines.len() > MAX_USER_ENGINES {
            return Err(GemError::EnginesTooMany { count: engines.len() }); // gem/i915_gem_context.c:758-760
        }
        let mut out = [UserEngine::Invalid; MAX_USER_ENGINES];
        let known = self.dev.engines();
        for (n, &(class, instance)) in engines.iter().enumerate() {
            if class == ENGINE_CLASS_INVALID && instance == ENGINE_INSTANCE_INVALID_NONE {
                continue; // gem/i915_gem_context.c:777-779
            }
            if known.lookup(class, instance).is_none() {
                return Err(GemError::EngineMissing { class, instance }); // gem/i915_gem_context.c:781-789
            }
            out[n] = UserEngine::Physical { class, instance };
        }
        Ok((out, engines.len() as u8))
    }

    /// `set_proto_ctx_param` (gem/i915_gem_context.c:877-965).
    fn set_proto_param(&self, slot: usize, pc: &mut ProtoContext, p: &ContextParam<'_>) -> Result<(), GemError> {
        let caps = self.session(slot).caps;
        let size0 = |p: &ContextParam<'_>| if p.size != 0 { Err(GemError::ParamSize { param: p.param, size: p.size }) } else { Ok(()) };
        match p.param {
            cp::I915_CONTEXT_PARAM_NO_ERROR_CAPTURE => {
                size0(p)?; // gem/i915_gem_context.c:886-887
                pc.no_error_capture = p.value != 0;
            }
            cp::I915_CONTEXT_PARAM_BANNABLE => {
                size0(p)?; // gem/i915_gem_context.c:895-896
                if !caps.sys_admin && p.value == 0 {
                    return Err(GemError::ParamPermission { param: p.param }); // gem/i915_gem_context.c:897-898
                }
                pc.bannable = p.value != 0; // protected content is never in a Gen9 proto context, so gem/i915_gem_context.c:901-902 cannot fire
            }
            cp::I915_CONTEXT_PARAM_LOW_LATENCY => {
                // intel_uc_uses_guc_submission() is false on this port (execlists only): -EINVAL, gem/i915_gem_context.c:907-912
                return Err(GemError::ParamUnknown { param: p.param });
            }
            cp::I915_CONTEXT_PARAM_RECOVERABLE => {
                size0(p)?; // gem/i915_gem_context.c:915-916
                pc.recoverable = p.value != 0;
            }
            cp::I915_CONTEXT_PARAM_PRIORITY => {
                pc.priority = validate_priority(self.dev.scheduler_caps(), caps, p.size, p.value)?; // gem/i915_gem_context.c:926-928
            }
            cp::I915_CONTEXT_PARAM_SSEU => {
                // set_proto_ctx_sseu: a short struct is -EINVAL (gem/i915_gem_context.c:818-819), then `GRAPHICS_VER(i915) != 11` -> -ENODEV (:825-826)
                if p.size < SSEU_PARAM_BYTES {
                    return Err(GemError::ParamSize { param: p.param, size: p.size });
                }
                return Err(GemError::ParamUnsupported { param: p.param });
            }
            cp::I915_CONTEXT_PARAM_VM => {
                size0(p)?; // set_proto_ctx_vm gem/i915_gem_context.c:373-374
                if p.value >> 32 != 0 {
                    return Err(GemError::VmMissing { id: p.value as u32 }); // gem/i915_gem_context.c:379-380
                }
                self.vm_lookup(slot, p.value as u32)?;
                pc.vm_id = Some(p.value as u32);
            }
            cp::I915_CONTEXT_PARAM_ENGINES => {
                if pc.engines.is_some() {
                    return Err(GemError::EnginesTwice); // gem/i915_gem_context.c:745-748
                }
                pc.engines = Some(self.parse_engines(p.engines)?);
            }
            cp::I915_CONTEXT_PARAM_PERSISTENCE => {
                size0(p)?; // gem/i915_gem_context.c:944-945
                pc.persistence = self.set_persistence_checked(p.value != 0)?;
            }
            cp::I915_CONTEXT_PARAM_PROTECTED_CONTENT => {
                // proto_context_set_protected (gem/i915_gem_context.c:253-): clearing is a no-op (:259-260); setting needs PXP, which Gen9
                // lacks: -ENODEV (gem/i915_gem_context.c:261-262)
                if p.value != 0 {
                    return Err(GemError::ParamUnsupported { param: p.param });
                }
            }
            // NO_ZEROMAP, BAN_PERIOD, RINGSIZE, CONTEXT_IMAGE and anything unknown: gem/i915_gem_context.c:956-962
            _ => return Err(GemError::ParamUnknown { param: p.param }),
        }
        Ok(())
    }

    /// `proto_context_set_persistence` / `__context_set_persistence` (gem/i915_gem_context.c:212-251, gem/i915_gem_context.c:1560-1600).
    fn set_persistence_checked(&self, persist: bool) -> Result<bool, GemError> {
        if persist {
            if !self.dev.enable_hangcheck {
                return Err(GemError::ParamUnknown { param: cp::I915_CONTEXT_PARAM_PERSISTENCE }); // gem/i915_gem_context.c:222-223, -EINVAL
            }
        } else if self.dev.scheduler_caps() & m::I915_SCHEDULER_CAP_PREEMPTION == 0 || !self.dev.desc.info.has_reset_engine {
            // preempt-to-idle is how a context is cancelled (gem/i915_gem_context.c:227-229), and without per-engine reset the cancel could not be
            // contained (gem/i915_gem_context.c:231-245): both -ENODEV
            return Err(GemError::ParamUnsupported { param: cp::I915_CONTEXT_PARAM_PERSISTENCE }); // gem/i915_gem_context.c:228-229, -ENODEV
        }
        Ok(persist)
    }

    /// `i915_gem_create_context` (gem/i915_gem_context.c:1603-) from a proto context. `id` forces the user id (the default context
    /// is 0); otherwise the lowest free id from 1 (`XA_FLAGS_ALLOC1`, gem/i915_gem_context.c:1750-1757).
    pub(crate) fn create_context_internal(
        &mut self,
        slot: usize,
        id: Option<u32>,
        pc: ProtoContext,
        init: Option<PpgttInit>,
        sink: &mut dyn EffectSink,
    ) -> Result<u32, GemError> {
        // The user-visible id: forced (the default context is 0) or the lowest free one from 1. The session's id table grows with use.
        let uid = match id {
            Some(i) => i as usize,
            None => self.session(slot).ctx.iter().enumerate().skip(1).find(|(_, c)| c.is_none()).map_or(self.session(slot).ctx.len().max(1), |(i, _)| i),
        };
        while self.session(slot).ctx.len() <= uid {
            let s = self.sessions[slot].as_mut().expect("session slot validated");
            table::push(&mut s.ctx, None, &mut self.quota, "context-ids")?;
        }
        // The address space: the one named by PARAM_VM, else a new one (`i915_ppgtt_create`, gem/i915_gem_context.c:1628-1640).
        let vm = match pc.vm_id {
            Some(v) => self.vm_lookup(slot, v)?,
            None => {
                let init = init.ok_or(GemError::VmPagesMissing)?;
                self.create_vm_slot(init, sink)?
            }
        };
        let ctx = Context {
            session: slot as u32,
            vm,
            closed: false,
            banned: false,
            bannable: pc.bannable,
            recoverable: pc.recoverable,
            persistence: pc.persistence,
            no_error_capture: pc.no_error_capture,
            priority: pc.priority,
            engines: pc.engines,
            guilty: 0,
            active: 0,
            hang_ts: [None; HANG_HISTORY],
            engine_ctx: [None; MAX_ENGINES],
        };
        // Nothing but the (possibly new) address space exists yet: if the table cannot grow, release the space again.
        let cslot = match place(&mut self.contexts, ctx, &mut self.quota, "contexts") {
            Ok(c) => c,
            Err(e) => {
                self.sweep_vms(sink);
                return Err(e);
            }
        };
        self.vm_ref(vm as usize);
        self.session_mut(slot).ctx[uid] = Some(cslot as u32);
        Ok(uid as u32)
    }

    /// `i915_gem_context_create_ioctl` (gem/i915_gem_context.c:2364-2435). `params` are the `CONTEXT_CREATE_EXT_SETPARAM`
    /// extensions in chain order; `vm_init` is needed only when no `PARAM_VM` names an existing space.
    pub fn context_create(&mut self, session: SessionId, flags: u32, params: &[ContextParam<'_>], vm_init: Option<PpgttInit>, sink: &mut dyn EffectSink) -> Result<u32, GemError> {
        let unknown = !(m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS | m::I915_CONTEXT_CREATE_FLAGS_SINGLE_TIMELINE);
        if flags & unknown != 0 {
            return Err(GemError::ContextCreateFlags { flags }); // gem/i915_gem_context.c:2376-2377
        }
        let slot = self.session_slot(session)?;
        let score = self.session(slot).ban_score;
        if score >= CLIENT_SCORE_BANNED {
            return Err(GemError::ClientBanned { score }); // client_is_banned, gem/i915_gem_context.c:2289-2292 and gem/i915_gem_context.c:2384-2389
        }
        let mut pc = ProtoContext { persistence: self.dev.enable_hangcheck, ..ProtoContext::default() }; // gem/i915_gem_context.c:296-297
        if flags & m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS != 0 {
            for p in params {
                self.set_proto_param(slot, &mut pc, p)?; // create_setparam, gem/i915_gem_context.c:2265-2277
            }
        } else if !params.is_empty() {
            return Err(GemError::ContextCreateFlags { flags }); // extensions without USE_EXTENSIONS are never read
        }
        self.create_context_internal(slot, None, pc, vm_init, sink)
    }

    /// `i915_gem_context_destroy_ioctl` (gem/i915_gem_context.c:2437-2470): context 0 cannot be destroyed.
    pub fn context_destroy(&mut self, session: SessionId, ctx_id: u32, pad: u32, sink: &mut dyn EffectSink) -> Result<(), GemError> {
        if pad != 0 {
            return Err(GemError::ParamSize { param: 0, size: pad }); // gem/i915_gem_context.c:2445-2446, -EINVAL
        }
        if ctx_id == 0 {
            return Err(GemError::ContextMissing { id: 0 }); // gem/i915_gem_context.c:2448-2449, -ENOENT
        }
        let slot = self.session_slot(session)?;
        let c = self.session(slot).ctx.get(ctx_id as usize).copied().flatten().ok_or(GemError::ContextMissing { id: ctx_id })?; // gem/i915_gem_context.c:2459-2460
        self.session_mut(slot).ctx[ctx_id as usize] = None;
        self.close_context_slot(c as usize); // context_close, gem/i915_gem_context.c:2466-2467
        self.sweep_contexts(sink);
        Ok(())
    }

    /// `context_close` (gem/i915_gem_context.c:1515-1558): mark closed; the context is freed when its requests retire.
    pub(crate) fn close_context_slot(&mut self, c: usize) {
        if let Some(ctx) = self.contexts[c].as_mut() {
            ctx.closed = true;
        }
    }

    /// Release closed contexts with no request in flight (`i915_gem_context_release_work`, gem/i915_gem_context.c:1280).
    pub(crate) fn sweep_contexts(&mut self, sink: &mut dyn EffectSink) {
        for c in 0..self.contexts.len() {
            let Some(ctx) = self.contexts[c] else { continue };
            if ctx.closed && !self.active.refs_context(c as u32) {
                self.contexts[c] = None;
                self.vm_unref(ctx.vm as usize);
                for (e, ectx) in ctx.engine_ctx.iter().enumerate() {
                    if let Some(ectx) = ectx {
                        self.active.forget(e as u8, *ectx); // the hardware context is gone: so is its seqno observation
                    }
                }
                sink.effect(Effect::ContextFreed { ctx_slot: c as u32 });
            }
        }
        self.sweep_vms(sink);
    }

    fn context_lookup(&self, slot: usize, id: u32) -> Result<usize, GemError> {
        let c = self.session(slot).ctx.get(id as usize).copied().flatten().ok_or(GemError::ContextMissing { id })?;
        match &self.contexts[c as usize] {
            Some(ctx) if !ctx.closed => Ok(c as usize),
            _ => Err(GemError::ContextMissing { id }),
        }
    }

    /// `i915_gem_context_getparam_ioctl` (gem/i915_gem_context.c:2527-2598).
    pub fn context_getparam(&mut self, session: SessionId, ctx_id: u32, param: u64, p_size: u32) -> Result<ParamValue, GemError> {
        let slot = self.session_slot(session)?;
        let c = self.context_lookup(slot, ctx_id)?;
        let ctx = self.contexts[c].expect("looked up");
        let v = |value: u64| Ok(ParamValue { size: 0, value });
        match param {
            cp::I915_CONTEXT_PARAM_GTT_SIZE => v(self.vm_total(VmId(ctx.vm)).unwrap_or(0)), // gem/i915_gem_context.c:2540-2545
            cp::I915_CONTEXT_PARAM_NO_ERROR_CAPTURE => v(ctx.no_error_capture as u64),
            cp::I915_CONTEXT_PARAM_BANNABLE => v(ctx.bannable as u64),
            cp::I915_CONTEXT_PARAM_RECOVERABLE => v(ctx.recoverable as u64),
            cp::I915_CONTEXT_PARAM_PRIORITY => v(ctx.priority as i64 as u64),
            cp::I915_CONTEXT_PARAM_PERSISTENCE => v(ctx.persistence as u64),
            cp::I915_CONTEXT_PARAM_PROTECTED_CONTENT => v(0), // get_protected: no PXP (gem/i915_gem_context.c:2098-2105)
            // get_sseu (gem/i915_gem_context.c:2472-): size 0 asks for the struct size (:2480-2481); a real query reads a user struct
            // and the engine's SSEU, which this port does not carry for Gen9 (only Gen11 reconfigures SSEU).
            cp::I915_CONTEXT_PARAM_SSEU if p_size == 0 => Ok(ParamValue { size: SSEU_PARAM_BYTES, value: 0 }),
            cp::I915_CONTEXT_PARAM_SSEU => Err(GemError::ParamUnsupported { param }),
            cp::I915_CONTEXT_PARAM_VM => {
                // get_ppgtt (gem/i915_gem_context.c:1870-1903): publish the context's VM under a new user-visible id.
                let id = match self.session(slot).vm_ids.iter().position(|v| v.is_none()) {
                    Some(i) => i,
                    None => {
                        let s = self.sessions[slot].as_mut().expect("session slot validated");
                        table::push(&mut s.vm_ids, None, &mut self.quota, "vm-ids")?
                    }
                };
                self.vm_ref(ctx.vm as usize);
                self.session_mut(slot).vm_ids[id] = Some(ctx.vm);
                v(id as u64 + 1)
            }
            // NO_ZEROMAP, BAN_PERIOD, ENGINES, RINGSIZE, CONTEXT_IMAGE, unknown: gem/i915_gem_context.c:2586-2590
            _ => Err(GemError::ParamUnknown { param }),
        }
    }

    /// `i915_gem_context_setparam_ioctl` -> `ctx_setparam` (gem/i915_gem_context.c:2190-2268, gem/i915_gem_context.c:2600-2632). VM and ENGINES can only
    /// be set at creation (gem/i915_gem_context.c:2246-2255).
    pub fn context_setparam(&mut self, session: SessionId, ctx_id: u32, p: &ContextParam<'_>) -> Result<(), GemError> {
        let slot = self.session_slot(session)?;
        let c = self.context_lookup(slot, ctx_id)?;
        let caps = self.session(slot).caps;
        let size0 = |p: &ContextParam<'_>| if p.size != 0 { Err(GemError::ParamSize { param: p.param, size: p.size }) } else { Ok(()) };
        match p.param {
            cp::I915_CONTEXT_PARAM_NO_ERROR_CAPTURE => {
                size0(p)?;
                self.contexts[c].as_mut().expect("looked up").no_error_capture = p.value != 0;
            }
            cp::I915_CONTEXT_PARAM_BANNABLE => {
                size0(p)?; // gem/i915_gem_context.c:2207-2208
                if !caps.sys_admin && p.value == 0 {
                    return Err(GemError::ParamPermission { param: p.param }); // gem/i915_gem_context.c:2209-2210
                }
                self.contexts[c].as_mut().expect("looked up").bannable = p.value != 0;
            }
            cp::I915_CONTEXT_PARAM_RECOVERABLE => {
                size0(p)?; // gem/i915_gem_context.c:2220-2221
                self.contexts[c].as_mut().expect("looked up").recoverable = p.value != 0;
            }
            cp::I915_CONTEXT_PARAM_PRIORITY => {
                let pr = validate_priority(self.dev.scheduler_caps(), caps, p.size, p.value)?; // set_priority, gem/i915_gem_context.c:2070-2090
                self.contexts[c].as_mut().expect("looked up").priority = pr;
            }
            cp::I915_CONTEXT_PARAM_PERSISTENCE => {
                size0(p)?; // set_persistence, gem/i915_gem_context.c:2060-2068
                let on = self.set_persistence_checked(p.value != 0)?;
                self.contexts[c].as_mut().expect("looked up").persistence = on;
            }
            cp::I915_CONTEXT_PARAM_SSEU => {
                // set_sseu: short struct -EINVAL (gem/i915_gem_context.c:2015-2016), not Gen11 -> -ENODEV (:2018-2019)
                if p.size < SSEU_PARAM_BYTES {
                    return Err(GemError::ParamSize { param: p.param, size: p.size });
                }
                return Err(GemError::ParamUnsupported { param: p.param });
            }
            // CONTEXT_IMAGE (needs CONFIG_DRM_I915_REPLAY_GPU_HANGS_API, gem/i915_gem_context.c:2117-2118), PROTECTED_CONTENT, NO_ZEROMAP,
            // BAN_PERIOD, RINGSIZE, VM, ENGINES and anything unknown: -EINVAL (gem/i915_gem_context.c:2242-2255)
            _ => return Err(GemError::ParamUnknown { param: p.param }),
        }
        Ok(())
    }

    /// The executor created and pinned an `i915_submit_core` context for `(ctx, engine)`; record it (`intel_context_pin`,
    /// gem/i915_gem_execbuffer.c:2565-2567). `engine_ctx` is the id `Engine::create_context` returned.
    pub fn attach_engine_context(&mut self, session: SessionId, ctx_id: u32, class: u16, instance: u16, engine_ctx: u32) -> Result<(), GemError> {
        let slot = self.session_slot(session)?;
        let c = self.context_lookup(slot, ctx_id)?;
        let idx = self.engine_index(class, instance).ok_or(GemError::EngineMissing { class, instance })?;
        self.contexts[c].as_mut().expect("looked up").engine_ctx[idx] = Some(engine_ctx);
        Ok(())
    }

    /// Index of `(class, instance)` in the device's user-visible engine order.
    pub fn engine_index(&self, class: u16, instance: u16) -> Option<usize> {
        self.dev.engines().as_slice().iter().position(|e| e.class == class && e.instance == instance)
    }

    /// The address space a context uses (`ctx->vm`).
    pub fn context_vm(&self, session: SessionId, ctx_id: u32) -> Result<VmId, GemError> {
        let slot = self.session_slot(session)?;
        let c = self.context_lookup(slot, ctx_id)?;
        Ok(VmId(self.contexts[c].expect("looked up").vm))
    }

    /// What `ExecEnv::context` reports (`i915_gem_context_user_engines`).
    pub(crate) fn context_info(&self, slot: usize, id: u32, ring_slot: u32) -> Option<ContextInfo> {
        let c = self.context_lookup(slot, id).ok()?;
        let ctx = self.contexts[c].as_ref()?;
        Some(match &ctx.engines {
            Some((e, n)) => ContextInfo {
                user_engines: Some(*n as u32),
                user_engine_present: (ring_slot as usize) < *n as usize && e[ring_slot as usize] != UserEngine::Invalid,
            },
            None => ContextInfo { user_engines: None, user_engine_present: true },
        })
    }

    pub(crate) fn context_slot(&self, slot: usize, id: u32) -> Result<usize, GemError> {
        self.context_lookup(slot, id)
    }
}
