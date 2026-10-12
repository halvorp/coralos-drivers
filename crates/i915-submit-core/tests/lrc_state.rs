// SPDX-License-Identifier: GPL-2.0-only
//! The context image against the dwords Linux's OWN `__lrc_init_regs` writes (intel_lrc.c:679-951 compiled by GCC around stubbed
//! `intel_context` / `intel_engine_cs` structures; the real intel_engine_regs.h, intel_gt_regs.h, intel_gpu_commands.h and
//! intel_lrc_reg.h supply every constant), the ELSP descriptor against `lrc_descriptor`, and RPCS against the real
//! `intel_sseu_make_rpcs`. Each list is every dword the Linux evaluation changed, `(index, value)`: from a zeroed page for the
//! `inhibit` cases, from a page poisoned with 0xa5a5a5a5 otherwise (so a dword the init must NOT touch is visible).
use i915_devinfo_core::sseu::SseuInfo;
use i915_submit_core::lrc_reg::*;
use i915_submit_core::lrc_state::*;
use i915_submit_core::ring::Ring;
use i915_submit_core::rpcs::*;
use i915_submit_core::wa_bb::{WaBb, WaCtx};
use i915_submit_core::SubmitError;

/// kbl_rcs_inhibit gen=9 render=1 base=0x2000 inhibit=1 pml4=0x1234567000 wa=(0,0xc0,ggtt 0x7000)
const KBL_RCS_INHIBIT: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00002244), (0x3, 0x000f0009), (0x4, 0x00002034), (0x6, 0x00002030), (0x8, 0x00002038),
    (0xa, 0x0000203c), (0xc, 0x00002168), (0xe, 0x00002140), (0x10, 0x00002110), (0x12, 0x0000211c), (0x14, 0x00002114),
    (0x16, 0x00002118), (0x18, 0x000021c0), (0x1a, 0x000021c4), (0x1b, 0x00007003), (0x1c, 0x000021c8), (0x1d, 0x00000980),
    (0x21, 0x11001011), (0x22, 0x000023a8), (0x24, 0x0000228c), (0x26, 0x00002288), (0x28, 0x00002284), (0x2a, 0x00002280),
    (0x2c, 0x0000227c), (0x2e, 0x00002278), (0x30, 0x00002274), (0x31, 0x00000012), (0x32, 0x00002270), (0x33, 0x34567000),
    (0x41, 0x11000001), (0x42, 0x000020c8), (0x51, 0x11001057), (0x52, 0x00002028), (0x54, 0x0000209c), (0x55, 0x01000000),
    (0x56, 0x000020c0), (0x58, 0x00002178), (0x5a, 0x0000217c), (0x5c, 0x00002358), (0x5e, 0x00002170), (0x60, 0x00002150),
    (0x62, 0x00002154), (0x64, 0x00002158), (0x66, 0x0000241c), (0x68, 0x00002600), (0x6a, 0x00002604), (0x6c, 0x00002608),
    (0x6e, 0x0000260c), (0x70, 0x00002610), (0x72, 0x00002614), (0x74, 0x00002618), (0x76, 0x0000261c), (0x78, 0x00002620),
    (0x7a, 0x00002624), (0x7c, 0x00002628), (0x7e, 0x0000262c), (0x80, 0x00002630), (0x82, 0x00002634), (0x84, 0x00002638),
    (0x86, 0x0000263c), (0x88, 0x00002640), (0x8a, 0x00002644), (0x8c, 0x00002648), (0x8e, 0x0000264c), (0x90, 0x00002650),
    (0x92, 0x00002654), (0x94, 0x00002658), (0x96, 0x0000265c), (0x98, 0x00002660), (0x9a, 0x00002664), (0x9c, 0x00002668),
    (0x9e, 0x0000266c), (0xa0, 0x00002670), (0xa2, 0x00002674), (0xa4, 0x00002678), (0xa6, 0x0000267c), (0xa8, 0x00002068),
    (0xaa, 0x05000000),
];
/// kbl_rcs_default gen=9 render=1 base=0x2000 inhibit=0 pml4=0x1234567000 wa=(0,0xc0,ggtt 0x7000)
const KBL_RCS_DEFAULT: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00002244), (0x3, 0x000f0008), (0x4, 0x00002034), (0x6, 0x00002030), (0x8, 0x00002038),
    (0xa, 0x0000203c), (0xc, 0x00002168), (0xe, 0x00002140), (0x10, 0x00002110), (0x12, 0x0000211c), (0x14, 0x00002114),
    (0x16, 0x00002118), (0x18, 0x000021c0), (0x1a, 0x000021c4), (0x1b, 0x00007003), (0x1c, 0x000021c8), (0x1d, 0x00000980),
    (0x21, 0x11001011), (0x22, 0x000023a8), (0x23, 0x00000000), (0x24, 0x0000228c), (0x26, 0x00002288), (0x28, 0x00002284),
    (0x2a, 0x00002280), (0x2c, 0x0000227c), (0x2e, 0x00002278), (0x30, 0x00002274), (0x31, 0x00000012), (0x32, 0x00002270),
    (0x33, 0x34567000), (0x41, 0x11000001), (0x42, 0x000020c8), (0x51, 0x11001057), (0x52, 0x00002028), (0x54, 0x0000209c),
    (0x55, 0xa5a5a4a5), (0x56, 0x000020c0), (0x58, 0x00002178), (0x5a, 0x0000217c), (0x5c, 0x00002358), (0x5e, 0x00002170),
    (0x60, 0x00002150), (0x62, 0x00002154), (0x64, 0x00002158), (0x65, 0x00000000), (0x66, 0x0000241c), (0x68, 0x00002600),
    (0x6a, 0x00002604), (0x6c, 0x00002608), (0x6e, 0x0000260c), (0x70, 0x00002610), (0x72, 0x00002614), (0x74, 0x00002618),
    (0x76, 0x0000261c), (0x78, 0x00002620), (0x7a, 0x00002624), (0x7c, 0x00002628), (0x7e, 0x0000262c), (0x80, 0x00002630),
    (0x82, 0x00002634), (0x84, 0x00002638), (0x86, 0x0000263c), (0x88, 0x00002640), (0x8a, 0x00002644), (0x8c, 0x00002648),
    (0x8e, 0x0000264c), (0x90, 0x00002650), (0x92, 0x00002654), (0x94, 0x00002658), (0x96, 0x0000265c), (0x98, 0x00002660),
    (0x9a, 0x00002664), (0x9c, 0x00002668), (0x9e, 0x0000266c), (0xa0, 0x00002670), (0xa2, 0x00002674), (0xa4, 0x00002678),
    (0xa6, 0x0000267c), (0xa8, 0x00002068),
];
/// kbl_bcs_inhibit gen=9 render=0 base=0x22000 inhibit=1 pml4=0xabcde000 wa=(0,0,ggtt 0)
const KBL_BCS_INHIBIT: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00022244), (0x3, 0x000f0009), (0x4, 0x00022034), (0x6, 0x00022030), (0x8, 0x00022038),
    (0xa, 0x0002203c), (0xc, 0x00022168), (0xe, 0x00022140), (0x10, 0x00022110), (0x12, 0x0002211c), (0x14, 0x00022114),
    (0x16, 0x00022118), (0x18, 0x000221c0), (0x1a, 0x000221c4), (0x1c, 0x000221c8), (0x21, 0x11001011), (0x22, 0x000223a8),
    (0x24, 0x0002228c), (0x26, 0x00022288), (0x28, 0x00022284), (0x2a, 0x00022280), (0x2c, 0x0002227c), (0x2e, 0x00022278),
    (0x30, 0x00022274), (0x32, 0x00022270), (0x33, 0xabcde000), (0x41, 0x11001001), (0x42, 0x00022200), (0x51, 0x11001057),
    (0x52, 0x00022028), (0x54, 0x0002209c), (0x55, 0x01000000), (0x56, 0x000220c0), (0x58, 0x00022178), (0x5a, 0x0002217c),
    (0x5c, 0x00022358), (0x5e, 0x00022170), (0x60, 0x00022150), (0x62, 0x00022154), (0x64, 0x00022158), (0x66, 0x0002241c),
    (0x68, 0x00022600), (0x6a, 0x00022604), (0x6c, 0x00022608), (0x6e, 0x0002260c), (0x70, 0x00022610), (0x72, 0x00022614),
    (0x74, 0x00022618), (0x76, 0x0002261c), (0x78, 0x00022620), (0x7a, 0x00022624), (0x7c, 0x00022628), (0x7e, 0x0002262c),
    (0x80, 0x00022630), (0x82, 0x00022634), (0x84, 0x00022638), (0x86, 0x0002263c), (0x88, 0x00022640), (0x8a, 0x00022644),
    (0x8c, 0x00022648), (0x8e, 0x0002264c), (0x90, 0x00022650), (0x92, 0x00022654), (0x94, 0x00022658), (0x96, 0x0002265c),
    (0x98, 0x00022660), (0x9a, 0x00022664), (0x9c, 0x00022668), (0x9e, 0x0002266c), (0xa0, 0x00022670), (0xa2, 0x00022674),
    (0xa4, 0x00022678), (0xa6, 0x0002267c), (0xa8, 0x00022068), (0xaa, 0x05000000),
];
/// kbl_vcs0_default gen=9 render=0 base=0x12000 inhibit=0 pml4=0xabcde000 wa=(0,0,ggtt 0)
const KBL_VCS0_DEFAULT: &[(usize, u32)] = &[
    (0x1, 0x1100101b), (0x2, 0x00012244), (0x3, 0x000f0008), (0x4, 0x00012034), (0x6, 0x00012030), (0x8, 0x00012038),
    (0xa, 0x0001203c), (0xc, 0x00012168), (0xe, 0x00012140), (0x10, 0x00012110), (0x12, 0x0001211c), (0x14, 0x00012114),
    (0x16, 0x00012118), (0x18, 0x000121c0), (0x1a, 0x000121c4), (0x1c, 0x000121c8), (0x21, 0x11001011), (0x22, 0x000123a8),
    (0x23, 0x00000007), (0x24, 0x0001228c), (0x26, 0x00012288), (0x28, 0x00012284), (0x2a, 0x00012280), (0x2c, 0x0001227c),
    (0x2e, 0x00012278), (0x30, 0x00012274), (0x31, 0x00000000), (0x32, 0x00012270), (0x33, 0xabcde000), (0x41, 0x11001001),
    (0x42, 0x00012200), (0x51, 0x11001057), (0x52, 0x00012028), (0x54, 0x0001209c), (0x55, 0xa5a5a4a5), (0x56, 0x000120c0),
    (0x58, 0x00012178), (0x5a, 0x0001217c), (0x5c, 0x00012358), (0x5e, 0x00012170), (0x60, 0x00012150), (0x62, 0x00012154),
    (0x64, 0x00012158), (0x65, 0x00000000), (0x66, 0x0001241c), (0x68, 0x00012600), (0x6a, 0x00012604), (0x6c, 0x00012608),
    (0x6e, 0x0001260c), (0x70, 0x00012610), (0x72, 0x00012614), (0x74, 0x00012618), (0x76, 0x0001261c), (0x78, 0x00012620),
    (0x7a, 0x00012624), (0x7c, 0x00012628), (0x7e, 0x0001262c), (0x80, 0x00012630), (0x82, 0x00012634), (0x84, 0x00012638),
    (0x86, 0x0001263c), (0x88, 0x00012640), (0x8a, 0x00012644), (0x8c, 0x00012648), (0x8e, 0x0001264c), (0x90, 0x00012650),
    (0x92, 0x00012654), (0x94, 0x00012658), (0x96, 0x0001265c), (0x98, 0x00012660), (0x9a, 0x00012664), (0x9c, 0x00012668),
    (0x9e, 0x0001266c), (0xa0, 0x00012670), (0xa2, 0x00012674), (0xa4, 0x00012678), (0xa6, 0x0001267c), (0xa8, 0x00012068),
];

