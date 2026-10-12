// SPDX-License-Identifier: GPL-2.0-only
//! The transaction state machine (ipc.c:158-440, :498-535): the happy path, every wait bound, and the stricter-than-Linux
//! correlation rules.
mod common;
use avs_ipc_core::error::IpcError;
use avs_ipc_core::header::*;
use avs_ipc_core::request::*;
use avs_ipc_core::txn::*;
use common::*;

const BUSY: u32 = 0x8000_0000;

fn wb(off: u32, value: u32) -> Output {
    Output::Write { off, value }
}

#[test]
fn a_message_without_payload_writes_ext_then_primary_with_busy_and_waits_300ms() {
    let mut ipc = ready_ipc(Correlation::Strict);
    let req = delete_pipeline(7).unwrap();
    let first = ipc.begin(&req, 0).unwrap();
    // ext dword 0x4C = 0 first, then the primary (0x48) with BUSY set: the write that starts the DSP is last (ipc.c:386-387)
    assert_eq!(first, wb(0x4C, 0));
    assert_eq!(ipc.step(Input::Wrote), wb(0x48, 0x1207_0000 | BUSY));
    assert_eq!(ipc.step(Input::Wrote), Output::WaitBusy { timeout_ms: 300 });
    assert!(ipc.in_flight());
    // the reply arrives: type 18 (DELETE_PIPELINE), global, status 0
    let o = ipc.on_response(reply(0, 18, 0, 0), 0).unwrap();
    assert_eq!(o, Outcome::Reply { copy_len: 0 });
    match ipc.step(Input::Woken) {
        Output::Done(Ok(c)) => {
            assert_eq!(c.status(), 0);
            assert_eq!(c.payload_len, 0);
            assert_eq!(c.header, reply(0, 18, 0, 0));
        }
        o => panic!("{o:?}"),
    }
    assert!(!ipc.in_flight());
}

#[test]
fn a_payload_is_copied_before_the_header_words() {
    let mut ipc = ready_ipc(Correlation::Strict);
    let req = set_dx(1, true).unwrap();
    assert_eq!(ipc.begin(&req, 0).unwrap(), Output::CopyPayload { len: 8 });
    assert_eq!(ipc.step(Input::Wrote), wb(0x4C, 0));
    assert_eq!(ipc.step(Input::Wrote), wb(0x48, 0x4700_0000 | BUSY));
    assert_eq!(ipc.step(Input::Wrote), Output::WaitBusy { timeout_ms: 300 });
}

#[test]
fn code_loading_gets_the_five_second_budget() {
    let mut ipc = ready_ipc(Correlation::Strict);
    let req = load_library(0, 2).unwrap();
    ipc.begin(&req, 0).unwrap();
    ipc.step(Input::Wrote);
    assert_eq!(ipc.step(Input::Wrote), Output::WaitBusy { timeout_ms: 5000 });
}

#[test]
fn the_extension_dword_is_written_to_the_ext_register() {
    let mut ipc = ready_ipc(Correlation::Strict);
    let req = bind(0x10, 2, 0x20, 3, 5, 6).unwrap();
    assert_eq!(ipc.begin(&req, 0).unwrap(), wb(0x4C, 0x3503_0020));
    assert_eq!(ipc.step(Input::Wrote), wb(0x48, 0x4502_0010 | BUSY));
}

#[test]
fn a_message_needs_a_ready_channel() {
    let mut ipc = Ipc::new(Correlation::Strict);
    assert_eq!(ipc.begin(&delete_pipeline(1).unwrap(), 0), Err(IpcError::NotReady));
    assert!(!ipc.in_flight());
}

#[test]
fn one_transaction_at_a_time() {
    let mut ipc = ready_ipc(Correlation::Strict);
    ipc.begin(&delete_pipeline(1).unwrap(), 0).unwrap();
    assert_eq!(ipc.begin(&delete_pipeline(2).unwrap(), 0), Err(IpcError::TransactionInFlight));
}

