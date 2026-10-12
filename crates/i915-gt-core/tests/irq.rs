// SPDX-License-Identifier: GPL-2.0-only
//! Gen8 GT interrupt decode and set-up against intel_gt_irq.c / i915_irq.c. Postinstall values are what GCC evaluates from the
//! Linux expressions (`irqs << GEN8_*_IRQ_SHIFT`, `~gt_interrupts[n]`), written out literally.
use i915_common_core::Reg;
use i915_gt_core::irq::*;
use i915_gt_core::{Effect, WritePlan};

#[test]
fn engine_chunk_bits_are_linux_literals() {
    // GT_RENDER_USER_INTERRUPT 1<<0, GT_CS_MASTER_ERROR_INTERRUPT 1<<3, GT_CONTEXT_SWITCH_INTERRUPT 1<<8, GT_WAIT_SEMAPHORE 1<<11
    assert_eq!(EngineIrq::decode(0x0001), EngineIrq { iir: 1, user_interrupt: true, ..Default::default() });
    assert_eq!(EngineIrq::decode(0x0008), EngineIrq { iir: 8, cs_error: true, ..Default::default() });
    assert_eq!(EngineIrq::decode(0x0100), EngineIrq { iir: 0x100, context_switch: true, ..Default::default() });
    assert_eq!(EngineIrq::decode(0x0800), EngineIrq { iir: 0x800, wait_semaphore: true, ..Default::default() });
    let all = EngineIrq::decode(0x0909);
    assert!(all.user_interrupt && all.cs_error && all.context_switch && all.wait_semaphore);
    assert!(!EngineIrq::decode(0).is_set());
    assert!(EngineIrq::decode(0x0002).is_set(), "an unrelated bit still triggers the handler (intel_engine_cs_irq: `if (iir)`)");
    assert_eq!(EngineIrq::decode(0x0002).user_interrupt, false);
}

#[test]
fn bank_zero_splits_rcs_low_bcs_high() {
    let iir = Gen8GtIir { bank0: (0x0100 << 16) | 0x0001, ..Default::default() };
    let d = decode_gen8_gt_irq(0x1 | 0x2, iir).unwrap();
    assert!(d.rcs0.user_interrupt && !d.rcs0.context_switch);
    assert!(d.bcs0.context_switch && !d.bcs0.user_interrupt);
    assert_eq!(d.acks.as_slice(), [WritePlan { reg: Reg::new(0x44308), value: (0x0100 << 16) | 1 }]);
}

#[test]
fn bank_one_splits_vcs0_low_vcs1_high() {
    let iir = Gen8GtIir { bank1: (0x0008 << 16) | 0x0800, ..Default::default() };
    let d = decode_gen8_gt_irq(0x4 | 0x8, iir).unwrap();
    assert!(d.vcs0.wait_semaphore);
    assert!(d.vcs1.cs_error);
    assert_eq!(d.acks.as_slice()[0].reg.offset(), 0x44318);
}

#[test]
fn bank_three_is_vecs() {
    let iir = Gen8GtIir { bank3: 0x0001, ..Default::default() };
    let d = decode_gen8_gt_irq(0x40, iir).unwrap();
    assert!(d.vecs0.user_interrupt);
    assert_eq!(d.acks.as_slice(), [WritePlan { reg: Reg::new(0x44338), value: 1 }]);
}

#[test]
fn bank_two_carries_pm_low_and_guc_high() {
    let iir = Gen8GtIir { bank2: 0xabcd_0042, ..Default::default() };
    let d = decode_gen8_gt_irq(0x10, iir).unwrap();
    assert_eq!((d.pm_iir, d.guc_iir), (0xabcd_0042, 0xabcd));
    assert_eq!(d.acks.as_slice(), [WritePlan { reg: Reg::new(0x44328), value: 0xabcd_0042 }]);
    let d = decode_gen8_gt_irq(0x20, iir).unwrap();
    assert_eq!(d.guc_iir, 0xabcd, "either the PM or the GuC bit selects bank 2");
}

