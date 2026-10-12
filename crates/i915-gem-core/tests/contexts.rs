// SPDX-License-Identifier: GPL-2.0-only
//! Contexts and address spaces: the default context, create/destroy, the params Mesa uses (BANNABLE, RECOVERABLE, PRIORITY, VM,
//! ENGINES, PERSISTENCE, GTT_SIZE), permissions, the engine map driving engine selection, and VM lifetime.
mod common;
use common::*;
use i915_gem_core::limits::*;
use i915_gem_core::*;
use i915_uapi_core::consts::ctx_param as cp;
use i915_uapi_core::consts::exec as ef;
use i915_uapi_core::consts::misc as m;

type R = Rig<16, 16>;

fn p(param: u64, value: u64) -> ContextParam<'static> {
    ContextParam::new(param, value)
}

#[test]
fn context_zero_exists_with_linux_defaults() {
    let mut r = R::new();
    let g = |r: &mut R, param| r.gem.context_getparam(r.s, 0, param, 0).unwrap().value;
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_BANNABLE), 1); // proto_context_create gem/i915_gem_context.c:293-295
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_RECOVERABLE), 1);
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_PERSISTENCE), 1); // enable_hangcheck is on (i915_params.h:63)
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_PRIORITY), 0);
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_NO_ERROR_CAPTURE), 0);
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_GTT_SIZE), 1 << 48, "vm->total of a full 48-bit PPGTT (gem/i915_gem_context.c:2540-2545)");
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_PROTECTED_CONTENT), 0);
    // the reply's size field is always 0 for these (args->size = 0)
    assert_eq!(r.gem.context_getparam(r.s, 0, cp::I915_CONTEXT_PARAM_BANNABLE, 0).unwrap().size, 0);
}

#[test]
fn context_zero_cannot_be_destroyed_and_new_contexts_take_the_lowest_free_id() {
    let mut r = R::new();
    assert_eq!(r.gem.context_destroy(r.s, 0, 0, &mut r.sink), Err(GemError::ContextMissing { id: 0 })); // gem/i915_gem_context.c:2448-2449
    assert_eq!(r.gem.context_destroy(r.s, 1, 7, &mut r.sink).unwrap_err().errno(), 22, "pad != 0: -EINVAL (gem/i915_gem_context.c:2445-2446)");
    let a = r.gem.context_create(r.s, 0, &[], Some(r.pool.init()), &mut r.sink).unwrap();
    let b = r.gem.context_create(r.s, 0, &[], Some(r.pool.init()), &mut r.sink).unwrap();
    assert_eq!((a, b), (1, 2), "XA_FLAGS_ALLOC1: ids start at 1 (gem/i915_gem_context.c:1750-1757)");
    r.gem.context_destroy(r.s, a, 0, &mut r.sink).unwrap();
    let c = r.gem.context_create(r.s, 0, &[], Some(r.pool.init()), &mut r.sink).unwrap();
    assert_eq!(c, 1, "the lowest free id is reused");
    assert_eq!(r.gem.context_destroy(r.s, 9, 0, &mut r.sink), Err(GemError::ContextMissing { id: 9 }));
    assert_eq!(r.gem.context_getparam(r.s, 9, cp::I915_CONTEXT_PARAM_BANNABLE, 0), Err(GemError::ContextMissing { id: 9 }));
}

