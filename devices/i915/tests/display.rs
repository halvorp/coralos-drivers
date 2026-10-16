// SPDX-License-Identifier: GPL-2.0-only
//! D17 display ownership, the parts every build has: startup ADOPTS the firmware's display (or refuses by name) without writing
//! anything, the adopted framebuffer's GGTT range is what bring-up must keep, and the PARKED device answers `DISPLAY-QUERY` from the
//! adoption while refusing every authority call (`DISPLAY-NOT-AUTHORIZED`: the parked device has no sessions, so no authority).
use i915_device::display::{Adoption, ADOPTED_SCANOUT};
use i915_device::startup::startup;
use i915_device::uapi::frame::answer_parked;
use i915_display_modeset_core::initial::AdoptRefusal;
use i915_memory_core::Preallocated;
use i915_wire::msg::display::{self, *};
use i915_wire::msg::{self, unsupported, Body, UnsupportedBody};
use i915_wire::*;
mod common;
use common::*;

fn adopted_atlas() -> (Adoption, Lines) {
    let (mut bar, mut cfg, mut lg) = (bar_atlas(), config_page(0x591c, 0x02), Lines::default());
    let ready = startup(&mut bar, &mut cfg, &mut lg).expect("the Pixelbook Go starts");
    (ready.adoption, lg)
}

#[test]
fn startup_adopts_the_pixelbook_go_firmware_display_and_writes_nothing() {
    let (a, lg) = adopted_atlas();
    let d = a.adopted().expect("adopted");
    assert_eq!((d.fb.width, d.fb.height, d.active_w, d.active_h), (1920, 1080, 3840, 2160));
    assert!(lg.has("[I915-ADOPT OK pipe=A trans=EDP plane=1 fb=1920x1080 stride=7680 fourcc=XR24 mod=0x0 ggtt=0x0+0x7e9000 panel=3840x2160 scaled=1 "), "{:?}", lg.0);
    assert!(lg.has(" writes=0]"));
    let last = lg.0.last().unwrap();
    assert!(last.starts_with("[I915-KBL-DEV READY") && last.ends_with("writes=0 refusals=0]"), "the adoption is judged before READY and writes nothing: {last}");
    assert_eq!(a.keep(), Some(Preallocated { start: 0, size: 0x7e_9000, what: ADOPTED_SCANOUT }), "the range bring-up must keep");
    assert_eq!(ADOPTED_SCANOUT, "adopted-scanout");
}

#[test]
fn a_display_this_adoption_cannot_keep_is_refused_by_name_and_left_alone() {
    let (mut bar, mut cfg, mut lg) = (bar_atlas(), config_page(0x591c, 0x02), Lines::default());
    bar.set(0x68180, 0); // the panel fitter off: 1920x1080 cannot be shown on 3840x2160 as the firmware set it
    let ready = startup(&mut bar, &mut cfg, &mut lg).unwrap();
    assert_eq!(ready.adoption, Adoption::Refused(AdoptRefusal::UnscaledSizeMismatch { src_w: 1920, src_h: 1080, active_w: 3840, active_h: 2160 }));
    assert!(lg.has("[I915-ADOPT REFUSED why=ADOPT-UNSCALED-SIZE-MISMATCH "), "{:?}", lg.0);
    assert_eq!(ready.adoption.keep(), None, "nothing adopted, nothing reserved");
    let q = ready.adoption.query_reply();
    assert_eq!((q.adopted, q.refusal, q.width, q.mode_count), (display::state::NOT_ADOPTED, display::refusal::UNSCALED_SIZE_MISMATCH, 0, 0));
    // vector A (an unscaled 1080p panel, X-tiled at 1 MiB) is adopted: the control
    let (mut bar, mut cfg, mut lg) = (bar_vector_a(), config_page(0x591c, 0x02), Lines::default());
    let r = startup(&mut bar, &mut cfg, &mut lg).unwrap();
    assert_eq!(r.adoption.keep(), Some(Preallocated { start: 0x10_0000, size: 7680 * 1080, what: ADOPTED_SCANOUT }));
    assert_eq!(Adoption::NoReadout.query_reply().refusal, display::refusal::READOUT_FAILED);
}