const POISON: u32 = 0xa5a5_a5a5;

fn expect(base: u32, list: &[(usize, u32)]) -> Vec<u32> {
    let mut page = vec![base; 1024];
    for (i, v) in list {
        page[*i] = *v;
    }
    page
}

fn params(render: bool, mmio_base: u32, pml4: u64, wa: Option<(u32, WaCtx)>, runtime_last: u32) -> CtxParams {
    CtxParams { gen: 9, render, mmio_base, pml4, ring_ggtt: 0x0010_0000, rpcs: 0, runtime_last, wa_ctx: wa }
}

fn wa_192() -> Option<(u32, WaCtx)> {
    Some((0x7000, WaCtx { indirect_ctx: WaBb { offset: 0, size: 192 }, per_ctx: WaBb { offset: 192, size: 0 } }))
}

#[test]
fn render_context_without_a_default_state_matches_linux_dword_for_dword() {
    let p = params(true, 0x2000, 0x12_3456_7000, wa_192(), 0);
    let mut regs = RegState::zeroed();
    init_regs(&mut regs, &p, true).unwrap();
    assert_eq!(regs.as_slice(), &expect(0, KBL_RCS_INHIBIT)[..]);
}

#[test]
fn render_context_over_a_default_state_leaves_every_other_dword_alone() {
    let p = params(true, 0x2000, 0x12_3456_7000, wa_192(), 0);
    let mut regs = RegState::from_default_state(&[POISON; PAGE_DWORDS]);
    init_regs(&mut regs, &p, false).unwrap();
    assert_eq!(regs.as_slice(), &expect(POISON, KBL_RCS_DEFAULT)[..]);
}

