// SPDX-License-Identifier: GPL-2.0-only
//! Accessory detect: the Atlas pdata written out, the interrupt decode for every event, and the level-interrupt discipline.
mod common;
use common::*;
use da7219_core::aad::*;
use da7219_core::runner::*;

const STATUS_A: usize = 0xc0;
const EVENT_A: usize = 0xc2;
const EVENT_B: usize = 0xc3;
const INSERTED: u8 = 0x01; // DA7219_E_JACK_INSERTED
const REMOVED: u8 = 0x02;
const DETECT_COMPLETE: u8 = 0x04;
const STS_INSERTED: u8 = 0x01;
const STS_FOUR_POLE: u8 = 0x02;

fn irq(f: &mut Fake, a: u8, b: u8, status: u8, state: AadState) -> (Result<IrqOutcome, Failure>, AadState) {
    f.regs[EVENT_A] = a;
    f.regs[EVENT_B] = b;
    f.regs[STATUS_A] = status;
    let mut m = Irq::new(ATLAS_AAD_PDATA, state);
    let mut input = Input::Begin;
    loop {
        match m.next(input) {
            IrqOutput::Do(s) => input = f.exec(s),
            IrqOutput::Done(o) => return (Ok(o), m.state()),
            IrqOutput::Failed(e) => return (Err(e), m.state()),
        }
    }
}

fn fresh() -> AadState {
    AadState::new(ATLAS_AAD_PDATA.jack_det_rate)
}

#[test]
fn jack_report_bits_equal_the_values_gcc_computes_from_jack_h() {
    // include/sound/jack.h:39-53 and da7219-aad.h:168-171, evaluated with gcc
    assert_eq!(SND_JACK_HEADPHONE, 0x1);
    assert_eq!(SND_JACK_MICROPHONE, 0x2);
    assert_eq!(SND_JACK_HEADSET, 0x3);
    assert_eq!(SND_JACK_LINEOUT, 0x4);
    assert_eq!(SND_JACK_MECHANICAL, 0x8);
    assert_eq!((SND_JACK_BTN_0, SND_JACK_BTN_1, SND_JACK_BTN_2, SND_JACK_BTN_3), (0x4000, 0x2000, 0x1000, 0x0800));
    assert_eq!(REPORT_ALL_MASK, 0x780f);
}