#[test]
fn a_destroyed_context_is_gone_for_lookups_but_lives_until_its_requests_retire() {
    let mut r = R::new();
    let ctx = r.gem.context_create(r.s, 0, &[], Some(r.pool.init()), &mut r.sink).unwrap();
    let ectx = r.eng.attach(&mut r.gem, r.s, ctx);
    let (a, b) = (r.create(4096), r.create(4096));
    let mut e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    e.ctx = ctx as u64;
    let done = r.exec(&e, 0).unwrap();
    assert_eq!(done.engine_ctx, ectx);
    let vms_before = r.gem.vm_count();
    r.sink.clear();
    r.gem.context_destroy(r.s, ctx, 0, &mut r.sink).unwrap();
    assert_eq!(r.gem.context_getparam(r.s, ctx, cp::I915_CONTEXT_PARAM_BANNABLE, 0), Err(GemError::ContextMissing { id: ctx }));
    assert!(r.exec(&e, 1).is_err(), "no new submissions on a destroyed context");
    assert!(!r.sink.effects().iter().any(|e| matches!(e, Effect::ContextFreed { .. })), "its request is still in flight");
    assert_eq!(r.gem.vm_count(), vms_before);
    r.gem.observe_seqno(done.engine, done.engine_ctx, done.seqno).unwrap();
    r.gem.retire(&mut r.sink);
    let eff = r.sink.effects();
    assert!(eff.iter().any(|e| matches!(e, Effect::ContextFreed { .. })), "{eff:?}");
    assert!(eff.iter().any(|e| matches!(e, Effect::VmFreed { .. })), "its private VM goes with it");
    assert_eq!(r.gem.vm_count(), vms_before - 1);
}

#[test]
fn params_at_create_time_follow_set_proto_ctx_param() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let nice = r.gem.open_session(SessionCaps { sys_admin: true, sys_nice: true }, r.pool.init(), &mut sink).unwrap();
    let ctx = r
        .gem
        .context_create(
            nice,
            m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS,
            &[p(cp::I915_CONTEXT_PARAM_PRIORITY, 10), p(cp::I915_CONTEXT_PARAM_BANNABLE, 0), p(cp::I915_CONTEXT_PARAM_RECOVERABLE, 0), p(cp::I915_CONTEXT_PARAM_NO_ERROR_CAPTURE, 1), p(cp::I915_CONTEXT_PARAM_PERSISTENCE, 0)],
            Some(r.pool.init()),
            &mut sink,
        )
        .unwrap();
    let g = |r: &mut R, param| r.gem.context_getparam(nice, ctx, param, 0).unwrap().value;
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_PRIORITY), 10);
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_BANNABLE), 0);
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_RECOVERABLE), 0);
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_NO_ERROR_CAPTURE), 1);
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_PERSISTENCE), 0);
    // negative priority round-trips as the signed value
    r.gem.context_setparam(nice, ctx, &p(cp::I915_CONTEXT_PARAM_PRIORITY, (-1023i64) as u64)).unwrap();
    assert_eq!(g(&mut r, cp::I915_CONTEXT_PARAM_PRIORITY) as i64, -1023);
}