#[test]
fn blitter_context_without_a_default_state_matches_linux() {
    let p = params(false, 0x22000, 0xabcde000, None, 0);
    let mut regs = RegState::zeroed();
    init_regs(&mut regs, &p, true).unwrap();
    assert_eq!(regs.as_slice(), &expect(0, KBL_BCS_INHIBIT)[..]);
}

#[test]
fn video_context_over_a_default_state_matches_linux_and_carries_the_runtime_timestamp() {
    let p = params(false, 0x12000, 0xabcde000, None, 7);
    let mut regs = RegState::from_default_state(&[POISON; PAGE_DWORDS]);
    init_regs(&mut regs, &p, false).unwrap();
    assert_eq!(regs.as_slice(), &expect(POISON, KBL_VCS0_DEFAULT)[..]);
    assert_eq!(regs.get(CTX_TIMESTAMP), 7);
}

#[test]
fn the_pml4_lands_in_pdp0_split_upper_and_lower() {
    let p = params(true, 0x2000, 0xfedc_ba98_7654_3000, None, 0);
    let mut regs = RegState::zeroed();
    init_regs(&mut regs, &p, true).unwrap();
    assert_eq!(regs.get(CTX_PDP0_UDW), 0xfedc_ba98);
    assert_eq!(regs.get(CTX_PDP0_LDW), 0x7654_3000);
    // the other PDP slots stay zero: "other PDP Descriptors are ignored" with a 4-level table (intel_lrc.c:894-897)
    for idx in [CTX_PDP1_UDW, CTX_PDP1_LDW, CTX_PDP2_UDW, CTX_PDP2_LDW, CTX_PDP3_UDW, CTX_PDP3_LDW] {
        assert_eq!(regs.get(idx), 0);
    }
}