#[test]
fn a_oversize_payload_never_starts() {
    let mut ipc = ready_ipc(Correlation::Strict);
    let big = vec![0u8; 4096];
    let req = Request { header: module_request(AVS_MOD_INIT_INSTANCE).unwrap(), payload: Payload::Borrowed(&big), timeout_ms: 300, flow: Flow::Msg, name: "x" };
    assert!(ipc.begin(&req, 0).is_ok());
    let mut ipc = ready_ipc(Correlation::Strict);
    let big = vec![0u8; 4097];
    let req = Request { header: module_request(AVS_MOD_INIT_INSTANCE).unwrap(), payload: Payload::Borrowed(&big), timeout_ms: 300, flow: Flow::Msg, name: "x" };
    assert_eq!(ipc.begin(&req, 0), Err(IpcError::PayloadTooBig { len: 4097, max: 4096 }));
    assert!(!ipc.in_flight());
}

fn start(ipc: &mut Ipc) -> Output {
    let req = delete_pipeline(7).unwrap();
    ipc.begin(&req, 0).unwrap();
    ipc.step(Input::Wrote);
    ipc.step(Input::Wrote)
}

#[test]
fn a_timeout_with_the_dsp_idle_is_a_timeout_and_an_exception() {
    // ipc.c:331-332 then :409-413
    let mut ipc = ready_ipc(Correlation::Strict);
    assert_eq!(start(&mut ipc), Output::WaitBusy { timeout_ms: 300 });
    assert_eq!(ipc.step(Input::TimedOut), Output::ReadRsp { off: 0x40 });
    assert_eq!(ipc.step(Input::Value(0)), Output::Done(Err(IpcError::Timeout { exception: true })));
    assert!(!ipc.ready(), "the DSP is declared dead");
    assert!(ipc.recovering());
    assert!(ipc.take_recovery_request());
    assert!(!ipc.take_recovery_request(), "once");
}

#[test]
fn a_timeout_with_the_dsp_busy_waits_once_more_not_forever() {
    // ipc.c:333-337: Linux's wait_for_completion_killable() is unbounded; here it is one more budget
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    assert_eq!(ipc.step(Input::TimedOut), Output::ReadRsp { off: 0x40 });
    assert_eq!(ipc.step(Input::Value(BUSY)), Output::WaitBusy { timeout_ms: 300 });
    // the response never gets processed
    assert_eq!(ipc.step(Input::TimedOut), Output::Done(Err(IpcError::Timeout { exception: true })));
    assert!(!ipc.ready());
}

#[test]
fn a_timeout_with_the_dsp_busy_that_then_completes_succeeds() {
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    ipc.step(Input::TimedOut);
    ipc.step(Input::Value(BUSY));
    ipc.on_response(reply(0, 18, 0, 0), 0).unwrap();
    assert!(matches!(ipc.step(Input::Woken), Output::Done(Ok(_))));
    assert!(ipc.ready());
}

#[test]
fn the_busy_bit_is_bit_31_of_the_response_register_only() {
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    ipc.step(Input::TimedOut);
    // every bit but 31 set: the DSP is NOT busy
    assert_eq!(ipc.step(Input::Value(0x7FFF_FFFF)), Output::Done(Err(IpcError::Timeout { exception: true })));
}

#[test]
fn an_exception_during_the_wait_makes_the_waiter_fail_not_ready() {
    // ipc.c:327-328
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    let o = ipc.on_response(notify(10, 0, 0), 0).unwrap();
    assert!(matches!(o, Outcome::Notification { event: Event::ExceptionCaught { recovery: Recovery::Started, .. }, .. }));
    assert_eq!(ipc.step(Input::Woken), Output::Done(Err(IpcError::NotReady)));
    // and a time-out after the exception is also "not ready", not a second exception
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    ipc.on_response(notify(10, 0, 0), 0).unwrap();
    assert_eq!(ipc.step(Input::TimedOut), Output::Done(Err(IpcError::NotReady)));
}

#[test]
fn a_notification_wakeup_without_a_reply_rearms_the_wait_at_most_128_times() {
    // ipc.c:320, :340-352
    let mut ipc = ready_ipc(Correlation::Strict);
    assert_eq!(start(&mut ipc), Output::WaitBusy { timeout_ms: 300 });
    let mut rewaits = 0;
    loop {
        ipc.on_response(notify(6, 0, 0), 0).unwrap(); // LOG_BUFFER_STATUS: wakes the waiter, completes nothing
        match ipc.step(Input::Woken) {
            Output::WaitBusy { timeout_ms: 300 } => rewaits += 1,
            Output::Done(Err(IpcError::Timeout { exception: true })) => break,
            o => panic!("{o:?}"),
        }
        assert!(rewaits <= 128, "the repeat bound was not enforced");
    }
    assert_eq!(rewaits, 128);
    assert!(!ipc.ready());
}

