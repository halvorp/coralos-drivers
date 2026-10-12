// SPDX-License-Identifier: GPL-2.0-only
//! The Kaby Lake workaround lists against Linux. Every expected (reg, clr, set, read) below is a number GCC evaluated from the
//! Linux headers (`_MASKED_BIT_ENABLE(..)`, `GEN8_MCR_SLICE(..)`, register macros) over the OR-merge `_wa_add` performs; the
//! expected side is Linux's, the actual side travels through the crate's tables and the merge code.
use i915_common_core::Reg;
use i915_devinfo_core::{lookup, Platform};
use i915_gt_core::engine::{engine_info, engines_for, EngineClass};
use i915_gt_core::sseu::{gen9_mcr_steer, gen9_sseu_info};
use i915_gt_core::step::{kbl_graphics_step, Step};
use i915_gt_core::wa::{lost_count, plan_apply, verify, Wa, WaList};
use i915_gt_core::wa_tables::*;
use i915_gt_core::{Effect, GtError, RmwPlan, WritePlan};

const RCS0: u32 = 0;
const BCS0: u32 = 1;
const VCS0: u32 = 10;
const VCS1: u32 = 11;
const VECS0: u32 = 18;

fn ctx(step: Step) -> WaContext {
    WaContext { platform: Platform::Kabylake, graphics_ver: 9, has_llc: true, step }
}

/// (reg, clr, set, read, masked, mcr)
type Want = (u32, u32, u32, u32, bool, bool);

fn check<const N: usize>(wal: &WaList<N>, want: &[Want], wa_count: u32) {
    let got: Vec<Want> = wal.entries().iter().map(|w| (w.reg.offset(), w.clr, w.set, w.read, w.masked, w.mcr)).collect();
    assert_eq!(got, want, "{} list on {}", wal.name, wal.engine);
    assert_eq!(wal.wa_count, wa_count, "{} wa_count", wal.name);
    assert_eq!(wal.overwrites, 0);
}

fn rcs() -> i915_gt_core::engine::EngineInfo {
    engine_info(9, RCS0).unwrap()
}

// context list: expected from `_MASKED_BIT_ENABLE/DISABLE/_MASKED_FIELD` over the Linux macros
fn ctx_want(pushconst: bool) -> [Want; 10] {
    [
        (0x2580, 0, 0x0007_0004, 0x0000_0007, true, false),
        (0x7004, 0, 0x0042_0042, 0x0000_0042, true, false),
        (0x7014, 0, if pushconst { 0x2100_2100 } else { 0x2000_2000 }, if pushconst { 0x2100 } else { 0x2000 }, true, false),
        (0x7300, 0, 0x8030_8030, 0x0000_8030, true, false),
        (0xe100, 0, 0x0010_0010, 0x0000_0010, true, true),
        (0xe180, 0, 0x2000_2000, 0x0000_2000, true, true),
        (0xe184, 0, 0x0002_0002, 0x0000_0002, true, true),
        (0xe188, 0, 0x0008_0000, 0x0000_0008, true, true),
        (0xe194, 0, 0x0114_0114, 0x0000_0114, true, true),
        (0xe4f0, 0, 0x8100_8100, 0x0000_8100, true, true),
    ]
}

#[test]
fn kbl_context_list_at_c0_or_later() {
    for revid in [2u8, 3, 4, 5, 6, 7] {
        let step = kbl_graphics_step(revid);
        let wal = kbl_ctx_workarounds(&ctx(step), &rcs()).unwrap();
        check(&wal, &ctx_want(true), 14);
    }
}

#[test]
fn kbl_context_list_before_c0_lacks_the_push_constant_fix() {
    // revid 1 = B0: WaToEnableHwFixForPushConstHWBug:kbl needs STEP_C0.. (intel_workarounds.c:615)
    let wal = kbl_ctx_workarounds(&ctx(kbl_graphics_step(1)), &rcs()).unwrap();
    check(&wal, &ctx_want(false), 13);
}

#[test]
fn context_list_is_render_only() {
    for id in [BCS0, VCS0, VCS1, VECS0] {
        let wal = kbl_ctx_workarounds(&ctx(Step::C0), &engine_info(9, id).unwrap()).unwrap();
        assert_eq!(wal.count(), 0, "engine {id}");
    }
}