#[test]
fn the_indirect_context_pointer_and_offset_follow_the_batch() {
    let p = params(true, 0x2000, 0, wa_192(), 0);
    let mut regs = RegState::zeroed();
    init_regs(&mut regs, &p, true).unwrap();
    // regs[lrc_ring_indirect_ptr + 1] = addr | size / 64 ; regs[lrc_ring_indirect_offset + 1] = 0x26 << 6 (intel_lrc.c:801-816)
    assert_eq!(regs.get(0x1a + 1), 0x7000 | 3);
    assert_eq!(regs.get(0x1c + 1), 0x26 << 6);
    assert_eq!(regs.get(0x1c + 1), 0x980);
    // no per-context batch: size 0 leaves the slot alone
    assert_eq!(regs.get(0x18 + 1), 0);
    // a per-context batch, were there one, is `(ggtt + offset) | 1`
    let mut with_per = p;
    with_per.wa_ctx = Some((0x7000, WaCtx { indirect_ctx: WaBb { offset: 0, size: 192 }, per_ctx: WaBb { offset: 192, size: 64 } }));
    let mut regs = RegState::zeroed();
    init_regs(&mut regs, &with_per, true).unwrap();
    assert_eq!(regs.get(0x18 + 1), (0x7000 + 192) | 1);
}

