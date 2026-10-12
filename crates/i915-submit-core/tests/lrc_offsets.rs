// SPDX-License-Identifier: GPL-2.0-only
//! The register-state offset tables: decoded by `set_offsets` and compared, dword for dword, with what Linux's OWN
//! `set_offsets` produces from Linux's OWN byte-encoded tables (intel_lrc.c:49-413 compiled by GCC with `MI_LOAD_REGISTER_IMM`
//! and friends stubbed; each list below is every non-zero dword of the 4 KiB page that evaluation wrote, `(index, value)`).
use i915_submit_core::lrc_offsets::*;
use i915_submit_core::SubmitError;

/// gen9_rcs base=0x2000 gen=9 close=0
const GEN9_RCS: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00002244), (0x4, 0x00002034), (0x6, 0x00002030), (0x8, 0x00002038), (0xa, 0x0000203c),
    (0xc, 0x00002168), (0xe, 0x00002140), (0x10, 0x00002110), (0x12, 0x0000211c), (0x14, 0x00002114), (0x16, 0x00002118),
    (0x18, 0x000021c0), (0x1a, 0x000021c4), (0x1c, 0x000021c8), (0x21, 0x11001011), (0x22, 0x000023a8), (0x24, 0x0000228c),
    (0x26, 0x00002288), (0x28, 0x00002284), (0x2a, 0x00002280), (0x2c, 0x0000227c), (0x2e, 0x00002278), (0x30, 0x00002274),
    (0x32, 0x00002270), (0x41, 0x11000001), (0x42, 0x000020c8), (0x51, 0x11001057), (0x52, 0x00002028), (0x54, 0x0000209c),
    (0x56, 0x000020c0), (0x58, 0x00002178), (0x5a, 0x0000217c), (0x5c, 0x00002358), (0x5e, 0x00002170), (0x60, 0x00002150),
    (0x62, 0x00002154), (0x64, 0x00002158), (0x66, 0x0000241c), (0x68, 0x00002600), (0x6a, 0x00002604), (0x6c, 0x00002608),
    (0x6e, 0x0000260c), (0x70, 0x00002610), (0x72, 0x00002614), (0x74, 0x00002618), (0x76, 0x0000261c), (0x78, 0x00002620),
    (0x7a, 0x00002624), (0x7c, 0x00002628), (0x7e, 0x0000262c), (0x80, 0x00002630), (0x82, 0x00002634), (0x84, 0x00002638),
    (0x86, 0x0000263c), (0x88, 0x00002640), (0x8a, 0x00002644), (0x8c, 0x00002648), (0x8e, 0x0000264c), (0x90, 0x00002650),
    (0x92, 0x00002654), (0x94, 0x00002658), (0x96, 0x0000265c), (0x98, 0x00002660), (0x9a, 0x00002664), (0x9c, 0x00002668),
    (0x9e, 0x0000266c), (0xa0, 0x00002670), (0xa2, 0x00002674), (0xa4, 0x00002678), (0xa6, 0x0000267c), (0xa8, 0x00002068),
];
/// gen9_rcs base=0x2000 gen=9 close=1
const GEN9_RCS_CLOSED: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00002244), (0x4, 0x00002034), (0x6, 0x00002030), (0x8, 0x00002038), (0xa, 0x0000203c),
    (0xc, 0x00002168), (0xe, 0x00002140), (0x10, 0x00002110), (0x12, 0x0000211c), (0x14, 0x00002114), (0x16, 0x00002118),
    (0x18, 0x000021c0), (0x1a, 0x000021c4), (0x1c, 0x000021c8), (0x21, 0x11001011), (0x22, 0x000023a8), (0x24, 0x0000228c),
    (0x26, 0x00002288), (0x28, 0x00002284), (0x2a, 0x00002280), (0x2c, 0x0000227c), (0x2e, 0x00002278), (0x30, 0x00002274),
    (0x32, 0x00002270), (0x41, 0x11000001), (0x42, 0x000020c8), (0x51, 0x11001057), (0x52, 0x00002028), (0x54, 0x0000209c),
    (0x56, 0x000020c0), (0x58, 0x00002178), (0x5a, 0x0000217c), (0x5c, 0x00002358), (0x5e, 0x00002170), (0x60, 0x00002150),
    (0x62, 0x00002154), (0x64, 0x00002158), (0x66, 0x0000241c), (0x68, 0x00002600), (0x6a, 0x00002604), (0x6c, 0x00002608),
    (0x6e, 0x0000260c), (0x70, 0x00002610), (0x72, 0x00002614), (0x74, 0x00002618), (0x76, 0x0000261c), (0x78, 0x00002620),
    (0x7a, 0x00002624), (0x7c, 0x00002628), (0x7e, 0x0000262c), (0x80, 0x00002630), (0x82, 0x00002634), (0x84, 0x00002638),
    (0x86, 0x0000263c), (0x88, 0x00002640), (0x8a, 0x00002644), (0x8c, 0x00002648), (0x8e, 0x0000264c), (0x90, 0x00002650),
    (0x92, 0x00002654), (0x94, 0x00002658), (0x96, 0x0000265c), (0x98, 0x00002660), (0x9a, 0x00002664), (0x9c, 0x00002668),
    (0x9e, 0x0000266c), (0xa0, 0x00002670), (0xa2, 0x00002674), (0xa4, 0x00002678), (0xa6, 0x0000267c), (0xa8, 0x00002068),
    (0xaa, 0x05000000),
];
/// gen9_xcs base=0x22000 gen=9 close=0
const GEN9_XCS: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00022244), (0x4, 0x00022034), (0x6, 0x00022030), (0x8, 0x00022038), (0xa, 0x0002203c),
    (0xc, 0x00022168), (0xe, 0x00022140), (0x10, 0x00022110), (0x12, 0x0002211c), (0x14, 0x00022114), (0x16, 0x00022118),
    (0x18, 0x000221c0), (0x1a, 0x000221c4), (0x1c, 0x000221c8), (0x21, 0x11001011), (0x22, 0x000223a8), (0x24, 0x0002228c),
    (0x26, 0x00022288), (0x28, 0x00022284), (0x2a, 0x00022280), (0x2c, 0x0002227c), (0x2e, 0x00022278), (0x30, 0x00022274),
    (0x32, 0x00022270), (0x41, 0x11001001), (0x42, 0x00022200), (0x51, 0x11001057), (0x52, 0x00022028), (0x54, 0x0002209c),
    (0x56, 0x000220c0), (0x58, 0x00022178), (0x5a, 0x0002217c), (0x5c, 0x00022358), (0x5e, 0x00022170), (0x60, 0x00022150),
    (0x62, 0x00022154), (0x64, 0x00022158), (0x66, 0x0002241c), (0x68, 0x00022600), (0x6a, 0x00022604), (0x6c, 0x00022608),
    (0x6e, 0x0002260c), (0x70, 0x00022610), (0x72, 0x00022614), (0x74, 0x00022618), (0x76, 0x0002261c), (0x78, 0x00022620),
    (0x7a, 0x00022624), (0x7c, 0x00022628), (0x7e, 0x0002262c), (0x80, 0x00022630), (0x82, 0x00022634), (0x84, 0x00022638),
    (0x86, 0x0002263c), (0x88, 0x00022640), (0x8a, 0x00022644), (0x8c, 0x00022648), (0x8e, 0x0002264c), (0x90, 0x00022650),
    (0x92, 0x00022654), (0x94, 0x00022658), (0x96, 0x0002265c), (0x98, 0x00022660), (0x9a, 0x00022664), (0x9c, 0x00022668),
    (0x9e, 0x0002266c), (0xa0, 0x00022670), (0xa2, 0x00022674), (0xa4, 0x00022678), (0xa6, 0x0002267c), (0xa8, 0x00022068),
];
/// gen9_xcs base=0x22000 gen=9 close=1
const GEN9_XCS_CLOSED: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00022244), (0x4, 0x00022034), (0x6, 0x00022030), (0x8, 0x00022038), (0xa, 0x0002203c),
    (0xc, 0x00022168), (0xe, 0x00022140), (0x10, 0x00022110), (0x12, 0x0002211c), (0x14, 0x00022114), (0x16, 0x00022118),
    (0x18, 0x000221c0), (0x1a, 0x000221c4), (0x1c, 0x000221c8), (0x21, 0x11001011), (0x22, 0x000223a8), (0x24, 0x0002228c),
    (0x26, 0x00022288), (0x28, 0x00022284), (0x2a, 0x00022280), (0x2c, 0x0002227c), (0x2e, 0x00022278), (0x30, 0x00022274),
    (0x32, 0x00022270), (0x41, 0x11001001), (0x42, 0x00022200), (0x51, 0x11001057), (0x52, 0x00022028), (0x54, 0x0002209c),
    (0x56, 0x000220c0), (0x58, 0x00022178), (0x5a, 0x0002217c), (0x5c, 0x00022358), (0x5e, 0x00022170), (0x60, 0x00022150),
    (0x62, 0x00022154), (0x64, 0x00022158), (0x66, 0x0002241c), (0x68, 0x00022600), (0x6a, 0x00022604), (0x6c, 0x00022608),
    (0x6e, 0x0002260c), (0x70, 0x00022610), (0x72, 0x00022614), (0x74, 0x00022618), (0x76, 0x0002261c), (0x78, 0x00022620),
    (0x7a, 0x00022624), (0x7c, 0x00022628), (0x7e, 0x0002262c), (0x80, 0x00022630), (0x82, 0x00022634), (0x84, 0x00022638),
    (0x86, 0x0002263c), (0x88, 0x00022640), (0x8a, 0x00022644), (0x8c, 0x00022648), (0x8e, 0x0002264c), (0x90, 0x00022650),
    (0x92, 0x00022654), (0x94, 0x00022658), (0x96, 0x0002265c), (0x98, 0x00022660), (0x9a, 0x00022664), (0x9c, 0x00022668),
    (0x9e, 0x0002266c), (0xa0, 0x00022670), (0xa2, 0x00022674), (0xa4, 0x00022678), (0xa6, 0x0002267c), (0xa8, 0x00022068),
    (0xaa, 0x05000000),
];
/// gen8_rcs base=0x2000 gen=8 close=0
const GEN8_RCS: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00002244), (0x4, 0x00002034), (0x6, 0x00002030), (0x8, 0x00002038), (0xa, 0x0000203c),
    (0xc, 0x00002168), (0xe, 0x00002140), (0x10, 0x00002110), (0x12, 0x0000211c), (0x14, 0x00002114), (0x16, 0x00002118),
    (0x18, 0x000021c0), (0x1a, 0x000021c4), (0x1c, 0x000021c8), (0x21, 0x11001011), (0x22, 0x000023a8), (0x24, 0x0000228c),
    (0x26, 0x00002288), (0x28, 0x00002284), (0x2a, 0x00002280), (0x2c, 0x0000227c), (0x2e, 0x00002278), (0x30, 0x00002274),
    (0x32, 0x00002270), (0x41, 0x11000001), (0x42, 0x000020c8),
];
/// gen8_xcs base=0x22000 gen=8 close=0
const GEN8_XCS: &[(usize, u32)] = &[
    (0x1, 0x11000015), (0x2, 0x00022244), (0x4, 0x00022034), (0x6, 0x00022030), (0x8, 0x00022038), (0xa, 0x0002203c),
    (0xc, 0x00022168), (0xe, 0x00022140), (0x10, 0x00022110), (0x12, 0x0002211c), (0x14, 0x00022114), (0x16, 0x00022118),
    (0x21, 0x11000011), (0x22, 0x000223a8), (0x24, 0x0002228c), (0x26, 0x00022288), (0x28, 0x00022284), (0x2a, 0x00022280),
    (0x2c, 0x0002227c), (0x2e, 0x00022278), (0x30, 0x00022274), (0x32, 0x00022270), (0x41, 0x11000003), (0x42, 0x00022200),
    (0x44, 0x00022028),
];
/// gen9_xcs_vcs0 base=0x12000 gen=9 close=0
const GEN9_XCS_VCS0: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00012244), (0x4, 0x00012034), (0x6, 0x00012030), (0x8, 0x00012038), (0xa, 0x0001203c),
    (0xc, 0x00012168), (0xe, 0x00012140), (0x10, 0x00012110), (0x12, 0x0001211c), (0x14, 0x00012114), (0x16, 0x00012118),
    (0x18, 0x000121c0), (0x1a, 0x000121c4), (0x1c, 0x000121c8), (0x21, 0x11001011), (0x22, 0x000123a8), (0x24, 0x0001228c),
    (0x26, 0x00012288), (0x28, 0x00012284), (0x2a, 0x00012280), (0x2c, 0x0001227c), (0x2e, 0x00012278), (0x30, 0x00012274),
    (0x32, 0x00012270), (0x41, 0x11001001), (0x42, 0x00012200), (0x51, 0x11001057), (0x52, 0x00012028), (0x54, 0x0001209c),
    (0x56, 0x000120c0), (0x58, 0x00012178), (0x5a, 0x0001217c), (0x5c, 0x00012358), (0x5e, 0x00012170), (0x60, 0x00012150),
    (0x62, 0x00012154), (0x64, 0x00012158), (0x66, 0x0001241c), (0x68, 0x00012600), (0x6a, 0x00012604), (0x6c, 0x00012608),
    (0x6e, 0x0001260c), (0x70, 0x00012610), (0x72, 0x00012614), (0x74, 0x00012618), (0x76, 0x0001261c), (0x78, 0x00012620),
    (0x7a, 0x00012624), (0x7c, 0x00012628), (0x7e, 0x0001262c), (0x80, 0x00012630), (0x82, 0x00012634), (0x84, 0x00012638),
    (0x86, 0x0001263c), (0x88, 0x00012640), (0x8a, 0x00012644), (0x8c, 0x00012648), (0x8e, 0x0001264c), (0x90, 0x00012650),
    (0x92, 0x00012654), (0x94, 0x00012658), (0x96, 0x0001265c), (0x98, 0x00012660), (0x9a, 0x00012664), (0x9c, 0x00012668),
    (0x9e, 0x0001266c), (0xa0, 0x00012670), (0xa2, 0x00012674), (0xa4, 0x00012678), (0xa6, 0x0001267c), (0xa8, 0x00012068),
];

