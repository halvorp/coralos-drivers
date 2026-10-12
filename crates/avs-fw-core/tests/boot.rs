// SPDX-License-Identifier: GPL-2.0-only
//! The whole Kaby Lake base-firmware boot (avs_cldma_load_basefw + the FW_READY wait) and the library/module loads,
//! driven against scripted devices, success and each failure.
mod common;
use avs_fw_core::bdl::*;
use avs_fw_core::boot::*;
use avs_fw_core::effect::*;
use common::*;

const K: u32 = 1024;

fn layout() -> ClDmaLayout {
    ClDmaLayout::plan(&[Extent { addr: 0x1000_0000, len: 128 * K }], 0x2000, 128 * K, 1).unwrap()
}

const POWER_OFF_TAIL: [&str; 8] =
    ["core-stall", "core-stall-ack", "core-stall-settle", "core-reset-enter", "core-reset-enter-ack", "core-power-off", "core-power-off-settle", "core-power-off-cpa"];

#[test]
fn a_small_firmware_boots_in_the_linux_order() {
    let mut b = BaseFwBoot::new(&layout(), 40_000).unwrap();
    let (log, out) = run(|i| b.next(i), ok_answer, 500);
    assert_eq!(out, Output::Finished);
    let l = labels(&log);
    // the ROM wait precedes the first copy, the copy precedes the run bit, and FW_READY is last
    let pos = |name: &str| l.iter().position(|x| *x == name).unwrap_or_else(|| panic!("{name} missing in {l:?}"));
    assert!(pos("core-power-on-cpa") < pos("core-reset-exit"));
    assert!(pos("core-reset-exit") < pos("cldma-srst-set"));
    assert!(pos("cldma-spib-enable") < pos("core-unstall"));
    assert!(pos("core-unstall") < pos("int-ipc"));
    assert!(pos("int-hipcctl") < pos("rom-init"));
    assert!(pos("rom-init") < pos("cldma-copy"));
    assert!(pos("cldma-copy") < pos("cldma-run"));
    assert!(pos("cldma-ioc") < pos("basefw-entered"));
    assert!(pos("basefw-entered") < pos("fw-ready"));
    assert_eq!(*l.last().unwrap(), "fw-ready");
    // nothing disabled the core on the success path
    assert!(!l.contains(&"core-power-off"));
}

#[test]
fn a_firmware_larger_than_the_ring_streams_through_several_completions() {
    let mut b = BaseFwBoot::new(&layout(), 243_016 - 0x548).unwrap(); // the Pixelbook Go's dsp_basefw.bin body
    let (log, out) = run(|i| b.next(i), ok_answer, 2000);
    assert_eq!(out, Output::Finished);
    let iocs = log.iter().filter(|s| s.label == "cldma-ioc").count();
    assert_eq!(iocs, 2, "241_648 bytes through a 131_072-byte ring is two fills");
    let copied: u32 = log.iter().filter_map(|s| if let Op::CopyToDma { len, .. } = s.op { Some(len) } else { None }).sum();
    assert_eq!(copied, 243_016 - 0x548);
}

fn fail_at(label: &'static str, answer: Input) -> (Vec<&'static str>, Output) {
    let mut b = BaseFwBoot::new(&layout(), 40_000).unwrap();
    let (log, out) = run(|i| b.next(i), |s| if s.label == label { answer } else { ok_answer(s) }, 500);
    (labels(&log), out)
}

#[test]
fn power_failure_before_anything_is_undone_returns_the_error_with_no_cleanup() {
    // loader.c:144-146
    let (l, out) = fail_at("core-power-on-cpa", Input::PollTimedOut(0));
    assert_eq!(*l.last().unwrap(), "core-power-on-cpa");
    assert!(matches!(out, Output::Failed(f) if f.step == "core-power-on-cpa"));
}

#[test]
fn cldma_reset_failure_returns_without_disabling_the_core() {
    // loader.c:152-156: `dev_err("cldma reset failed"); return ret;`
    let (l, out) = fail_at("cldma-srst-ack", Input::PollTimedOut(0));
    assert_eq!(*l.last().unwrap(), "cldma-srst-ack");
    assert!(matches!(out, Output::Failed(f) if f.step == "cldma-srst-ack"));
}

