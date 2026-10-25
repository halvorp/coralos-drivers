// SPDX-License-Identifier: GPL-2.0-only
//! Frame-level answers that need no service. Used by the default build (every uAPI request is `Unsupported`) and by the service for
//! frames it cannot even route.
use i915_wire::msg::{self, unsupported};
use i915_wire::{Header, Opcode, Status, WireError, HEADER_BYTES};

/// The reply to a frame whose HEADER did not decode: status `Malformed` (or `BadVersion`), `-errno` of the refusal, opcode `Hello`
/// and request id 0 (nothing in the frame can be trusted to echo).
pub fn malformed_reply(err: WireError, out: &mut [u8]) -> Result<usize, WireError> {
    let h = Header::request(Opcode::Hello, 0, 0, 0);
    let have = out.len();
    let n = msg::encode_reply::<msg::Empty>(out.get_mut(HEADER_BYTES..).ok_or(WireError::OutputTooSmall { need: HEADER_BYTES + 8, have })?, err.status(), -err.errno(), None)?;
    finish(h, n, out)
}

/// A refusal of a call whose header DID decode: the request's opcode, id and incarnation echoed, `status`, and `-errno` (a zero-size
/// body: the refusals of the transport layer happen before any handler knows the body of its reply). Returns the frame length.
pub fn refusal_reply(request: Header, status: Status, errno: i32, out: &mut [u8]) -> Result<usize, WireError> {
    let have = out.len();
    let n = msg::encode_reply::<msg::Empty>(out.get_mut(HEADER_BYTES..).ok_or(WireError::OutputTooSmall { need: HEADER_BYTES + 8, have })?, status, -errno, None)?;
    finish(request, n, out)
}

/// Write the reply header for a payload already placed at `out[HEADER_BYTES..HEADER_BYTES + payload_len]`. Returns the frame length.
pub fn finish(request: Header, payload_len: usize, out: &mut [u8]) -> Result<usize, WireError> {
    request.reply(payload_len as u32).encode(out)?;
    Ok(HEADER_BYTES + payload_len)
}

/// Answer a frame with no uAPI service behind it: `Hello` is `Ok`; every other operation is `Unsupported` with the reason
/// [`unsupported::INCREMENT_NOT_ENABLED`] (a uAPI opcode) or [`unsupported::NOT_IMPLEMENTED`] (the first generation's operations).
/// A frame that does not decode is `Malformed`. The answer is always a frame; `Err` only when `out` cannot hold even a header.
pub fn answer_without_service(frame: &[u8], out: &mut [u8]) -> Result<usize, WireError> {
    let (h, _payload) = match Header::decode(frame) {
        Ok(x) => x,
        Err(e) => return malformed_reply(e, out),
    };
    let have = out.len();
    let area = out.get_mut(HEADER_BYTES..).ok_or(WireError::OutputTooSmall { need: HEADER_BYTES + 8, have })?;
    let n = if h.opcode == Opcode::Hello {
        msg::encode_reply::<msg::Empty>(area, Status::Ok, 0, None)?
    } else if h.opcode as u32 >= 0x100 {
        msg::encode_unsupported(area, unsupported::INCREMENT_NOT_ENABLED)?
    } else {
        msg::encode_unsupported(area, unsupported::NOT_IMPLEMENTED)?
    };
    finish(h, n, out)
}

/// Is this opcode one of the uAPI calls (a call the `p3-exec` increment serves)?
pub const fn is_uapi(op: Opcode) -> bool {
    op as u32 >= 0x100
}


/// The PARKED device's answer: a display opcode is answered from the adoption (`DISPLAY-QUERY` described; an authority call refused
/// `DISPLAY-NOT-AUTHORIZED` - the parked device has no sessions, so it has no authority), everything else as
/// [`answer_without_service`]. Returns the frame length and the refusal name, if the answer was a named display refusal.
pub fn answer_parked(frame: &[u8], adoption: &crate::display::Adoption, out: &mut [u8]) -> Result<(usize, Option<&'static str>), WireError> {
    if let Ok((h, payload)) = Header::decode(frame) {
        let have = out.len();
        let area = out.get_mut(HEADER_BYTES..).ok_or(WireError::OutputTooSmall { need: HEADER_BYTES + 8, have })?;
        if let Some(a) = crate::display::answer::answer_without_authority(&h, payload, adoption, area) {
            let a = a?;
            return finish(h, a.len, out).map(|n| (n, a.refused));
        }
    }
    answer_without_service(frame, out).map(|n| (n, None))
}