fn decode(items: &[Item], base: u32, gen: u8, close: bool) -> Vec<(usize, u32)> {
    let mut page = [0u32; 1024];
    set_offsets(&mut page, items, "test", base, gen, close).unwrap();
    page.iter().enumerate().filter(|(_, v)| **v != 0).map(|(i, v)| (i, *v)).collect()
}

#[test]
fn gen9_render_table_decodes_to_the_dwords_linux_produces() {
    assert_eq!(decode(&GEN9_RCS_OFFSETS, 0x2000, 9, false), GEN9_RCS);
    assert_eq!(decode(&GEN9_RCS_OFFSETS, 0x2000, 9, true), GEN9_RCS_CLOSED);
    assert_ne!(GEN9_RCS, GEN9_RCS_CLOSED, "close adds exactly the MI_BATCH_BUFFER_END");
    assert_eq!(GEN9_RCS_CLOSED.len(), GEN9_RCS.len() + 1);
    let extra: Vec<_> = GEN9_RCS_CLOSED.iter().filter(|e| !GEN9_RCS.contains(e)).collect();
    assert_eq!(extra.len(), 1);
    assert_eq!(extra[0].1, 0x0500_0000, "MI_BATCH_BUFFER_END = MI_INSTR(0x0a, 0) = 0x0a << 23 (intel_gpu_commands.h:62)");
}

