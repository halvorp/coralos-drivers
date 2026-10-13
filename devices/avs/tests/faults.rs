// SPDX-License-Identifier: GPL-2.0-only
//! Every way the bring-up and the running stream can fail: each fails BY NAME, within a bound, and leaves the codec in the verified safe state
//! and the DSP off. A guardrail is only a guardrail if the test shows it failing first: each case below makes exactly one thing go wrong.
mod sim;
use avs_ipc_core::header::*;
use avs_wire::msg::*;
use avs_wire::*;
use sim::dsp::Act;
use sim::harness::*;

/// Open a stream with one write queued and ask it to start; returns (start status, the phase reply, the status snapshot, virtual ms spent).
fn start_with(dev: &mut avs_device::Device<sim::Sim>, c: &mut Client) -> (Status, PhaseReply, StatusBody, u64, u32) {
    let id = open_stream(dev, c);
    call(dev, c, Opcode::PcmWrite, &pcm_payload(id, &ramp(1024, 0)));
    let t0 = dev.rig.m.t_us;
    let (s, ph) = start_stream(dev, c, id);
    let ms = (dev.rig.m.t_us - t0) / 1000;
    let st = get_status(dev, c);
    (s, ph, st, ms, id)
}

fn assert_safe_and_down(dev: &avs_device::Device<sim::Sim>) {
    assert!(dev.st.safe_verified, "the codec must be proven safe after a failure");
    let m = &dev.rig.m;
    // the codec registers say the same thing the status claims: outputs muted and off
    for (reg, mask, want) in [(0x6bu8, 0xc8u8, 0x40u8), (0x6c, 0xc8, 0x40), (0x69, 0xc0, 0x40), (0x6a, 0xc0, 0x40), (0x6e, 0x80, 0), (0x6f, 0x80, 0), (0x47, 0x80, 0)] {
        assert_eq!(m.codec.regs[reg as usize] & mask, want, "codec register {reg:#x}");
    }
    assert_eq!(m.dsp.r32(0x04) & (1 << 16), 0, "DSP core must be powered down");
    assert!(dev.dsp.is_none());
    assert_eq!(m.amp_touches(), 0);
    assert!(m.refused_writes.is_empty());
}

fn faulted(st: &StatusBody, step: &str) {
    assert_eq!((st.state, st.cause), (State::Faulted, Cause::InitFailed), "{st:?}");
    assert_eq!(st.step_str(), step);
}

#[test]
fn a_dsp_that_never_answers_an_ipc_fails_the_start_by_name_inside_the_bound() {
    let Some(mut dev) = started(true) else { return };
    dev.rig.m.dsp.hook = Some(Box::new(|r| if r.target == 0 && r.ty == AVS_GLB_CREATE_PIPELINE { Act::Silent } else { Act::Normal }));
    let mut c = Client::new();
    let (s, ph, st, ms, _) = start_with(&mut dev, &mut c);
    assert_eq!(s, Status::InitFailed);
    assert_eq!(ph.cause, Cause::InitFailed);
    faulted(&st, "create pipeline");
    // 300 ms for the wait, one more 300 ms after a time-out with the DSP still showing busy would be 600 ms; plus the unwind
    assert!(ms < 2500, "the failure took {ms} ms of virtual time");
    assert_safe_and_down(&dev);
}

#[test]
fn a_firmware_that_refuses_a_message_fails_with_its_status_as_the_detail() {
    let Some(mut dev) = started(true) else { return };
    dev.rig.m.dsp.hook = Some(Box::new(|r| if r.target == 0 && r.ty == AVS_GLB_SET_PIPELINE_STATE { Act::Status(5) } else { Act::Normal }));
    let mut c = Client::new();
    let (s, _, st, _, _) = start_with(&mut dev, &mut c);
    assert_eq!(s, Status::InitFailed);
    assert_eq!((st.state, st.cause), (State::Faulted, Cause::InitFailed));
    assert_eq!(st.detail, 5, "the firmware's own code reaches the status");
    assert_safe_and_down(&dev);
}