#[test]
fn priority_and_bannable_permissions_are_linuxs() {
    let mut r = R::new(); // an unprivileged session
    let set = |r: &mut R, param, v: u64| r.gem.context_setparam(r.s, 0, &p(param, v));
    assert_eq!(set(&mut r, cp::I915_CONTEXT_PARAM_PRIORITY, 1024), Err(GemError::PriorityRange { priority: 1024 })); // gem/i915_gem_context.c:184-186
    assert_eq!(set(&mut r, cp::I915_CONTEXT_PARAM_PRIORITY, (-1024i64) as u64), Err(GemError::PriorityRange { priority: -1024 }));
    assert_eq!(set(&mut r, cp::I915_CONTEXT_PARAM_PRIORITY, 1), Err(GemError::ParamPermission { param: cp::I915_CONTEXT_PARAM_PRIORITY })); // CAP_SYS_NICE, gem/i915_gem_context.c:188-190
    assert_eq!(GemError::ParamPermission { param: 0 }.errno(), 1);
    assert!(set(&mut r, cp::I915_CONTEXT_PARAM_PRIORITY, 0).is_ok());
    assert!(set(&mut r, cp::I915_CONTEXT_PARAM_PRIORITY, (-1023i64) as u64).is_ok(), "lowering needs no privilege");
    assert_eq!(set(&mut r, cp::I915_CONTEXT_PARAM_BANNABLE, 0), Err(GemError::ParamPermission { param: cp::I915_CONTEXT_PARAM_BANNABLE })); // gem/i915_gem_context.c:2209-2210
    assert!(set(&mut r, cp::I915_CONTEXT_PARAM_BANNABLE, 1).is_ok());
    // a non-zero size is -EINVAL for every scalar param
    let mut bad = p(cp::I915_CONTEXT_PARAM_RECOVERABLE, 1);
    bad.size = 8;
    assert_eq!(r.gem.context_setparam(r.s, 0, &bad), Err(GemError::ParamSize { param: cp::I915_CONTEXT_PARAM_RECOVERABLE, size: 8 }));
    // VM and ENGINES exist only at creation (gem/i915_gem_context.c:2246-2255); the write-only/obsolete params are unknown
    for param in [cp::I915_CONTEXT_PARAM_VM, cp::I915_CONTEXT_PARAM_ENGINES, cp::I915_CONTEXT_PARAM_NO_ZEROMAP, cp::I915_CONTEXT_PARAM_BAN_PERIOD, cp::I915_CONTEXT_PARAM_RINGSIZE, 0x99] {
        assert_eq!(set(&mut r, param, 1), Err(GemError::ParamUnknown { param }), "param {param:#x}");
        assert_eq!(set(&mut r, param, 1).unwrap_err().errno(), 22);
    }
    // SSEU is Gen11-only: a short struct is -EINVAL (gem/i915_gem_context.c:2015-2016), a full one -ENODEV (gem/i915_gem_context.c:2018-2019)
    assert_eq!(set(&mut r, cp::I915_CONTEXT_PARAM_SSEU, 1), Err(GemError::ParamSize { param: cp::I915_CONTEXT_PARAM_SSEU, size: 0 }));
    let mut sseu = p(cp::I915_CONTEXT_PARAM_SSEU, 0);
    sseu.size = 32;
    assert_eq!(r.gem.context_setparam(r.s, 0, &sseu), Err(GemError::ParamUnsupported { param: cp::I915_CONTEXT_PARAM_SSEU }));
    assert_eq!(GemError::ParamUnsupported { param: 0 }.errno(), 19, "-ENODEV");
    // PROTECTED_CONTENT and CONTEXT_IMAGE are in the default -EINVAL group of ctx_setparam (gem/i915_gem_context.c:2242-2255)
    for param in [cp::I915_CONTEXT_PARAM_PROTECTED_CONTENT, cp::I915_CONTEXT_PARAM_CONTEXT_IMAGE] {
        assert_eq!(set(&mut r, param, 1), Err(GemError::ParamUnknown { param }));
    }
    assert_eq!(r.gem.context_getparam(r.s, 0, cp::I915_CONTEXT_PARAM_ENGINES, 0), Err(GemError::ParamUnknown { param: cp::I915_CONTEXT_PARAM_ENGINES }));
    // get_sseu: size 0 asks for the struct size (gem/i915_gem_context.c:2480-2481)
    assert_eq!(r.gem.context_getparam(r.s, 0, cp::I915_CONTEXT_PARAM_SSEU, 0), Ok(ParamValue { size: 32, value: 0 }));
    assert_eq!(r.gem.context_getparam(r.s, 0, cp::I915_CONTEXT_PARAM_SSEU, 32), Err(GemError::ParamUnsupported { param: cp::I915_CONTEXT_PARAM_SSEU }));
}