#[test]
fn gen9_other_engine_table_decodes_to_the_dwords_linux_produces() {
    assert_eq!(decode(&GEN9_XCS_OFFSETS, 0x22000, 9, false), GEN9_XCS);
    assert_eq!(decode(&GEN9_XCS_OFFSETS, 0x22000, 9, true), GEN9_XCS_CLOSED);
    // the same table at the VCS0 base: only the register addresses move
    assert_eq!(decode(&GEN9_XCS_OFFSETS, 0x12000, 9, false), GEN9_XCS_VCS0);
}

#[test]
fn gen8_tables_decode_to_the_dwords_linux_produces() {
    assert_eq!(decode(&GEN8_RCS_OFFSETS, 0x2000, 8, false), GEN8_RCS);
    assert_eq!(decode(&GEN8_XCS_OFFSETS, 0x22000, 8, false), GEN8_XCS);
}

/// Counts written out from the `LRI(count, ...)` lines of intel_lrc.c (:110, :112, ..., :386-388).
#[test]
fn every_table_has_the_lri_runs_linux_declares() {
    fn runs(items: &[Item]) -> Vec<(u8, bool)> {
        items.iter().filter_map(|i| if let Item::Lri { count, posted } = i { Some((*count, *posted)) } else { None }).collect()
    }
    assert_eq!(runs(&GEN8_XCS_OFFSETS), [(11, false), (9, false), (2, false)]);
    assert_eq!(runs(&GEN9_XCS_OFFSETS), [(14, true), (9, true), (1, true), (44, true)]);
    assert_eq!(runs(&GEN8_RCS_OFFSETS), [(14, true), (9, true), (1, false)]);
    assert_eq!(runs(&GEN9_RCS_OFFSETS), [(14, true), (9, true), (1, false), (44, true)]);
    // each LRI is followed by exactly `count` registers, and the register count of each table is the sum
    for (items, total) in [(&GEN8_XCS_OFFSETS[..], 22), (&GEN9_XCS_OFFSETS[..], 68), (&GEN8_RCS_OFFSETS[..], 24), (&GEN9_RCS_OFFSETS[..], 68)] {
        assert_eq!(items.iter().filter(|i| matches!(i, Item::Reg(_))).count(), total);
    }
    assert_eq!(GEN8_XCS_OFFSETS.len(), 28);
    assert_eq!(GEN9_XCS_OFFSETS.len(), 76);
    assert_eq!(GEN8_RCS_OFFSETS.len(), 30);
    assert_eq!(GEN9_RCS_OFFSETS.len(), 76);
}