#[test]
fn pdata_enums_carry_the_linux_codes() {
    // include/sound/da7219-aad.h:11-76
    assert_eq!((MicbiasPulse::Off as u8, MicbiasPulse::V2_8 as u8, MicbiasPulse::V2_9 as u8), (0, 6, 7));
    assert_eq!([BtnCfg::Ms2 as u8, BtnCfg::Ms5 as u8, BtnCfg::Ms10 as u8, BtnCfg::Ms50 as u8, BtnCfg::Ms100 as u8, BtnCfg::Ms200 as u8, BtnCfg::Ms500 as u8], [1, 2, 3, 4, 5, 6, 7]);
    assert_eq!([MicDetThr::Ohm200 as u8, MicDetThr::Ohm500 as u8, MicDetThr::Ohm750 as u8, MicDetThr::Ohm1000 as u8], [0, 1, 2, 3]);
    assert_eq!([JackInsDeb::Ms5 as u8, JackInsDeb::Ms10 as u8, JackInsDeb::Ms20 as u8, JackInsDeb::Ms50 as u8, JackInsDeb::Ms100 as u8, JackInsDeb::Ms200 as u8, JackInsDeb::Ms500 as u8, JackInsDeb::S1 as u8], [0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!([JackInsDetPty::Low as u8, JackInsDetPty::High as u8], [0, 1]);
    assert_eq!([JackDetRate::Ms32_64 as u8, JackDetRate::Ms64_128 as u8, JackDetRate::Ms128_256 as u8, JackDetRate::Ms256_512 as u8], [0, 1, 2, 3]);
    assert_eq!([JackRemDeb::Ms1 as u8, JackRemDeb::Ms5 as u8, JackRemDeb::Ms10 as u8, JackRemDeb::Ms20 as u8], [0, 1, 2, 3]);
    assert_eq!([BtnAvg::N1 as u8, BtnAvg::N2 as u8, BtnAvg::N4 as u8, BtnAvg::N8 as u8], [0, 1, 2, 3]);
    assert_eq!([Adc1BitRpt::N1 as u8, Adc1BitRpt::N2 as u8, Adc1BitRpt::N4 as u8, Adc1BitRpt::N8 as u8], [0, 1, 2, 3]);
    assert_eq!([gnd_switch_delay_ms(JackDetRate::Ms32_64), gnd_switch_delay_ms(JackDetRate::Ms64_128), gnd_switch_delay_ms(JackDetRate::Ms128_256), gnd_switch_delay_ms(JackDetRate::Ms256_512)], [32, 64, 128, 256]);
}

#[test]
fn the_atlas_pdata_is_the_recovery_acpi_values() {
    let p = ATLAS_AAD_PDATA;
    assert_eq!((p.btn_cfg, p.mic_det_thr, p.jack_ins_deb, p.jack_det_rate, p.jack_rem_deb), (BtnCfg::Ms50, MicDetThr::Ohm200, JackInsDeb::Ms20, JackDetRate::Ms32_64, JackRemDeb::Ms1));
    assert_eq!((p.a_d_btn_thr, p.d_b_btn_thr, p.b_c_btn_thr, p.c_mic_btn_thr), (0x0a, 0x16, 0x21, 0x3e));
    assert_eq!((p.btn_avg, p.adc_1bit_rpt), (BtnAvg::N4, Adc1BitRpt::N1));
}

#[test]
fn init_writes_the_atlas_configuration_to_the_registers_and_unmasks_last() {
    let mut f = Fake::reset();
    f.run(init_plan(ATLAS_AAD_PDATA).unwrap()).unwrap();
    // Linux reset defaults then the pdata (reset values in da7219.c:2397-2405: CONFIG_1 0xd6, _2 0x34, _7 0x01)
    assert_eq!(f.reg(0xc6), 0xd6 & !0x30 & !0x0e, "mic_det_thr 200 ohm clears bits 4-5; button detect off clears bits 1-3");
    assert_eq!(f.reg(0xc7), 0x34 & !0xfe | (2 << 1), "debounce 20 ms = 2, rate 32/64 = 0, removal 1 ms = 0");
    assert_eq!((f.reg(0xc8), f.reg(0xc9), f.reg(0xca), f.reg(0xcb)), (0x0a, 0x16, 0x21, 0x3e));
    assert_eq!(f.reg(0xcc), 0x01 & !0x0f | (2 << 0), "btn_avg 4 = 2, adc 1-bit repeat 1 = 0");
    assert_eq!(f.reg(0x75), 0x80, "insertion polarity low");
    assert_eq!(f.reg(0xf0), 0x00, "page select locked again");
    // the page switch brackets the polarity write (aad.c:879-889)
    let w: Vec<_> = f.writes.iter().map(|w| (w.1, w.2)).collect();
    let i = w.iter().position(|x| *x == (0xf0, 0x8b)).unwrap();
    assert_eq!(&w[i..i + 3], &[(0xf0, 0x8b), (0x75, 0x80), (0xf0, 0x00)]);
    // the interrupt masks are the LAST thing written, and are cleared (unmasked)
    assert_eq!(*f.steps.last().unwrap(), "aad-irq-unmask");
    assert_eq!((f.reg(0xc4), f.reg(0xc5)), (0, 0));
    assert_only_codec(&f);
}

#[test]
fn a_high_insertion_polarity_clears_the_polarity_register_instead() {
    let mut f = Fake::reset();
    f.regs[0x75] = 0x80;
    let p = AadPdata { jack_ins_det_pty: JackInsDetPty::High, ..ATLAS_AAD_PDATA };
    f.run(init_plan(p).unwrap()).unwrap();
    assert_eq!(f.reg(0x75), 0x00);
}

#[test]
fn exit_masks_everything_and_detection_toggles_only_the_enable_bit() {
    let mut f = Fake::reset();
    f.run(exit_plan()).unwrap();
    assert_eq!((f.reg(0xc4), f.reg(0xc5)), (0xff, 0xff));
    f.regs[0xc6] = 0x70;
    f.run(detection_plan(true)).unwrap();
    assert_eq!(f.reg(0xc6), 0x71);
    f.run(detection_plan(false)).unwrap();
    assert_eq!(f.reg(0xc6), 0x70);
}

#[test]
fn an_interrupt_with_no_events_is_spurious_reads_once_and_clears_nothing() {
    let mut f = Fake::reset();
    let (o, st) = irq(&mut f, 0, 0, STS_INSERTED, fresh());
    let o = o.unwrap();
    assert!(o.spurious);
    assert_eq!(o.report, None);
    assert_eq!(f.steps, vec!["aad-irq-events"], "one read, nothing else");
    assert_eq!(st, fresh());
}

#[test]
fn insertion_reports_mechanical_and_schedules_the_ground_switch_by_the_clock_state() {
    // no MCLK present (PLL_SRM_STS bit 4 clear): 32 * 2 - 2 = 62 ms (aad.c:380)
    let mut f = Fake::reset();
    f.regs[0x25] = 0x00;
    let (o, st) = irq(&mut f, INSERTED, 0, STS_INSERTED, fresh());
    let o = o.unwrap();
    assert_eq!(o.report, Some(JackReport { report: SND_JACK_MECHANICAL, mask: SND_JACK_MECHANICAL }));
    assert_eq!(o.ground_switch_after_ms, Some(62));
    assert!(st.jack_inserted);
    assert_eq!(f.reg(EVENT_A as u8), 0, "events cleared, so the level-low line deasserts");
    // MCLK present: 32 * 1 - 2 = 30 ms
    let mut g = Fake::reset();
    g.regs[0x25] = 0x10;
    let (o, _) = irq(&mut g, INSERTED, 0, STS_INSERTED, fresh());
    assert_eq!(o.unwrap().ground_switch_after_ms, Some(30));
    // a slower detect rate scales it
    let (o, _) = irq(&mut Fake::reset(), INSERTED, 0, STS_INSERTED, AadState::new(JackDetRate::Ms256_512));
    assert_eq!(o.unwrap().ground_switch_after_ms, Some(256 * 2 - 2));
}

#[test]
fn a_four_pole_jack_is_a_headset_and_starts_button_detection_with_the_output_driven() {
    let mut f = Fake::reset();
    let st = AadState { jack_inserted: true, gnd_switch_delay_ms: 32 };
    let (o, st2) = irq(&mut f, DETECT_COMPLETE, 0, STS_INSERTED | STS_FOUR_POLE, st);
    let o = o.unwrap();
    assert_eq!(o.pole, Some(Pole::Four));
    assert!(!o.hp_test_skipped);
    assert_eq!(o.report, Some(JackReport { report: SND_JACK_HEADSET, mask: SND_JACK_HEADSET | SND_JACK_LINEOUT }));
    assert!(st2.jack_inserted);
    // follow-up: ground switch off, outputs driven (left then right), mic bias on, button detection on at btn_cfg 50 ms (= 4 << 1)
    let w: Vec<_> = f.writes.iter().map(|w| w.0).collect();
    assert_eq!(&w[w.len() - 6..], &["aad-irq-clear", "ground-switch-off", "hp-l-oe-on", "hp-r-oe-on", "mic-bias-enable", "aad-button-detect-on"]);
    assert_eq!(f.reg(0xc6) & 0x0e, 4 << 1);
    assert_eq!((f.reg(0x6b) & 0x08, f.reg(0x6c) & 0x08), (0x08, 0x08));
    assert_eq!(f.reg(0x62) & 0x08, 0x08, "mic bias");
    assert_eq!(f.poll_reads, 5, "MICBIAS_UP never rises in the fake: five bounded looks, tolerated, detection still enabled");
}

#[test]
fn a_three_pole_jack_is_reported_as_headphones_and_the_impedance_test_is_skipped_by_name() {
    let mut f = Fake::reset();
    let before = f.regs;
    let st = AadState { jack_inserted: true, gnd_switch_delay_ms: 32 };
    let (o, _) = irq(&mut f, DETECT_COMPLETE, 0, STS_INSERTED, st);
    let o = o.unwrap();
    assert_eq!(o.pole, Some(Pole::Three));
    assert!(o.hp_test_skipped);
    assert_eq!(o.report, Some(JackReport { report: SND_JACK_HEADPHONE, mask: SND_JACK_HEADSET | SND_JACK_LINEOUT }));
    // the test tone, the DAC and the headphone gains, the tone generator and the HP test block are never touched
    for reg in [0x45usize, 0x46, 0x48, 0x49, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xbb, 0xcd] {
        assert_eq!(f.regs[reg], before[reg], "reg {reg:#x} must not change");
    }
    // the output enable is driven, as the end of the test would; the amplifier stays muted
    assert_eq!((f.reg(0x6b) & 0x48, f.reg(0x6c) & 0x48), (0x48, 0x48));
}

#[test]
fn every_button_press_and_release_decodes_to_its_own_jack_bit() {
    // presses: A..D = event_b bits 0..3 -> BTN_0..BTN_3 (aad.c:435-443); releases: A..D = bits 7..4 (aad.c:445-451)
    let presses = [(0x01u8, SND_JACK_BTN_0), (0x02, SND_JACK_BTN_1), (0x04, SND_JACK_BTN_2), (0x08, SND_JACK_BTN_3)];
    let releases = [(0x80u8, SND_JACK_BTN_0), (0x40, SND_JACK_BTN_1), (0x20, SND_JACK_BTN_2), (0x10, SND_JACK_BTN_3)];
    let st = AadState { jack_inserted: true, gnd_switch_delay_ms: 32 };
    for (bit, jack) in presses {
        let (o, _) = irq(&mut Fake::reset(), 0, bit, STS_INSERTED | STS_FOUR_POLE, st);
        let o = o.unwrap();
        assert_eq!(o.press_report, Some(JackReport { report: jack, mask: jack }), "press {bit:#x}");
        assert_eq!(o.report, o.press_report);
        assert_eq!(o.report.unwrap().buttons_down().iter().filter(|b| **b).count(), 1);
    }
    for (bit, jack) in releases {
        let (o, _) = irq(&mut Fake::reset(), 0, bit, STS_INSERTED | STS_FOUR_POLE, st);
        let o = o.unwrap();
        assert_eq!(o.report, Some(JackReport { report: 0, mask: jack }), "release {bit:#x}");
    }
    // a press and the release of another in one interrupt: the press is reported first, then the final report holds both effects
    let (o, _) = irq(&mut Fake::reset(), 0, 0x01 | 0x40, STS_INSERTED | STS_FOUR_POLE, st);
    let o = o.unwrap();
    assert_eq!(o.press_report, Some(JackReport { report: SND_JACK_BTN_0, mask: SND_JACK_BTN_0 }));
    assert_eq!(o.report, Some(JackReport { report: SND_JACK_BTN_0, mask: SND_JACK_BTN_0 | SND_JACK_BTN_1 }));
    // a press and the release of the same button: the release wins (cleared after the press loop)
    let (o, _) = irq(&mut Fake::reset(), 0, 0x01 | 0x80, STS_INSERTED | STS_FOUR_POLE, st);
    assert_eq!(o.unwrap().report, Some(JackReport { report: 0, mask: SND_JACK_BTN_0 }));
}

#[test]
fn buttons_are_ignored_on_a_three_pole_jack() {
    let st = AadState { jack_inserted: true, gnd_switch_delay_ms: 32 };
    let (o, _) = irq(&mut Fake::reset(), 0, 0x0f, STS_INSERTED, st);
    let o = o.unwrap();
    assert_eq!(o.press_report, None);
    assert_eq!(o.report, Some(JackReport { report: 0, mask: 0 }));
}

#[test]
fn removal_reports_empty_over_the_whole_mask_and_undrives_every_output() {
    let mut f = Fake::reset();
    f.regs[0x6b] = 0x08 | 0x80;
    f.regs[0x6c] = 0x08 | 0x80;
    f.regs[0xc6] = 0x0e;
    f.regs[0x62] = 0x08;
    f.regs[0xfb] = 0x01;
    let st = AadState { jack_inserted: true, gnd_switch_delay_ms: 32 };
    let (o, st2) = irq(&mut f, REMOVED, 0, 0, st);
    let o = o.unwrap();
    assert_eq!(o.report, Some(JackReport { report: 0, mask: REPORT_ALL_MASK }));
    assert_eq!(o.pole, None);
    assert!(!st2.jack_inserted);
    // right then left, as Linux (aad.c:465-469), button detection off, mic bias off, ground switch off
    let w: Vec<_> = f.writes.iter().map(|w| w.0).collect();
    assert_eq!(&w[w.len() - 6..], &["aad-irq-clear", "hp-r-oe-off", "hp-l-oe-off", "aad-button-detect-off", "mic-bias-disable", "ground-switch-off"]);
    assert_eq!((f.reg(0x6b) & 0x08, f.reg(0x6c) & 0x08, f.reg(0xc6) & 0x0e, f.reg(0x62) & 0x08, f.reg(0xfb) & 1), (0, 0, 0, 0, 0));
    assert_eq!(f.reg(0x6b) & 0x80, 0x80, "removal does not power the amplifier down - that is the DAPM's and the safe state's");
}

#[test]
fn a_level_interrupt_is_always_cleared_before_the_machine_finishes() {
    for (a, b, s) in [(INSERTED, 0, STS_INSERTED), (DETECT_COMPLETE, 0, STS_INSERTED | STS_FOUR_POLE), (REMOVED, 0, 0), (0, 0x0f, STS_INSERTED | STS_FOUR_POLE), (0x07, 0xff, STS_INSERTED)] {
        let mut f = Fake::reset();
        let (o, _) = irq(&mut f, a, b, s, AadState { jack_inserted: true, gnd_switch_delay_ms: 32 });
        o.unwrap();
        assert_eq!((f.reg(EVENT_A as u8), f.reg(EVENT_B as u8)), (0, 0), "events {a:#x}/{b:#x} left asserted");
        assert_only_codec(&f);
    }
}

#[test]
fn a_bus_failure_in_the_event_read_is_named_and_leaves_the_state_alone() {
    let mut f = Fake::reset();
    f.nak_at = Some("aad-irq-events");
    let (o, st) = irq(&mut f, INSERTED, 0, STS_INSERTED, fresh());
    assert_eq!(o.unwrap_err(), Failure { step: "aad-irq-events", cause: Cause::BusNak });
    assert_eq!(st, fresh());
    let mut g = Fake::reset();
    g.nak_at = Some("aad-irq-clear");
    let (o, _) = irq(&mut g, REMOVED, 0, 0, AadState { jack_inserted: true, gnd_switch_delay_ms: 32 });
    assert_eq!(o.unwrap_err().step, "aad-irq-clear");
    assert_eq!(g.reg(0x6b) & 0x08, 0, "no decode follow-up ran after a failed clear");
}

#[test]
fn the_button_detect_plan_pulses_the_mic_bias_only_when_configured() {
    let mut f = Fake::reset();
    f.run(button_detect_plan(ATLAS_AAD_PDATA).unwrap()).unwrap();
    assert!(!f.steps.contains(&"mic-bias-pulse-up"));
    let p = AadPdata { micbias_pulse_lvl: MicbiasPulse::V2_9, micbias_pulse_time_ms: 20, ..ATLAS_AAD_PDATA };
    let mut g = Fake::reset();
    g.run(button_detect_plan(p).unwrap()).unwrap();
    assert!(g.steps.contains(&"mic-bias-pulse-up") && g.steps.contains(&"mic-bias-pulse-hold"));
    assert_eq!(g.elapsed_ms >= 20, true);
    // the mic-bias wait is five reads 10 ms apart, tolerated (aad.c:70-83)
    assert_eq!(button_detect_plan(ATLAS_AAD_PDATA).unwrap().max_wait_ms(), 50);
}

#[test]
fn the_mic_bias_wait_is_bounded_and_does_not_abort_button_detection() {
    let mut f = Fake::reset();
    // the fake's STATUS_A is a plain register; MICBIAS_UP never rises
    let t = f.run(button_detect_plan(ATLAS_AAD_PDATA).unwrap()).unwrap();
    assert_eq!(t.unwrap().cause, Cause::PollBound { reads: 5, last: 0 });
    assert!(f.steps.contains(&"aad-button-detect-on"));
}