#[test]
fn create_flags_and_unusable_create_params_are_refused() {
    let mut r = R::new();
    let init = r.pool.init();
    assert_eq!(r.gem.context_create(r.s, 1 << 5, &[], Some(init), &mut r.sink), Err(GemError::ContextCreateFlags { flags: 1 << 5 })); // gem/i915_gem_context.c:2376-2377
    // LOW_LATENCY needs GuC submission; this port has none
    let ext = m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    assert_eq!(r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_LOW_LATENCY, 1)], Some(init), &mut r.sink), Err(GemError::ParamUnknown { param: cp::I915_CONTEXT_PARAM_LOW_LATENCY }));
    // PROTECTED_CONTENT: clearing is a no-op, setting needs PXP (-ENODEV), gem/i915_gem_context.c:259-262
    assert!(r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_PROTECTED_CONTENT, 0)], Some(r.pool.init()), &mut r.sink).is_ok());
    assert_eq!(r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_PROTECTED_CONTENT, 1)], Some(init), &mut r.sink), Err(GemError::ParamUnsupported { param: cp::I915_CONTEXT_PARAM_PROTECTED_CONTENT }));
    // a create that fails part-way leaves nothing behind: no context, no VM
    let (ctxs, vms) = (r.gem.context_getparam(r.s, 1, 0, 0).is_err(), r.gem.vm_count());
    assert!(ctxs);
    let _ = r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_PRIORITY, 5000)], Some(r.pool.init()), &mut r.sink);
    assert_eq!(r.gem.vm_count(), vms, "validation happens before the address space is made");
    // a new context with no PARAM_VM and no pages for a new address space
    assert_eq!(r.gem.context_create(r.s, 0, &[], None, &mut r.sink), Err(GemError::VmPagesMissing));
    // params are ignored without USE_EXTENSIONS in Linux (the chain is never read); this port refuses the mismatch by name
    assert!(matches!(r.gem.context_create(r.s, 0, &[p(cp::I915_CONTEXT_PARAM_PRIORITY, 0)], Some(init), &mut r.sink), Err(GemError::ContextCreateFlags { .. })));
}

#[test]
fn vms_are_session_scoped_ids_that_contexts_share() {
    let mut r = R::new();
    let ext = m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    let before = r.gem.vm_count();
    let vm = r.gem.vm_create(r.s, 0, 0, r.pool.init(), &mut r.sink).unwrap();
    assert_eq!(vm, 1, "vm_xa ids start at 1 (gem/i915_gem_context.c:1757)");
    let a = r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_VM, vm as u64)], None, &mut r.sink).unwrap();
    let b = r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_VM, vm as u64)], None, &mut r.sink).unwrap();
    assert_eq!(r.gem.vm_count(), before + 1, "two contexts, one new address space");
    assert_eq!(r.gem.context_vm(r.s, a).unwrap(), r.gem.context_vm(r.s, b).unwrap());
    assert_ne!(r.gem.context_vm(r.s, a).unwrap(), r.gem.context_vm(r.s, 0).unwrap());
    // PARAM_VM of a context publishes its VM under a fresh id
    let id = r.gem.context_getparam(r.s, a, cp::I915_CONTEXT_PARAM_VM, 0).unwrap().value;
    assert_eq!(id, 2);
    // destroying the user ids leaves the VM alive while contexts use it
    r.gem.vm_destroy(r.s, 1, 0, 0, &mut r.sink).unwrap();
    r.gem.vm_destroy(r.s, 2, 0, 0, &mut r.sink).unwrap();
    assert_eq!(r.gem.vm_count(), before + 1);
    r.gem.context_destroy(r.s, a, 0, &mut r.sink).unwrap();
    assert_eq!(r.gem.vm_count(), before + 1);
    r.gem.context_destroy(r.s, b, 0, &mut r.sink).unwrap();
    assert_eq!(r.gem.vm_count(), before, "last holder gone: freed");
    assert!(r.sink.effects().iter().any(|e| matches!(e, Effect::VmFreed { .. })));
    // errors
    assert_eq!(r.gem.vm_destroy(r.s, 1, 0, 0, &mut r.sink), Err(GemError::VmMissing { id: 1 }));
    assert_eq!(r.gem.vm_destroy(r.s, 0, 0, 0, &mut r.sink), Err(GemError::VmMissing { id: 0 }));
    assert_eq!(r.gem.vm_create(r.s, 1, 0, r.pool.init(), &mut r.sink), Err(GemError::VmFlags { flags: 1 }));
    assert_eq!(r.gem.vm_destroy(r.s, 1, 1, 0, &mut r.sink), Err(GemError::VmFlags { flags: 1 }));
    assert_eq!(r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_VM, 77)], None, &mut r.sink), Err(GemError::VmMissing { id: 77 }));
    assert_eq!(r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_VM, 1 << 32)], None, &mut r.sink), Err(GemError::VmMissing { id: 0 }), "upper_32_bits -> -ENOENT (gem/i915_gem_context.c:379-380)");
}