#[test]
fn a_bank_the_master_control_did_not_select_is_never_acked() {
    let iir = Gen8GtIir { bank0: 1, bank1: 1, bank2: 1, bank3: 1 };
    let d = decode_gen8_gt_irq(0, iir).unwrap();
    assert!(d.acks.is_empty());
    assert!(!d.rcs0.is_set() && !d.vcs0.is_set() && !d.vecs0.is_set());
    // only RCS selected: only bank 0 is acked
    let d = decode_gen8_gt_irq(0x1, iir).unwrap();
    assert_eq!(d.acks.len(), 1);
    assert_eq!(d.acks.as_slice()[0].reg.offset(), 0x44308);
}

#[test]
fn a_zero_iir_is_not_acked() {
    let d = decode_gen8_gt_irq(0x7f, Gen8GtIir::default()).unwrap();
    assert!(d.acks.is_empty(), "`if (likely(iir))` guards the write-back");
}

#[test]
fn all_four_banks_can_fire_at_once() {
    let iir = Gen8GtIir { bank0: 1, bank1: 1, bank2: 1, bank3: 1 };
    let d = decode_gen8_gt_irq(GEN8_GT_IRQS, iir).unwrap();
    assert_eq!(d.acks.len(), 4);
    assert_eq!(GEN8_GT_IRQS, 0x7f);
}

#[test]
fn reset_plan_is_gen2_irq_reset_on_each_bank() {
    let plan = plan_gen8_gt_irq_reset().unwrap();
    assert_eq!(plan.len(), 28);
    let bank = |imr: u32, ier: u32, iir: u32| {
        vec![
            Effect::Write(WritePlan { reg: Reg::new(imr), value: 0xffff_ffff }),
            Effect::PostingRead(Reg::new(imr)),
            Effect::Write(WritePlan { reg: Reg::new(ier), value: 0 }),
            Effect::Write(WritePlan { reg: Reg::new(iir), value: 0xffff_ffff }),
            Effect::PostingRead(Reg::new(iir)),
            Effect::Write(WritePlan { reg: Reg::new(iir), value: 0xffff_ffff }),
            Effect::PostingRead(Reg::new(iir)),
        ]
    };
    let mut want = vec![];
    for (imr, ier, iir) in [(0x44304, 0x4430c, 0x44308), (0x44314, 0x4431c, 0x44318), (0x44324, 0x4432c, 0x44328), (0x44334, 0x4433c, 0x44338)] {
        want.extend(bank(imr, ier, iir));
    }
    assert_eq!(plan.as_slice(), want.as_slice());
}

#[test]
fn postinstall_plan_programs_ier_then_imr_with_gcc_evaluated_masks() {
    let plan = plan_gen8_gt_irq_postinstall().unwrap();
    let init = |ier: u32, ier_v: u32, imr: u32, imr_v: u32| {
        [
            Effect::Write(WritePlan { reg: Reg::new(ier), value: ier_v }),
            Effect::Write(WritePlan { reg: Reg::new(imr), value: imr_v }),
            Effect::PostingRead(Reg::new(imr)),
        ]
    };
    let mut want = vec![];
    // bank 0: RCS | BCS; bank 1: VCS0 | VCS1 (same shape); bank 2: PM/GuC off (pm_ier = 0, pm_imr = ~0); bank 3: VECS
    want.extend(init(0x4430c, 0x0909_0909, 0x44304, 0xf6f6_f6f6));
    want.extend(init(0x4431c, 0x0909_0909, 0x44314, 0xf6f6_f6f6));
    want.extend(init(0x4432c, 0x0000_0000, 0x44324, 0xffff_ffff));
    want.extend(init(0x4433c, 0x0000_0909, 0x44334, 0xffff_f6f6));
    assert_eq!(plan.as_slice(), want.as_slice());
}

#[test]
fn irq_init_asserts_the_iir_register_of_each_bank() {
    assert_eq!(irq_init_iir_reg(0).unwrap().offset(), 0x44308);
    assert_eq!(irq_init_iir_reg(3).unwrap().offset(), 0x44338);
    assert_eq!(irq_init_iir_reg(4).unwrap_err().name(), "BAD-IRQ-BANK");
}