#[test]
fn no_llc_drops_the_compressed_hash_rows() {
    let mut c = ctx(Step::C0);
    c.has_llc = false;
    let wal = kbl_ctx_workarounds(&c, &rcs()).unwrap();
    // COMMON_SLICE_CHICKEN2 keeps only the push-constant bit; GEN9_HALF_SLICE_CHICKEN7 loses the sampler-hash bit
    assert_eq!(wal.find(Reg::new(0x7014)).unwrap().set, 0x0100_0100);
    assert_eq!(wal.find(Reg::new(0xe194)).unwrap().set, 0x0014_0014);
    assert_eq!(wal.wa_count, 12);
}

#[test]
fn gt2_gt_list_at_c0_or_later() {
    // GT2 fuse sample: 1 slice, subslice 3 disabled -> steer slice 0 / subslice 0 -> mcr = GEN8_MCR_SLICE(0)|GEN8_MCR_SUBSLICE(0) = 0
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0]);
    let steer = gen9_mcr_steer(&sseu).unwrap();
    assert_eq!((steer.value, steer.mask), (0x0000_0000, 0x0f00_0000));
    let wal = kbl_gt_workarounds(&ctx(Step::C0), &steer).unwrap();
    let want: [Want; 5] = [
        (0x0fdc, 0x0f00_0000, 0, 0x0f00_0000, false, false),
        (0x4090, 0x0200_0100, 0x0200_0100, 0x0200_0100, false, false),
        (0x4ab0, 0x0004_0000, 0x0004_0000, 0x0004_0000, false, false),
        (0x4ddc, 0x8800_0000, 0x8800_0000, 0x8800_0000, false, false),
        (0x940c, 0x0000_4000, 0x0000_4000, 0x0000_4000, false, false),
    ];
    check(&wal, &want, 6);
}

#[test]
fn gt_list_before_c0_adds_the_dynamic_credit_sharing_fix() {
    let steer = gen9_mcr_steer(&gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0, 0, 0])).unwrap();
    let wal = kbl_gt_workarounds(&ctx(Step::B0), &steer).unwrap();
    assert_eq!(wal.count(), 6);
    assert_eq!(wal.wa_count, 7);
    let e = wal.find(Reg::new(0x4ab8)).unwrap();
    assert_eq!((e.clr, e.set, e.read), (0x1000_0000, 0x1000_0000, 0x1000_0000));
    // sorted by offset: 0x4ab0 < 0x4ab8 < 0x4ddc
    let offs: Vec<u32> = wal.entries().iter().map(|w| w.reg.offset()).collect();
    assert_eq!(offs, [0xfdc, 0x4090, 0x4ab0, 0x4ab8, 0x4ddc, 0x940c]);
}

#[test]
fn mcr_steering_follows_the_first_enabled_slice_and_subslice() {
    // GT3-like: slices 1 and 2 enabled (slice 0 fused off), subslice 1 is the first enabled one of slice 1
    let fuse2 = (0b110u32 << 25) | (0b0001 << 20); // subslice 0 disabled in every slice
    let sseu = gen9_sseu_info(fuse2, [0, 0, 0]);
    let s = gen9_mcr_steer(&sseu).unwrap();
    assert_eq!((s.slice, s.subslice), (1, 1));
    assert_eq!(s.value, 0x0400_0000 | 0x0100_0000); // GEN8_MCR_SLICE(1) | GEN8_MCR_SUBSLICE(1)
    let steer = gen9_mcr_steer(&sseu).unwrap();
    let wal = kbl_gt_workarounds(&ctx(Step::C0), &steer).unwrap();
    let e = wal.find(Reg::new(0xfdc)).unwrap();
    assert_eq!((e.clr, e.set, e.read), (0x0f00_0000, 0x0500_0000, 0x0f00_0000));
    // nothing enabled -> named refusal
    assert_eq!(gen9_mcr_steer(&gen9_sseu_info(0, [0; 3])).unwrap_err().name(), "NO-ENABLED-SLICE");
    let all_ss_off = gen9_sseu_info((0b001 << 25) | (0b1111 << 20), [0; 3]);
    assert_eq!(gen9_mcr_steer(&all_ss_off).unwrap_err().name(), "NO-ENABLED-SUBSLICE");
}