#[test]
fn a_vm_id_of_another_session_names_no_vm() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let b = r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    let vm = r.gem.vm_create(r.s, 0, 0, r.pool.init(), &mut r.sink).unwrap();
    let ext = m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    assert_eq!(r.gem.context_create(b, ext, &[p(cp::I915_CONTEXT_PARAM_VM, vm as u64)], None, &mut sink), Err(GemError::VmMissing { id: vm }));
    assert_eq!(r.gem.vm_destroy(b, vm, 0, 0, &mut sink), Err(GemError::VmMissing { id: vm }));
}

#[test]
fn an_engine_map_selects_engines_by_slot() {
    let mut r = R::new();
    let engines = [(0u16, 0u16), (2, 0), (0xffff, 0xffff)];
    let ext = m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    let mut param = p(cp::I915_CONTEXT_PARAM_ENGINES, 0);
    param.engines = &engines;
    let ctx = r.gem.context_create(r.s, ext, &[param], Some(r.pool.init()), &mut r.sink).unwrap();
    r.eng.attach_on(&mut r.gem, r.s, ctx, 0);
    r.eng.attach_on(&mut r.gem, r.s, ctx, 2);
    let (a, b) = (r.create(4096), r.create(4096));
    let run = |r: &mut R, slot: u64| {
        let mut e = Exec::new(slot, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
        e.ctx = ctx as u64;
        r.exec(&e, 0)
    };
    assert_eq!(run(&mut r, 0).unwrap().engine, r.gem.engine_index(0, 0).unwrap(), "slot 0 = RCS0");
    assert_eq!(run(&mut r, 1).unwrap().engine, r.gem.engine_index(2, 0).unwrap(), "slot 1 = VCS0");
    // the INVALID placeholder slot and a slot beyond the map have no engine (i915_gem_context.h:213-216)
    assert!(matches!(run(&mut r, 2), Err(GemError::Uapi(e)) if e.name() == "EXEC-ENGINE-INVALID"));
    assert!(matches!(run(&mut r, 3), Err(GemError::Uapi(e)) if e.name() == "EXEC-ENGINE-INVALID"));
    // the legacy ring selectors do not exist on a context with an engine map: slot index only
    let _ = ef::I915_EXEC_BSD;
}

#[test]
fn engine_map_refusals() {
    let mut r = R::new();
    let ext = m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    let mk = |eng: &'static [(u16, u16)]| {
        let mut x = p(cp::I915_CONTEXT_PARAM_ENGINES, 0);
        x.engines = eng;
        x
    };
    // an engine the device does not have: -ENOENT (gem/i915_gem_context.c:781-789); KBL GT2 has one VCS, so (2, 1) is absent
    assert_eq!(r.gem.context_create(r.s, ext, &[mk(&[(2, 1)])], Some(r.pool.init()), &mut r.sink), Err(GemError::EngineMissing { class: 2, instance: 1 }));
    assert_eq!(r.gem.context_create(r.s, ext, &[mk(&[(4, 0)])], Some(r.pool.init()), &mut r.sink).unwrap_err().errno(), 2, "no compute engine");
    // set twice: -EINVAL (gem/i915_gem_context.c:745-748)
    assert_eq!(r.gem.context_create(r.s, ext, &[mk(&[(0, 0)]), mk(&[(0, 0)])], Some(r.pool.init()), &mut r.sink), Err(GemError::EnginesTwice));
    // exactly 64 slots are allowed, 65 are not (I915_EXEC_RING_MASK + 1, gem/i915_gem_context.c:758-760)
    static SIXTY_FIVE: [(u16, u16); 65] = [(0, 0); 65];
    assert_eq!(MAX_USER_ENGINES, 64);
    assert!(r.gem.context_create(r.s, ext, &[mk(&SIXTY_FIVE[..64])], Some(r.pool.init()), &mut r.sink).is_ok());
    assert_eq!(r.gem.context_create(r.s, ext, &[mk(&SIXTY_FIVE)], Some(r.pool.init()), &mut r.sink), Err(GemError::EnginesTooMany { count: 65 }));
}