#[test]
fn a_reply_after_some_notification_wakeups_still_completes() {
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    for _ in 0..5 {
        ipc.on_response(notify(6, 0, 0), 0).unwrap();
        assert_eq!(ipc.step(Input::Woken), Output::WaitBusy { timeout_ms: 300 });
    }
    ipc.on_response(reply(0, 18, 0, 0), 0).unwrap();
    assert!(matches!(ipc.step(Input::Woken), Output::Done(Ok(_))));
}

#[test]
fn strict_correlation_rejects_a_reply_of_another_type_or_target() {
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc); // DELETE_PIPELINE: type 18, global
    assert_eq!(
        ipc.on_response(reply(0, 19, 0, 0), 0).unwrap(),
        Outcome::Uncorrelated { expected_type: 18, got_type: 19, expected_target: 0, got_target: 0 }
    );
    assert_eq!(
        ipc.on_response(reply(0, 18, 1, 0), 0).unwrap(),
        Outcome::Uncorrelated { expected_type: 18, got_type: 18, expected_target: 0, got_target: 1 }
    );
    // the stray reply did not complete the request: the waiter goes back to waiting
    assert_eq!(ipc.step(Input::Woken), Output::WaitBusy { timeout_ms: 300 });
    ipc.on_response(reply(0, 18, 0, 0), 0).unwrap();
    assert!(matches!(ipc.step(Input::Woken), Output::Done(Ok(_))));
}

#[test]
fn linux_correlation_accepts_any_reply() {
    let mut ipc = ready_ipc(Correlation::Linux);
    start(&mut ipc);
    assert_eq!(ipc.on_response(reply(0, 19, 1, 0), 0).unwrap(), Outcome::Reply { copy_len: 0 });
    assert!(matches!(ipc.step(Input::Woken), Output::Done(Ok(_))));
}

#[test]
fn a_reply_with_nothing_in_flight_is_stale_and_poisons_nothing() {
    let mut ipc = ready_ipc(Correlation::Strict);
    assert_eq!(ipc.on_response(reply(0, 18, 0, 0), 0).unwrap(), Outcome::StaleReply);
    // the next transaction does not see it as its answer
    start(&mut ipc);
    assert_eq!(ipc.step(Input::Woken), Output::WaitBusy { timeout_ms: 300 });
    // ... also after a timed-out transaction
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    ipc.step(Input::TimedOut);
    ipc.step(Input::Value(0)); // exception, transaction over
    assert_eq!(ipc.on_response(reply(0, 18, 0, 0), 0).unwrap(), Outcome::StaleReply);
}

#[test]
fn a_failed_status_is_returned_not_hidden() {
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    ipc.on_response(reply(5, 18, 0, 0), 0).unwrap();
    match ipc.step(Input::Woken) {
        Output::Done(Ok(c)) => assert_eq!(c.status(), 5),
        o => panic!("{o:?}"),
    }
}

#[test]
fn a_large_config_get_reply_states_how_much_payload_to_copy() {
    let mut ipc = ready_ipc(Correlation::Strict);
    let req = get_fw_config().unwrap();
    ipc.begin(&req, 4096).unwrap();
    ipc.step(Input::Wrote);
    ipc.step(Input::Wrote);
    // module-targeted reply of type LARGE_CONFIG_GET carrying 0x123 bytes
    assert_eq!(ipc.on_response(reply(0, 3, 1, 0x123), 0).unwrap(), Outcome::Reply { copy_len: 0x123 });
    match ipc.step(Input::Woken) {
        Output::Done(Ok(c)) => assert_eq!(c.payload_len, 0x123),
        o => panic!("{o:?}"),
    }
    // a failed one copies nothing
    let mut ipc = ready_ipc(Correlation::Strict);
    ipc.begin(&req, 4096).unwrap();
    ipc.step(Input::Wrote);
    ipc.step(Input::Wrote);
    assert_eq!(ipc.on_response(reply(2, 3, 1, 0x123), 0).unwrap(), Outcome::Reply { copy_len: 0 });
}

// ---- ROM messages -------------------------------------------------------------------------------------------------------------