#[test]
fn an_indirect_batch_that_is_not_whole_cachelines_is_refused() {
    let mut p = params(true, 0x2000, 0, None, 0);
    p.wa_ctx = Some((0x7000, WaCtx { indirect_ctx: WaBb { offset: 0, size: 100 }, per_ctx: WaBb::default() }));
    let mut regs = RegState::zeroed();
    assert_eq!(init_regs(&mut regs, &p, true).unwrap_err().name(), "LRC-TABLE-MALFORMED");
    p.wa_ctx = Some((0x7020, WaCtx { indirect_ctx: WaBb { offset: 0, size: 128 }, per_ctx: WaBb::default() }));
    assert_eq!(init_regs(&mut regs, &p, true).unwrap_err(), SubmitError::GgttUnaligned { what: "indirect ctx batch", addr: 0x7020 });
}

#[test]
fn only_gen8_and_gen9_are_carried() {
    let mut p = params(true, 0x2000, 0, None, 0);
    let mut regs = RegState::zeroed();
    for gen in [7u8, 10, 11, 12] {
        p.gen = gen;
        assert_eq!(init_regs(&mut regs, &p, true).unwrap_err(), SubmitError::UnsupportedGeneration { gen });
        assert_eq!(descriptor(gen, 0x1000).unwrap_err(), SubmitError::UnsupportedGeneration { gen });
    }
}

/// (gen, lrca, lrc_descriptor(), lrc_descriptor() | CTX_DESC_FORCE_RESTORE): Linux's expression, GCC-evaluated.
const DESCRIPTORS: [(u8, u32, u32, u32); 8] = [
    (8, 0, 0x139, 0x13d),
    (8, 0x1000, 0x1139, 0x113d),
    (8, 0x12345000, 0x12345139, 0x1234513d),
    (8, 0xfffff000, 0xfffff139, 0xfffff13d),
    (9, 0, 0x119, 0x11d),
    (9, 0x1000, 0x1119, 0x111d),
    (9, 0x12345000, 0x12345119, 0x1234511d),
    (9, 0xfffff000, 0xfffff119, 0xfffff11d),
];

#[test]
fn descriptors_equal_lrc_descriptor() {
    for (gen, lrca, desc, forced) in DESCRIPTORS {
        assert_eq!(descriptor(gen, lrca).unwrap(), desc, "gen{gen} lrca {lrca:#x}");
        assert_eq!(desc | CTX_DESC_FORCE_RESTORE as u32, forced);
    }
}

#[test]
fn a_descriptor_for_an_unaligned_lrca_is_refused() {
    for lrca in [1u32, 0x800, 0x1001, 0xfff] {
        assert_eq!(descriptor(9, lrca).unwrap_err(), SubmitError::LrcaUnaligned { lrca });
    }
    assert_eq!(descriptor(9, 0x7000_0000).unwrap(), 0x7000_0119);
}

/// (ring size, RING_CTL_SIZE(size) | RING_VALID): intel_lrc.c:1550.
const CTLS: [(u32, u32); 3] = [
    (0x1000, 0x1),
    (0x2000, 0x1001),
    (0x200000, 0x1ff001),
];

