// SPDX-License-Identifier: GPL-2.0-only
//! P4 adoption: the write set is exactly `PLANE_SURF` of PLANE_1 on pipe A, the layout must be unchanged, the old buffer is released
//! only after `PLANE_SURFLIVE` shows the new address, and every wait is bounded.
mod common;
use common::Sim;
use coralos_hal::Mmio;
use i915_common_core::{Reg, WriteGuard};
use i915_display_link_core::plan::Effect;
use i915_display_modeset_core::adopt::*;
use i915_kbl_display_core::plane::{PlaneFormat, Rotation, Tiling};
use i915_kbl_display_core::readout::read_display_state;
use i915_kbl_display_core::types::Pipe;
use i915_kbl_display_core::KBL_DISPLAY;
use std::collections::BTreeMap;

/// The firmware (GOP) state of the Pixelbook Go: the readout test's "vector A" - eDP 1080p60 on pipe A, the primary plane
/// XRGB8888 X-tiled at GGTT 0x100000, stride 15 tiles; PLANE_SURF reads 0x00100005 (low bits set by the firmware).
fn vector_a() -> BTreeMap<u32, u32> {
    let mut r = BTreeMap::new();
    for (k, v) in [
        (0x45400u32, 0x3000_0003u32),
        (0x45404, 0xc000_000c),
        (0x42000, 0x8e00_0000),
        (0x45504, 0),
        (0x46010, 0xc000_0000),
        (0x6c058, 3),
        (0x6c05c, 1),
        (0x6c060, 1),
        (0x46000, 0x382),
        (0x6f400, 0x8203_0002),
        (0x7f008, 0x8000_0000),
        (0x64000, 0x8000_0002),
        (0x6f000, 0x0897_077f),
        (0x6f004, 0x0897_077f),
        (0x6f008, 0x0803_07d7),
        (0x6f00c, 0x0464_0437),
        (0x6f010, 0x0464_0437),
        (0x6f014, 0x0440_043b),
        (0x6001c, 0x077f_0437),
        (0x6f030, 0x7e69_9999),
        (0x6f034, 0x0080_0000),
        (0x6f040, 288_358),
        (0x6f044, 0x0008_0000),
        (0x70180, 0x8400_0400),
        (0x70188, 15),
        (0x70190, 0x0437_077f),
        (0x7019c, 0x0010_0005),
    ] {
        r.insert(k, v);
    }
    r
}

fn scanout() -> FirmwareScanout {
    let regs = vector_a();
    let mut rd = |a: u32| regs.get(&a).copied().unwrap_or(0);
    let state = read_display_state(&mut rd, &KBL_DISPLAY);
    FirmwareScanout::from_readout(&state, Pipe::A, 0x0010_0005).unwrap()
}

fn new_surface() -> ReplacementSurface {
    ReplacementSurface {
        gtt_addr: 0x0020_0000,
        format: PlaneFormat::Xrgb8888,
        rgb_order: false,
        alpha: false,
        tiling: Tiling::X,
        stride_bytes: 7680,
        width: 1920,
        height: 1080,
        offset_x: 0,
        offset_y: 0,
        rotation: Rotation::R0,
    }
}

#[test]
fn readout_describes_the_firmware_scanout() {
    let fw = scanout();
    assert_eq!(fw.pipe, Pipe::A);
    assert!(fw.plane.enabled);
    assert_eq!((fw.plane.surf, fw.plane.stride_bytes, fw.plane.width, fw.plane.height), (0x0010_0000, Some(7680), 1920, 1080));
    assert_eq!((fw.plane.format, fw.plane.tiling), (Some(PlaneFormat::Xrgb8888), Some(Tiling::X)));
}

#[test]
fn the_write_set_is_exactly_plane_surf() {
    assert_eq!(ADOPT_WRITE_ALLOWLIST.len(), 1);
    assert_eq!(ADOPT_WRITE_ALLOWLIST[0].offset(), 0x7019c, "PLANE_SURF(PIPE_A, PLANE_1)");
    let p = plan_surface_replacement(&scanout(), &new_surface()).unwrap();
    assert_eq!(p.written(), ADOPT_WRITE_ALLOWLIST.to_vec());
    assert_eq!(p.write, vec![Effect::Write { reg: Reg::new(0x7019c), value: 0x0020_0000 }, Effect::PostingRead { reg: Reg::new(0x7019c) }]);
    // The rollback writes the RAW register the firmware left (low bits included) and is inside the same allow-list.
    assert_eq!(p.rollback, vec![Effect::Write { reg: Reg::new(0x7019c), value: 0x0010_0005 }, Effect::PostingRead { reg: Reg::new(0x7019c) }]);
    assert_eq!(i915_display_link_core::plan::written_regs(&p.rollback), ADOPT_WRITE_ALLOWLIST.to_vec());
    assert_eq!((p.old_surf, p.new_surf), (0x0010_0000, 0x0020_0000));
}

