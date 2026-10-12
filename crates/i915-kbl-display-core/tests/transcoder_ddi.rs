// SPDX-License-Identifier: GPL-2.0-only
//! TRANS_DDI_FUNC_CTL, TRANSCONF, the eDP input select and DDI_BUF_CTL: every member of every enum-like family is
//! driven through the decode with a LITERAL register word and the exact classification asserted.
use i915_kbl_display_core::ddi::*;
use i915_kbl_display_core::error::ReadoutError;
use i915_kbl_display_core::transcoder::*;
use i915_kbl_display_core::types::*;

const ENABLE: u32 = 0x8000_0000;

#[test]
fn mode_select_family_all_members() {
    // bits 26:24 (intel_display_regs.h:2245-2250); 5, 6, 7 are undefined
    let table: [(u32, Option<DdiMode>); 8] = [
        (0, Some(DdiMode::Hdmi)),
        (1, Some(DdiMode::Dvi)),
        (2, Some(DdiMode::DpSst)),
        (3, Some(DdiMode::DpMst)),
        (4, Some(DdiMode::Fdi)),
        (5, None),
        (6, None),
        (7, None),
    ];
    for (field, want) in table {
        let raw = ENABLE | (field << 24) | 0x2; // pipe-B transcoder with the lane field 1
        let r = decode_func_ctl(Transcoder::B, raw);
        match want {
            Some(m) => assert_eq!(r.unwrap().unwrap().mode, m, "mode field {field}"),
            None => assert_eq!(r, Err(ReadoutError::DdiModeReserved { trans: "B", field }), "field {field}"),
        }
    }
    assert!(DdiMode::DpSst.is_dp() && DdiMode::DpMst.is_dp());
    assert!(!DdiMode::Hdmi.is_dp() && !DdiMode::Dvi.is_dp() && !DdiMode::Fdi.is_dp());
    assert_eq!(
        [DdiMode::Hdmi.name(), DdiMode::Dvi.name(), DdiMode::DpSst.name(), DdiMode::DpMst.name(), DdiMode::Fdi.name()],
        ["HDMI", "DVI", "DP-SST", "DP-MST", "FDI"]
    );
}

#[test]
fn bpc_family_all_members() {
    // bits 22:20: 0 = 8, 1 = 10, 2 = 6, 3 = 12 bpc -> 24/30/18/36 bpp; 4..7 undefined (intel_ddi.c:4122-4135)
    for (field, bpp) in [(0u32, Some(24)), (1, Some(30)), (2, Some(18)), (3, Some(36)), (4, None), (5, None), (6, None), (7, None)] {
        let raw = ENABLE | 0x0200_0000 | (field << 20) | 0x2;
        let r = decode_func_ctl(Transcoder::A, raw);
        match bpp {
            Some(b) => assert_eq!(r.unwrap().unwrap().bpp, b, "bpc field {field}"),
            None => assert_eq!(r, Err(ReadoutError::BpcReserved { trans: "A", field })),
        }
    }
}

#[test]
fn port_select_family_all_members() {
    // bits 30:28 on a pipe transcoder: 0 = none, 1..4 = DDI B..E (TRANS_DDI_SELECT_PORT(x) = x << 28); 5..7 absent
    for (field, want) in [
        (0u32, Ok(None)),
        (1, Ok(Some(Ddi::B))),
        (2, Ok(Some(Ddi::C))),
        (3, Ok(Some(Ddi::D))),
        (4, Ok(Some(Ddi::E))),
        (5, Err(ReadoutError::DdiPortAbsent { trans: "C", field: 5 })),
        (6, Err(ReadoutError::DdiPortAbsent { trans: "C", field: 6 })),
        (7, Err(ReadoutError::DdiPortAbsent { trans: "C", field: 7 })),
    ] {
        let raw = ENABLE | (field << 28) | 0x0200_0002;
        assert_eq!(decode_func_ctl(Transcoder::C, raw).map(|o| o.unwrap().ddi), want, "port field {field}");
    }
    // the eDP transcoder ignores the port bits: always DDI A (intel_display_regs.h:2238)
    for field in 0..8u32 {
        let raw = ENABLE | (field << 28) | 0x0200_0002;
        assert_eq!(decode_func_ctl(Transcoder::Edp, raw).unwrap().unwrap().ddi, Some(Ddi::A), "edp, port bits {field}");
    }
}