#[test]
fn rcs_engine_list_merges_the_shared_registers() {
    let wal = kbl_engine_workarounds(&ctx(Step::C0), &rcs()).unwrap();
    let want: [Want; 6] = [
        (0x20d4, 0, 0x0004_0004, 0x0000_0004, true, false),
        (0x20e0, 0, 0x4000_4000, 0x0000_4000, true, false),
        (0xb004, 0x80, 0x80, 0x80, false, false),
        (0xb008, 0x1, 0, 0x1, false, false),
        // L3SQCREG4: wa_write_or(FLUSH_COHERENT_LINES) + wa_write_clr_set(NONIA_COHERENT_ATOMICS_ENABLE, 0)
        (0xb118, 0x0060_0000, 0x0020_0000, 0x0060_0000, false, true),
        // BDW_SCRATCH1 (write_or LBS_SLA) + GEN9_SCRATCH1 (clr EVICTION_PERF_FIX): the same register 0xb11c
        (0xb11c, 0x104, 0x4, 0x104, false, true),
    ];
    check(&wal, &want, 8);
}

#[test]
fn xcs_semaphore_poll_workaround_is_stepping_gated_and_per_engine() {
    for (id, base) in [(BCS0, 0x22000u32), (VCS0, 0x12000), (VCS1, 0x1c000), (VECS0, 0x1a000)] {
        let e = engine_info(9, id).unwrap();
        assert_eq!(e.mmio_base, base);
        // WaKBLVECSSemaphoreWaitPoll:kbl, STEP_A0..STEP_F0 (intel_workarounds.c:2691): B0 C0 D0 D1 E0 all in range
        for step in [Step::B0, Step::C0, Step::D0, Step::D1, Step::E0] {
            let wal = kbl_engine_workarounds(&ctx(step), &e).unwrap();
            check(&wal, &[(base + 0x24c, 0xffff_ffff, 1, 0xffff_ffff, false, false)], 1);
        }
        for step in [Step::F0, Step::G0] {
            assert_eq!(kbl_engine_workarounds(&ctx(step), &e).unwrap().count(), 0, "F0 and later are outside [A0, F0)");
        }
    }
}

#[test]
fn whitelist_is_render_only_sorted_and_read_write() {
    let wal = kbl_whitelist(&ctx(Step::C0), &rcs()).unwrap();
    let want: [Want; 5] = [
        (0x2248, 0, 0, 0, false, false), // GEN9_CTX_PREEMPT_REG
        (0x2580, 0, 0, 0, false, false), // GEN8_CS_CHICKEN1
        (0x7014, 0, 0, 0, false, false), // COMMON_SLICE_CHICKEN2
        (0x7304, 0, 0, 0, false, false), // GEN8_HDC_CHICKEN1
        (0xb118, 0, 0, 0, false, true),  // GEN8_L3SQCREG4 (MCR)
    ];
    check(&wal, &want, 5);
    for id in [BCS0, VCS0, VCS1, VECS0] {
        assert_eq!(kbl_whitelist(&ctx(Step::C0), &engine_info(9, id).unwrap()).unwrap().count(), 0);
    }
}

#[test]
fn whitelist_flags_ride_in_the_register_offset_and_are_validated() {
    let mut w = WaList::<16>::new("whitelist", "rcs0");
    // CFL+ read-only range-of-4 entry (intel_workarounds.c:1987): PS_INVOCATION_COUNT 0x2348 | ACCESS_RD | RANGE_4
    whitelist_reg_ext(&mut w, Reg::new(0x2348), false, (1 << 28) | 1).unwrap();
    assert_eq!(w.entries()[0].reg.offset(), 0x2348 | 0x1000_0001);
    // INVALID access (3 << 28) and stray bits are refused by name
    assert_eq!(whitelist_reg_ext(&mut w, Reg::new(0x2000), false, 3 << 28).unwrap_err().name(), "NONPRIV-FLAGS-INVALID");
    assert_eq!(whitelist_reg_ext(&mut w, Reg::new(0x2000), false, 1 << 5).unwrap_err().name(), "NONPRIV-FLAGS-INVALID");
    assert!(nonpriv_flags_valid(0) && nonpriv_flags_valid(1 << 30) && nonpriv_flags_valid(2 << 28 | 3));
    assert!(!nonpriv_flags_valid(1 << 31));
}

