// SPDX-License-Identifier: GPL-2.0-only
//! The frame-level answers that need no service. In the DEFAULT build (no `p3-exec`) these are the ONLY uAPI answers there are: every
//! uAPI request is `Unsupported` with the reason INCREMENT-NOT-ENABLED, never a timeout and never a silent drop.
use i915_device::uapi::frame::{answer_without_service, is_uapi};
use i915_wire::msg::{self, unsupported, UnsupportedBody};
use i915_wire::*;

fn ask(op: Opcode, inc: u64, payload: &[u8]) -> (Header, Vec<u8>) {
    let mut f = vec![0u8; HEADER_BYTES + payload.len()];
    let h = Header::request(op, 0x77, inc, payload.len() as u32);
    f[HEADER_BYTES..].copy_from_slice(payload);
    h.encode(&mut f).unwrap();
    let mut out = [0u8; 128];
    let n = answer_without_service(&f, &mut out).unwrap();
    let (rh, rp) = Header::decode(&out[..n]).unwrap();
    (rh, rp.to_vec())
}

#[test]
fn every_uapi_opcode_is_unsupported_with_the_named_reason_when_the_increment_is_off() {
    let mut n = 0;
    for op in Opcode::ALL.iter().copied().filter(|o| is_uapi(*o)) {
        let (h, p) = ask(op, 9, &[0u8; 16]);
        assert!(h.is_reply());
        assert_eq!((h.opcode, h.request_id, h.session_incarnation), (op, 0x77, 9), "the reply echoes the request");
        let r = msg::decode_reply::<UnsupportedBody>(&p).unwrap();
        assert_eq!((r.status, r.errno, r.body.reason), (Status::Unsupported, -errno::EOPNOTSUPP, unsupported::INCREMENT_NOT_ENABLED), "{}", op.name());
        n += 1;
    }
    assert_eq!(n, 50, "every uAPI opcode was asked (50 of them)");
}

#[test]
fn hello_is_ok_the_skeleton_operations_are_not_implemented_and_garbage_is_malformed() {
    let (_, p) = ask(Opcode::Hello, 0, &[]);
    let r = msg::decode_reply::<msg::Empty>(&p).unwrap();
    assert_eq!((r.status, r.errno), (Status::Ok, 0));
    for op in [Opcode::GetReadout, Opcode::SetMode, Opcode::PageFlip, Opcode::AllocScanout, Opcode::SubmitBatch] {
        let (_, p) = ask(op, 0, &[]);
        let r = msg::decode_reply::<UnsupportedBody>(&p).unwrap();
        assert_eq!((r.status, r.body.reason), (Status::Unsupported, unsupported::NOT_IMPLEMENTED));
    }
    let mut out = [0u8; 128];
    // bad magic: Malformed, -EINVAL
    let n = answer_without_service(&[0u8; 48], &mut out).unwrap();
    let (_, p) = Header::decode(&out[..n]).unwrap();
    let r = msg::decode_reply::<msg::Empty>(p).unwrap();
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::EINVAL));
    // too short for a header: Malformed, -EFAULT
    let n = answer_without_service(&[1, 2, 3], &mut out).unwrap();
    let (_, p) = Header::decode(&out[..n]).unwrap();
    let r = msg::decode_reply::<msg::Empty>(p).unwrap();
    assert_eq!((r.status, r.errno), (Status::Malformed, -errno::EFAULT));
    // a major we do not speak: BadVersion
    let mut f = [0u8; HEADER_BYTES];
    Header::request(Opcode::Hello, 1, 0, 0).encode(&mut f).unwrap();
    f[4] = 7;
    let n = answer_without_service(&f, &mut out).unwrap();
    let (_, p) = Header::decode(&out[..n]).unwrap();
    assert_eq!(msg::decode_reply::<msg::Empty>(p).unwrap().status, Status::BadVersion);
    // a buffer that cannot hold a reply is the only Err
    assert!(matches!(answer_without_service(&f, &mut [0u8; 10]), Err(WireError::OutputTooSmall { .. })));
}