#[test]
fn reg_offsets_picks_the_table_linux_picks() {
    assert_eq!(reg_offsets(9, true).unwrap().name, "gen9_rcs_offsets");
    assert_eq!(reg_offsets(9, false).unwrap().name, "gen9_xcs_offsets");
    assert_eq!(reg_offsets(8, true).unwrap().name, "gen8_rcs_offsets");
    assert_eq!(reg_offsets(8, false).unwrap().name, "gen8_xcs_offsets");
    assert!(core::ptr::eq(reg_offsets(9, true).unwrap().items, &GEN9_RCS_OFFSETS[..]));
    assert!(core::ptr::eq(reg_offsets(9, false).unwrap().items, &GEN9_XCS_OFFSETS[..]));
    assert!(core::ptr::eq(reg_offsets(8, true).unwrap().items, &GEN8_RCS_OFFSETS[..]));
    assert!(core::ptr::eq(reg_offsets(8, false).unwrap().items, &GEN8_XCS_OFFSETS[..]));
    for gen in [7u8, 10, 11, 12] {
        for render in [true, false] {
            assert_eq!(reg_offsets(gen, render).unwrap_err(), SubmitError::UnsupportedGeneration { gen });
        }
    }
}

/// (gen, render, mi_mode, bb_offset, gpr0, wa_bb_per_ctx, indirect_ptr, indirect_offset): the values Linux's own `lrc_ring_*`
/// functions return (-1 = no such slot), GCC-evaluated.
const SLOTS: [(u8, bool, i32, i32, i32, i32, i32, i32); 4] = [
    (9, true, 84, 100, 104, 24, 26, 28),
    (9, false, 84, 100, 104, 24, 26, 28),
    (8, true, 88, 196, 216, 24, 26, 28),
    (8, false, -1, -1, -1, -1, -1, -1),
];

