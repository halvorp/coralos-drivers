// SPDX-License-Identifier: GPL-2.0-only
//! The command emitters against dwords GCC computes from Linux's own intel_gpu_commands.h, using the flag composition written
//! out in gt/gen8_engine_cs.c (the expressions are in the commit's generator transcript; each array below is the evaluated
//! result). Plus the reservation, overflow and alignment refusals.
use i915_gt_core::emit::*;
use i915_gt_core::error::GtError;
use i915_gt_core::step::Step;
use i915_gt_core::wa_tables::WaContext;
use i915_devinfo_core::Platform;

// ---- expected dwords (GCC over intel_gpu_commands.h; scratch addr 0x34*4 = 0xd0; hwsp 0x1040; seqno 0x1234) ----
const FLUSH_ONLY: [u32; 6] = [0x7a000004, 0x001010a1, 0x000000d0, 0x00000000, 0x00000000, 0x00000000];
const INV_GEN9_PLAIN: [u32; 12] = [0x7a000004, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x00344c1c, 0x000000d0, 0x00000000, 0x00000000, 0x00000000];
const INV_KBL_PRE_C0: [u32; 24] = [0x7a000004, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x00000020, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x00344c1c, 0x000000d0, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x00100000, 0x00000000, 0x00000000, 0x00000000, 0x00000000];
const BARRIER_KBL_PRE_C0: [u32; 24] = [0x7a000004, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x00000020, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x00345cbd, 0x000000d0, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x00100000, 0x00000000, 0x00000000, 0x00000000, 0x00000000];
const BARRIER_GEN9_PLAIN: [u32; 12] = [0x7a000004, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x00345cbd, 0x000000d0, 0x00000000, 0x00000000, 0x00000000];
const XCS_FLUSH: [u32; 4] = [0x13204002, 0x000000d0, 0x00000000, 0x00000000];
const XCS_INV: [u32; 4] = [0x13244002, 0x000000d0, 0x00000000, 0x00000000];
const VCS_INV: [u32; 4] = [0x13244082, 0x000000d0, 0x00000000, 0x00000000];
const BB_START_ARB: [u32; 6] = [0x04000001, 0x18800101, 0x23456000, 0x00000001, 0x04000000, 0x00000000];
const BB_START_SECURE: [u32; 6] = [0x04000001, 0x18800001, 0x23456000, 0x00000001, 0x04000000, 0x00000000];
const BB_START_NOARB: [u32; 4] = [0x04000000, 0x18800101, 0x23456000, 0x00000001];
const INIT_BREADCRUMB: [u32; 6] = [0x10400002, 0x00001040, 0x00000000, 0x00001233, 0x00000000, 0x02800000];
const FINI_XCS_SEM: [u32; 14] = [0x13004002, 0x00001044, 0x00000000, 0x00001234, 0x01000000, 0x04000001, 0x02800000, 0x0e40c002, 0x00000000, 0x002000c8, 0x00000000, 0x00000000, 0x02800000, 0x00000000];
const FINI_XCS_NOSEM: [u32; 8] = [0x13004002, 0x00001044, 0x00000000, 0x00001234, 0x01000000, 0x04000001, 0x02800000, 0x00000000];
const FINI_RCS_SEM: [u32; 22] = [0x7a000004, 0x00141021, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x01104080, 0x00001040, 0x00000000, 0x00001234, 0x00000000, 0x01000000, 0x04000001, 0x02800000, 0x0e40c002, 0x00000000, 0x002000c8, 0x00000000, 0x00000000, 0x02800000, 0x00000000];
const FINI_RCS_NOSEM: [u32; 16] = [0x7a000004, 0x00141021, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x7a000004, 0x01104080, 0x00001040, 0x00000000, 0x00001234, 0x00000000, 0x01000000, 0x04000001, 0x02800000, 0x00000000];

const PREEMPT_ADDR: u32 = 0x20_00c8;
const HWSP: u32 = 0x1040;
const SEQNO: u32 = 0x1234;

fn quirks(step: Step) -> FlushQuirks {
    FlushQuirks::for_context(&WaContext { platform: Platform::Kabylake, graphics_ver: 9, has_llc: true, step })
}

fn run<F: FnOnce(&mut CsBuf) -> Result<(), GtError>>(cap: usize, f: F) -> Result<Vec<u32>, GtError> {
    let mut buf = vec![0xdead_beefu32; cap];
    let mut cs = CsBuf::new(&mut buf);
    f(&mut cs)?;
    Ok(cs.written().to_vec())
}

#[test]
fn flush_rcs_only_flush_has_no_workarounds_on_any_stepping() {
    for step in [Step::B0, Step::C0, Step::G0] {
        assert_eq!(run(32, |c| emit_flush_rcs(c, EMIT_FLUSH, quirks(step))).unwrap(), FLUSH_ONLY, "{step:?}");
    }
}

