// SPDX-License-Identifier: GPL-2.0-only
mod common;
use common::*;
use i915_vbt_core::child::ChildDevice;
use i915_vbt_core::panel::*;
use i915_vbt_core::{Vbt, VbtError};

#[test]
fn layout_constants() {
    assert_eq!(EDID_DTD_SIZE, 18); // intel_vbt_defs.h:55-90
    assert_eq!(EDID_PNP_ID_SIZE, 10); // :43-49
    assert_eq!(EDID_PRODUCT_NAME_SIZE, 13); // :51-53
    assert_eq!(PANEL_COUNT, 16);
    assert_eq!(LFP_DATA_PTRS_SIZE, 148); // 1 + 16*9 + 3, :1237 (real block 41 is 148 bytes)
}

#[test]
fn panel_bits_vectors() {
    // intel_bios.c:781-789
    assert_eq!(panel_bits(0b1110_0100, 0, 2), 0);
    assert_eq!(panel_bits(0b1110_0100, 1, 2), 1);
    assert_eq!(panel_bits(0b1110_0100, 2, 2), 2);
    assert_eq!(panel_bits(0b1110_0100, 3, 2), 3);
    assert_eq!(panel_bits(0xffff_ffff, 15, 2), 3);
    assert_eq!(panel_bits(0x8000, 15, 1), 1);
    assert!(panel_bool(0b100, 2) && !panel_bool(0b100, 1));
}

#[test]
fn dtd_decode_vector() {
    // hand-encoded 1024x768 (common.rs); expected = the fields written out literally
    let m = decode_dtd(&dtd_1024(0x18));
    assert_eq!(m.clock_khz, 65000); // 0x1964 = 6500 * 10
    assert_eq!((m.hdisplay, m.hsync_start, m.hsync_end, m.htotal), (1024, 1048, 1184, 1344));
    assert_eq!((m.vdisplay, m.vsync_start, m.vsync_end, m.vtotal), (768, 771, 777, 806));
    assert!(!m.hsync_positive && !m.vsync_positive); // flags 0x18: digital only
    assert_eq!((m.width_mm, m.height_mm), (300, 200));
}

#[test]
fn dtd_polarity_bits_are_independent() {
    // byte 17: vsync_positive bit5, hsync_positive bit6 (intel_vbt_defs.h:84-89)
    let a = decode_dtd(&dtd_1024(0x18 | 0x20));
    assert!(a.vsync_positive && !a.hsync_positive);
    let b = decode_dtd(&dtd_1024(0x18 | 0x40));
    assert!(b.hsync_positive && !b.vsync_positive);
    let c = decode_dtd(&dtd_1024(0xff));
    assert!(c.hsync_positive && c.vsync_positive);
}

#[test]
fn dtd_high_bits_land_in_the_right_fields() {
    // every hi/lo nibble pair of the bitfield layout, one at a time (intel_vbt_defs.h:55-90)
    let base = [0u8; 18];
    let with = |i: usize, v: u8| { let mut d = base; d[i] = v; decode_dtd(&d) };
    assert_eq!(with(2, 0xff).hdisplay, 0xff); // hactive_lo
    assert_eq!(with(4, 0x30).hdisplay, 0x300); // hactive_hi = high nibble of byte 4
    assert_eq!(with(3, 0x77).htotal - with(3, 0x77).hdisplay, 0x77); // hblank_lo
    assert_eq!(with(4, 0x03).htotal, 0x300); // hblank_hi = low nibble of byte 4 (hdisplay 0)
    assert_eq!(with(5, 0xff).vdisplay, 0xff);
    assert_eq!(with(7, 0x30).vdisplay, 0x300);
    assert_eq!(with(6, 0x55).vtotal, 0x55);
    assert_eq!(with(7, 0x02).vtotal, 0x200);
    // sync fields: hsync_off, hsync_pulse (clamped by htotal, so give a big hblank)
    let mut d = base; d[4] = 0x0f; d[3] = 0xff; // htotal 0xfff
    let mut e = d; e[8] = 0x11; assert_eq!(decode_dtd(&e).hsync_start, 0x11);
    let mut e = d; e[11] = 0x40; assert_eq!(decode_dtd(&e).hsync_start, 0x100); // hsync_off_hi bits 6-7 << 8
    let mut e = d; e[9] = 0x22; assert_eq!(decode_dtd(&e).hsync_end, 0x22);
    let mut e = d; e[11] = 0x10; assert_eq!(decode_dtd(&e).hsync_end, 0x100); // hsync_pulse_width_hi bits 4-5
    let mut d = base; d[7] = 0x0f; d[6] = 0xff; // vtotal 0xfff
    let mut e = d; e[10] = 0x70; assert_eq!(decode_dtd(&e).vsync_start, 7); // vsync_off_lo high nibble of byte 10
    let mut e = d; e[11] = 0x04; assert_eq!(decode_dtd(&e).vsync_start, 0x10); // vsync_off_hi bits 2-3 << 4
    let mut e = d; e[10] = 0x05; assert_eq!(decode_dtd(&e).vsync_end, 5); // vsync_pulse_width_lo low nibble
    let mut e = d; e[11] = 0x01; assert_eq!(decode_dtd(&e).vsync_end, 0x10); // vsync_pulse_width_hi bits 0-1 << 4
    assert_eq!(with(12, 0xab).width_mm, 0xab);
    assert_eq!(with(14, 0x30).width_mm, 0x300); // himage_hi high nibble of byte 14
    assert_eq!(with(13, 0xcd).height_mm, 0xcd);
    assert_eq!(with(14, 0x03).height_mm, 0x300);
    assert_eq!(with(1, 0x01).clock_khz, 2560); // 0x0100 * 10
}