#[test]
fn whitelist_is_bounded_at_twelve_slots() {
    let mut w = WaList::<16>::new("whitelist", "rcs0");
    for i in 0..12u32 {
        whitelist_reg_ext(&mut w, Reg::new(0x2000 + 4 * i), false, 0).unwrap();
    }
    assert_eq!(w.count(), 12);
    // RING_MAX_NONPRIV_SLOTS = 12 (intel_engine_regs.h:230): the 13th is a named refusal, not a silent drop
    assert_eq!(whitelist_reg_ext(&mut w, Reg::new(0x3000), false, 0), Err(GtError::NonprivSlotsFull { limit: 12 }));
    assert_eq!(w.count(), 12);
}

#[test]
fn merge_rule_ors_set_clr_read_and_counts_every_call() {
    let mut w = WaList::<4>::new("t", "e");
    let r = Reg::new(0x1234);
    w.write_or(r, false, 0x1).unwrap();
    w.write_or(r, false, 0x10).unwrap();
    let e = *w.find(r).unwrap();
    assert_eq!((e.clr, e.set, e.read), (0x11, 0x11, 0x11));
    assert_eq!((w.count(), w.wa_count, w.overwrites), (1, 2, 0));
}

#[test]
fn overwriting_a_previously_set_bit_is_dropped_and_counted_like_linux() {
    // intel_workarounds.c:184-190: a later clear of bits an earlier entry set removes them from `set`
    let mut w = WaList::<4>::new("t", "e");
    let r = Reg::new(0x1234);
    w.write_or(r, false, 0x3).unwrap(); // clr 3 set 3
    w.write_clr(r, false, 0x1).unwrap(); // clr 1: wa->clr & ~old.clr == 0 -> discard
    let e = *w.find(r).unwrap();
    assert_eq!(w.overwrites, 1);
    assert_eq!((e.clr, e.set), (0x3, 0x2));
}

#[test]
fn lists_are_bounded_and_refuse_by_name() {
    let mut w = WaList::<3>::new("tiny", "e");
    for i in 0..3 {
        w.write_or(Reg::new(0x100 + 4 * i), false, 1).unwrap();
    }
    assert_eq!(w.write_or(Reg::new(0x200), false, 1), Err(GtError::ListFull { list: "tiny", capacity: 3 }));
    assert_eq!(w.count(), 3);
    // a merge into an existing entry needs no room
    w.write_or(Reg::new(0x100), false, 2).unwrap();
}

#[test]
fn masked_helpers_refuse_values_outside_the_16_bit_encoding() {
    let mut w = WaList::<4>::new("t", "e");
    assert_eq!(w.masked_en(Reg::new(0x7000), false, 1 << 16).unwrap_err().name(), "BAD-MASKED-VALUE");
    assert_eq!(w.masked_field_set(Reg::new(0x7000), false, 0x3, 0x4).unwrap_err().name(), "BAD-MASKED-VALUE");
    assert_eq!(w.count(), 0);
}

#[test]
fn apply_plan_is_writes_for_pure_sets_and_rmw_for_clears() {
    let wal = kbl_ctx_workarounds(&ctx(Step::C0), &rcs()).unwrap();
    let plan = plan_apply(&wal).unwrap();
    // every ctx entry has clr == 0 -> an unconditional write of `set` (wa_list_apply: `val != old || !wa->clr`)
    assert_eq!(plan.effects.len(), 10);
    assert!(plan.effects.as_slice().iter().all(|e| matches!(e, Effect::Write(_))));
    assert_eq!(plan.effects.as_slice()[0], Effect::Write(WritePlan { reg: Reg::new(0x2580), value: 0x0007_0004 }));
    assert_eq!(plan.forcewake, 0b001, "all ctx registers are in the RENDER well (0x2000-0x7fff, 0xe000-0xe8ff)");

    let steer = gen9_mcr_steer(&gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0; 3])).unwrap();
    let gt = kbl_gt_workarounds(&ctx(Step::C0), &steer).unwrap();
    let plan = plan_apply(&gt).unwrap();
    assert_eq!(plan.effects.as_slice()[0], Effect::Rmw(RmwPlan { reg: Reg::new(0xfdc), clear: 0x0f00_0000, set: 0, skip_if_unchanged: true }));
    assert_eq!(plan.forcewake, 0b111, "0x940c sits in the RENDER|MEDIA range, the rest in GT");
}