#[test]
fn flush_rcs_invalidate_on_gen9_gets_the_vf_flush_null_pipe_control() {
    assert_eq!(run(32, |c| emit_flush_rcs(c, EMIT_INVALIDATE, quirks(Step::C0))).unwrap(), INV_GEN9_PLAIN);
    assert_eq!(run(32, |c| emit_flush_rcs(c, EMIT_BARRIER, quirks(Step::D0))).unwrap(), BARRIER_GEN9_PLAIN);
}

#[test]
fn flush_rcs_on_kbl_before_c0_adds_the_gam_hang_dc_flush_sequence() {
    // WaForGAMHang:kbl: IS_KABYLAKE && IS_GRAPHICS_STEP(0, STEP_C0) (gen8_engine_cs.c:46-47) -> +12 dwords
    assert_eq!(run(32, |c| emit_flush_rcs(c, EMIT_INVALIDATE, quirks(Step::B0))).unwrap(), INV_KBL_PRE_C0);
    assert_eq!(run(32, |c| emit_flush_rcs(c, EMIT_BARRIER, quirks(Step::B0))).unwrap(), BARRIER_KBL_PRE_C0);
}

#[test]
fn flush_rcs_reserved_length_matches_what_is_emitted() {
    for step in [Step::B0, Step::C0] {
        for mode in [EMIT_FLUSH, EMIT_INVALIDATE, EMIT_BARRIER] {
            let q = quirks(step);
            let got = run(64, |c| emit_flush_rcs(c, mode, q)).unwrap();
            assert_eq!(got.len(), flush_rcs_len(mode, q), "mode {mode} step {step:?}");
        }
    }
    assert_eq!(flush_rcs_len(EMIT_INVALIDATE, quirks(Step::B0)), 24);
    assert_eq!(flush_rcs_len(EMIT_INVALIDATE, quirks(Step::C0)), 12);
    assert_eq!(flush_rcs_len(EMIT_FLUSH, quirks(Step::B0)), 6);
}

#[test]
fn flush_xcs_encodings() {
    assert_eq!(run(8, |c| emit_flush_xcs(c, EMIT_FLUSH, false)).unwrap(), XCS_FLUSH);
    assert_eq!(run(8, |c| emit_flush_xcs(c, EMIT_INVALIDATE, false)).unwrap(), XCS_INV);
    assert_eq!(run(8, |c| emit_flush_xcs(c, EMIT_BARRIER, true)).unwrap(), VCS_INV, "video decode adds MI_INVALIDATE_BSD");
    assert_eq!(run(8, |c| emit_flush_xcs(c, EMIT_FLUSH, true)).unwrap(), XCS_FLUSH, "BSD invalidate only with INVALIDATE");
}

#[test]
fn bb_start_encodings() {
    let off = 0x0000_0001_2345_6000u64;
    assert_eq!(run(8, |c| emit_bb_start(c, off, 0, false)).unwrap(), BB_START_ARB);
    assert_eq!(run(8, |c| emit_bb_start(c, off, I915_DISPATCH_SECURE, false)).unwrap(), BB_START_SECURE);
    assert_eq!(run(8, |c| emit_bb_start(c, off, 0, true)).unwrap(), BB_START_NOARB, "nopreempt takes the noarb form");
    assert_eq!(run(8, |c| emit_bb_start_noarb(c, off, 0)).unwrap(), BB_START_NOARB);
}

#[test]
fn init_breadcrumb_encoding_stores_seqno_minus_one() {
    let mut buf = [0u32; 8];
    let mut cs = CsBuf::new(&mut buf);
    assert_eq!(emit_init_breadcrumb(&mut cs, HWSP, SEQNO).unwrap(), 6);
    assert_eq!(cs.written(), INIT_BREADCRUMB);
    let mut buf = [0u32; 8];
    let mut cs = CsBuf::new(&mut buf);
    emit_init_breadcrumb(&mut cs, HWSP, 0).unwrap();
    assert_eq!(cs.written()[3], 0xffff_ffff, "seqno 0 wraps");
}

#[test]
fn fini_breadcrumb_xcs_encodings_and_ring_offsets() {
    let mut buf = [0u32; 32];
    let mut cs = CsBuf::new(&mut buf);
    let ends = emit_fini_breadcrumb_xcs(&mut cs, SEQNO, HWSP, Some(PREEMPT_ADDR)).unwrap();
    assert_eq!(cs.written(), FINI_XCS_SEM);
    assert_eq!(ends, BreadcrumbEnds { tail: 12, wa_tail: 14 }, "rq->tail precedes the two WaIdleLiteRestore dwords");
    let mut buf = [0u32; 32];
    let mut cs = CsBuf::new(&mut buf);
    let ends = emit_fini_breadcrumb_xcs(&mut cs, SEQNO, HWSP, None).unwrap();
    assert_eq!(cs.written(), FINI_XCS_NOSEM);
    assert_eq!(ends, BreadcrumbEnds { tail: 6, wa_tail: 8 });
}

