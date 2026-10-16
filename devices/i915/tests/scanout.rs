// SPDX-License-Identifier: GPL-2.0-only
//! The scanout pair's lifecycle (D17, i915-wire 1.5) on the Pixelbook Go's adopted display, against the REAL surface-replacement
//! planner. The ruling's RED controls are here, each failing at its named invariant: presenting another session's image, rendering into
//! a COMMITTED or SCANNING_OUT image, and a suppressed latch - which must release nothing.
use i915_device::display::scanout::*;
use i915_device::display::Adoption;
use i915_display_modeset_core::adopt::{FirmwareScanout, ADOPT_WRITE_ALLOWLIST};
use i915_display_modeset_core::plane_plan::{LATCH_INTERVAL_US, LATCH_TIMEOUT_US};
use i915_display_modeset_core::ModesetError;
use i915_display_link_core::plan::Effect;
use i915_kbl_display_core::types::Pipe;
use i915_wire::errno;
mod common;
use common::*;

const OWNER: u64 = 7;
const H0: u32 = 0x40;
const H1: u32 = 0x41;
const K0: u64 = 100;
const K1: u64 = 101;
/// 256 KiB aligned (`skl_plane_min_alignment` for linear), past the adopted framebuffer's 0x7e9000.
const G0: u32 = 0x0080_0000;
const G1: u32 = 0x00c0_0000;
/// The firmware's framebuffer address (PLANE_SURF 0 on the Pixelbook Go).
const FW: u32 = 0;

fn pair() -> ScanoutPair {
    let (mut bar, mut cfg, mut lg) = (bar_atlas(), config_page(0x591c, 0x02), Lines::default());
    let ready = i915_device::startup::startup(&mut bar, &mut cfg, &mut lg).unwrap();
    let Adoption::Adopted(a) = ready.adoption else { panic!("the Pixelbook Go is adopted") };
    let live = FirmwareScanout::from_readout(&ready.state, Pipe::A, FW).unwrap();
    ScanoutPair::new(OWNER, live, &a, [(H0, K0, G0), (H1, K1, G1)])
}

/// Present `h` and arm it (its GPU work done); returns the sequence and the armed plan.
fn present_armed(p: &mut ScanoutPair, h: u32) -> (u64, i915_display_modeset_core::adopt::AdoptPlan) {
    p.begin(OWNER, h).unwrap();
    let seq = p.present(OWNER, h).unwrap();
    (seq, p.ready_to_arm().expect("armed"))
}

fn state(p: &ScanoutPair, h: u32) -> ImageState {
    p.images().iter().find(|i| i.handle == h).unwrap().state
}

#[test]
fn a_present_writes_exactly_plane_surf_and_only_after_the_gpu_is_done() {
    let mut p = pair();
    p.begin(OWNER, H0).unwrap();
    let seq = p.present(OWNER, H0).unwrap();
    assert_eq!((seq, state(&p, H0)), (1, ImageState::Committed { seq: 1 }));
    assert_eq!(p.awaiting_gpu(), Some(K0), "the flip waits for the GPU work writing image 0");
    assert!(!p.wants_latch_reads(), "nothing is written, so nothing is read for");
    let plan = p.ready_to_arm().unwrap();
    assert_eq!(plan.written(), ADOPT_WRITE_ALLOWLIST.to_vec(), "the one register the adoption allow-list grants");
    assert_eq!(plan.write[0], Effect::Write { reg: ADOPT_WRITE_ALLOWLIST[0], value: G0 });
    assert_eq!((plan.old_surf, plan.new_surf), (FW, G0));
    assert!(p.wants_latch_reads());
    assert_eq!(p.ready_to_arm(), None, "armed once");
}