#[test]
fn context_tables_are_bounded() {
    let mut r = R::new();
    let ext = m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    let vm = r.gem.vm_create(r.s, 0, 0, r.pool.init(), &mut r.sink).unwrap() as u64; // contexts share it: the VM table is not what bounds us here
    let mut ids = vec![];
    // per-session: id 0 is the default, so MAX_CTX_PER_SESSION - 1 more fit
    for _ in 1..MAX_CTX_PER_SESSION {
        ids.push(r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_VM, vm)], None, &mut r.sink).unwrap());
    }
    let full = r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_VM, vm)], None, &mut r.sink);
    assert_eq!(full, Err(GemError::ContextTableFull { cap: MAX_CTX_PER_SESSION }));
    assert_eq!(full.unwrap_err().errno(), 12);
    r.gem.context_destroy(r.s, ids.pop().unwrap(), 0, &mut r.sink).unwrap();
    assert!(r.gem.context_create(r.s, ext, &[p(cp::I915_CONTEXT_PARAM_VM, vm)], None, &mut r.sink).is_ok());
}

#[test]
fn the_global_context_table_is_bounded_across_sessions() {
    let mut r = R::new();
    let ext = m::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    let mut sessions = vec![r.s];
    let mut made = 1; // session 0's default context
    let mut sink = Sink::default();
    for _ in 1..4 {
        sessions.push(r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap());
        made += 1;
    }
    let mut vms = vec![];
    for s in &sessions {
        vms.push(r.gem.vm_create(*s, 0, 0, r.pool.init(), &mut sink).unwrap() as u64);
    }
    'outer: for _ in 0..MAX_CTX_PER_SESSION {
        for (s, vm) in sessions.iter().zip(&vms) {
            match r.gem.context_create(*s, ext, &[p(cp::I915_CONTEXT_PARAM_VM, *vm)], None, &mut sink) {
                Ok(_) => made += 1,
                Err(GemError::ContextTableFull { cap }) => {
                    assert_eq!(cap, MAX_CONTEXTS);
                    break 'outer;
                }
                Err(e) => panic!("{e:?}"),
            }
        }
    }
    assert_eq!(made, MAX_CONTEXTS);
}

#[test]
fn the_vm_table_and_per_session_vm_ids_are_bounded() {
    let mut r = R::new();
    // the default context's VM occupies one of MAX_VMS; user ids are limited per session as well
    let mut made = 0;
    loop {
        match r.gem.vm_create(r.s, 0, 0, r.pool.init(), &mut r.sink) {
            Ok(_) => made += 1,
            Err(GemError::VmTableFull { cap }) => {
                assert!(cap == MAX_VMS || cap == MAX_VM_IDS_PER_SESSION, "cap {cap}");
                break;
            }
            Err(e) => panic!("{e:?}"),
        }
    }
    assert_eq!(made, MAX_VMS - 1);
    assert_eq!(MAX_VM_IDS_PER_SESSION, MAX_VMS);
}