#[test]
fn dtd_bogus_sync_end_is_clamped() {
    // "Some VBTs have bogus h/vsync_end values": reduce to htotal/vtotal (intel_bios.c:566-577)
    let mut d = [0u8; 18];
    d[2] = 100; d[3] = 10; // hdisplay 100, htotal 110
    d[5] = 50; d[6] = 5; // vdisplay 50, vtotal 55
    d[8] = 5; d[9] = 200; // hsync_end 305 > 110
    d[10] = 0x0f | 0x30; // vsync_off 3, vsync_pw 15 -> 50+3+15 = 68 > 55
    let m = decode_dtd(&d);
    assert_eq!((m.hsync_start, m.hsync_end, m.htotal), (105, 110, 110));
    assert_eq!((m.vsync_start, m.vsync_end, m.vtotal), (53, 55, 55));
}

fn opts_block(panel: u8, panel2: u8, bits: u8, len: usize, dps: u32) -> Vec<u8> {
    let mut d = vec![0u8; len];
    d[0] = panel; d[1] = panel2; d[2] = bits;
    if len >= 20 { d[16..20].copy_from_slice(&dps.to_le_bytes()); }
    d
}

#[test]
fn lfp_options_decode() {
    let mk = |blk: Vec<u8>, pt: u8| { let b = vbt(221, &[(40, blk)]); parse_lfp_options(&Vbt::parse(&b).unwrap(), pt).unwrap().unwrap() };
    let o = mk(opts_block(2, 7, 0x20, 26, 0), 2);
    assert_eq!((o.panel_type, o.panel_type2, o.lvds_dither), (2, 7, true)); // pixel_dither = byte 2 bit 5
    assert!(!mk(opts_block(2, 7, 0xdf, 26, 0), 2).lvds_dither);
    // short blocks (< 16 bytes) skip the DRRS decode (intel_bios.c:807-812): 4, 14
    assert_eq!(mk(opts_block(2, 0, 0, 4, 0), 2).drrs_type, None);
    assert_eq!(mk(opts_block(2, 0, 0, 14, 0), 2).drrs_type, None);
    // dps_panel_type_bits: 2 bits per panel: 0 static, 2 seamless, 1/3 none (intel_bios.c:814-835)
    let dps = 0b00_11_10_01_00u32; // panel0=0 panel1=1 panel2=2 panel3=3
    assert_eq!(mk(opts_block(0, 0, 0, 26, dps), 0).drrs_type, Some(DrrsType::Static));
    assert_eq!(mk(opts_block(0, 0, 0, 26, dps), 1).drrs_type, Some(DrrsType::None));
    assert_eq!(mk(opts_block(0, 0, 0, 26, dps), 2).drrs_type, Some(DrrsType::Seamless));
    assert_eq!(mk(opts_block(0, 0, 0, 26, dps), 3).drrs_type, Some(DrrsType::None));
    // absent / too small
    let b = vbt(221, &[(1, vec![0; 5])]);
    assert_eq!(parse_lfp_options(&Vbt::parse(&b).unwrap(), 0), Ok(None));
    let b = vbt(221, &[(40, vec![0; 3])]);
    assert!(matches!(parse_lfp_options(&Vbt::parse(&b).unwrap(), 0), Err(VbtError::BlockTooSmall { id: 40, .. })));
}