#[test]
fn rom_init_timeout_disables_the_core_but_does_not_stop_the_loader() {
    // loader.c:170-175
    let (l, out) = fail_at("rom-init", Input::PollTimedOut(0x0));
    assert_eq!(&l[l.len() - 8..], &POWER_OFF_TAIL);
    assert!(!l[l.iter().position(|x| *x == "rom-init").unwrap()..].contains(&"cldma-run-off"));
    assert!(matches!(out, Output::Failed(f) if f.step == "rom-init"));
}

#[test]
fn a_dma_error_stops_the_loader_then_disables_the_core() {
    // loader.c:183-188 reached through a transfer that fails
    let mut b = BaseFwBoot::new(&layout(), 40_000).unwrap();
    let (log, out) = run(|i| b.next(i), |s| if s.label == "cldma-ioc" { Input::IocStatus(0x10) } else { ok_answer(s) }, 500);
    let l = labels(&log);
    let from = l.iter().position(|x| *x == "cldma-ioc").unwrap();
    assert_eq!(
        &l[from + 1..],
        &["cldma-irq-off", "cldma-run-off", "cldma-stopped", "core-stall", "core-stall-ack", "core-stall-settle", "core-reset-enter", "core-reset-enter-ack", "core-power-off", "core-power-off-settle", "core-power-off-cpa"]
    );
    assert!(matches!(out, Output::Failed(f) if f.step == "cldma-ioc"));
}

#[test]
fn the_rom_never_entering_basefw_stops_and_disables() {
    // loader.c:180-189
    let (l, out) = fail_at("basefw-entered", Input::PollTimedOut(0x5));
    let from = l.iter().position(|x| *x == "basefw-entered").unwrap();
    assert_eq!(l[from + 1], "cldma-irq-off");
    assert_eq!(*l.last().unwrap(), "core-power-off-cpa");
    assert!(matches!(out, Output::Failed(f) if f.step == "basefw-entered"));
}

#[test]
fn fw_ready_timeout_disables_the_core_after_the_loader_was_stopped() {
    // loader.c:589-598
    let (l, out) = fail_at("fw-ready", Input::EventTimedOut);
    let from = l.iter().position(|x| *x == "fw-ready").unwrap();
    assert_eq!(l[from + 1], "core-stall", "no second loader stop");
    assert_eq!(*l.last().unwrap(), "core-power-off-cpa");
    assert_eq!(l.iter().filter(|x| **x == "cldma-irq-off").count(), 2, "reset's stop and the post-transfer stop, no more");
    assert!(matches!(out, Output::Failed(f) if f.step == "fw-ready"));
}

#[test]
fn a_stop_that_does_not_complete_after_a_good_boot_is_ignored() {
    // loader.c:183 ignores hda_cldma_stop()'s result
    let mut b = BaseFwBoot::new(&layout(), 40_000).unwrap();
    let mut seen_entered = false;
    let (_, out) = run(
        |i| b.next(i),
        |s| {
            if s.label == "basefw-entered" {
                seen_entered = true;
            }
            if seen_entered && s.label == "cldma-stopped" {
                Input::PollTimedOut(2)
            } else {
                ok_answer(s)
            }
        },
        500,
    );
    assert_eq!(out, Output::Finished);
}

#[test]
fn rom_init_is_bounded_by_reads_even_if_the_caller_never_times_it_out() {
    let mut b = BaseFwBoot::new(&layout(), 40_000).unwrap();
    let (log, out) = run(|i| b.next(i), |s| if s.label == "rom-init" { Input::Value(0) } else { ok_answer(s) }, 1_000_000);
    // 1_000_000us / 5us + 2
    assert_eq!(log.iter().filter(|s| s.label == "rom-init").count(), 200_002);
    assert!(matches!(out, Output::Failed(f) if f.step == "rom-init"));
}

#[test]
fn an_empty_image_is_refused() {
    assert!(BaseFwBoot::new(&layout(), 0).is_err());
    assert!(CodeLoad::library(&layout(), 0, 1).is_err());
}