#[test]
fn a_rom_message_needs_no_fw_ready_and_waits_for_the_ack() {
    let mut ipc = Ipc::new(Correlation::Strict);
    assert!(!ipc.ready());
    let req = set_boot_config(3, true).unwrap();
    assert_eq!(ipc.begin(&req, 0).unwrap(), wb(0x4C, 0));
    assert_eq!(ipc.step(Input::Wrote), wb(0x48, 0x0100_4600 | BUSY)); // ROM_CONTROL, dma 3 << 9, purge << 14
    // the core is un-stalled only AFTER the message is on the wire (ipc.c:513)
    assert_eq!(ipc.step(Input::Wrote), Output::UnstallMainCore);
    assert_eq!(ipc.step(Input::Stalled(true)), Output::WaitTxDone { timeout_ms: 300 });
    assert_eq!(ipc.step(Input::Woken), Output::Done(Ok(Completed { header: 0, payload_len: 0 })));
}

#[test]
fn a_rom_message_that_cannot_unstall_or_is_not_acked_fails_by_name() {
    let mut ipc = Ipc::new(Correlation::Strict);
    ipc.begin(&set_boot_config(3, true).unwrap(), 0).unwrap();
    ipc.step(Input::Wrote);
    ipc.step(Input::Wrote);
    assert_eq!(ipc.step(Input::Stalled(false)), Output::Done(Err(IpcError::UnstallFailed)));
    let mut ipc = Ipc::new(Correlation::Strict);
    ipc.begin(&set_boot_config(3, true).unwrap(), 0).unwrap();
    ipc.step(Input::Wrote);
    ipc.step(Input::Wrote);
    ipc.step(Input::Stalled(true));
    // ipc.c:519: -ETIMEDOUT, and no exception: there is no firmware to have crashed
    assert_eq!(ipc.step(Input::TimedOut), Output::Done(Err(IpcError::Timeout { exception: false })));
    assert!(!ipc.recovering());
}

#[test]
fn a_rom_message_carries_no_payload() {
    // ipc.c:507-510: the SRAM windows are not configured while the core is stalled
    let mut ipc = Ipc::new(Correlation::Strict);
    let req = Request { header: global_request(AVS_GLB_ROM_CONTROL).unwrap(), payload: Payload::Borrowed(&[1, 2, 3, 4]), timeout_ms: 300, flow: Flow::Rom, name: "x" };
    assert_eq!(ipc.begin(&req, 0), Err(IpcError::PayloadTooBig { len: 4, max: 0 }));
}

// ---- notifications --------------------------------------------------------------------------------------------------------------

#[test]
fn fw_ready_opens_the_channel_and_nothing_else_does_before_it() {
    let mut ipc = Ipc::new(Correlation::Strict);
    // ipc.c:218-222: everything but FW_READY is ignored until the handshake
    for t in [4, 5, 6, 10, 12, 99] {
        assert_eq!(ipc.on_response(notify(t, 0, 0), 0).unwrap(), Outcome::IgnoredNotReady, "type {t}");
        assert!(!ipc.ready());
        assert!(!ipc.recovering(), "an exception before FW_READY must not start a recovery");
    }
    assert_eq!(ipc.on_response(notify(8, 0, 0), 0).unwrap(), Outcome::Notification { payload_len: 0, event: Event::FwReady });
    assert!(ipc.ready());
}

#[test]
fn an_exception_closes_the_channel_and_starts_one_recovery() {
    let mut ipc = ready_ipc(Correlation::Strict);
    let n = notify(10, (0x80 << 2) | 1, 0);
    assert_eq!(
        ipc.on_response(n, 0).unwrap(),
        Outcome::Notification { payload_len: 0, event: Event::ExceptionCaught { recovery: Recovery::Started, core_id: 1, stack_dump_size: 0x80 } }
    );
    assert!(!ipc.ready());
    assert!(ipc.recovering());
    // the second exception during the recovery: the channel is already closed, so it is ignored (not ready) - and
    // `exception_caught` itself reports the slot is taken
    assert_eq!(ipc.exception_caught(), Recovery::AlreadyInProgress);
    ipc.recovery_finished();
    assert!(!ipc.recovering());
    // FW_READY of the rebooted firmware reopens the channel
    ipc.on_response(notify(8, 0, 0), 0).unwrap();
    assert!(ipc.ready());
    assert!(matches!(ipc.on_response(notify(10, 0, 0), 0).unwrap(), Outcome::Notification { event: Event::ExceptionCaught { recovery: Recovery::Started, .. }, .. }));
}