#[test]
fn panel_type_sources_and_ladder() {
    // vbt_get_panel_type (intel_bios.c:632-656)
    let mk = |blk: Vec<u8>| vbt(221, &[(40, blk)]);
    let lfp1 = ChildDevice::from_bytes(221, &child(39, &[(0, 0x08), (2, 6)]));
    let lfp2 = ChildDevice::from_bytes(221, &child(39, &[(0, 0x80), (2, 6)]));
    let b = mk(opts_block(3, 9, 0, 26, 0));
    let v = Vbt::parse(&b).unwrap();
    assert_eq!(vbt_panel_type(&v, Some(&lfp1)).unwrap(), Some(3));
    assert_eq!(vbt_panel_type(&v, Some(&lfp2)).unwrap(), Some(9));
    assert_eq!(vbt_panel_type(&v, None).unwrap(), Some(3));
    // 0x10..0xfe invalid -> none; 0xff and 0x0f valid
    for (pt, want) in [(0x0fu8, Some(0x0f)), (0x10, None), (0xfe, None), (0xff, Some(0xff)), (0, Some(0))] {
        let b = mk(opts_block(pt, 0, 0, 26, 0));
        assert_eq!(vbt_panel_type(&Vbt::parse(&b).unwrap(), None).unwrap(), want, "{pt:#x}");
    }
    let b = vbt(221, &[(1, vec![0; 5])]);
    assert_eq!(vbt_panel_type(&Vbt::parse(&b).unwrap(), None).unwrap(), None);

    // get_panel_type ladder (intel_bios.c:764-772)
    let s = |o, v, p| PanelTypeSources { opregion: o, vbt: v, pnpid: p };
    assert_eq!(select_panel_type(s(Some(5), Some(2), Some(7)), false), Some(5)); // OpRegion wins
    assert_eq!(select_panel_type(s(None, Some(0xff), Some(7)), false), Some(7)); // VBT 0xff defers to PNPID
    assert_eq!(select_panel_type(s(None, Some(0xff), None), false), None); // ... but without PNPID: fallback only
    assert_eq!(select_panel_type(s(None, Some(0xff), None), true), Some(0));
    assert_eq!(select_panel_type(s(None, Some(2), Some(7)), false), Some(2)); // VBT beats PNPID
    assert_eq!(select_panel_type(s(None, None, Some(7)), true), Some(0)); // no VBT value: fallback (not PNPID)
    assert_eq!(select_panel_type(s(None, None, None), false), None);
}

