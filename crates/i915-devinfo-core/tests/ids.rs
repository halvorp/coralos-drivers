// SPDX-License-Identifier: GPL-2.0-only
//! The id table and rows against Linux's literals (pciids.h, i915_pci.c). Expected ids are written out by hand, per macro.
use i915_devinfo_core::*;

// ---- expected id lists, copied from include/drm/intel/pciids.h (line cited per list) --------------------------------------
const CHV_IDS: [u16; 4] = [0x22b0, 0x22b1, 0x22b2, 0x22b3]; // pciids.h:339-343
const SKL_GT1_IDS: [u16; 8] = [0x1906, 0x1913, 0x190e, 0x1915, 0x1902, 0x190a, 0x190b, 0x1917]; // pciids.h:345-360
const SKL_GT2_IDS: [u16; 7] = [0x1916, 0x1921, 0x191e, 0x1912, 0x191a, 0x191b, 0x191d]; // pciids.h:361-375
const SKL_GT3_IDS: [u16; 6] = [0x1923, 0x1926, 0x1927, 0x192a, 0x192b, 0x192d]; // pciids.h:376-386
const SKL_GT4_IDS: [u16; 4] = [0x1932, 0x193a, 0x193b, 0x193d]; // pciids.h:387-392
const KBL_GT1_IDS: [u16; 8] = [0x5906, 0x5913, 0x590e, 0x5915, 0x5902, 0x5908, 0x590a, 0x590b]; // pciids.h:410-425
const KBL_GT2_IDS: [u16; 8] = [0x5916, 0x5921, 0x591e, 0x5912, 0x5917, 0x591a, 0x591b, 0x591d]; // pciids.h:426-441
const KBL_GT3_IDS: [u16; 3] = [0x5926, 0x5923, 0x5927]; // pciids.h:442-449
const KBL_GT4_IDS: [u16; 1] = [0x593b]; // pciids.h:450-451 (bound to kbl_gt3_info, i915_pci.c:839)
const AML_KBL_GT2_IDS: [u16; 2] = [0x591c, 0x87c0]; // pciids.h:454-456 (the Pixelbook Go is 0x591c)

#[test]
fn every_listed_id_resolves_to_the_row_linux_binds() {
    for id in CHV_IDS {
        let d = lookup(id).unwrap();
        assert_eq!((d.platform(), d.gen(), d.gt()), (Platform::Cherryview, 8, 0), "{id:#x}");
    }
    for (ids, gt) in [(&SKL_GT1_IDS[..], 1u8), (&SKL_GT2_IDS[..], 2), (&SKL_GT3_IDS[..], 3), (&SKL_GT4_IDS[..], 4)] {
        for &id in ids {
            let d = lookup(id).unwrap();
            assert_eq!((d.platform(), d.gen(), d.gt()), (Platform::Skylake, 9, gt), "{id:#x}");
        }
    }
    for (ids, gt) in [(&KBL_GT1_IDS[..], 1u8), (&KBL_GT2_IDS[..], 2), (&KBL_GT3_IDS[..], 3), (&KBL_GT4_IDS[..], 3), (&AML_KBL_GT2_IDS[..], 2)] {
        for &id in ids {
            let d = lookup(id).unwrap();
            assert_eq!((d.platform(), d.gen(), d.gt()), (Platform::Kabylake, 9, gt), "{id:#x}");
        }
    }
}

#[test]
fn the_table_holds_exactly_the_listed_ids_and_no_more() {
    let n = CHV_IDS.len() + SKL_GT1_IDS.len() + SKL_GT2_IDS.len() + SKL_GT3_IDS.len() + SKL_GT4_IDS.len()
        + KBL_GT1_IDS.len() + KBL_GT2_IDS.len() + KBL_GT3_IDS.len() + KBL_GT4_IDS.len() + AML_KBL_GT2_IDS.len();
    assert_eq!(n, 51);
    assert_eq!(ID_TABLE.len(), 51);
    assert_eq!(ID_TABLE.len(), n);
    let all: Vec<u16> = [&CHV_IDS[..], &SKL_GT1_IDS, &SKL_GT2_IDS, &SKL_GT3_IDS, &SKL_GT4_IDS, &KBL_GT1_IDS, &KBL_GT2_IDS, &KBL_GT3_IDS, &KBL_GT4_IDS, &AML_KBL_GT2_IDS]
        .concat();
    for r in ID_TABLE.iter() {
        assert!(all.contains(&r.devid), "{:#x} is in the table but not in Linux's lists", r.devid);
    }
    for (i, a) in all.iter().enumerate() {
        assert!(!all[i + 1..].contains(a), "{a:#x} listed twice");
    }
}