#[test]
fn a_rom_that_never_announces_itself_fails_rom_init_after_its_second() {
    let Some(mut dev) = started(true) else { return };
    dev.rig.m.dsp.rom_never_ready = true;
    let mut c = Client::new();
    let (s, _, st, ms, _) = start_with(&mut dev, &mut c);
    assert_eq!(s, Status::InitFailed);
    faulted(&st, "rom-init");
    assert!((1000..2500).contains(&ms), "ROM init is bounded by 1 s (SKL_ROM_INIT_TIMEOUT_US), took {ms} ms");
    assert_safe_and_down(&dev);
}

#[test]
fn a_firmware_that_never_says_fw_ready_fails_after_three_seconds() {
    let Some(mut dev) = started(true) else { return };
    dev.rig.m.dsp.fw_never_ready = true;
    let mut c = Client::new();
    let (s, _, st, ms, _) = start_with(&mut dev, &mut c);
    assert_eq!(s, Status::InitFailed);
    faulted(&st, "fw-ready");
    assert!((3000..4500).contains(&ms), "FW_READY is bounded by 3 s, took {ms} ms");
    assert_safe_and_down(&dev);
}

#[test]
fn a_rom_that_never_reports_the_base_firmware_entered_fails_that_step_by_name() {
    let Some(mut dev) = started(true) else { return };
    dev.rig.m.at_ms(0, sim::Ev::Custom(Box::new(|s| s.dsp.basefw_len = usize::MAX)));
    let mut c = Client::new();
    let (s, ph, _st, _ms, _) = start_with(&mut dev, &mut c);
    // an image the ROM never considers delivered: the loader's DMA completes, the ROM never says BASEFW_ENTERED
    assert_eq!(s, Status::InitFailed);
    assert_eq!(ph.cause, Cause::InitFailed);
    assert_eq!(dev.st.step_str(), "basefw-entered");
    assert_safe_and_down(&dev);
}

#[test]
fn a_dsp_exception_while_playing_puts_the_codec_in_the_verified_safe_state_and_reports_it() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    let id = open_stream(&mut dev, &mut c);
    for chunk in ramp(8192, 0).chunks(16384) {
        call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, chunk));
    }
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase), (Status::Ok, StreamPhase::Running));
    dev.rig.m.at_ms(30, sim::Ev::Custom(Box::new(|s| {
        let mut n = notification(avs_ipc_core::notify::AVS_NOTIFY_EXCEPTION_CAUGHT).unwrap();
        NOTIFY_COREDUMP_CORE_ID.put(&mut n, 0).unwrap();
        s.dsp.notify(n);
    })));
    let ev = run_ms(&mut dev, 100);
    let st = get_status(&mut dev, &mut c);
    faulted(&st, "dsp-exception");
    assert_eq!(st.stream, StreamPhase::Suspended, "the stream stays open for the client to decide");
    assert!(ev.iter().any(|g| g.op == Opcode::EvStatus && StatusBody::decode(&g.payload).unwrap().state == State::Faulted), "the client was told");
    assert_safe_and_down(&dev);
    // a start is a retry: the fault is cleared and the stream runs again
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase), (Status::Ok, StreamPhase::Running));
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause), (State::Running, Cause::None));
}

#[test]
fn a_stream_whose_interrupts_stop_is_caught_by_the_watchdog() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    start_stream(&mut dev, &mut c, id);
    run_ms(&mut dev, 50);
    dev.rig.m.hda.mute_irqs = true;
    let t0 = dev.rig.m.t_us;
    run_ms(&mut dev, 1000);
    let st = get_status(&mut dev, &mut c);
    faulted(&st, "period-timeout");
    let waited = (dev.rig.m.t_us - t0) / 1000;
    assert!(waited < 1000, "the watchdog fired in {waited} ms (the bound is three rings)");
    assert_safe_and_down(&dev);
}

#[test]
fn a_descriptor_error_on_the_host_stream_is_a_fault_not_a_glitch() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    start_stream(&mut dev, &mut c, id);
    dev.rig.m.hda.desc_error = true;
    run_ms(&mut dev, 100);
    let st = get_status(&mut dev, &mut c);
    faulted(&st, "stream-error");
    assert_safe_and_down(&dev);
}