#[test]
fn the_firmware_framebuffer_is_kept_until_the_first_replacement_is_proven_latched() {
    let mut p = pair();
    let (seq, _) = present_armed(&mut p, H0);
    assert_eq!(p.observe(FW), LatchStep::StillWaiting, "SURFLIVE still shows the firmware's surface");
    assert!(p.firmware_live());
    assert_eq!(p.status().scanning, 0, "the firmware's framebuffer is on the glass");
    assert_eq!(p.observe(G0), LatchStep::Latched { seq, released: Released::Firmware });
    assert!(!p.firmware_live());
    assert_eq!(state(&p, H0), ImageState::ScanningOut { seq });
    assert_eq!(p.status(), Status { latched_seq: 1, scanning: H0, free_mask: 0b10, latch_timeouts: 0 });
    // the second flip releases image 0, never before image 1 is latched
    let (seq2, plan2) = present_armed(&mut p, H1);
    assert_eq!((plan2.old_surf, plan2.new_surf), (G0, G1), "the planner sees what is live now");
    assert_eq!(p.observe(G0), LatchStep::StillWaiting);
    assert_eq!(state(&p, H0), ImageState::ScanningOut { seq: 1 }, "not released before the latch");
    assert_eq!(p.observe(G1), LatchStep::Latched { seq: seq2, released: Released::Image { handle: H0 } });
    assert_eq!((state(&p, H0), state(&p, H1)), (ImageState::Free, ImageState::ScanningOut { seq: 2 }));
    assert_eq!(p.seq_state(1), SeqState::Latched);
    assert_eq!(p.seq_state(2), SeqState::Latched);
    assert_eq!(p.seq_state(3), SeqState::Unknown);
}

/// RED: present of ANOTHER session's image is refused, and changes nothing.
#[test]
fn presenting_another_sessions_image_is_refused_by_name() {
    let mut p = pair();
    p.begin(OWNER, H0).unwrap();
    let e = p.present(OWNER + 1, H0).unwrap_err();
    assert_eq!(e, ScanoutError::NotYours { inc: OWNER + 1, owner: OWNER });
    assert_eq!((e.name(), e.errno()), ("SCANOUT-NOT-YOURS", errno::EACCES));
    assert_eq!(p.begin(OWNER + 1, H1).unwrap_err().name(), "SCANOUT-NOT-YOURS", "nor begin one");
    assert_eq!(state(&p, H0), ImageState::Rendering, "the owner's image is untouched");
    assert_eq!(p.present(OWNER, 0x99).unwrap_err(), ScanoutError::UnknownImage { handle: 0x99 });
    assert_eq!(p.present(OWNER, H0), Ok(1), "the control: the owner presents it");
}

/// RED: rendering into a COMMITTED or SCANNING_OUT image is refused - by SCANOUT-BEGIN and as a GPU write.
#[test]
fn rendering_into_a_committed_or_scanning_out_image_is_a_reuse_violation() {
    let mut p = pair();
    p.begin(OWNER, H0).unwrap();
    assert_eq!(p.check_render_write(K0), Ok(()), "RENDERING: the GPU may write it");
    let seq = p.present(OWNER, H0).unwrap();
    let committed = ScanoutError::ReuseViolation { handle: H0, state: ImageState::Committed { seq } };
    assert_eq!(p.begin(OWNER, H0), Err(committed));
    assert_eq!(p.check_render_write(K0), Err(committed));
    assert_eq!((committed.name(), committed.errno()), ("SCANOUT-REUSE-VIOLATION", errno::EBUSY));
    assert!(p.display_holds(K0));
    p.ready_to_arm().unwrap();
    assert_eq!(p.observe(G0), LatchStep::Latched { seq, released: Released::Firmware });
    let on_glass = ScanoutError::ReuseViolation { handle: H0, state: ImageState::ScanningOut { seq } };
    assert_eq!(p.begin(OWNER, H0), Err(on_glass));
    assert_eq!(p.check_render_write(K0), Err(on_glass));
    assert!(p.display_holds(K0));
    // the controls: the FREE image and any object that is not an image
    assert_eq!(p.check_render_write(K1), Err(ScanoutError::ReuseViolation { handle: H1, state: ImageState::Free }), "a FREE image is begun first");
    p.begin(OWNER, H1).unwrap();
    assert_eq!(p.check_render_write(K1), Ok(()));
    assert_eq!(p.check_render_write(999), Ok(()), "not an image: the pair has no say");
    assert!(!p.display_holds(K1));
}

