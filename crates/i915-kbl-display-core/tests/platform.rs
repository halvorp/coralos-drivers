// SPDX-License-Identifier: GPL-2.0-only
//! The Kaby Lake id table and caps, against the literal Linux lists (pciids.h:410-456, intel_display_device.c:723-760).
use i915_kbl_display_core::platform::*;
use i915_kbl_display_core::types::*;

#[test]
fn every_kbl_id_is_listed_with_its_gt_and_subplatform() {
    // Written out by hand from INTEL_KBL_*_IDS / INTEL_AML_KBL_GT2_IDS and i915_pci.c:836-840.
    let expected: [(u16, u8, SubPlatform); 22] = [
        (0x5906, 1, SubPlatform::Ult),
        (0x5913, 1, SubPlatform::Ult),
        (0x590e, 1, SubPlatform::Ulx),
        (0x5915, 1, SubPlatform::Ulx),
        (0x5902, 1, SubPlatform::None),
        (0x5908, 1, SubPlatform::None),
        (0x590a, 1, SubPlatform::None),
        (0x590b, 1, SubPlatform::None),
        (0x5916, 2, SubPlatform::Ult),
        (0x5921, 2, SubPlatform::Ult),
        (0x591e, 2, SubPlatform::Ulx),
        (0x5912, 2, SubPlatform::None),
        (0x5917, 2, SubPlatform::None),
        (0x591a, 2, SubPlatform::None),
        (0x591b, 2, SubPlatform::None),
        (0x591d, 2, SubPlatform::None),
        (0x591c, 2, SubPlatform::Ulx),
        (0x87c0, 2, SubPlatform::Ulx),
        (0x5926, 3, SubPlatform::Ult),
        (0x5923, 3, SubPlatform::None),
        (0x5927, 3, SubPlatform::None),
        (0x593b, 3, SubPlatform::None),
    ];
    assert_eq!(KBL_IDS.len(), expected.len());
    for (i, (id, gt, sub)) in expected.iter().enumerate() {
        assert_eq!(KBL_IDS[i], KblId { device_id: *id, gt: *gt, sub: *sub }, "row {i}");
        // and the lookup path, not just the table, finds every one
        let d = identify(0x8086, *id, 0).expect("listed id must identify");
        assert_eq!(d.id.device_id, *id);
        assert_eq!(d.id.gt, *gt);
        assert_eq!(d.id.sub, *sub);
    }
    // no duplicates
    for i in 0..KBL_IDS.len() {
        for j in i + 1..KBL_IDS.len() {
            assert_ne!(KBL_IDS[i].device_id, KBL_IDS[j].device_id);
        }
    }
}

#[test]
fn the_pixelbook_go_is_591c_ulx_gt2() {
    assert_eq!(PIXELBOOK_GO_DEVICE_ID, 0x591c);
    let d = identify(0x8086, 0x591c, 0x02).unwrap();
    assert_eq!(d.id.sub, SubPlatform::Ulx);
    assert_eq!(d.id.gt, 2);
    assert_eq!(d.stepping, Some("B0")); // revision 2, kbl_steppings[2] = STEP_B0
}

#[test]
fn kbl_display_caps_are_skl_display() {
    let c = KBL_DISPLAY;
    assert_eq!(c.display_ver, 9);
    assert_eq!(DISPLAY_VER, 9);
    assert_eq!(c.pipe_mask, 0b111); // three pipes
    assert_eq!(c.num_pipes(), 3);
    assert_eq!(c.transcoder_mask, 0b1_0111); // A B C + EDP at bit 4
    assert_eq!(c.port_mask, 0b1_1111); // A..E
    assert_eq!(c.dbuf_blocks, 892);
    assert_eq!(c.scalers, [2, 2, 1]);
    assert!(c.has_ddi && c.has_dp_mst && c.has_psr && c.has_dmc);
    for p in Pipe::ALL {
        assert!(c.has_pipe(p));
    }
    for t in [Transcoder::A, Transcoder::B, Transcoder::C, Transcoder::Edp] {
        assert!(c.has_transcoder(t));
    }
    for d in Ddi::ALL {
        assert!(c.has_port(d));
    }
    assert_eq!(c.scalers_of(Pipe::A), 2);
    assert_eq!(c.scalers_of(Pipe::B), 2);
    assert_eq!(c.scalers_of(Pipe::C), 1);
}