#[test]
fn port_width_family_all_members() {
    // DDI_PORT_WIDTH_DECODE (intel_display_regs.h:2369): fields 0..=3 mean 1..=4 lanes, and field 4 means THREE.
    // Encode is (width == 3 ? 4 : width - 1) (:2368): 1 -> 0, 2 -> 1, 3 -> 4, 4 -> 3.
    let table: [(u32, Option<u32>); 8] =
        [(0, Some(1)), (1, Some(2)), (2, Some(3)), (3, Some(4)), (4, Some(3)), (5, None), (6, None), (7, None)];
    for (field, lanes) in table {
        assert_eq!(decode_port_width(field), lanes, "width field {field}");
        let raw = ENABLE | 0x0200_0000 | (field << 1);
        let r = decode_func_ctl(Transcoder::B, raw);
        match lanes {
            Some(n) => assert_eq!(r.unwrap().unwrap().lanes, Some(n)),
            None => assert_eq!(r, Err(ReadoutError::PortWidthReserved { trans: "B", field })),
        }
    }
    // HDMI / DVI carry no DP lane count and no width validation (the field is unused there)
    let h = decode_func_ctl(Transcoder::B, ENABLE | 0x0000_000e).unwrap().unwrap();
    assert_eq!(h.lanes, None);
}

#[test]
fn sync_polarity_bits_are_separate() {
    let none = decode_func_ctl(Transcoder::B, ENABLE | 0x0200_0002).unwrap().unwrap();
    assert!(!none.hsync_positive && !none.vsync_positive);
    let h = decode_func_ctl(Transcoder::B, ENABLE | 0x0200_0002 | 0x0001_0000).unwrap().unwrap();
    assert!(h.hsync_positive && !h.vsync_positive);
    let v = decode_func_ctl(Transcoder::B, ENABLE | 0x0200_0002 | 0x0002_0000).unwrap().unwrap();
    assert!(!v.hsync_positive && v.vsync_positive);
}

#[test]
fn disabled_function_is_none_and_not_validated() {
    // FUNC_ENABLE clear: reserved junk in the other fields is not an error (the transcoder is simply off)
    assert_eq!(decode_func_ctl(Transcoder::A, 0x0000_0000), Ok(None));
    assert_eq!(decode_func_ctl(Transcoder::A, 0x7777_7777), Ok(None));
}

#[test]
fn func_ctl_literal_edp_1080p_two_lane() {
    // eDP, DP-SST, 8 bpc, +hsync +vsync, input A_ON, 2 lanes: 0x8000_0000 | 0x0200_0000 | 0x0003_0000 | 0 | 0x2
    let f = decode_func_ctl(Transcoder::Edp, 0x8203_0002).unwrap().unwrap();
    assert_eq!(f.mode, DdiMode::DpSst);
    assert_eq!(f.ddi, Some(Ddi::A));
    assert_eq!(f.bpp, 24);
    assert_eq!(f.lanes, Some(2));
    assert!(f.hsync_positive && f.vsync_positive);
    assert_eq!(f.raw, 0x8203_0002);
}

#[test]
fn edp_input_family_all_members() {
    // bits 14:12: 0 A_ON, 4 A_ONOFF, 5 B, 6 C, 7 D(absent on KBL); 1, 2, 3 undefined
    let t: [(u32, Result<Pipe, ReadoutError>); 8] = [
        (0, Ok(Pipe::A)),
        (1, Err(ReadoutError::EdpInputReserved { field: 1 })),
        (2, Err(ReadoutError::EdpInputReserved { field: 2 })),
        (3, Err(ReadoutError::EdpInputReserved { field: 3 })),
        (4, Ok(Pipe::A)),
        (5, Ok(Pipe::B)),
        (6, Ok(Pipe::C)),
        (7, Err(ReadoutError::EdpInputPipeAbsent { field: 7 })),
    ];
    for (field, want) in t {
        assert_eq!(edp_input_pipe(ENABLE | (field << 12) | 0x0200_0000), want, "edp input {field}");
    }
    // only A_ONOFF means "panel fitter in the path"
    for field in 0..8u32 {
        assert_eq!(edp_force_thru(field << 12 | ENABLE), field == 4, "force_thru for input {field}");
    }
}

