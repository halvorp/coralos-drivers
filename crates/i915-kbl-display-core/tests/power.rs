// SPDX-License-Identifier: GPL-2.0-only
//! Power wells: request/state bit positions per well, the Gen9 BIOS-register OR rule, domain gating, DC state.
use i915_kbl_display_core::power::*;
use i915_kbl_display_core::types::*;

fn state(bios: u32, drv: u32) -> PowerState {
    let mut wells = [decode_well(Well::Pw1, bios, drv); 7];
    for w in Well::ALL {
        wells[w as usize] = decode_well(w, bios, drv);
    }
    PowerState { wells, fuse_status: 0, dc_state: decode_dc_state(0) }
}

#[test]
fn request_and_state_bit_positions_for_every_well() {
    // HSW_PWR_WELL_CTL_REQ(idx) = 0x2 << 2*idx, _STATE(idx) = 0x1 << 2*idx (intel_display_regs.h:2129-2130)
    let wells = [
        (Well::Pw1, 14u32, 0x2000_0000u32, 0x1000_0000u32, "PW1"),
        (Well::Pw2, 15, 0x8000_0000, 0x4000_0000, "PW2"),
        (Well::MiscIo, 0, 0x2, 0x1, "MISC_IO"),
        (Well::DdiAE, 1, 0x8, 0x4, "DDI_A_E"),
        (Well::DdiB, 2, 0x20, 0x10, "DDI_B"),
        (Well::DdiC, 3, 0x80, 0x40, "DDI_C"),
        (Well::DdiD, 4, 0x200, 0x100, "DDI_D"),
    ];
    for (w, idx, req, st, name) in wells {
        assert_eq!(w.idx(), idx, "{name} idx");
        assert_eq!(w.name(), name);
        assert_eq!(well_req(idx), req, "{name} req");
        assert_eq!(well_state(idx), st, "{name} state");
    }
    assert_eq!(fuse_pg_dist_status(0), 1 << 27);
    assert_eq!(fuse_pg_dist_status(1), 1 << 26);
    assert_eq!(fuse_pg_dist_status(2), 1 << 25);
}

#[test]
fn a_well_needs_both_request_and_state() {
    // PW2 in the DRIVER register (CTL2): req 0x8000_0000 | state 0x4000_0000
    for (drv, want) in [(0xc000_0000u32, true), (0x8000_0000, false), (0x4000_0000, false), (0, false)] {
        let s = decode_well(Well::Pw2, 0, drv);
        assert_eq!(s.enabled, want, "driver {drv:#x}");
        assert_eq!((s.drv_req, s.drv_state), (drv & 0x8000_0000 != 0, drv & 0x4000_0000 != 0));
    }
    // PW2's BIOS register bits do NOT count (only PW1 and MISC_IO get the OR, intel_display_power_well.c:634-636)
    assert!(!decode_well(Well::Pw2, 0xc000_0000, 0).enabled);
    assert!(!decode_well(Well::DdiB, 0x30, 0).enabled);
}

#[test]
fn pw1_and_misc_io_count_the_bios_register_on_gen9() {
    // intel_display_power_well.c:634-636: the DMC bug leaves the driver's PW1/MISC_IO request bits unrestored
    let pw1 = decode_well(Well::Pw1, 0x3000_0000, 0);
    assert!(pw1.enabled && pw1.bios_req && pw1.bios_state && !pw1.drv_req);
    let io = decode_well(Well::MiscIo, 0x3, 0);
    assert!(io.enabled);
    // ... split across the two registers also counts: request from one, state from the other
    assert!(decode_well(Well::Pw1, 0x2000_0000, 0x1000_0000).enabled);
    // neither register: off
    assert!(!decode_well(Well::Pw1, 0, 0).enabled);
    // the same OR does not apply to the DDI wells
    assert!(!decode_well(Well::DdiAE, 0xc, 0).enabled);
    assert!(decode_well(Well::DdiAE, 0, 0xc).enabled);
}

#[test]
fn bits_of_other_wells_are_ignored() {
    // every bit set except PW2's pair: PW2 reads off, everything else on
    let all_but_pw2 = !0xc000_0000u32;
    assert!(!decode_well(Well::Pw2, 0, all_but_pw2).enabled);
    for w in [Well::Pw1, Well::MiscIo, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD] {
        assert!(decode_well(w, 0, all_but_pw2).enabled, "{w:?}");
    }
}