#[test]
fn every_kbl_member_gets_the_same_caps() {
    for e in KBL_IDS.iter() {
        assert_eq!(identify(0x8086, e.device_id, 0).unwrap().caps, KBL_DISPLAY);
    }
}

#[test]
fn stepping_family_covers_every_revision() {
    // intel_display_device.c:737-745
    let expected = [None, Some("B0"), Some("B0"), Some("B0"), Some("C0"), Some("B1"), Some("B1"), Some("C0"), None, None];
    for (rev, want) in expected.iter().enumerate() {
        assert_eq!(kbl_stepping(rev as u8), *want, "revision {rev}");
    }
    assert_eq!(kbl_stepping(0xff), None);
}

#[test]
fn foreign_ids_are_refused_by_name_never_aliased() {
    // Cherry Trail (Gen8) ids: INTEL_CHV_IDS, pciids.h:340-343. 8086:591c must never be served by that row, and
    // those ids must not be served by this one.
    for chv in [0x22b0u16, 0x22b1, 0x22b2, 0x22b3] {
        assert_eq!(identify(0x8086, chv, 0), Err(PlatformError::NotKabyLake { device: chv }));
    }
    // Skylake GT2 (0x1912) and Coffee Lake (0x3e92) are neighbours with the same display block but other rows
    assert_eq!(identify(0x8086, 0x1912, 0), Err(PlatformError::NotKabyLake { device: 0x1912 }));
    assert_eq!(identify(0x8086, 0x3e92, 0), Err(PlatformError::NotKabyLake { device: 0x3e92 }));
    // an id one off from the Pixelbook Go's
    assert_eq!(identify(0x8086, 0x591f, 0), Err(PlatformError::NotKabyLake { device: 0x591f }));
    // right device id, wrong vendor
    assert_eq!(identify(0x10de, 0x591c, 0), Err(PlatformError::NotIntel { vendor: 0x10de }));
    assert_eq!(PlatformError::NotIntel { vendor: 0 }.name(), "not-intel");
    assert_eq!(PlatformError::NotKabyLake { device: 0 }.name(), "not-kaby-lake");
}

#[test]
fn vendor_literal() {
    assert_eq!(PCI_VENDOR_ID_INTEL, 0x8086);
}

#[test]
fn capability_queries_say_no_as_well_as_yes() {
    // a part with only pipe A, transcoder A, port A: every other question is a no
    let one = DisplayCaps { pipe_mask: 0b001, transcoder_mask: 0b0_0001, port_mask: 0b00001, ..KBL_DISPLAY };
    assert!(one.has_pipe(Pipe::A) && !one.has_pipe(Pipe::B) && !one.has_pipe(Pipe::C));
    assert!(one.has_transcoder(Transcoder::A));
    assert!(!one.has_transcoder(Transcoder::B) && !one.has_transcoder(Transcoder::C) && !one.has_transcoder(Transcoder::Edp));
    assert!(one.has_port(Ddi::A));
    for d in [Ddi::B, Ddi::C, Ddi::D, Ddi::E] {
        assert!(!one.has_port(d), "{d:?}");
    }
    assert_eq!(one.num_pipes(), 1);
    // EDP is bit 4, not bit 3 (TRANSCODER_D sits between C and EDP, intel_display_limits.h:37-43)
    let edp_only = DisplayCaps { transcoder_mask: 0b1_0000, ..KBL_DISPLAY };
    assert!(edp_only.has_transcoder(Transcoder::Edp) && !edp_only.has_transcoder(Transcoder::C));
}