#[test]
fn fini_breadcrumb_rcs_encodings_and_ring_offsets() {
    let mut buf = [0u32; 32];
    let mut cs = CsBuf::new(&mut buf);
    let ends = emit_fini_breadcrumb_rcs(&mut cs, SEQNO, HWSP, Some(PREEMPT_ADDR)).unwrap();
    assert_eq!(cs.written(), FINI_RCS_SEM);
    assert_eq!(ends, BreadcrumbEnds { tail: 20, wa_tail: 22 });
    let mut buf = [0u32; 32];
    let mut cs = CsBuf::new(&mut buf);
    let ends = emit_fini_breadcrumb_rcs(&mut cs, SEQNO, HWSP, None).unwrap();
    assert_eq!(cs.written(), FINI_RCS_NOSEM);
    assert_eq!(ends, BreadcrumbEnds { tail: 14, wa_tail: 16 });
}

#[test]
fn fini_breadcrumb_lengths_match_the_emitted_dwords() {
    assert_eq!(fini_breadcrumb_len(false, true), FINI_XCS_SEM.len());
    assert_eq!(fini_breadcrumb_len(false, false), FINI_XCS_NOSEM.len());
    assert_eq!(fini_breadcrumb_len(true, true), FINI_RCS_SEM.len());
    assert_eq!(fini_breadcrumb_len(true, false), FINI_RCS_NOSEM.len());
}

#[test]
fn a_buffer_one_dword_short_is_refused_by_name_and_nothing_is_written() {
    for (len, emit) in [
        (FINI_RCS_SEM.len(), 0u8),
        (FLUSH_ONLY.len(), 1),
        (XCS_FLUSH.len(), 2),
        (BB_START_ARB.len(), 3),
        (INIT_BREADCRUMB.len(), 4),
    ] {
        let mut buf = vec![0xdead_beefu32; len - 1];
        let mut cs = CsBuf::new(&mut buf);
        let r = match emit {
            0 => emit_fini_breadcrumb_rcs(&mut cs, SEQNO, HWSP, Some(PREEMPT_ADDR)).map(|_| ()),
            1 => emit_flush_rcs(&mut cs, EMIT_FLUSH, quirks(Step::C0)),
            2 => emit_flush_xcs(&mut cs, EMIT_FLUSH, false),
            3 => emit_bb_start(&mut cs, 0x1000, 0, false),
            _ => emit_init_breadcrumb(&mut cs, HWSP, SEQNO).map(|_| ()),
        };
        assert_eq!(r, Err(GtError::CsOverflow { need: len as u32, have: (len - 1) as u32 }), "emitter {emit}");
        assert_eq!(r.unwrap_err().name(), "CS-OVERFLOW");
        assert_eq!(cs.len(), 0, "a refused emission leaves the buffer untouched");
        assert!(buf.iter().all(|&d| d == 0xdead_beef));
    }
}

#[test]
fn ggtt_write_offsets_are_checked_like_the_gem_bug_ons() {
    let mut buf = [0u32; 16];
    let mut cs = CsBuf::new(&mut buf);
    // bit 5 must be zero for MI_FLUSH_DW addresses (gen8_engine_cs.h:132) ; 8-byte alignment (:134)
    assert_eq!(emit_ggtt_write(&mut cs, 1, 0x1060, 0).unwrap_err().name(), "BAD-GGTT-OFFSET");
    assert_eq!(emit_ggtt_write(&mut cs, 1, 0x1044, 0).unwrap_err().name(), "BAD-GGTT-OFFSET");
    assert_eq!(emit_ggtt_write_rcs(&mut cs, 1, 0x1044, 0).unwrap_err().name(), "BAD-GGTT-OFFSET");
    assert_eq!(cs.len(), 0);
    emit_ggtt_write_rcs(&mut cs, 1, 0x1060, 0).unwrap(); // bit 5 is fine for the pipe-control form
    assert_eq!(cs.len(), 6);
}

#[test]
fn quirks_follow_the_platform_and_step() {
    let q = |p, ver, step| FlushQuirks::for_context(&WaContext { platform: p, graphics_ver: ver, has_llc: true, step });
    assert_eq!(q(Platform::Kabylake, 9, Step::B0), FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: true });
    assert_eq!(q(Platform::Kabylake, 9, Step::C0), FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false });
    assert_eq!(q(Platform::Skylake, 9, Step::A0), FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, "WaForGAMHang is KBL-only");
    assert_eq!(q(Platform::Cherryview, 8, Step::A0).vf_flush_wa, false);
}

#[test]
fn constants_are_linux_literals() {
    assert_eq!(LRC_PPHWSP_SCRATCH_ADDR, 0xd0);
    assert_eq!(I915_GEM_HWS_PREEMPT_ADDR, 0xc8);
    assert_eq!((EMIT_INVALIDATE, EMIT_FLUSH, EMIT_BARRIER), (1, 2, 3));
    assert_eq!(I915_DISPATCH_SECURE, 1);
}
