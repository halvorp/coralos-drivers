// SPDX-License-Identifier: GPL-2.0-only
//! D17 display ownership on the LIVE service (`p3-exec`): render bring-up keeps the adopted framebuffer's GGTT entries exactly as
//! the firmware left them (and a control shows bring-up WOULD overwrite them), `DISPLAY-QUERY` is answered with or without a session,
//! and every authority call is refused unless the session is the one the device bound - a `SESSION-OPEN` flag never makes one.
#![cfg(feature = "p3-exec")]
mod common;
mod exec_common;
use exec_common::{built, kbl, Chip, Gm, GGTT_BASE};
use i915_device::display::Adoption;
use i915_device::exec::Executor;
use i915_device::uapi::{bring_up_render, Handled, Hw, RenderConfig, UapiService};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, Gen9Clock};
use i915_devinfo_core::sseu::gen9_sseu_info;
use i915_gt_core::emit::FlushQuirks;
use i915_memory_core::probe::GgttGeometry;
use i915_uapi_core::UapiDevice;
use i915_wire::msg::display::{self, *};
use i915_wire::msg::session::*;
use i915_wire::msg::{self, Body};
use i915_wire::{errno, Header, Opcode, Status, HEADER_BYTES};

/// The Pixelbook Go's firmware display, adopted by a real startup over the simulated BAR.
fn atlas_adoption() -> Adoption {
    let (mut bar, mut cfg, mut lg) = (common::bar_atlas(), common::config_page(0x591c, 0x02), common::Lines::default());
    i915_device::startup::startup(&mut bar, &mut cfg, &mut lg).unwrap().adoption
}

/// The firmware's framebuffer PTEs as the chip holds them before bring-up: entry i -> stolen page i, present.
const FB_ENTRIES: u32 = 0x7e9;
fn fw_pte(i: u32) -> u64 {
    0x7b80_0000 + (i as u64) * 4096 | 1
}

fn seed_firmware_ptes(chip: &Chip) {
    let mut s = chip.0.borrow_mut();
    for i in 0..FB_ENTRIES + 4 {
        let v = fw_pte(i);
        s.ggtt[2 * i as usize] = v as u32;
        s.ggtt[2 * i as usize + 1] = (v >> 32) as u32;
    }
}

struct W {
    chip: Chip,
    ex: Executor<Chip>,
    gm: Gm,
    log: exec_common::Lines,
    svc: UapiService,
    rid: u64,
}

fn world(adoption: Adoption) -> W {
    let (chip, _mem, mut ex, mut gm) = built();
    seed_firmware_ptes(&chip);
    let mut log = exec_common::Lines::default();
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let Gen9Clock::Hz(hz) = gen9_clock_from_ctc(2 << 1) else { panic!("crystal expected") };
    let dev = UapiDevice::new(kbl(), 0x09, sseu, hz, 8 << 30).unwrap();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    let cfg = RenderConfig { quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, stall_bound_ms: 1000, keep: adoption.keep() };
    let mut svc = bring_up_render(&mut Hw { ex: &mut ex, gm: &mut gm, log: &mut log, now_ms: 0 }, dev, &geom, cfg).unwrap();
    svc.set_display(adoption);
    W { chip, ex, gm, log, svc, rid: 0 }
}

impl W {
    /// One call; returns (status, errno, body).
    fn call(&mut self, op: Opcode, inc: u64, payload: &[u8]) -> (Header, Status, i32, Vec<u8>) {
        self.rid += 1;
        let mut f = vec![0u8; HEADER_BYTES + payload.len()];
        Header::request(op, self.rid, inc, payload.len() as u32).encode(&mut f).unwrap();
        f[HEADER_BYTES..].copy_from_slice(payload);
        let mut out = vec![0u8; 4096];
        let n = match self.svc.handle(&mut Hw { ex: &mut self.ex, gm: &mut self.gm, log: &mut self.log, now_ms: 1 }, &f, &mut out).unwrap() {
            Handled::Reply(n) => n,
            Handled::Parked => panic!("{} parked", op.name()),
        };
        let (h, p) = Header::decode(&out[..n]).unwrap();
        let mut r = i915_wire::codec::Reader::new(p);
        let (status, e) = msg::decode_reply_head(&mut r).unwrap();
        (h, status, e, p[8..].to_vec())
    }
    fn open(&mut self, caps: u32) -> u64 {
        let mut p = [0u8; 8];
        SessionOpenReq { caps, ..Default::default() }.to_payload(&mut p).unwrap();
        let (h, s, e, _) = self.call(Opcode::SessionOpen, 0, &p);
        assert_eq!((s, e), (Status::Ok, 0));
        h.session_incarnation
    }
    fn set_mode(&mut self, inc: u64, w: u32, h: u32, r: u32) -> (Status, i32) {
        let mut p = [0u8; 16];
        DisplayModeReq { width: w, height: h, refresh_millihz: r, ..Default::default() }.to_payload(&mut p).unwrap();
        let (_, s, e, _) = self.call(Opcode::DisplaySetMode, inc, &p);
        (s, e)
    }
}

#[test]
fn render_bring_up_keeps_the_adopted_framebuffer_ptes_exactly_as_the_firmware_left_them() {
    let w = world(atlas_adoption());
    for i in 0..FB_ENTRIES {
        assert_eq!(w.chip.ggtt_entry(GGTT_BASE + 8 * i), fw_pte(i), "entry {i:#x} of the live framebuffer was rewritten");
    }
    assert_ne!(w.chip.ggtt_entry(GGTT_BASE + 8 * FB_ENTRIES), fw_pte(FB_ENTRIES), "the first entry past the framebuffer is cleared (it is the error-capture slot)");
    assert!(w.log.has("[I915-GGTT kept=adopted-scanout start=0x0 size=0x7e9000 - not written, never bound over]"), "{:?}", w.log.0);
}