#[test]
fn the_pixelbook_go_0x591c_is_kbl_gt2_ulx() {
    let d = lookup(0x591c).unwrap();
    assert_eq!(d.name(), "kbl-gt2");
    assert_eq!(d.sub, SubPlatform::Ulx); // intel_display_device.c:733
    assert_eq!(d.gen(), 9);
    assert_eq!(d.info.display_ver, 9); // intel_display_device.c:675
    assert!(d.info.has_llc);
    assert!(!d.is_gen9_lp());
    assert_eq!(d.info.ppgtt_type, PpgttType::Full);
    assert_eq!(d.info.ppgtt_size, 48);
    assert_eq!(d.info.dma_mask_size, 39);
    assert_eq!(d.info.page_sizes, 0x1000 | 0x1_0000);
    assert!(d.info.has_gt_uc);
    assert_eq!(d.require_gen9(), Ok(()));
    // the two siblings Astra named
    assert_eq!(lookup(0x87c0).unwrap().name(), "kbl-gt2");
    assert_eq!(lookup(0x591e).unwrap().name(), "kbl-gt2");
}

#[test]
fn engines_follow_the_gt_row() {
    // RCS0 | VCS0 | BCS0 | VECS0 with the intel_engine_id positions 0, 10, 1, 18
    let four = (1 << 0) | (1 << 10) | (1 << 1) | (1 << 18);
    assert_eq!(four, 0x4_0403);
    assert_eq!(lookup(0x591c).unwrap().info.engine_mask, four); // kbl_gt2_info: G75 engines
    assert_eq!(lookup(0x5906).unwrap().info.engine_mask, four); // kbl_gt1_info
    assert_eq!(lookup(0x1912).unwrap().info.engine_mask, four); // skl_gt2_info
    // GT3+ adds VCS1 (position 11)
    assert_eq!(lookup(0x5926).unwrap().info.engine_mask, four | (1 << 11));
    assert_eq!(lookup(0x593b).unwrap().info.engine_mask, four | (1 << 11), "GT4 id uses kbl_gt3_info");
    assert_eq!(lookup(0x1926).unwrap().info.engine_mask, four | (1 << 11));
    assert_eq!(lookup(0x1932).unwrap().info.engine_mask, four | (1 << 11));
    assert_eq!(engine::RCS0, 0);
    assert_eq!(engine::BCS0, 1);
    assert_eq!(engine::VCS0, 10);
    assert_eq!(engine::VCS1, 11);
    assert_eq!(engine::VECS0, 18);
}

#[test]
fn sub_platform_groups_match_the_ult_ulx_lists() {
    // kbl_ult_ids (intel_display_device.c:723-728): ULT GT1, GT2, GT3 lists
    for id in [0x5906u16, 0x5913, 0x5916, 0x5921, 0x5926] {
        assert_eq!(lookup(id).unwrap().sub, SubPlatform::Ult, "{id:#x}");
    }
    // kbl_ulx_ids (:730-735): ULX GT1, GT2, AML
    for id in [0x590eu16, 0x5915, 0x591e, 0x591c, 0x87c0] {
        assert_eq!(lookup(id).unwrap().sub, SubPlatform::Ulx, "{id:#x}");
    }
    // everything else in the KBL table is neither
    for id in [0x5902u16, 0x5908, 0x590a, 0x590b, 0x5912, 0x5917, 0x591a, 0x591b, 0x591d, 0x5923, 0x5927, 0x593b] {
        assert_eq!(lookup(id).unwrap().sub, SubPlatform::None, "{id:#x}");
    }
    // skl_ult_ids (:686-691) / skl_ulx_ids (:693-697)
    for id in [0x1906u16, 0x1913, 0x1916, 0x1921, 0x1923, 0x1926, 0x1927] {
        assert_eq!(lookup(id).unwrap().sub, SubPlatform::Ult, "{id:#x}");
    }
    for id in [0x190eu16, 0x1915, 0x191e] {
        assert_eq!(lookup(id).unwrap().sub, SubPlatform::Ulx, "{id:#x}");
    }
}

#[test]
fn every_row_has_its_marker_name() {
    // one id per row: the names the device prints in [I915-PROOF-OF-LIFE platform=..]
    for (id, name) in [
        (0x22b0u16, "chv"), (0x1906, "skl-gt1"), (0x1916, "skl-gt2"), (0x1923, "skl-gt3"), (0x1932, "skl-gt4"),
        (0x5906, "kbl-gt1"), (0x5916, "kbl-gt2"), (0x5926, "kbl-gt3"), (0x593b, "kbl-gt3"), (0x591c, "kbl-gt2"),
    ] {
        assert_eq!(lookup(id).unwrap().name(), name, "{id:#x}");
    }
    let names: Vec<&str> = [0x22b0u16, 0x1906, 0x1916, 0x1923, 0x1932, 0x5906, 0x5916, 0x5926].iter().map(|i| lookup(*i).unwrap().name()).collect();
    for (i, a) in names.iter().enumerate() {
        assert!(!names[i + 1..].contains(a), "{a} is not distinct");
        assert_ne!(*a, "unknown");
    }
}