#[test]
fn rmw_skips_an_unchanged_write_only_when_asked() {
    let skip = RmwPlan { reg: Reg::new(0x1), clear: 0xf, set: 0x1, skip_if_unchanged: true };
    assert_eq!(skip.result(0x31), None);
    assert_eq!(skip.result(0x3f), Some(0x31));
    let always = RmwPlan { skip_if_unchanged: false, ..skip };
    assert_eq!(always.result(0x31), Some(0x31), "intel_uncore_rmw writes back unconditionally");
}

#[test]
fn verify_names_the_first_lost_workaround() {
    let wal = kbl_ctx_workarounds(&ctx(Step::C0), &rcs()).unwrap();
    // a register file where every masked workaround reads back as programmed (the masked write-enable half reads 0)
    let good = |r: Reg| wal.find(r).map(|w| w.set & w.read).unwrap_or(0);
    assert_eq!(verify(&wal, good), Ok(10));
    assert_eq!(lost_count(&wal, good), 0);
    // reset-style loss: HDC_CHICKEN0 reverted to its default, and CS_CHICKEN1 too
    let lossy = |r: Reg| if r.offset() == 0x7300 || r.offset() == 0x2580 { 0 } else { good(r) };
    let err = verify(&wal, lossy).unwrap_err();
    assert_eq!(err, GtError::WaLost { list: "context", reg: Reg::new(0x2580), got: 0, relevant: 0, expected: 0x4 });
    assert_eq!(err.name(), "WA-LOST");
    assert_eq!(lost_count(&wal, lossy), 2);
    // only the bits in `read` matter: junk outside the compared bits is fine
    let noisy = |r: Reg| good(r) | wal.find(r).map(|w| !w.read & 0xffff_0000).unwrap_or(0);
    assert_eq!(verify(&wal, noisy), Ok(10));
}

#[test]
fn ctx_wa_lri_payload_matches_intel_engine_emit_ctx_wa() {
    use i915_gt_core::emit::{emit_ctx_wa, CsBuf};
    let wal = kbl_ctx_workarounds(&ctx(Step::C0), &rcs()).unwrap();
    let mut buf = [0u32; 32];
    let mut cs = CsBuf::new(&mut buf);
    // all entries are masked: no register is read (intel_workarounds.c:1028)
    let n = emit_ctx_wa(&mut cs, &wal, |_| panic!("a masked workaround must not read the register")).unwrap();
    assert_eq!(n, 22);
    let w = cs.written();
    assert_eq!(w[0], 0x1100_0013, "MI_LOAD_REGISTER_IMM(10) = MI_INSTR(0x22, 2*10-1)");
    assert_eq!(&w[1..5], &[0x2580, 0x0007_0004, 0x7004, 0x0042_0042]);
    assert_eq!(w[21], 0, "trailing MI_NOOP");
    // a non-masked entry reads the register and merges: clr/set
    let mut w2 = WaList::<4>::new("ctx", "rcs0");
    w2.write_clr_set(Reg::new(0x9000), false, 0xf0, 0x30).unwrap();
    let mut buf = [0u32; 8];
    let mut cs = CsBuf::new(&mut buf);
    emit_ctx_wa(&mut cs, &w2, |r| {
        assert_eq!(r.offset(), 0x9000);
        Some(0xffff)
    })
    .unwrap();
    assert_eq!(cs.written(), [0x1100_0001, 0x9000, 0xff3f, 0]);
    // and refuses by name when the caller cannot supply the read
    let mut buf = [0u32; 8];
    let mut cs = CsBuf::new(&mut buf);
    assert_eq!(emit_ctx_wa(&mut cs, &w2, |_| None).unwrap_err().name(), "CTX-WA-NEEDS-READ");
    assert_eq!(cs.len(), 0);
}