/// A minimal `Mmio` over a `Sim`, so the plan can be driven through the real `WriteGuard`.
struct SimMmio(Sim);
impl Mmio for SimMmio {
    fn r8(&mut self, _: u32) -> u8 {
        0
    }
    fn r16(&mut self, _: u32) -> u16 {
        0
    }
    fn r32(&mut self, reg: u32) -> u32 {
        self.0.get(reg)
    }
    fn w8(&mut self, _: u32, _: u8) {}
    fn w16(&mut self, _: u32, _: u16) {}
    fn w32(&mut self, reg: u32, v: u32) {
        self.0.put(reg, v)
    }
}

fn apply(plan: &[Effect], g: &mut impl Mmio) {
    for e in plan {
        match e {
            Effect::Write { reg, value } => g.w32(reg.offset(), *value),
            Effect::PostingRead { reg } => {
                g.r32(reg.offset());
            }
            _ => panic!("adoption plans contain only writes and posting reads"),
        }
    }
}

#[test]
fn the_real_write_guard_accepts_the_plan_and_refuses_anything_else() {
    static ALLOW: [Reg; 1] = ADOPT_WRITE_ALLOWLIST;
    let p = plan_surface_replacement(&scanout(), &new_surface()).unwrap();
    let mut g = WriteGuard::new(SimMmio(Sim::default()), &ALLOW);
    apply(&p.write, &mut g);
    apply(&p.rollback, &mut g);
    assert_eq!((g.refusals(), g.writes()), (0, 2));
    assert!(g.check().is_ok());
    // Negative control: the guard really refuses a neighbour (PLANE_CTL) - so "0 refusals" above means something.
    g.w32(0x70180, 0);
    g.w32(0x70188, 0);
    assert_eq!(g.refusals(), 2);
    assert_eq!(g.first_refusal().unwrap().offset, 0x70180);
}

#[test]
fn only_pipe_a_plane_1_is_in_the_allow_list() {
    let mut fw = scanout();
    fw.pipe = Pipe::B;
    assert_eq!(plan_surface_replacement(&fw, &new_surface()).unwrap_err().name(), "ADOPT-PLANE-NOT-IN-ALLOW-LIST");
    let mut fw = scanout();
    fw.plane.plane_id = 1;
    assert_eq!(plan_surface_replacement(&fw, &new_surface()).unwrap_err().name(), "ADOPT-PLANE-NOT-IN-ALLOW-LIST");
}

#[test]
fn a_disabled_plane_has_nothing_to_replace() {
    let mut fw = scanout();
    fw.plane.enabled = false;
    assert_eq!(plan_surface_replacement(&fw, &new_surface()).unwrap_err().name(), "ADOPT-PLANE-DISABLED");
}

#[test]
fn any_layout_difference_is_refused_and_named() {
    let fw = scanout();
    let cases: Vec<(&str, Box<dyn Fn(&mut ReplacementSurface)>)> = vec![
        ("format", Box::new(|n| n.format = PlaneFormat::Rgb565)),
        ("format", Box::new(|n| n.rgb_order = true)),
        ("format", Box::new(|n| n.alpha = true)),
        ("tiling", Box::new(|n| n.tiling = Tiling::Linear)),
        ("tiling", Box::new(|n| n.tiling = Tiling::Y)),
        ("rotation", Box::new(|n| n.rotation = Rotation::R180)),
        ("stride", Box::new(|n| n.stride_bytes = 8192)),
        ("size", Box::new(|n| n.width = 1280)),
        ("size", Box::new(|n| n.height = 720)),
        ("offset", Box::new(|n| n.offset_x = 8)),
        ("offset", Box::new(|n| n.offset_y = 8)),
    ];
    for (field, mutate) in cases {
        let mut n = new_surface();
        mutate(&mut n);
        let e = plan_surface_replacement(&fw, &n).unwrap_err();
        assert_eq!(e.name(), "ADOPT-LAYOUT-DIFFERS");
        assert_eq!(e, i915_display_modeset_core::ModesetError::LayoutDiffers { field }, "{field}");
    }
    // render compression in the firmware's plane is a layout this port does not replace
    let mut fw2 = fw;
    fw2.plane.render_decompression = true;
    assert_eq!(plan_surface_replacement(&fw2, &new_surface()).unwrap_err().name(), "ADOPT-LAYOUT-DIFFERS");
}

