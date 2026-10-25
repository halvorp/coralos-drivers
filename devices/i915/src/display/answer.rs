// SPDX-License-Identifier: GPL-2.0-only
//! The display answers that need no session and no authority: `DISPLAY-QUERY` (it describes and changes nothing, so the parked
//! device answers it as well as the live one), and the refusal every AUTHORITY call (`DISPLAY-SET-MODE`, `SCANOUT-*`) gets when the
//! frame does not come from the display authority: `-EACCES`, `DISPLAY-NOT-AUTHORIZED`. A flag in the request can never make a
//! session the authority; only the binding the system made can (see `crate::display::scanout`).
use super::Adoption;
use i915_wire::msg::display::{ScanoutAcquireRep, ScanoutPresentRep, ScanoutStatusRep};
use i915_wire::msg::{self, Body};
use i915_wire::{errno, Header, Opcode, Status, WireError};

/// The refusal name of an authority call from a frame that is not the display authority's.
pub const NOT_AUTHORIZED: &str = "DISPLAY-NOT-AUTHORIZED";

/// Is this one of the display opcodes (ABI 1.5)?
pub const fn is_display(op: Opcode) -> bool {
    matches!(op, Opcode::DisplayQuery | Opcode::DisplaySetMode | Opcode::ScanoutAcquire | Opcode::ScanoutBegin | Opcode::ScanoutPresent | Opcode::ScanoutWait)
}

/// The fixed reply body of each display opcode (zeroes on a failure), 0 for every other opcode.
pub const fn reply_body_len(op: Opcode) -> usize {
    match op {
        Opcode::DisplayQuery => <i915_wire::msg::display::DisplayQueryRep as Body>::SIZE,
        Opcode::ScanoutAcquire => <ScanoutAcquireRep as Body>::SIZE,
        Opcode::ScanoutPresent => <ScanoutPresentRep as Body>::SIZE,
        Opcode::ScanoutWait => <ScanoutStatusRep as Body>::SIZE,
        _ => 0,
    }
}

/// What a session-less answer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Answered {
    /// Reply payload bytes written at the start of the area.
    pub len: usize,
    /// The refusal's name, when it was one (for the caller's log line).
    pub refused: Option<&'static str>,
}

/// Answer a display opcode with no authority behind the frame: `DISPLAY-QUERY` from `adoption`; any authority call refused
/// `DISPLAY-NOT-AUTHORIZED`. `None` for an opcode that is not a display opcode. The reply PAYLOAD goes to `area`.
pub fn answer_without_authority(h: &Header, payload: &[u8], adoption: &Adoption, area: &mut [u8]) -> Option<Result<Answered, WireError>> {
    if !is_display(h.opcode) {
        return None;
    }
    Some(if h.opcode == Opcode::DisplayQuery {
        query(payload, adoption, area)
    } else {
        refuse(h.opcode, Status::Ok, errno::EACCES, NOT_AUTHORIZED, area)
    })
}

/// `DISPLAY-QUERY`: the request carries nothing (any byte is `Malformed`); the reply is the adoption as the wire states it.
pub fn query(payload: &[u8], adoption: &Adoption, area: &mut [u8]) -> Result<Answered, WireError> {
    if let Err(e) = msg::Empty::from_payload(payload) {
        return refuse(Opcode::DisplayQuery, Status::Malformed, e.errno(), e.name(), area);
    }
    let len = msg::encode_reply(area, Status::Ok, 0, Some(&adoption.query_reply()))?;
    Ok(Answered { len, refused: None })
}

/// A display refusal: `status`, `-errno`, and the opcode's reply body as zeroes.
pub fn refuse(op: Opcode, status: Status, errno: i32, name: &'static str, area: &mut [u8]) -> Result<Answered, WireError> {
    let mut w = i915_wire::codec::Writer::new(area);
    msg::encode_reply_head(&mut w, status, -errno)?;
    w.zeros(reply_body_len(op))?;
    Ok(Answered { len: w.pos(), refused: Some(name) })
}