/// NEGATIVE CONTROL: the same bring-up with nothing adopted points entry 0 - the panel's first page - at the scratch page.
#[test]
fn without_the_adoption_render_bring_up_overwrites_the_live_scanout() {
    let w = world(Adoption::NoReadout);
    assert_ne!(w.chip.ggtt_entry(GGTT_BASE), fw_pte(0));
    assert!(!w.log.has("[I915-GGTT kept="));
}

#[test]
fn display_query_is_answered_with_or_without_a_session() {
    let mut w = world(atlas_adoption());
    let (_, s, e, body) = w.call(Opcode::DisplayQuery, 0, &[]);
    assert_eq!((s, e), (Status::Ok, 0));
    let q = DisplayQueryRep::from_payload(&body).unwrap();
    assert_eq!((q.adopted, q.width, q.height, q.active_width, q.active_height, q.mode_count), (display::state::ADOPTED, 1920, 1080, 3840, 2160, 1));
    let inc = w.open(0);
    let (_, s, e, body2) = w.call(Opcode::DisplayQuery, inc, &[]);
    assert_eq!((s, e, body2), (Status::Ok, 0, body));
}

#[test]
fn authority_calls_need_the_bound_session_and_no_session_open_flag_makes_one() {
    let mut w = world(atlas_adoption());
    let m = w.svc.display_authority();
    assert_eq!(m, None, "nobody is the authority until the transport binds one");
    let all_caps = w.open(CAPS_KNOWN);
    let (s, e) = w.set_mode(all_caps, 1920, 1080, 0);
    assert_eq!((s, e), (Status::Ok, -errno::EACCES), "every SESSION-OPEN cap and still not the display authority");
    assert!(w.log.has("refused name=DISPLAY-NOT-AUTHORIZED"), "{:?}", w.log.0);
    for op in [Opcode::ScanoutAcquire, Opcode::ScanoutBegin, Opcode::ScanoutPresent, Opcode::ScanoutWait] {
        let (_, s, e, body) = w.call(op, all_caps, &[0u8; 16][..if op == Opcode::ScanoutWait { 16 } else if op == Opcode::ScanoutAcquire { 0 } else { 8 }]);
        assert_eq!((s, e), (Status::Ok, -errno::EACCES), "{}", op.name());
        assert!(body.iter().all(|x| *x == 0));
    }
    // the transport binds the compositor's session
    let mut lg = exec_common::Lines::default();
    w.svc.bind_display_authority(&mut lg, all_caps).unwrap();
    assert!(lg.has(&format!("[I915-DISPLAY authority bound inc={}]", all_caps)));
    let mode = atlas_adoption().adopted().unwrap().mode();
    assert_eq!(w.set_mode(all_caps, 1920, 1080, mode.refresh_millihz), (Status::Ok, 0), "the adopted mode: accepted, nothing changes");
    assert_eq!(w.set_mode(all_caps, 3840, 2160, mode.refresh_millihz), (Status::Ok, -errno::EINVAL), "the panel's native size is not the adopted mode");
    assert!(w.log.has("[I915-DISPLAY mode refused name=MODE-NOT-ADOPTED asked=3840x2160@"), "{:?}", w.log.0);
    // a second session can neither take the binding nor act as the authority
    let other = w.open(0);
    assert_eq!(w.svc.bind_display_authority(&mut lg, other).unwrap_err().name, "DISPLAY-AUTHORITY-HELD");
    assert_eq!(w.set_mode(other, 1920, 1080, mode.refresh_millihz), (Status::Ok, -errno::EACCES));
    assert_eq!(w.svc.bind_display_authority(&mut lg, 0xdead).unwrap_err().name, "DISPLAY-AUTHORITY-NO-SESSION");
    // the authority's session ends: the binding ends with it and the next authority binds afresh
    let (_, s, e, _) = w.call(Opcode::SessionClose, all_caps, &[]);
    assert_eq!((s, e), (Status::Ok, 0));
    assert_eq!(w.svc.display_authority(), None);
    assert!(w.log.has(&format!("[I915-DISPLAY authority ended inc={}]", all_caps)));
    w.svc.bind_display_authority(&mut lg, other).unwrap();
    assert_eq!(w.set_mode(other, 1920, 1080, mode.refresh_millihz), (Status::Ok, 0));
}

#[test]
fn with_nothing_adopted_the_query_says_why_and_no_mode_is_offered() {
    let mut w = world(Adoption::NoReadout);
    let (_, s, e, body) = w.call(Opcode::DisplayQuery, 0, &[]);
    assert_eq!((s, e), (Status::Ok, 0));
    let q = DisplayQueryRep::from_payload(&body).unwrap();
    assert_eq!((q.adopted, q.refusal, q.mode_count), (display::state::NOT_ADOPTED, display::refusal::READOUT_FAILED, 0));
    let inc = w.open(0);
    let mut lg = exec_common::Lines::default();
    w.svc.bind_display_authority(&mut lg, inc).unwrap();
    assert_eq!(w.set_mode(inc, 1920, 1080, 0), (Status::Ok, -errno::ENODEV), "DISPLAY-NOT-ADOPTED");
}