#[test]
fn update_regs_points_the_image_at_the_ring() {
    let p = params(true, 0x2000, 0, None, 0);
    for (size, ctl) in CTLS {
        let ring = Ring::new(size).unwrap();
        let mut regs = RegState::zeroed();
        let mut p = p;
        p.rpcs = 0x8005_8000;
        let d = update_regs(&mut regs, &p, 0x12345000, &ring, 0).unwrap();
        assert_eq!(d, 0x1234511d, "descriptor | CTX_DESC_FORCE_RESTORE");
        assert_eq!(regs.get(CTX_RING_START), 0x0010_0000);
        assert_eq!(regs.get(CTX_RING_HEAD), 0);
        assert_eq!(regs.get(CTX_RING_TAIL), 0);
        assert_eq!(regs.get(CTX_RING_CTL), ctl);
        assert_eq!(regs.get(CTX_R_PWR_CLK_STATE), 0x8005_8000, "render engines carry the RPCS");
        // a non-render engine does not touch that slot
        let mut q = p;
        q.render = false;
        let mut regs = RegState::zeroed();
        update_regs(&mut regs, &q, 0x12345000, &ring, 0).unwrap();
        assert_eq!(regs.get(CTX_R_PWR_CLK_STATE), 0);
    }
}

#[test]
fn update_regs_refuses_an_unaligned_ring_or_head() {
    let ring = Ring::new(4096).unwrap();
    let mut p = params(true, 0x2000, 0, None, 0);
    let mut regs = RegState::zeroed();
    assert_eq!(update_regs(&mut regs, &p, 0x12345000, &ring, 4).unwrap_err(), SubmitError::RingOffsetInvalid { pos: 4, size: 4096 });
    assert_eq!(update_regs(&mut regs, &p, 0x12345000, &ring, 4096).unwrap_err(), SubmitError::RingOffsetInvalid { pos: 4096, size: 4096 });
    p.ring_ggtt = 0x10_0800;
    assert_eq!(update_regs(&mut regs, &p, 0x12345000, &ring, 0).unwrap_err(), SubmitError::GgttUnaligned { what: "ring", addr: 0x10_0800 });
}

fn sseu(slice_pg: bool, subslice_pg: bool, eu_pg: bool) -> SseuInfo {
    SseuInfo {
        slice_mask: 1,
        max_slices: 1,
        max_subslices: 3,
        max_eus_per_subslice: 8,
        subslice_mask: [0xf, 0, 0],
        eu_mask: [[0; 4]; 3],
        subslice_7eu: [0; 3],
        eu_total: 24,
        eu_per_subslice: 8,
        has_slice_pg: slice_pg,
        has_subslice_pg: subslice_pg,
        has_eu_pg: eu_pg,
    }
}