#[test]
fn an_interrupt_with_no_cause_is_counted_and_then_called_a_storm() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    start_stream(&mut dev, &mut c, id);
    dev.rig.m.force_irq = true;
    let steps0 = dev.rig.m.waits.len();
    run_ms(&mut dev, 50);
    let st = get_status(&mut dev, &mut c);
    faulted(&st, "irq-storm");
    // bounded: 64 empty firings, not a core burnt
    assert!(dev.rig.m.waits.len() - steps0 < 200);
    dev.rig.m.force_irq = false;
    assert_safe_and_down(&dev);
}

#[test]
fn a_codec_that_stops_answering_mid_start_fails_the_start_and_the_status_names_the_step() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let n = dev.rig.m.codec.txs.len();
    // let the first dozen power-up transactions through, then go silent
    dev.rig.m.codec.nak_after = Some(n + 40);
    let (s, _, st, _, _) = start_with(&mut dev, &mut c);
    assert_eq!(s, Status::InitFailed);
    assert_eq!((st.state, st.cause), (State::Faulted, Cause::InitFailed));
    assert!(!st.step_str().is_empty());
    // the codec could not be made safe either: the status must not claim it was
    assert!(!st.safe_verified, "an unanswering codec cannot be proven safe, and the status says so");
    assert!(dev.dsp.is_none());
    assert_eq!(dev.rig.m.dsp.r32(0x04) & (1 << 16), 0);
}

#[test]
fn the_client_dying_takes_the_stream_with_it_and_leaves_the_safe_state() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 0)));
    start_stream(&mut dev, &mut c, id);
    run_ms(&mut dev, 30);
    dev.rig.m.at_ms(5, sim::Ev::PeerDead);
    run_ms(&mut dev, 50);
    assert!(dev.stream.is_none());
    assert_eq!(dev.subs, 0);
    assert_safe_and_down(&dev);
    assert_eq!(dev.st.state, State::Idle);
}

#[test]
fn a_dma_budget_too_small_for_the_buffers_is_a_named_init_failure() {
    let Some(mut dev) = started(true) else { return };
    dev.rig.m.dma_budget = Some(64 * 1024);
    let mut c = Client::new();
    let (s, _, st, _, _) = start_with(&mut dev, &mut c);
    assert_eq!(s, Status::InitFailed);
    faulted(&st, "dma-alloc");
    assert_safe_and_down(&dev);
}


#[test]
fn a_dead_pad_interrupt_owner_takes_the_path_down_because_an_unplug_could_no_longer_be_seen() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 1)));
    start_stream(&mut dev, &mut c, id);
    dev.rig.m.at_ms(20, sim::Ev::JackOwnerDead);
    run_ms(&mut dev, 100);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause, st.step_str()), (State::Down, Cause::BackendUnavailable, "jack-owner-gone"));
    assert_eq!(st.stream, StreamPhase::Suspended, "the client's stream is still open, and held");
    assert_safe_and_down(&dev);
    // a start does not bring it back: the cause is named and stays
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase), (Status::BackendUnavailable, StreamPhase::Suspended));
}

#[test]
fn a_dead_clients_session_endpoints_are_handed_back_and_the_next_client_is_served_one_at_a_time() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    assert!(dev.rig.m.session_open);
    // a second client connects while the first is served: it waits, it is not accepted, nothing is released
    dev.rig.m.listen_pending += 1;
    let (s, _) = {
        let (s, b, _) = call(&mut dev, &mut c, Opcode::StreamOpen, &s16_open());
        (s, b)
    };
    assert_eq!(s, Status::StreamBusy);
    assert_eq!((dev.rig.m.listen_pending, dev.rig.m.released), (1, 0));
    start_stream(&mut dev, &mut c, id);
    dev.rig.m.at_ms(5, sim::Ev::PeerDead);
    run_ms(&mut dev, 50);
    assert_eq!(dev.rig.m.released, 1, "SESSION_RELEASE for the dead client's pair");
    assert!(dev.stream.is_none());
    assert_safe_and_down(&dev);
    // the waiting client is accepted and gets a working, fresh device
    run_ms(&mut dev, 5);
    assert!(dev.rig.m.session_open && dev.rig.m.listen_pending == 0);
    let mut c2 = Client::new();
    let id2 = open_stream(&mut dev, &mut c2);
    assert_ne!(id, id2);
}