#[test]
fn ids_in_neither_ult_nor_ulx_list_have_no_sub_platform() {
    // skl_ult_ids / skl_ulx_ids (intel_display_device.c:686-697) do NOT list these Skylake ids; Cherryview has no sub-platform at all
    for id in [0x1902u16, 0x190a, 0x190b, 0x1917, 0x1912, 0x191a, 0x191b, 0x191d, 0x192a, 0x192b, 0x192d, 0x1932, 0x193a, 0x193b, 0x193d] {
        assert_eq!(lookup(id).unwrap().sub, SubPlatform::None, "{id:#x}");
    }
    for id in CHV_IDS {
        assert_eq!(lookup(id).unwrap().sub, SubPlatform::None, "{id:#x}");
    }
    // the family is covered as a family: every table row is in exactly one of the three groups
    let ult = [0x1906u16, 0x1913, 0x1916, 0x1921, 0x1923, 0x1926, 0x1927, 0x5906, 0x5913, 0x5916, 0x5921, 0x5926];
    let ulx = [0x190eu16, 0x1915, 0x191e, 0x590e, 0x5915, 0x591e, 0x591c, 0x87c0];
    for r in ID_TABLE.iter() {
        let want = if ult.contains(&r.devid) { SubPlatform::Ult } else if ulx.contains(&r.devid) { SubPlatform::Ulx } else { SubPlatform::None };
        assert_eq!(r.sub, want, "{:#x}", r.devid);
    }
}

#[test]
fn the_skl_row_differs_from_kbl_only_in_platform() {
    let (s, k) = (lookup(0x1916).unwrap(), lookup(0x5916).unwrap());
    assert_eq!(s.platform(), Platform::Skylake);
    assert_eq!(k.platform(), Platform::Kabylake);
    let mut a = s.info;
    a.platform = Platform::Kabylake;
    assert_eq!(a, k.info);
}

// ---- the negative control ---------------------------------------------------------------------------------------------

#[test]
fn chv_is_recognised_by_name_and_refused_by_generation() {
    let d = lookup(0x22b0).unwrap();
    assert_eq!(d.name(), "chv");
    assert_eq!(d.gen(), 8);
    assert_eq!(d.info.display_ver, 8); // intel_display_device.c:653
    assert_eq!(d.require_gen9(), Err(I915Error::UnsupportedGeneration { devid: 0x22b0, gen: 8 }));
    assert_eq!(d.require_gen9().unwrap_err().name(), "UNSUPPORTED-GENERATION");
    // chv_info's own fields (i915_pci.c:450-469): not the Gen9 row
    assert!(!d.info.has_llc);
    assert!(d.info.has_snoop);
    assert!(!d.info.has_coherent_ggtt);
    assert_eq!(d.info.ppgtt_size, 32);
    assert_eq!(d.info.page_sizes, 0x1000);
    assert!(!d.info.has_gt_uc);
    for id in CHV_IDS {
        assert!(lookup(id).unwrap().require_gen9().is_err(), "{id:#x}");
    }
}

#[test]
fn an_unknown_id_is_refused_by_name_never_aliased() {
    for id in [0x0000u16, 0xffff, 0x0166 /* IVB */, 0x1616 /* BDW GT2 (a Linux row we do not carry) */, 0x3e92 /* CFL */, 0x5a85 /* APL */, 0x22b4, 0x22af, 0x591f, 0x5900, 0x5929] {
        assert_eq!(lookup(id), Err(I915Error::UnknownDevice { devid: id }), "{id:#x}");
    }
    assert_eq!(lookup(0x1616).unwrap_err().name(), "UNKNOWN-DEVICE");
}

#[test]
fn a_cherry_trail_id_is_never_in_a_gen9_family() {
    // The mutation the plan names: planting 0x22b0 in the KBL list must change the answer. Guard that no KBL/SKL row is Gen8
    // and no CHV row is Gen9, by walking the real table.
    for r in ID_TABLE.iter() {
        let d = lookup(r.devid).unwrap();
        match d.platform() {
            Platform::Cherryview => assert_eq!(d.gen(), 8),
            _ => assert_eq!(d.gen(), 9),
        }
        if (0x22b0..=0x22b3).contains(&r.devid) {
            assert_eq!(d.platform(), Platform::Cherryview, "{:#x}", r.devid);
        }
    }
}

#[test]
fn row_flags_gen9_and_chv() {
    let k = lookup(0x5916).unwrap().info;
    assert!(k.has_llc && k.has_coherent_ggtt && k.has_logical_ring_contexts && k.has_64bit_reloc);
    assert!(k.has_reset_engine && k.has_rc6 && k.has_rps && k.has_runtime_pm && k.has_gt_uc);
    assert!(!k.has_snoop);
    assert_eq!(k.max_pat_index, 3);
    let c = lookup(0x22b1).unwrap().info;
    assert!(c.has_logical_ring_contexts && c.has_64bit_reloc && c.has_reset_engine && c.has_rc6 && c.has_rps && c.has_runtime_pm);
    assert_eq!(c.dma_mask_size, 39);
    assert_eq!(c.max_pat_index, 3);
}
