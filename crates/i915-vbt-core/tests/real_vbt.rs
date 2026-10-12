// SPDX-License-Identifier: GPL-2.0-only
//! Parses the REAL Pixelbook Go (Atlas) VBT, extracted from the ChromeOS recovery image's coreboot CBFS
//! ("vbt.bin", LZMA, 4608 bytes; see coral_handoffs/atlas/recovery/configs/coreboot/vbt-decoded.txt). The file is
//! loaded from $I915_ATLAS_VBT or coral_handoffs/atlas/sources/atlas-vbt.bin and the test SKIPS when absent; it is
//! not checked in (Google/coreboot-provided data, provenance kept outside this GPL repo).
//! Expected values are the independent Python decoder's output (coral_handoffs/atlas/sources/atlas-vbt-decode.txt).
use i915_vbt_core::backlight::parse_lfp_backlight;
use i915_vbt_core::child::{AuxCh, Port};
use i915_vbt_core::edp::parse_edp;
use i915_vbt_core::general::*;
use i915_vbt_core::panel::*;
use i915_vbt_core::psr::{parse_psr, PsrPlatform};
use i915_vbt_core::Vbt;

fn load() -> Option<Vec<u8>> {
    let p = std::env::var("I915_ATLAS_VBT")
        .unwrap_or_else(|_| "/home/peter/Projects/coral_handoffs/atlas/sources/atlas-vbt.bin".into());
    std::fs::read(p).ok()
}