/// RED: a suppressed latch (SURFLIVE never shows the new surface) releases NOTHING: the firmware's framebuffer stays live, the image
/// stays COMMITTED, the timeout is counted once, further presents are refused - and a late latch is still recognised.
#[test]
fn a_suppressed_latch_releases_nothing() {
    let mut p = pair();
    let (seq, plan) = present_armed(&mut p, H0);
    let max = plan.latch.max_polls;
    assert_eq!(max, LATCH_TIMEOUT_US / LATCH_INTERVAL_US + 1, "the planner's bound: poll_timeout_us reads once, then once per interval");
    for _ in 1..max {
        assert_eq!(p.observe(FW), LatchStep::StillWaiting);
    }
    assert_eq!(p.observe(FW), LatchStep::TimedOut { seq, got: FW });
    assert!(p.firmware_live(), "the old surface is NOT released");
    assert_eq!(state(&p, H0), ImageState::Committed { seq });
    assert_eq!(p.status(), Status { latched_seq: 0, scanning: 0, free_mask: 0b10, latch_timeouts: 1 });
    assert_eq!(p.seq_state(seq), SeqState::LatchUnconfirmed);
    assert!(p.wants_latch_reads(), "reads go on");
    p.begin(OWNER, H1).unwrap();
    assert_eq!(p.present(OWNER, H1), Err(ScanoutError::FlipInFlight { seq }), "one flip at a time, an unconfirmed one included");
    // a whole further window without the latch: still nothing released, still counted once
    for _ in 0..max {
        assert_eq!(p.observe(FW), LatchStep::StillWaiting);
    }
    assert_eq!(p.status().latch_timeouts, 1);
    assert!(p.firmware_live());
    // the hardware latches after all: recognised, and only now is the firmware's framebuffer released
    assert_eq!(p.observe(G0), LatchStep::Latched { seq, released: Released::Firmware });
    assert_eq!(p.present(OWNER, H1), Ok(seq + 1));
}

#[test]
fn a_present_needs_a_rendering_image_and_a_failed_arm_changes_nothing_on_the_glass() {
    let mut p = pair();
    assert_eq!(p.present(OWNER, H0), Err(ScanoutError::NotRendering { handle: H0, state: ImageState::Free }), "never begun");
    p.begin(OWNER, H0).unwrap();
    let seq = p.present(OWNER, H0).unwrap();
    p.ready_to_arm().unwrap();
    assert_eq!(p.arm_failed(), Some(seq), "the caller's guard refused the write");
    assert_eq!(state(&p, H0), ImageState::Rendering, "back to the authority, nothing on the glass changed");
    assert!(p.firmware_live() && !p.wants_latch_reads());
    assert_eq!(p.observe(G0), LatchStep::Idle);
    assert_eq!(p.present(OWNER, H0), Ok(seq + 1));
}

#[test]
fn an_image_that_is_not_the_adopted_layout_is_refused_by_the_planner() {
    // a misaligned image address: skl_plane_min_alignment for linear is 256 KiB
    let (mut bar, mut cfg, mut lg) = (bar_atlas(), config_page(0x591c, 0x02), Lines::default());
    let ready = i915_device::startup::startup(&mut bar, &mut cfg, &mut lg).unwrap();
    let Adoption::Adopted(a) = ready.adoption else { panic!() };
    let live = FirmwareScanout::from_readout(&ready.state, Pipe::A, FW).unwrap();
    let mut p = ScanoutPair::new(OWNER, live, &a, [(H0, K0, 0x1000), (H1, K1, FW)]);
    p.begin(OWNER, H0).unwrap();
    let e = p.present(OWNER, H0).unwrap_err();
    assert_eq!(e, ScanoutError::Plan(ModesetError::SurfMisaligned { surf: 0x1000, need: 256 * 1024 }));
    assert_eq!((e.name(), e.errno()), ("ADOPT-SURF-MISALIGNED", errno::EINVAL));
    assert_eq!(state(&p, H0), ImageState::Rendering, "a refused present commits nothing");
    p.begin(OWNER, H1).unwrap();
    assert_eq!(p.present(OWNER, H1).unwrap_err(), ScanoutError::Plan(ModesetError::SameSurface), "the surface on the glass is not a flip");
}