#[test]
fn enabled_transcoder_mask_per_pipe() {
    let edp_a = 0x8203_0002u32; // EDP enabled, input A_ON
    let edp_b = 0x8203_5002u32; // EDP enabled, input B_ONOFF
    let own = 0x9203_0006u32; // a pipe transcoder enabled
    // eDP on A: pipe A's mask is the eDP bit (0x10); pipes B and C see nothing
    assert_eq!(enabled_transcoders(Pipe::A, edp_a, 0), Ok(0x10));
    assert_eq!(enabled_transcoders(Pipe::B, edp_a, 0), Ok(0));
    assert_eq!(enabled_transcoders(Pipe::C, edp_a, 0), Ok(0));
    // eDP on B
    assert_eq!(enabled_transcoders(Pipe::B, edp_b, 0), Ok(0x10));
    assert_eq!(enabled_transcoders(Pipe::A, edp_b, 0), Ok(0));
    // a pipe's own transcoder: bit = 1 << pipe
    assert_eq!(enabled_transcoders(Pipe::A, 0, own), Ok(0x01));
    assert_eq!(enabled_transcoders(Pipe::B, 0, own), Ok(0x02));
    assert_eq!(enabled_transcoders(Pipe::C, 0, own), Ok(0x04));
    // both for one pipe -> both bits (picked as an error below)
    assert_eq!(enabled_transcoders(Pipe::A, edp_a, own), Ok(0x11));
    // eDP function disabled: its input bits are ignored (reserved junk included)
    assert_eq!(enabled_transcoders(Pipe::A, 0x0000_3000, 0), Ok(0));
    // an enabled eDP transcoder with a reserved input is a named error
    assert_eq!(enabled_transcoders(Pipe::A, ENABLE | (2 << 12), 0), Err(ReadoutError::EdpInputReserved { field: 2 }));
}

#[test]
fn pick_transcoder_cases() {
    assert_eq!(pick_transcoder(Pipe::A, 0), Ok(None));
    assert_eq!(pick_transcoder(Pipe::A, 0x10), Ok(Some(Transcoder::Edp)));
    assert_eq!(pick_transcoder(Pipe::A, 0x01), Ok(Some(Transcoder::A)));
    assert_eq!(pick_transcoder(Pipe::B, 0x02), Ok(Some(Transcoder::B)));
    assert_eq!(pick_transcoder(Pipe::C, 0x04), Ok(Some(Transcoder::C)));
    assert_eq!(pick_transcoder(Pipe::A, 0x11), Err(ReadoutError::MultipleTranscoders { mask: 0x11 }));
    // a mask naming another pipe's transcoder cannot come from this pipe
    assert_eq!(pick_transcoder(Pipe::A, 0x02), Err(ReadoutError::MultipleTranscoders { mask: 0x02 }));
    assert_eq!(Transcoder::Edp.name(), "EDP");
    assert_eq!(Transcoder::A.name(), "A");
}

#[test]
fn transconf_decode() {
    let off = decode_transconf(0);
    assert!(!off.enabled && !off.interlaced);
    let on = decode_transconf(0x8000_0000);
    assert!(on.enabled && !on.interlaced);
    let i = decode_transconf(0x8060_0000);
    assert!(i.enabled && i.interlaced);
    // bit 30 is TRANSCONF_STATE_ENABLE (a status), not the enable
    assert!(!decode_transconf(0x4000_0000).enabled);
}

#[test]
fn ddi_buf_ctl_decode() {
    // enabled, 2 lanes, idle clear, trans select 5
    let b = decode_ddi_buf(0x8000_0000 | 0x0500_0000 | 0x2);
    assert!(b.enabled && !b.idle && !b.reversed && !b.a_4_lanes);
    assert_eq!(b.width_lanes, Some(2));
    assert_eq!(b.trans_select, 5);
    // DDI A with 4 lanes: DDI_A_4_LANES and width 3 -> 4
    let a = decode_ddi_buf(0x8000_0016);
    assert!(a.a_4_lanes && a.enabled);
    assert_eq!(a.width_lanes, Some(4));
    // each single-bit flag on its own
    assert!(decode_ddi_buf(0x80).idle && !decode_ddi_buf(0x80).enabled);
    assert!(decode_ddi_buf(0x1_0000).reversed);
    assert!(decode_ddi_buf(0x10).a_4_lanes);
    assert_eq!(decode_ddi_buf(0).width_lanes, Some(1));
    assert_eq!(decode_ddi_buf(0xe).width_lanes, None); // field 7
    assert_eq!(decode_ddi_buf(0x8).width_lanes, Some(3)); // field 4 -> 3 lanes
    assert_eq!(decode_ddi_buf(0x0f00_0000).trans_select, 0xf);
    assert_eq!(decode_ddi_buf(0x0100_0000).trans_select, 1);
    assert!(!decode_ddi_buf(0x7fff_ffff).enabled);
}
