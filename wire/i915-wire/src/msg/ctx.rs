//! Contexts, address spaces and reset statistics.
use super::{msg, pad8, Pad32};
use crate::codec::{Reader, Writer};
use crate::WireError;

/// `CONTEXT_PARAM_ENGINES` slots: `I915_EXEC_RING_MASK + 1` (a 6-bit selector, so slot 64 cannot be named).
pub const MAX_ENGINE_SLOTS: u32 = 64;
/// Params per CONTEXT-CREATE-EXT (Linux walks an extension chain bounded at 512 deep, `i915_user_extensions.c`).
pub const MAX_CREATE_PARAMS: u32 = 512;
pub const PARAM_HEAD_BYTES: usize = 24;

msg!(
    /// The legacy `GEM_CONTEXT_CREATE` carries no arguments of its own.
    ContextCreateReq {}
);
msg!(ContextRep { ctx_id: u32, _pad: Pad32 });

msg!(
    /// `flags` is `I915_CONTEXT_CREATE_FLAGS_*`; `param_count` [`ParamRecord`]s follow (the `SETPARAM` extension chain, in order).
    ContextCreateExtHead { flags: u32, param_count: u32 }
);

msg!(ContextDestroyReq { ctx_id: u32, pad: u32 });

msg!(ContextGetParamReq { ctx_id: u32, size: u32, param: u64 });
msg!(ContextGetParamRep { size: u32, _pad: Pad32, value: u64 });

msg!(
    /// `extensions` must be zero (the VM extension table is empty on Gen9).
    VmCreateReq { flags: u32, _pad: Pad32, extensions: u64 }
);
msg!(VmCreateRep { vm_id: u32, _pad: Pad32 });
msg!(VmDestroyReq { vm_id: u32, flags: u32, extensions: u64 });

msg!(GetResetStatsReq { ctx_id: u32, flags: u32, pad: u32, _pad: Pad32 });
msg!(GetResetStatsRep { reset_count: u32, batch_active: u32, batch_pending: u32, _pad: Pad32 });

/// One `drm_i915_gem_context_param`, flattened. For `CONTEXT_PARAM_ENGINES` the engine array follows the head as
/// `engine_count` `(class: u16, instance: u16)` pairs, zero-padded to 8 bytes; for every other param `engine_count` is zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamRecord<'a> {
    pub param: u64,
    pub size: u32,
    pub value: u64,
    engines: &'a [u8],
}

impl<'a> ParamRecord<'a> {
    pub fn engine_count(&self) -> usize {
        self.engines.len() / 4
    }
    pub fn engine(&self, i: usize) -> Option<(u16, u16)> {
        let b = self.engines.get(i.checked_mul(4)?..)?.get(..4)?;
        Some((u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]])))
    }
    pub fn read(r: &mut Reader<'a>) -> Result<ParamRecord<'a>, WireError> {
        let param = r.u64("ctxparam.param")?;
        let size = r.u32("ctxparam.size")?;
        let n = r.u32("ctxparam.engine_count")?;
        let value = r.u64("ctxparam.value")?;
        if n > MAX_ENGINE_SLOTS {
            return Err(WireError::LimitExceeded { what: "ctxparam.engine_count", got: n as u64, max: MAX_ENGINE_SLOTS as u64 });
        }
        let engines = r.take("ctxparam.engines", n as usize * 4)?;
        let pad = pad8(engines.len()) - engines.len();
        if r.take("ctxparam.pad", pad)?.iter().any(|b| *b != 0) {
            return Err(WireError::FieldReservedNonZero { what: "ctxparam.pad", got: 1 });
        }
        Ok(ParamRecord { param, size, value, engines })
    }
    pub fn write(w: &mut Writer<'_>, param: u64, size: u32, value: u64, engines: &[(u16, u16)]) -> Result<(), WireError> {
        w.u64(param)?;
        w.u32(size)?;
        w.u32(engines.len() as u32)?;
        w.u64(value)?;
        for (c, i) in engines {
            w.u16(*c)?;
            w.u16(*i)?;
        }
        w.zeros(pad8(engines.len() * 4) - engines.len() * 4)
    }
}

/// A decoded CONTEXT-CREATE-EXT request: the head, then `head.param_count` records (walk with [`ContextCreateExtReq::next_param`]).
#[derive(Clone, Copy, Debug)]
pub struct ContextCreateExtReq<'a> {
    pub head: ContextCreateExtHead,
    rest: Reader<'a>,
    left: u32,
}

impl<'a> ContextCreateExtReq<'a> {
    /// Check the whole payload before returning (transactional): every record must parse and nothing may trail.
    pub fn decode(payload: &'a [u8]) -> Result<ContextCreateExtReq<'a>, WireError> {
        let mut r = Reader::new(payload);
        let head = <ContextCreateExtHead as super::Body>::decode(&mut r)?;
        if head.param_count > MAX_CREATE_PARAMS {
            return Err(WireError::LimitExceeded { what: "ContextCreateExt.param_count", got: head.param_count as u64, max: MAX_CREATE_PARAMS as u64 });
        }
        let rest = r;
        for _ in 0..head.param_count {
            ParamRecord::read(&mut r)?;
        }
        r.finish()?;
        Ok(ContextCreateExtReq { head, rest, left: head.param_count })
    }
    pub fn next_param(&mut self) -> Option<ParamRecord<'a>> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        ParamRecord::read(&mut self.rest).ok()
    }
}

/// A decoded CONTEXT-SETPARAM request: `ctx_id`, padding, then one [`ParamRecord`].
#[derive(Clone, Copy, Debug)]
pub struct ContextSetParamReq<'a> {
    pub ctx_id: u32,
    pub param: ParamRecord<'a>,
}

impl<'a> ContextSetParamReq<'a> {
    pub fn decode(payload: &'a [u8]) -> Result<ContextSetParamReq<'a>, WireError> {
        let mut r = Reader::new(payload);
        let ctx_id = r.u32("ContextSetParam.ctx_id")?;
        r.zero32("ContextSetParam._pad")?;
        let param = ParamRecord::read(&mut r)?;
        r.finish()?;
        Ok(ContextSetParamReq { ctx_id, param })
    }
    pub fn encode(out: &mut [u8], ctx_id: u32, param: u64, size: u32, value: u64, engines: &[(u16, u16)]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32(ctx_id)?;
        w.u32(0)?;
        ParamRecord::write(&mut w, param, size, value, engines)?;
        Ok(w.pos())
    }
}

/// Encode a CONTEXT-CREATE-EXT request from `(param, size, value, engines)` tuples.
pub fn encode_context_create_ext(out: &mut [u8], flags: u32, params: &[(u64, u32, u64, &[(u16, u16)])]) -> Result<usize, WireError> {
    let mut w = Writer::new(out);
    w.u32(flags)?;
    w.u32(params.len() as u32)?;
    for (p, s, v, e) in params {
        ParamRecord::write(&mut w, *p, *s, *v, e)?;
    }
    Ok(w.pos())
}