/// ((has_slice_pg, has_subslice_pg, has_eu_pg), (slice_mask, subslice_mask, min_eus, max_eus), rpcs): the real
/// `intel_sseu_make_rpcs` (intel_sseu.c:654-760) for graphics version 9, GCC-evaluated.
const RPCS: [((bool, bool, bool), (u32, u32, u32, u32), u32); 32] = [
((false, false, false), (1, 0x7, 2, 8), 0x00000000),
    ((false, false, false), (7, 0xf, 2, 8), 0x00000000),
    ((false, false, false), (1, 0x1, 8, 8), 0x00000000),
    ((false, false, false), (3, 0x3, 0, 1), 0x00000000),
    ((true, false, false), (1, 0x7, 2, 8), 0x80048000),
    ((true, false, false), (7, 0xf, 2, 8), 0x80058000),
    ((true, false, false), (1, 0x1, 8, 8), 0x80048000),
    ((true, false, false), (3, 0x3, 0, 1), 0x80050000),
    ((false, true, false), (1, 0x7, 2, 8), 0x80000b00),
    ((false, true, false), (7, 0xf, 2, 8), 0x80000c00),
    ((false, true, false), (1, 0x1, 8, 8), 0x80000900),
    ((false, true, false), (3, 0x3, 0, 1), 0x80000a00),
    ((true, true, false), (1, 0x7, 2, 8), 0x80048b00),
    ((true, true, false), (7, 0xf, 2, 8), 0x80058c00),
    ((true, true, false), (1, 0x1, 8, 8), 0x80048900),
    ((true, true, false), (3, 0x3, 0, 1), 0x80050a00),
    ((false, false, true), (1, 0x7, 2, 8), 0x80000082),
    ((false, false, true), (7, 0xf, 2, 8), 0x80000082),
    ((false, false, true), (1, 0x1, 8, 8), 0x80000088),
    ((false, false, true), (3, 0x3, 0, 1), 0x80000010),
    ((true, false, true), (1, 0x7, 2, 8), 0x80048082),
    ((true, false, true), (7, 0xf, 2, 8), 0x80058082),
    ((true, false, true), (1, 0x1, 8, 8), 0x80048088),
    ((true, false, true), (3, 0x3, 0, 1), 0x80050010),
    ((false, true, true), (1, 0x7, 2, 8), 0x80000b82),
    ((false, true, true), (7, 0xf, 2, 8), 0x80000c82),
    ((false, true, true), (1, 0x1, 8, 8), 0x80000988),
    ((false, true, true), (3, 0x3, 0, 1), 0x80000a10),
    ((true, true, true), (1, 0x7, 2, 8), 0x80048b82),
    ((true, true, true), (7, 0xf, 2, 8), 0x80058c82),
    ((true, true, true), (1, 0x1, 8, 8), 0x80048988),
    ((true, true, true), (3, 0x3, 0, 1), 0x80050a10),
];

#[test]
fn rpcs_equals_intel_sseu_make_rpcs() {
    for ((s, ss, eu), (slice, subslice, emin, emax), want) in RPCS {
        let req = SseuRequest { slice_mask: slice as u8, subslice_mask: subslice as u8, min_eus_per_subslice: emin as u8, max_eus_per_subslice: emax as u8 };
        assert_eq!(make_rpcs_gen9(&sseu(s, ss, eu), &req).unwrap(), want, "pg=({s},{ss},{eu}) req=({slice},{subslice:#x},{emin},{emax})");
    }
}

#[test]
fn a_request_wider_than_the_rpcs_field_is_refused_by_name() {
    // 8 slices do not fit the 3-bit S_CNT field (GEM_BUG_ON(val & ~mask), intel_sseu.c:731)
    let req = SseuRequest { slice_mask: 0xff, subslice_mask: 1, min_eus_per_subslice: 1, max_eus_per_subslice: 1 };
    let e = make_rpcs_gen9(&sseu(true, false, false), &req).unwrap_err();
    assert_eq!(e, SubmitError::RpcsFieldOverflow { field: "slice count", value: 8 });
    assert_eq!(e.name(), "RPCS-FIELD-OVERFLOW");
    // 8 subslices do not fit the 3-bit SS_CNT field
    let req = SseuRequest { slice_mask: 1, subslice_mask: 0xff, min_eus_per_subslice: 1, max_eus_per_subslice: 1 };
    assert_eq!(make_rpcs_gen9(&sseu(false, true, false), &req).unwrap_err().name(), "RPCS-FIELD-OVERFLOW");
    // 16 EUs do not fit the 4-bit field
    let req = SseuRequest { slice_mask: 1, subslice_mask: 1, min_eus_per_subslice: 1, max_eus_per_subslice: 16 };
    assert_eq!(make_rpcs_gen9(&sseu(false, false, true), &req).unwrap_err().name(), "RPCS-FIELD-OVERFLOW");
}

#[test]
fn the_default_request_is_the_whole_device() {
    let s = sseu(true, true, true);
    let r = sseu_from_device_info(&s);
    // intel_sseu_from_device_info sets BOTH eu bounds to max_eus_per_subslice (intel_sseu.h:113-114)
    assert_eq!(r, SseuRequest { slice_mask: 1, subslice_mask: 0xf, min_eus_per_subslice: 8, max_eus_per_subslice: 8 });
}