#[test]
fn atlas_vbt_matches_the_independent_decoder() {
    let Some(buf) = load() else { eprintln!("SKIP: no Atlas VBT on this host"); return; };
    let vbt = Vbt::parse(&buf).expect("real VBT validates");
    assert_eq!(vbt.version, 221); // "BDB version 221 hdr 22 size 4400"
    assert_eq!(vbt.bdb_header_size, 22);
    assert_eq!(vbt.bdb_size, 4400);
    assert_eq!(vbt.vbt_size, 4449);
    assert!(vbt.bdb_signature_ok);

    // block census "blocks: [(1, 5), (2, 317), (3, 1), (6, 117) ... (254, 234)]"
    // The walk stops at block 51: its 9 data bytes end at 4401 > bdb_size 4400 (the VBT carries 4449 bytes, the
    // Python decoder walked past bdb_size). Linux's find_raw_section() returns NULL there (intel_bios.c:117) and
    // so can never see blocks 51/252/253/254 - harmless, none is read by i915 on this platform. Named, not hidden:
    let mut got: Vec<(u8, usize)> = Vec::new();
    let mut tail = None;
    for b in vbt.blocks() {
        match b { Ok(b) => got.push((b.id, b.len())), Err(e) => tail = Some(e) }
    }
    assert_eq!(tail, Some(i915_vbt_core::VbtError::BlockOverflows { id: 51, size: 9, offset: 4392, bdb_size: 4400 }));
    assert_eq!(vbt.find_block(51), Err(i915_vbt_core::VbtError::BlockOverflows { id: 51, size: 9, offset: 4392, bdb_size: 4400 }));
    let want: Vec<(u8, usize)> = vec![
        (1, 5), (2, 317), (3, 1), (6, 117), (7, 7), (8, 61), (9, 96), (10, 203), (12, 19), (13, 3), (17, 8), (18, 12),
        (20, 158), (26, 2), (27, 748), (28, 72), (31, 92), (32, 32), (40, 26), (41, 148), (42, 1298), (43, 129),
        (44, 22), (46, 304),
    ];
    // (the walk order on the wire is 254,1,253,2,3,252,6,...; the Python census prints sorted by id)
    // 252/253/254 are VBIOS-only/PRD blocks that DO sit inside bdb_size, so they are walked; 51 is the one that overflows.
    got.sort();
    let mut want = want;
    want.extend([(252, 122), (253, 50), (254, 234)]);
    want.sort();
    assert_eq!(got, want);

    let gd = parse_general_definitions(&vbt).unwrap();
    assert_eq!(gd.crt_ddc_pin, 2);
    assert_eq!(gd.child_dev_size, 39);
    assert_eq!(gd.size_mismatch, None);
    assert_eq!(gd.children.len(), 3);
    // child handle=0x0008 type=0x1806 dvo_port=10(DP-A(eDP)) ddc_pin=0x03 aux=0x40(AUX-A)
    let c0 = &gd.children[0];
    assert_eq!((c0.handle(), c0.device_type(), c0.dvo_port(), c0.ddc_pin(), c0.aux_channel()), (0x0008, 0x1806, 10, 3, 0x40));
    assert_eq!(c0.port(), Some(Port::A));
    assert_eq!(c0.aux_ch(), Some(AuxCh::A));
    assert!(c0.supports_edp() && c0.supports_dp());
    assert!(!c0.supports_dvi() && !c0.supports_hdmi());
    assert!(!c0.lane_reversal() && !c0.iboost() && !c0.hpd_invert() && !c0.use_vbt_vswing());
    // child handle=0x0004 type=0x68c6 dvo_port=7(DP-B) aux=0x00 ; handle=0x0040 dvo_port=8 (DP-C)
    let (c1, c2) = (&gd.children[1], &gd.children[2]);
    assert_eq!((c1.handle(), c1.device_type(), c1.dvo_port(), c1.aux_channel()), (0x0004, 0x68c6, 7, 0));
    assert_eq!((c2.handle(), c2.device_type(), c2.dvo_port(), c2.aux_channel()), (0x0040, 0x68c6, 8, 0));
    assert_eq!((c1.port(), c2.port()), (Some(Port::B), Some(Port::C)));
    assert_eq!(c1.aux_ch(), None);
    assert!(c1.supports_dp() && !c1.supports_edp()); // 0x68c6 has no INTERNAL_CONNECTOR bit... (1<<12)=0x1000 not set
    assert_eq!(c0.dp_iboost_level(), 0);
    assert_eq!(c0.hdmi_iboost_level(), 2); // "iboost_level=0x20"

    // LFP options "LFP panel_type 2 panel_type2 0"
    let o = parse_lfp_options(&vbt, 2).unwrap().unwrap();
    assert_eq!((o.panel_type, o.panel_type2), (2, 0));
    let pt = select_panel_type(PanelTypeSources { vbt: vbt_panel_type(&vbt, Some(c0)).unwrap(), ..Default::default() }, false);
    assert_eq!(pt, Some(2));

    // "panel DTD: 1024x768 clock 65000 kHz hblank 320 vblank 38 hso 24 hsw 136 vso 3 vsw 6"
    let p = parse_lfp_panel(&vbt, 2).unwrap();
    let m = p.mode;
    assert_eq!((m.hdisplay, m.vdisplay, m.clock_khz), (1024, 768, 65000));
    assert_eq!((m.htotal - m.hdisplay, m.vtotal - m.vdisplay), (320, 38));
    assert_eq!((m.hsync_start - m.hdisplay, m.hsync_end - m.hsync_start), (24, 136));
    assert_eq!((m.vsync_start - m.vdisplay, m.vsync_end - m.vsync_start), (3, 6));
    // "panel pnp mfg MS_ product 0x3 raw 367f030003000000000c"
    assert_eq!(p.pnp_id, [0x36, 0x7f, 0x03, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x0c]);
    assert_eq!(parse_lfp_data_ptrs(&vbt).unwrap().fp_timing_size, 32 + 6);

    // "eDP power seq (100us units) t1_t3,t8,t9,t10,t11_t12 = (2000, 10, 2000, 500, 5000)"
    let e = parse_edp(&vbt, 2).unwrap().unwrap();
    let s = e.power_seq;
    assert_eq!((s.t1_t3, s.t8, s.t9, s.t10, s.t11_t12), (2000, 10, 2000, 500, 5000));
    // "color depth field 0", "fast_link rate 0 lanes 1 preemph 0 vswing 0", "edp_vswing_preemph bit 0"
    assert_eq!((e.bpp, e.rate, e.lanes, e.preemphasis, e.vswing), (Some(18), Some(162000), Some(2), Some(0), Some(0)));
    assert_eq!(e.low_vswing, Some(true));

    // "backlight entry 4ac8000058aa type 2 active_low 0 ... pwm_hz 200". The Python decoder says "controller 1";
    // the block's backlight_control[16] bytes are all 0x02 and `struct lfp_backlight_control_method` is
    // { u8 type:4; u8 controller:4; } (intel_vbt_defs.h:1313-1316), low nibble first: type 2, controller 0. The
    // decoder was wrong (it is the only disagreement); the bytes settle it.
    let bl = parse_lfp_backlight(&vbt, 2).unwrap().unwrap();
    assert!(bl.present);
    assert_eq!((bl.entry_type, bl.active_low_pwm, bl.pwm_freq_hz), (2, false, 200));
    assert_eq!((bl.control_type, bl.controller), (Some(2), 0));
    assert_eq!(bl.min_brightness, 0);

    // PSR "raw 000002000200": full_link 0, tp1 2, tp2_tp3 2 -> BDB 205+ on display ver 9: 2 == 2500 us
    let psr = parse_psr(&vbt, 2, PsrPlatform::KABYLAKE).unwrap().unwrap();
    assert_eq!((psr.full_link, psr.require_aux_wakeup, psr.idle_frames), (false, false, 0));
    assert_eq!((psr.tp1_wakeup_time_us, psr.tp2_tp3_wakeup_time_us, psr.psr2_tp2_tp3_wakeup_time_us), (2500, 2500, 2500));
}