#[test]
fn address_rules() {
    let fw = scanout();
    let mut n = new_surface();
    n.gtt_addr = fw.plane.surf; // the surface already scanned out
    assert_eq!(plan_surface_replacement(&fw, &n).unwrap_err().name(), "ADOPT-SAME-SURFACE");
    for bad in [0x0020_1000u32, 0x0020_0800, 0x0021_0000, 0x0022_0000 + 0x1000] {
        let mut n = new_surface();
        n.gtt_addr = bad;
        assert_eq!(plan_surface_replacement(&fw, &n).unwrap_err().name(), "ADOPT-SURF-MISALIGNED", "{bad:#x}");
    }
    // Y tiling would need 1 MiB: the firmware's plane is X tiled, so a Y claim is a layout difference first.
    let mut n = new_surface();
    n.gtt_addr = 0x0030_0000;
    assert!(plan_surface_replacement(&fw, &n).is_ok(), "256 KiB alignment is enough for X tiling");
}

#[test]
fn the_old_buffer_is_released_only_after_surflive_shows_the_new_address() {
    let p = plan_surface_replacement(&scanout(), &new_surface()).unwrap();
    let mut s = AdoptSession::begin(&p);
    assert_eq!(s.release_old_buffer().unwrap_err().name(), "ADOPT-OLD-BUFFER-STILL-LIVE");
    // the hardware keeps showing the old surface for two reads (the write is armed, not yet latched)
    assert!(s.observe(0x0010_0000));
    assert_eq!(s.release_old_buffer().unwrap_err().name(), "ADOPT-OLD-BUFFER-STILL-LIVE");
    assert!(s.observe(0x0010_0000));
    // low bits of SURFLIVE are not address bits
    assert!(!s.observe(0x0020_0005));
    assert_eq!(s.state(), LatchState::Confirmed { polls: 3 });
    assert_eq!(s.release_old_buffer().unwrap(), OldBufferRelease { old_surf: 0x0010_0000 });
    // once decided, further samples change nothing
    assert!(!s.observe(0));
    assert_eq!(s.state(), LatchState::Confirmed { polls: 3 });
}

#[test]
fn a_latch_that_never_happens_stops_at_the_bound_and_keeps_the_old_buffer() {
    let p = plan_surface_replacement(&scanout(), &new_surface()).unwrap();
    assert_eq!(p.latch.max_polls, 100_000 / 500 + 1);
    let mut s = AdoptSession::begin(&p);
    let mut reads = 0u32;
    while s.observe(0x0010_0000) {
        reads += 1;
        assert!(reads < 1_000_000, "the latch wait is unbounded");
    }
    assert_eq!(reads + 1, p.latch.max_polls, "exactly max_polls samples were taken");
    assert!(matches!(s.state(), LatchState::TimedOut(_)));
    assert_eq!(s.release_old_buffer().unwrap_err().name(), "MODESET-WAIT-TIMED-OUT");
    // even a late 'success' sample cannot resurrect a timed-out session
    assert!(!s.observe(0x0020_0000));
    assert!(s.release_old_buffer().is_err());
}

#[test]
fn from_readout_refuses_an_inactive_pipe() {
    let regs = BTreeMap::from([(0x45400u32, 0x3000_0003u32), (0x45404, 0xc000_000c), (0x42000, 0x8e00_0000)]);
    let mut rd = |a: u32| regs.get(&a).copied().unwrap_or(0);
    let state = read_display_state(&mut rd, &KBL_DISPLAY);
    assert_eq!(FirmwareScanout::from_readout(&state, Pipe::A, 0).unwrap_err().name(), "MODESET-READOUT-MISMATCH");
}