#[test]
fn slot_locators_equal_what_linux_returns() {
    let opt = |v: i32| if v < 0 { None } else { Some(v as usize) };
    for (gen, render, mi_mode, bb, gpr0, per_ctx, ind_ptr, ind_off) in SLOTS {
        assert_eq!(lrc_ring_mi_mode(gen, render), opt(mi_mode), "mi_mode gen{gen} render={render}");
        assert_eq!(lrc_ring_bb_offset(gen, render), opt(bb), "bb_offset gen{gen} render={render}");
        assert_eq!(lrc_ring_gpr0(gen, render), opt(gpr0), "gpr0 gen{gen} render={render}");
        assert_eq!(lrc_ring_wa_bb_per_ctx(gen, render), opt(per_ctx), "wa_bb_per_ctx gen{gen} render={render}");
        assert_eq!(lrc_ring_indirect_ptr(gen, render), opt(ind_ptr), "indirect_ptr gen{gen} render={render}");
        assert_eq!(lrc_ring_indirect_offset(gen, render), opt(ind_off), "indirect_offset gen{gen} render={render}");
    }
}

#[test]
fn a_malformed_table_is_refused_by_name_never_half_decoded() {
    let mut page = [0u32; 1024];
    // an LRI whose registers are missing
    let e = set_offsets(&mut page, &[Item::Lri { count: 2, posted: false }, Item::Reg(0x244)], "t", 0, 9, false).unwrap_err();
    assert_eq!(e, SubmitError::LrcTableMalformed { table: "t", at: 2 });
    assert_eq!(e.name(), "LRC-TABLE-MALFORMED");
    // an LRI of zero registers (Linux GEM_BUG_ON(!count)) and one too large to encode (count >= BIT(6))
    assert_eq!(set_offsets(&mut page, &[Item::Lri { count: 0, posted: false }], "t", 0, 9, false).unwrap_err().name(), "LRC-TABLE-MALFORMED");
    assert_eq!(set_offsets(&mut page, &[Item::Lri { count: 64, posted: false }], "t", 0, 9, false).unwrap_err().name(), "LRC-TABLE-MALFORMED");
    // a register outside an LRI, and a register offset that is not dword aligned
    assert_eq!(set_offsets(&mut page, &[Item::Reg(0x244)], "t", 0, 9, false).unwrap_err().name(), "LRC-TABLE-MALFORMED");
    assert_eq!(set_offsets(&mut page, &[Item::Lri { count: 1, posted: false }, Item::Reg(0x245)], "t", 0, 9, false).unwrap_err().name(), "LRC-TABLE-MALFORMED");
}