#[test]
fn lfp_data_pointer_table_and_panel() {
    let mut lfp = Lfp::new();
    lfp.dtds[3] = { let mut d = dtd_1024(0x18 | 0x60); d[0] = 0x00; d[1] = 0x20; d }; // panel 3: clock 8192*10
    lfp.pnp[3] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let b = vbt(221, &lfp.blocks());
    let v = Vbt::parse(&b).unwrap();
    let p = parse_lfp_data_ptrs(&v).unwrap();
    assert_eq!(p.fp_timing_size, 38); // 32 + the 6 unaccounted bytes (intel_bios.c:277-282)
    assert_eq!(p.fp_timing[0].offset, 0);
    assert_eq!(p.dvo_timing[0].offset, 38);
    assert_eq!(p.pnp_id[0].offset, 56);
    assert_eq!(p.fp_timing[1].offset, 66);
    assert_eq!(p.panel_name, DataPtr { offset: 16 * 66, size: 13 });
    let panel = parse_lfp_panel(&v, 3).unwrap();
    assert_eq!(panel.mode.clock_khz, 81920);
    assert!(panel.mode.hsync_positive && panel.mode.vsync_positive);
    assert_eq!(panel.pnp_id, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!((panel.fp_x_res, panel.fp_y_res), (1024, 768));
    assert_eq!(panel.bios_lvds_val, Some(0x1000_0003)); // fp_timing.lvds_reg_val @8 of entry 3
    assert_eq!(panel.panel_name.unwrap()[0], b'D');
    assert_eq!(parse_lfp_panel(&v, 0).unwrap().panel_name.unwrap()[0], b'A');
    assert_eq!(parse_lfp_panel(&v, 16), Err(VbtError::PanelTypeOutOfRange { panel_type: 16 }));
    assert_eq!(all_dtds(&v).unwrap().len(), 16);
}

#[test]
fn lfp_x_y_mismatch_drops_bios_lvds_val() {
    // intel_bios.c:870-876: only trusted when fp_timing x/y equal the DTD's
    let mut lfp = Lfp::new();
    lfp.dtds[0] = { let mut d = dtd_1024(0x18); d[2] = 0x80; d }; // hactive 0x480 != 1024
    let b = vbt(221, &lfp.blocks());
    assert_eq!(parse_lfp_panel(&Vbt::parse(&b).unwrap(), 0).unwrap().bios_lvds_val, None);
}

#[test]
fn lfp_without_name_table_and_with_unpadded_fp() {
    let mut lfp = Lfp::new();
    lfp.fp_pad = 0; // old VBT: fp_timing is exactly its table_size
    lfp.fp_table_size = 38;
    lfp.name_size = 0;
    let b = vbt(221, &lfp.blocks());
    let v = Vbt::parse(&b).unwrap();
    let p = parse_lfp_data_ptrs(&v).unwrap();
    assert_eq!(p.fp_timing_size, 38);
    assert_eq!(parse_lfp_panel(&v, 1).unwrap().panel_name, None);
}

fn malformed(b: Vec<u8>, check: &str) {
    let v = Vbt::parse(&b).unwrap();
    match parse_lfp_data_ptrs(&v) {
        Err(VbtError::MalformedLfpDataPtrs { check: c }) => assert!(c.contains(check), "got {c:?}, want {check:?}"),
        other => panic!("want malformed({check}), got {other:?}"),
    }
}

#[test]
fn every_lfp_pointer_validation_has_a_refusal() {
    let lfp = Lfp::new();
    let good = lfp.blocks();
    // helper: mutate one byte of block 41 (index into the ptr table) and rebuild
    let mutate = |f: &dyn Fn(&mut Vec<u8>)| { let mut g = good.clone(); f(&mut g[0].1); vbt(221, &g) };
    malformed(mutate(&|p| p[0] = 2), "num_entries");
    malformed(mutate(&|p| p[3] = 31), "table sizes"); // ptr[0].fp_timing.size
    malformed(mutate(&|p| p[6] = 17), "table sizes"); // dvo size
    malformed(mutate(&|p| p[9] = 11), "table sizes"); // pnp size
    malformed(mutate(&|p| p[1 + 16 * 9 + 2] = 12), "panel_name size");
    malformed(mutate(&|p| p[1 + 9 + 2] = 31), "non-uniform table sizes"); // ptr[1].fp size
    malformed(mutate(&|p| p[1 + 9 * 5] = p[1 + 9 * 5].wrapping_add(1)), "non-uniform entry stride");
    malformed(mutate(&|p| { p[1 + 6] = p[1 + 6].wrapping_add(2); }), ""); // pnp[0] moved: stride/contiguity
    malformed(mutate(&|p| { p[1] = 1; p[2] = 0; }), "pointer below"); // fp[0].offset < data block start
    // data block problems
    let mut g = good.clone(); g[1].1.truncate(100); // too small for 16 entries
    malformed(vbt(221, &g), "16 entries exceed");
    let mut g = good.clone(); g[1].1.clear(); g.push((60, vec![0]));
    malformed(vbt(221, &g), "empty LFP data block");
    // terminator removed from entry 5
    let mut g = good.clone(); let o = 5 * 66 + 36; g[1].1[o] = 0; malformed(vbt(221, &g), "terminator");
    // missing blocks
    let b = vbt(221, &[good[1].clone()]);
    assert_eq!(parse_lfp_data_ptrs(&Vbt::parse(&b).unwrap()), Err(VbtError::LfpDataPtrsMissing));
    let b = vbt(221, &[good[0].clone()]);
    assert_eq!(parse_lfp_data_ptrs(&Vbt::parse(&b).unwrap()), Err(VbtError::BlockMissing { id: 42 }));
    let mut g = good.clone(); g[0].1.truncate(100);
    assert!(matches!(parse_lfp_data_ptrs(&Vbt::parse(&vbt(221, &g)).unwrap()), Err(VbtError::BlockTooSmall { id: 41, .. })));
}

#[test]
fn entry_size_must_add_up() {
    // fp + dvo + pnp must equal the stride even after the +6 adjustment (intel_bios.c:285)
    let mut lfp = Lfp::new();
    lfp.fp_pad = 3; // 32 + 3 + 28 = 63 stride; neither 32+28 nor 32+6+28
    malformed(vbt(221, &lfp.blocks()), "entry size");
}

#[test]
fn pnpid_selects_full_match_then_dateless() {
    // pnpid_get_panel_type, intel_bios.c:658-706; id = mfg u16, product u16, serial u32, week u8 @8, year u8 @9
    let mut lfp = Lfp::new();
    lfp.pnp[4] = [0x36, 0x7f, 3, 0, 3, 0, 0, 0, 0, 0]; // no date
    lfp.pnp[9] = [0x36, 0x7f, 3, 0, 3, 0, 0, 0, 12, 24];
    let b = vbt(221, &lfp.blocks());
    let v = Vbt::parse(&b).unwrap();
    // full match wins even when a dateless entry comes first
    assert_eq!(pnpid_panel_type(&v, &[0x36, 0x7f, 3, 0, 3, 0, 0, 0, 12, 24]).unwrap(), Some(9));
    // different date: falls back to the dateless entry
    assert_eq!(pnpid_panel_type(&v, &[0x36, 0x7f, 3, 0, 3, 0, 0, 0, 1, 2]).unwrap(), Some(4));
    // nothing matches
    assert_eq!(pnpid_panel_type(&v, &[9; 10]).unwrap(), None);
}
