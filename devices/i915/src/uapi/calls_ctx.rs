// SPDX-License-Identifier: GPL-2.0-only
//! Contexts, address spaces and reset statistics.
use super::fail::Fail;
use super::service::{vm_init, Done, Hw, VM_BASE_PAGES};
use super::UapiService;
use crate::exec::dma::CpuMem;
use crate::log::Log;
use alloc::vec::Vec;
use coralos_hal::{Dma, Mmio};
use i915_gem_core::{ContextParam, SessionId};
use i915_wire::msg::ctx::*;
use i915_wire::msg::{self, Body};
use i915_wire::{errno, Opcode, Status};

/// Copy a record's engine pairs into an owned list (the planner wants a slice of tuples; the payload holds bytes).
fn engines_of(rec: &ParamRecord<'_>) -> Result<Vec<(u16, u16)>, Fail> {
    let mut v = Vec::new();
    v.try_reserve_exact(rec.engine_count()).map_err(|_| Fail::oom("context-engines"))?;
    for i in 0..rec.engine_count() {
        v.push(rec.engine(i).unwrap_or((0, 0)));
    }
    Ok(v)
}

impl UapiService {
    pub(crate) fn call_ctx<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, op: Opcode, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        let sid = SessionId(inc);
        macro_rules! ok {
            ($body:expr) => {
                Ok(Done::Len(msg::encode_reply(out, Status::Ok, 0, Some(&$body))?))
            };
        }
        match op {
            Opcode::ContextCreate | Opcode::ContextCreateExt => {
                let (flags, owned): (u32, Vec<(u64, u32, u64, Vec<(u16, u16)>)>) = if op == Opcode::ContextCreate {
                    ContextCreateReq::from_payload(p)?;
                    (0, Vec::new())
                } else {
                    let mut r = ContextCreateExtReq::decode(p)?;
                    let mut v = Vec::new();
                    v.try_reserve_exact(r.head.param_count as usize).map_err(|_| Fail::oom("context-params"))?;
                    while let Some(rec) = r.next_param() {
                        v.push((rec.param, rec.size, rec.value, engines_of(&rec)?));
                    }
                    (r.head.flags, v)
                };
                let mut params: Vec<ContextParam<'_>> = Vec::new();
                params.try_reserve_exact(owned.len()).map_err(|_| Fail::oom("context-params"))?;
                for (param, size, value, engines) in &owned {
                    params.push(ContextParam { param: *param, size: *size, value: *value, engines });
                }
                self.vms.try_reserve(1).map_err(|_| Fail::oom("vm-records"))?;
                let base = self.pool.alloc(hw.gm, VM_BASE_PAGES)?;
                let init = vm_init(&base);
                let (res, err) = self.with_sink(hw, inc, |gem, sink| gem.context_create(sid, flags, &params, Some(init), sink));
                // the pages belong to a new address space only if the planner used them (no PARAM_VM named an existing one)
                self.record_vm(base, init.pml4);
                let id = res?;
                if let Some(e) = err {
                    return Err(e);
                }
                ok!(ContextRep { ctx_id: id, ..Default::default() })
            }
            Opcode::ContextDestroy => {
                let r = ContextDestroyReq::from_payload(p)?;
                let (res, err) = self.with_sink(hw, inc, |gem, sink| gem.context_destroy(sid, r.ctx_id, r.pad, sink));
                res?;
                err.map_or(Ok(()), Err)?;
                Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?))
            }
            Opcode::ContextGetParam => {
                let r = ContextGetParamReq::from_payload(p)?;
                let v = self.gem.context_getparam(sid, r.ctx_id, r.param, r.size)?;
                ok!(ContextGetParamRep { size: v.size, value: v.value, ..Default::default() })
            }
            Opcode::ContextSetParam => {
                let r = ContextSetParamReq::decode(p)?;
                let engines = engines_of(&r.param)?;
                let cp = ContextParam { param: r.param.param, size: r.param.size, value: r.param.value, engines: &engines };
                self.gem.context_setparam(sid, r.ctx_id, &cp)?;
                Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?))
            }
            Opcode::VmCreate => {
                let r = VmCreateReq::from_payload(p)?;
                self.vms.try_reserve(1).map_err(|_| Fail::oom("vm-records"))?;
                let base = self.pool.alloc(hw.gm, VM_BASE_PAGES)?;
                let init = vm_init(&base);
                let (res, err) = self.with_sink(hw, inc, |gem, sink| gem.vm_create(sid, r.flags, r.extensions, init, sink));
                match res {
                    Ok(id) => {
                        self.record_vm(base, init.pml4);
                        if let Some(e) = err {
                            return Err(e);
                        }
                        ok!(VmCreateRep { vm_id: id, ..Default::default() })
                    }
                    Err(e) => {
                        self.pool.release(base);
                        Err(e.into())
                    }
                }
            }
            Opcode::VmDestroy => {
                let r = VmDestroyReq::from_payload(p)?;
                let (res, err) = self.with_sink(hw, inc, |gem, sink| gem.vm_destroy(sid, r.vm_id, r.flags, r.extensions, sink));
                res?;
                err.map_or(Ok(()), Err)?;
                Ok(Done::Len(msg::encode_reply::<msg::Empty>(out, Status::Ok, 0, None)?))
            }
            Opcode::GetResetStats => {
                let r = GetResetStatsReq::from_payload(p)?;
                let s = self.gem.reset_stats(sid, r.ctx_id, r.flags, r.pad)?;
                ok!(GetResetStatsRep { reset_count: s.reset_count, batch_active: s.batch_active, batch_pending: s.batch_pending, ..Default::default() })
            }
            _ => Err(Fail::new(errno::EINVAL, "UNROUTED")),
        }
    }
}