#[test]
fn domain_gating_follows_pw2() {
    let on = state(0x3000_0003, 0xc000_000c);
    let off = state(0x3000_0003, 0x0000_000c); // PW1 on, PW2 off
    for p in Pipe::ALL {
        assert_eq!(on.pipe_powered(p), true);
    }
    assert!(off.pipe_powered(Pipe::A)); // pipe A's block is in the always-on domain
    assert!(!off.pipe_powered(Pipe::B));
    assert!(!off.pipe_powered(Pipe::C));
    assert!(on.transcoder_powered(Transcoder::Edp) && off.transcoder_powered(Transcoder::Edp));
    for t in [Transcoder::A, Transcoder::B, Transcoder::C] {
        assert!(on.transcoder_powered(t));
        assert!(!off.transcoder_powered(t), "{t:?}");
    }
    assert!(off.ddi_lanes_powered(Ddi::A));
    for d in [Ddi::B, Ddi::C, Ddi::D, Ddi::E] {
        assert!(on.ddi_lanes_powered(d));
        assert!(!off.ddi_lanes_powered(d), "{d:?}");
    }
}

#[test]
fn well_for_ddi() {
    assert_eq!(Well::for_ddi(Ddi::A), Well::DdiAE);
    assert_eq!(Well::for_ddi(Ddi::E), Well::DdiAE);
    assert_eq!(Well::for_ddi(Ddi::B), Well::DdiB);
    assert_eq!(Well::for_ddi(Ddi::C), Well::DdiC);
    assert_eq!(Well::for_ddi(Ddi::D), Well::DdiD);
}

#[test]
fn dc_state_family() {
    // DC_STATE_EN (intel_display_regs.h:2826-2829): 0 off, 1 up to DC5, 2 up to DC6, 3 undefined; DC9 is bit 3
    assert_eq!(decode_dc_state(0).state, DcState::Off);
    assert_eq!(decode_dc_state(1).state, DcState::UpToDc5);
    assert_eq!(decode_dc_state(2).state, DcState::UpToDc6);
    assert_eq!(decode_dc_state(3).state, DcState::Reserved);
    assert!(decode_dc_state(8).dc9 && decode_dc_state(8).state == DcState::Off);
    assert!(!decode_dc_state(7).dc9);
    assert_eq!(decode_dc_state(0x4000_0001).state, DcState::UpToDc5); // DC3CO (bit 30) is not a DC5/DC6 selector
}

#[test]
fn fuse_download_flag() {
    let mut s = state(0, 0);
    s.fuse_status = 0x8e00_0000;
    assert!(s.fuse_download_done());
    s.fuse_status = 0x0e00_0000;
    assert!(!s.fuse_download_done());
}

#[test]
fn each_register_reports_only_its_own_bits() {
    for w in Well::ALL {
        let idx = w.idx();
        let (req, st) = (0x2u32 << (idx * 2), 0x1u32 << (idx * 2));
        // BIOS register carrying only the request bit
        let s = decode_well(w, req, 0);
        assert_eq!((s.bios_req, s.bios_state, s.drv_req, s.drv_state), (true, false, false, false), "{w:?} bios req");
        let s = decode_well(w, st, 0);
        assert_eq!((s.bios_req, s.bios_state, s.drv_req, s.drv_state), (false, true, false, false), "{w:?} bios state");
        // driver register likewise
        let s = decode_well(w, 0, req);
        assert_eq!((s.bios_req, s.bios_state, s.drv_req, s.drv_state), (false, false, true, false), "{w:?} drv req");
        let s = decode_well(w, 0, st);
        assert_eq!((s.bios_req, s.bios_state, s.drv_req, s.drv_state), (false, false, false, true), "{w:?} drv state");
        // a different well's bits in either register are invisible to this one
        let other = if idx == 0 { 0x3u32 << 2 } else { 0x3 };
        let s = decode_well(w, other, other);
        assert_eq!((s.bios_req, s.bios_state, s.drv_req, s.drv_state, s.enabled), (false, false, false, false, false), "{w:?} foreign bits");
    }
}