fn ask(a: &Adoption, op: Opcode, inc: u64, payload: &[u8]) -> (Header, Vec<u8>, Option<&'static str>) {
    let mut f = vec![0u8; HEADER_BYTES + payload.len()];
    Header::request(op, 0x55, inc, payload.len() as u32).encode(&mut f).unwrap();
    f[HEADER_BYTES..].copy_from_slice(payload);
    let mut out = [0u8; 256];
    let (n, refused) = answer_parked(&f, a, &mut out).unwrap();
    let (h, p) = Header::decode(&out[..n]).unwrap();
    (h, p.to_vec(), refused)
}

#[test]
fn the_parked_device_describes_the_adopted_display_to_anyone() {
    let (a, _) = adopted_atlas();
    for inc in [0, 7] {
        let (h, p, refused) = ask(&a, Opcode::DisplayQuery, inc, &[]);
        assert_eq!((h.opcode, h.request_id, refused), (Opcode::DisplayQuery, 0x55, None));
        let r = msg::decode_reply::<DisplayQueryRep>(&p).unwrap();
        assert_eq!((r.status, r.errno), (Status::Ok, 0));
        let b = r.body;
        assert_eq!((b.adopted, b.refusal, b.pipe, b.plane), (display::state::ADOPTED, display::refusal::NONE, 0, 0));
        assert_eq!((b.width, b.height, b.stride, b.fourcc, b.modifier), (1920, 1080, 7680, u32::from_le_bytes(*b"XR24"), 0));
        assert_eq!((b.active_width, b.active_height, b.scaled, b.mode_count), (3840, 2160, 1, 1));
    }
    // a query is empty: a byte is Malformed
    let (_, p, _) = ask(&a, Opcode::DisplayQuery, 0, &[1]);
    assert_eq!(msg::decode_reply::<DisplayQueryRep>(&p).unwrap().status, Status::Malformed);
}

/// NEGATIVE CONTROL for authority: with no session there is no authority, so every authority call is refused by name - including the
/// one that would change nothing (the adopted mode itself).
#[test]
fn every_authority_call_is_refused_by_name_on_the_parked_device() {
    let (a, _) = adopted_atlas();
    let m = a.adopted().unwrap().mode();
    let mode = DisplayModeReq { width: m.width, height: m.height, refresh_millihz: m.refresh_millihz, ..Default::default() };
    let mut mp = [0u8; 16];
    mode.to_payload(&mut mp).unwrap();
    for (op, body) in [
        (Opcode::DisplaySetMode, 0usize),
        (Opcode::ScanoutAcquire, <ScanoutAcquireRep as Body>::SIZE),
        (Opcode::ScanoutBegin, 0),
        (Opcode::ScanoutPresent, <ScanoutPresentRep as Body>::SIZE),
        (Opcode::ScanoutWait, <ScanoutStatusRep as Body>::SIZE),
    ] {
        let (h, p, refused) = ask(&a, op, 3, &mp);
        assert_eq!(refused, Some("DISPLAY-NOT-AUTHORIZED"), "{}", op.name());
        assert_eq!(h.opcode, op);
        assert_eq!(&p[..8], &[0, 0, 0, 0, 0xf3, 0xff, 0xff, 0xff], "{}: status Ok, errno -EACCES (-13)", op.name());
        assert_eq!(p.len(), 8 + body, "{}: the reply body is the opcode's, zeroed", op.name());
        assert!(p[8..].iter().all(|x| *x == 0));
    }
    // the rest of the parked device is unchanged
    let (_, p, _) = ask(&a, Opcode::Hello, 0, &[]);
    assert_eq!(msg::decode_reply::<msg::Empty>(&p).unwrap().status, Status::Ok);
    let (_, p, _) = ask(&a, Opcode::GemCreate, 0, &[0u8; 8]);
    let r = msg::decode_reply::<UnsupportedBody>(&p).unwrap();
    assert_eq!((r.status, r.body.reason), (Status::Unsupported, unsupported::INCREMENT_NOT_ENABLED));
}