#[test]
fn notification_events_and_payload_sizes() {
    let mut ipc = ready_ipc(Correlation::Strict);
    assert_eq!(ipc.on_response(notify(6, 0, 0x2000), 0).unwrap(), Outcome::Notification { payload_len: 0, event: Event::LogBufferStatus { core: 2 } });
    assert_eq!(ipc.on_response(notify(4, 0, 0), 0).unwrap(), Outcome::Notification { payload_len: 4, event: Event::PhraseDetected });
    assert_eq!(ipc.on_response(notify(5, 0, 0), 0).unwrap(), Outcome::Notification { payload_len: 40, event: Event::ResourceEvent });
    assert_eq!(ipc.on_response(notify(12, 0, 0), 20).unwrap(), Outcome::Notification { payload_len: 32, event: Event::ModuleEvent });
    assert_eq!(ipc.on_response(notify(77, 0, 0), 0).unwrap(), Outcome::Notification { payload_len: 0, event: Event::Unknown(77) });
    assert_eq!(ipc.on_response(notify(12, 0, 0), 5000), Err(IpcError::NotifyPayloadTooBig { size: 5012, max: 4096 }));
}

#[test]
fn block_closes_the_channel_and_abandons_the_transaction() {
    // avs_ipc_block (ipc.c:577-583)
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    ipc.block();
    assert!(!ipc.ready());
    assert!(!ipc.in_flight());
    assert_eq!(ipc.begin(&delete_pipeline(1).unwrap(), 0), Err(IpcError::NotReady));
}

#[test]
fn out_of_order_answers_are_protocol_failures() {
    let mut ipc = ready_ipc(Correlation::Strict);
    ipc.begin(&delete_pipeline(1).unwrap(), 0).unwrap();
    assert_eq!(ipc.step(Input::Woken), Output::Done(Err(IpcError::Protocol)));
    assert!(!ipc.in_flight());
    // stepping an idle Ipc
    assert_eq!(ipc.step(Input::Wrote), Output::Done(Err(IpcError::Protocol)));
}

#[test]
fn a_full_exchange_through_the_driver_helper() {
    let mut ipc = ready_ipc(Correlation::Strict);
    let req = get_pipeline_state(9).unwrap();
    let first = ipc.begin(&req, 0).unwrap();
    let mut woken = false;
    let (log, r) = drive(
        &mut ipc,
        first,
        |o| match o {
            Output::WaitBusy { .. } => {
                woken = true;
                Input::Woken
            }
            _ => Input::Wrote,
        },
        400,
    );
    // nobody delivered a reply: the wake-ups run the repeat bound out and the exchange ends in a timeout
    assert!(woken);
    assert_eq!(r, Err(IpcError::Timeout { exception: true }));
    // first wait + 128 re-waits, each one a WaitBusy
    assert_eq!(log.iter().filter(|o| matches!(o, Output::WaitBusy { .. })).count(), 129);
}

#[test]
fn a_one_byte_payload_is_still_copied_to_the_mailbox() {
    // ipc.c:384-385 `if (tx->size) memcpy_toio(...)`
    let mut ipc = ready_ipc(Correlation::Strict);
    let req = Request { header: module_request(AVS_MOD_INIT_INSTANCE).unwrap(), payload: Payload::Borrowed(&[7]), timeout_ms: 300, flow: Flow::Msg, name: "x" };
    assert_eq!(ipc.begin(&req, 0).unwrap(), Output::CopyPayload { len: 1 });
}

#[test]
fn a_transaction_forgets_the_previous_reply() {
    let mut ipc = ready_ipc(Correlation::Strict);
    start(&mut ipc);
    ipc.on_response(reply(0, 18, 0, 0x55), 0).unwrap();
    match ipc.step(Input::Woken) {
        Output::Done(Ok(c)) => assert_eq!(c.header, reply(0, 18, 0, 0x55)),
        o => panic!("{o:?}"),
    }
    // the next transaction must be answered by ITS reply; the old header is not reported
    start(&mut ipc);
    ipc.on_response(reply(3, 18, 0, 0), 0).unwrap();
    match ipc.step(Input::Woken) {
        Output::Done(Ok(c)) => assert_eq!((c.header, c.status()), (reply(3, 18, 0, 0), 3)),
        o => panic!("{o:?}"),
    }
}