#[test]
fn platform_conditions_gate_rows() {
    // Skylake context: the KBL-only rows do not apply (Cond::KabylakeStep requires the KBL platform)
    let mut c = ctx(Step::C0);
    c.platform = Platform::Skylake;
    assert!(!KBL_CTX_ROWS[0].cond.holds(&c));
    assert!(GEN9_CTX_ROWS[8].cond.holds(&c), "WaDisableSamplerPowerBypassForSOPingPong applies to SKL too");
    c.platform = Platform::Cherryview;
    assert!(!GEN9_CTX_ROWS[8].cond.holds(&c));
    c.graphics_ver = 8;
    assert!(!RCS_ENGINE_ROWS[0].cond.holds(&c), "the per-context preemption control is Gen9+");
    assert!(!RCS_ENGINE_ROWS[2].cond.holds(&c));
}

#[test]
fn every_row_names_its_linux_workaround_and_file_line() {
    let all: Vec<&WaRow> = GEN9_CTX_ROWS
        .iter()
        .chain(KBL_CTX_ROWS.iter())
        .chain(GEN9_GT_ROWS.iter())
        .chain(KBL_GT_ROWS.iter())
        .chain(RCS_ENGINE_ROWS.iter())
        .chain(XCS_ENGINE_ROWS.iter())
        .chain(KBL_WHITELIST_ROWS.iter())
        .collect();
    assert_eq!(all.len(), 34);
    for row in all {
        assert!(!row.name.is_empty());
        assert!(row.src.starts_with("intel_workarounds.c:"), "{}", row.name);
        assert!(row.src["intel_workarounds.c:".len()..].parse::<u32>().is_ok(), "{}: {}", row.name, row.src);
        assert!(!row.linux_reg.is_empty());
    }
}

#[test]
fn row_set_sizes_are_pinned() {
    assert_eq!((GEN9_CTX_ROWS.len(), KBL_CTX_ROWS.len()), (12, 2));
    assert_eq!((GEN9_GT_ROWS.len(), KBL_GT_ROWS.len()), (3, 3));
    assert_eq!((RCS_ENGINE_ROWS.len(), XCS_ENGINE_ROWS.len(), KBL_WHITELIST_ROWS.len()), (8, 1, 5));
}

#[test]
fn pixelbook_go_device_builds_all_four_lists_from_its_descriptor() {
    // 8086:591C = KBL GT2 ULX (AML): rows come from devinfo, the topology from a GT2 fuse sample
    let d = lookup(0x591c).unwrap();
    assert_eq!(d.platform(), Platform::Kabylake);
    let engines = engines_for(&d.info).unwrap();
    let names: Vec<&str> = engines.as_slice().iter().map(|e| e.name).collect();
    assert_eq!(names, ["rcs0", "bcs0", "vcs0", "vecs0"]);
    let c = WaContext { platform: d.platform(), graphics_ver: d.gen(), has_llc: d.info.has_llc, step: kbl_graphics_step(9) };
    assert_eq!(c.step, Step::FUTURE, "a revid past the table is STEP_FUTURE (intel_step.c:249)");
    let sseu = gen9_sseu_info((0b001 << 25) | (0b1000 << 20), [0; 3]);
    let steer = gen9_mcr_steer(&sseu).unwrap();
    let gt = kbl_gt_workarounds(&c, &steer).unwrap();
    assert_eq!(gt.count(), 5, "FUTURE >= C0: no pre-C0 GT row");
    for e in engines.as_slice() {
        let ctxl = kbl_ctx_workarounds(&c, e).unwrap();
        let eng = kbl_engine_workarounds(&c, e).unwrap();
        let wl = kbl_whitelist(&c, e).unwrap();
        if e.class == EngineClass::Render {
            assert_eq!((ctxl.count(), eng.count(), wl.count()), (10, 6, 5));
        } else {
            assert_eq!((ctxl.count(), eng.count(), wl.count()), (0, 0, 0), "FUTURE is outside [A0,F0): no semaphore-poll row");
        }
    }
}

#[allow(dead_code)]
fn _unused(_: Wa) {}
