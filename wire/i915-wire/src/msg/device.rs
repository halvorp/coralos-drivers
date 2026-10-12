//! GETPARAM, QUERY and REG-READ: questions about the device itself.
use super::{msg, pad8};
use crate::codec::{Reader, Writer};
use crate::WireError;

msg!(
    /// `drm_i915_getparam` without its pointer: `param` is the selector.
    GetParamReq { param: u32, _rsvd: super::Pad32 }
);
msg!(
    /// The value is a signed 32-bit integer, exactly what `*gp->value` received.
    GetParamRep { value: i32, _pad: super::Pad32 }
);

msg!(
    /// `drm_i915_reg_read`: the low bits of `offset` are flags (`I915_REG_READ_8B_WA` = 1).
    RegReadReq { offset: u64 }
);
msg!(RegReadRep { val: u64 });

/// Items per QUERY call: Linux iterates `num_items` without a bound and there are 7 query ids; this is the validation limit.
pub const MAX_QUERY_ITEMS: u32 = 16;
pub const QUERY_REQUEST_HEAD_BYTES: usize = 8;
pub const QUERY_ITEM_BYTES: usize = 32;
/// Reply: `status`, `errno`, `num_items`, `pad` (16 bytes), then per item `length: i32`, `data_len: u32` and the data zero-padded to 8.
pub const QUERY_REPLY_HEAD_BYTES: usize = 16;

/// `drm_i915_query_item` flattened: `data_ptr` is replaced by `hdr`, a snapshot of the first 16 bytes of the client's data buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryItem {
    pub query_id: u64,
    /// The client buffer's capacity (`0` = size probe).
    pub length: i32,
    pub flags: u32,
    pub hdr: [u32; 4],
}

impl QueryItem {
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<(), WireError> {
        w.u64(self.query_id)?;
        w.i32(self.length)?;
        w.u32(self.flags)?;
        for h in self.hdr {
            w.u32(h)?;
        }
        Ok(())
    }
    pub fn decode(r: &mut Reader<'_>) -> Result<QueryItem, WireError> {
        Ok(QueryItem { query_id: r.u64("query.item.id")?, length: r.i32("query.item.length")?, flags: r.u32("query.item.flags")?, hdr: [r.u32("query.item.hdr")?, r.u32("query.item.hdr")?, r.u32("query.item.hdr")?, r.u32("query.item.hdr")?] })
    }
}

/// Write a QUERY request payload.
pub fn encode_query_request(out: &mut [u8], flags: u32, items: &[QueryItem]) -> Result<usize, WireError> {
    if items.len() > MAX_QUERY_ITEMS as usize {
        return Err(WireError::LimitExceeded { what: "query.num_items", got: items.len() as u64, max: MAX_QUERY_ITEMS as u64 });
    }
    let mut w = Writer::new(out);
    w.u32(items.len() as u32)?;
    w.u32(flags)?;
    for i in items {
        i.encode(&mut w)?;
    }
    Ok(w.pos())
}

/// A decoded QUERY request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryRequest {
    pub flags: u32,
    pub num_items: u32,
}

/// Decode a QUERY request: the head and the borrowed item table, transactionally.
pub fn decode_query_request(payload: &[u8]) -> Result<(QueryRequest, QueryItems<'_>), WireError> {
    let mut r = Reader::new(payload);
    let num_items = r.u32("query.num_items")?;
    let flags = r.u32("query.flags")?;
    if num_items > MAX_QUERY_ITEMS {
        return Err(WireError::LimitExceeded { what: "query.num_items", got: num_items as u64, max: MAX_QUERY_ITEMS as u64 });
    }
    let bytes = r.take("query.items", num_items as usize * QUERY_ITEM_BYTES)?;
    r.finish()?;
    Ok((QueryRequest { flags, num_items }, QueryItems(bytes)))
}

#[derive(Clone, Copy, Debug)]
pub struct QueryItems<'a>(&'a [u8]);

impl QueryItems<'_> {
    pub fn len(&self) -> usize {
        self.0.len() / QUERY_ITEM_BYTES
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get(&self, i: usize) -> Option<QueryItem> {
        let b = self.0.get(i.checked_mul(QUERY_ITEM_BYTES)?..)?.get(..QUERY_ITEM_BYTES)?;
        QueryItem::decode(&mut Reader::new(b)).ok()
    }
}

/// One item of a QUERY reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryReplyItem<'a> {
    /// The item's `length` after the call: the produced size, or `-errno`.
    pub length: i32,
    pub data: &'a [u8],
}

/// Walk a QUERY reply payload (after its `status`/`errno` head): `num_items` items, each `length`, `data_len`, data padded to 8.
pub fn decode_query_reply(payload: &[u8]) -> Result<(i32, [Option<QueryReplyItem<'_>>; MAX_QUERY_ITEMS as usize]), WireError> {
    let mut r = Reader::new(payload);
    let (_status, errno) = super::decode_reply_head(&mut r)?;
    let n = r.u32("query.reply.num_items")?;
    r.zero32("query.reply.pad")?;
    if n > MAX_QUERY_ITEMS {
        return Err(WireError::LimitExceeded { what: "query.reply.num_items", got: n as u64, max: MAX_QUERY_ITEMS as u64 });
    }
    let mut items = [None; MAX_QUERY_ITEMS as usize];
    for slot in items.iter_mut().take(n as usize) {
        let length = r.i32("query.reply.length")?;
        let len = r.u32("query.reply.data_len")? as usize;
        let data = r.take("query.reply.data", len)?;
        r.take("query.reply.pad", pad8(len) - len)?;
        *slot = Some(QueryReplyItem { length, data });
    }
    r.finish()?;
    Ok((errno, items))
}