#[test]
fn a_library_load_sends_the_ipc_between_the_first_fill_and_the_run_and_awaits_its_reply() {
    // loader.c:194-213
    let mut c = CodeLoad::library(&layout(), 16_556, 2).unwrap();
    let (log, out) = run(|i| c.next(i), ok_answer, 200);
    assert_eq!(out, Output::Finished);
    let l = labels(&log);
    assert_eq!(l, vec!["cldma-spib-read", "cldma-copy", "cldma-spib-write", "code-ipc-send", "cldma-start-delay", "cldma-irq-on", "cldma-run", "cldma-running", "cldma-ioc", "code-ipc-reply", "cldma-irq-off", "cldma-run-off", "cldma-stopped"]);
    assert_eq!(log[3].op, Op::IpcSend(IpcCall::LoadLibrary { lib_id: 2 }));
    assert_eq!(log[4].op, Op::DelayUs(100_000));
    assert_eq!(log[9].op, Op::IpcAwait { timeout_ms: 5000 });
}

#[test]
fn a_refused_library_still_stops_the_loader_and_reports_the_firmwares_status() {
    let mut c = CodeLoad::library(&layout(), 16_556, 2).unwrap();
    let (log, out) = run(|i| c.next(i), |s| if s.label == "code-ipc-reply" { Input::IpcStatus(5) } else { ok_answer(s) }, 200);
    let l = labels(&log);
    assert_eq!(&l[l.len() - 3..], &["cldma-irq-off", "cldma-run-off", "cldma-stopped"]);
    assert!(!l.contains(&"core-power-off"), "a library failure does not disable the core");
    assert_eq!(out, Output::Failed(Failure { step: "code-ipc-reply", cause: Cause::IpcRefused { status: 5 } }));
}

#[test]
fn a_dma_failure_during_a_library_load_still_stops_the_loader() {
    let mut c = CodeLoad::library(&layout(), 16_556, 2).unwrap();
    let (log, out) = run(|i| c.next(i), |s| if s.label == "cldma-ioc" { Input::EventTimedOut } else { ok_answer(s) }, 200);
    let l = labels(&log);
    assert_eq!(&l[l.len() - 3..], &["cldma-irq-off", "cldma-run-off", "cldma-stopped"]);
    assert_eq!(out, Output::Failed(Failure { step: "cldma-ioc", cause: Cause::EventTimeout }));
}

#[test]
fn a_module_load_sends_load_multiple_modules() {
    let mut c = CodeLoad::module(&layout(), 12_288, 0x1d).unwrap();
    let (log, out) = run(|i| c.next(i), ok_answer, 200);
    assert_eq!(out, Output::Finished);
    assert_eq!(log.iter().find(|s| s.label == "code-ipc-send").unwrap().op, Op::IpcSend(IpcCall::LoadModule { module_id: 0x1d }));
}

#[test]
fn an_empty_image_is_refused_with_the_length_it_had() {
    assert_eq!(BaseFwBoot::new(&layout(), 0).unwrap_err(), avs_fw_core::error::FwError::ImageTooShort { have: 0, need: 1 });
    assert_eq!(CodeLoad::module(&layout(), 0, 3).unwrap_err(), avs_fw_core::error::FwError::ImageTooShort { have: 0, need: 1 });
}

#[test]
fn the_base_firmware_transfer_starts_at_once_but_a_library_waits_100_ms() {
    // loader.c:179 hda_cldma_transfer(cl, 0) versus loader.c:201 msecs_to_jiffies(AVS_CLDMA_START_DELAY_MS)
    let mut b = BaseFwBoot::new(&layout(), 40_000).unwrap();
    let (log, _) = run(|i| b.next(i), ok_answer, 500);
    assert!(!log.iter().any(|s| s.label == "cldma-start-delay"));
    let mut c = CodeLoad::library(&layout(), 40_000, 1).unwrap();
    let (log, _) = run(|i| c.next(i), ok_answer, 500);
    assert_eq!(log.iter().filter(|s| s.op == Op::DelayUs(100_000)).count(), 1);
}