#[test]
fn a_table_that_would_run_off_the_state_page_is_refused_not_wrapped() {
    let mut small = [0u32; 8];
    // NOP(7) then an LRI header lands on dword 7, its register address on dword 8 = one past the end
    let e = set_offsets(&mut small, &[Item::Nop(7), Item::Lri { count: 1, posted: false }, Item::Reg(0x28)], "t", 0, 9, false).unwrap_err();
    assert_eq!(e, SubmitError::LrcOverrun { index: 8, limit: 8 });
    assert_eq!(e.name(), "LRC-OVERRUN");
    // a header past the end
    assert_eq!(set_offsets(&mut small, &[Item::Nop(8), Item::Lri { count: 1, posted: false }, Item::Reg(0x28)], "t", 0, 9, false).unwrap_err(), SubmitError::LrcOverrun { index: 8, limit: 8 });
    // the closing MI_BATCH_BUFFER_END past the end
    assert_eq!(set_offsets(&mut small, &[Item::Nop(8)], "t", 0, 9, true).unwrap_err(), SubmitError::LrcOverrun { index: 8, limit: 8 });
    // the exact fit is fine
    set_offsets(&mut small, &[Item::Nop(6), Item::Lri { count: 1, posted: false }, Item::Reg(0x28)], "t", 0, 9, false).unwrap();
}

#[test]
fn the_gen11_bits_are_what_distinguish_gen8_from_gen11_decoding() {
    // intel_lrc.c:81-82: `if (GRAPHICS_VER >= 11) *regs |= MI_LRI_LRM_CS_MMIO`, and the close dword gets BIT(0) (:101-103).
    let mut p9 = [0u32; 16];
    let mut p11 = [0u32; 16];
    let t = [Item::Lri { count: 1, posted: false }, Item::Reg(0x28)];
    set_offsets(&mut p9, &t, "t", 0, 9, true).unwrap();
    set_offsets(&mut p11, &t, "t", 0, 11, true).unwrap();
    assert_eq!(p9[0], 0x1100_0001);
    assert_eq!(p11[0], 0x1108_0001);
    assert_eq!(p9[3], 0x0500_0000);
    assert_eq!(p11[3], 0x0500_0001);
}
