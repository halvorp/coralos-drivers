// SPDX-License-Identifier: GPL-2.0-only
//! The accessory-detect interrupt machine end to end: insertion, removal, buttons, the delayed ground switch, a spurious or stuck line.
mod sim;
use avs_wire::msg::*;
use avs_wire::*;
use da7219_core::aad_fields::*;
use da7219_core::regs::*;
use sim::harness::*;
use sim::Ev;

fn jacks(frames: &[Got]) -> Vec<JackEvent> {
    frames.iter().filter(|g| g.op == Opcode::EvJack).map(|g| JackEvent::decode(&g.payload).unwrap()).collect()
}

#[test]
fn a_three_pole_plug_is_headphones_a_four_pole_is_a_headset_and_removal_is_absent() {
    let Some(mut dev) = started(false) else { return };
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    dev.rig.m.at_ms(1, Ev::Plug { four_pole: false });
    let ev = run_ms(&mut dev, 20);
    assert_eq!(jacks(&ev).iter().map(|j| j.jack).collect::<Vec<_>>(), vec![Jack::Headphones]);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.jack, st.cause, st.state), (Jack::Headphones, Cause::None, State::Idle));
    assert!(st.safe_verified, "with no stream the codec is back in the safe state, outputs un-driven");
    assert_eq!(dev.rig.m.codec.regs[DA7219_HP_L_CTRL as usize] & 0x08, 0, "the jack output stage is only driven while playing");
    dev.rig.m.at_ms(1, Ev::Unplug);
    let ev = run_ms(&mut dev, 20);
    assert_eq!(jacks(&ev).iter().map(|j| j.jack).collect::<Vec<_>>(), vec![Jack::Absent]);
    dev.rig.m.at_ms(1, Ev::Plug { four_pole: true });
    let ev = run_ms(&mut dev, 20);
    assert_eq!(jacks(&ev).iter().map(|j| j.jack).collect::<Vec<_>>(), vec![Jack::Headset]);
    assert_eq!(dev.rig.m.amp_touches(), 0);
}

#[test]
fn headset_buttons_are_reported_and_cleared_and_a_press_does_not_change_the_jack() {
    let Some(mut dev) = started(false) else { return };
    let mut c = Client::new();
    subscribe_all(&mut dev, &mut c);
    dev.rig.m.at_ms(1, Ev::Plug { four_pole: true });
    run_ms(&mut dev, 150); // the four-pole follow-up waits for the microphone bias (up to 50 ms) before it is done
    dev.rig.m.at_ms(1, Ev::Press { button: 0 });
    dev.rig.m.at_ms(30, Ev::Release { button: 0 });
    dev.rig.m.at_ms(40, Ev::Press { button: 2 });
    let ev = run_ms(&mut dev, 100);
    let j = jacks(&ev);
    assert_eq!(j.iter().map(|x| x.buttons).collect::<Vec<_>>(), vec![0b001, 0b000, 0b100], "{j:?}");
    assert!(j.iter().all(|x| x.jack == Jack::Headset));
    // unplugging with a button down clears the buttons
    dev.rig.m.at_ms(1, Ev::Unplug);
    let ev = run_ms(&mut dev, 20);
    let j = jacks(&ev);
    assert_eq!(j.last().map(|x| (x.jack, x.buttons)), Some((Jack::Absent, 0)));
}

#[test]
fn each_event_costs_one_pass_when_the_pad_reads_back_high_and_the_pad_is_re_armed_each_time() {
    let Some(mut dev) = started(false) else { return };
    let acks0 = dev.rig.m.jack_acks;
    let p0 = dev.stats.jack_passes;
    dev.rig.m.at_ms(1, Ev::Plug { four_pole: false });
    run_ms(&mut dev, 20);
    assert_eq!(dev.stats.jack_passes - p0, 1, "level-low serviced: one pass read the events, cleared them, and the pad went high");
    assert_eq!(dev.rig.m.jack_acks - acks0, 1);
    assert!(!dev.rig.m.codec.irq_asserted());
}

#[test]
fn a_line_that_will_not_go_quiet_is_a_named_storm_after_a_bounded_number_of_passes() {
    let Some(mut dev) = started(false) else { return };
    dev.rig.m.codec.reassert = true;
    dev.rig.m.at_ms(1, Ev::Plug { four_pole: false });
    let p0 = dev.stats.jack_passes;
    let acks0 = dev.rig.m.jack_acks;
    run_ms(&mut dev, 50);
    assert_eq!(dev.stats.jack_passes - p0, 8, "exactly the bound");
    assert_eq!((dev.st.state, dev.st.cause, dev.st.step_str()), (State::Faulted, Cause::InitFailed, "jack-irq-storm"));
    assert_eq!(dev.rig.m.jack_acks, acks0, "a stuck line is left masked at the pad owner: no re-arm, no storm");
    let passes = dev.stats.jack_passes;
    run_ms(&mut dev, 500);
    assert_eq!(dev.stats.jack_passes, passes, "nothing re-serviced it on its own");
    // the codec comes right and a client's start retries the line
    dev.rig.m.codec.reassert = false;
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    let (s, _) = start_stream(&mut dev, &mut c, id);
    assert_eq!(s, Status::Ok);
    assert!(!dev.jack_masked_for_test());
    assert!(dev.rig.m.jack_acks > acks0, "the retry re-armed the pad");
}

#[test]
fn an_event_message_with_nothing_pending_is_irq_none_and_not_a_fault() {
    let Some(mut dev) = started(false) else { return };
    dev.rig.m.at_ms(1, Ev::Custom(Box::new(|s| {
        // the pad owner delivers a message for a line that is already quiet
        s.codec.regs[DA7219_ACCDET_IRQ_EVENT_A as usize] = 0;
        s.rearm_jack_for_test();
    })));
    run_ms(&mut dev, 20);
    assert_eq!((dev.st.state, dev.st.cause), (State::Idle, Cause::HeadphoneDisconnected));
}

#[test]
fn the_ground_switch_is_enabled_after_its_delay_unless_detection_completes_first() {
    let Some(mut dev) = started(false) else { return };
    let ins_only = |s: &mut sim::Sim| {
        s.codec.regs[DA7219_ACCDET_IRQ_EVENT_A as usize] |= DA7219_E_JACK_INSERTED_MASK;
        s.codec.regs[DA7219_ACCDET_STATUS_A as usize] = DA7219_JACK_INSERTION_STS_MASK;
        s.poke_jack_line();
    };
    // insertion only: the delayed work is armed and the main loop's wait carries it as a deadline
    dev.rig.m.at_ms(1, Ev::Custom(Box::new(ins_only)));
    assert_eq!(dev.step(), avs_device::machine::Wake::Jack, "the insertion pass");
    let due = dev.ground_switch_due().expect("the delayed enable is armed");
    assert_eq!(dev.rig.m.codec.regs[0xfb] & 1, 0, "not yet");
    // the codec's delay for a 32-64 ms detect rate with the SRM locked: 32 - 2 ms, measured from the end of the pass
    let left = due - dev.rig.m.t_us / 1000;
    assert!((25..=30).contains(&left), "due in {left} ms");
    assert_eq!(dev.step(), avs_device::machine::Wake::Deadline, "the next wake is the timer itself");
    assert_eq!(dev.rig.m.codec.regs[0xfb] & 1, 1, "the ground switch was enabled after the codec's delay");
    assert_eq!(dev.ground_switch_due(), None);
    assert!(dev.rig.m.t_us / 1000 >= due);
    // the same, but detection completes inside the delay: cancelled
    let Some(mut dev) = started(false) else { return };
    dev.rig.m.at_ms(1, Ev::Custom(Box::new(ins_only)));
    dev.rig.m.at_ms(20, Ev::Custom(Box::new(|s| {
        s.codec.regs[DA7219_ACCDET_IRQ_EVENT_A as usize] |= DA7219_E_JACK_DETECT_COMPLETE_MASK;
        s.poke_jack_line();
    })));
    run_ms(&mut dev, 200);
    assert_eq!(dev.ground_switch_due(), None);
    assert_eq!(dev.rig.m.codec.regs[0xfb] & 1, 0, "detect-complete cancelled the delayed enable and turned the switch off");
    assert_eq!(dev.jack, Jack::Headphones);
}

#[test]
fn removal_during_a_session_stops_the_dma_and_proves_the_safe_state_before_anything_else_is_served() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 1)));
    start_stream(&mut dev, &mut c, id);
    run_ms(&mut dev, 50);
    assert!(dev.rig.m.hda.running(7));
    dev.rig.m.at_ms(1, Ev::Unplug);
    // a request that arrives at the same instant: the removal is served first, so the answer reflects the path being DOWN
    dev.rig.m.at_ms(1, Ev::Request(c.frame(Opcode::Status, &[]).1));
    let ev = run_ms(&mut dev, 30);
    assert!(!dev.rig.m.hda.running(7), "the DMA stopped");
    let status_reply = ev.iter().find(|g| g.reply && g.op == Opcode::Status).expect("the queued status request was answered");
    let st = StatusBody::decode(&status_of(status_reply).1).unwrap();
    assert_eq!((st.state, st.cause, st.jack), (State::Suspended, Cause::HeadphoneDisconnected, Jack::Absent), "it saw the state AFTER the removal was handled");
    assert!(st.safe_verified);
}

#[test]
fn a_removal_that_arrives_while_the_dsp_is_booting_is_handled_the_moment_the_boot_is_over() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 1)));
    dev.rig.m.at_ms(100, Ev::Unplug); // inside the ~265 ms the start takes
    let (s, ph) = start_stream(&mut dev, &mut c, id);
    assert_eq!((s, ph.phase), (Status::Ok, StreamPhase::Running), "the start itself succeeded: the jack was in when it began");
    run_ms(&mut dev, 50);
    let st = get_status(&mut dev, &mut c);
    assert_eq!((st.state, st.cause), (State::Suspended, Cause::HeadphoneDisconnected));
    assert!(st.safe_verified);
    assert!(dev.dsp.is_none());
}

#[test]
fn the_emergency_path_makes_everything_safe_with_register_writes_and_the_codec_plans_only() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 1)));
    start_stream(&mut dev, &mut c, id);
    run_ms(&mut dev, 50);
    assert!(dev.dsp.is_some() && dev.rig.m.hda.running(7));
    let outbox0 = dev.rig.m.outbox.len();
    let rep = dev.emergency_safe_state();
    assert_eq!((rep.codec_safe, rep.dma_stopped, rep.dsp_off), (true, true, true));
    assert!(!dev.rig.m.hda.running(7));
    assert_eq!(dev.rig.m.dsp.r32(0x04) & (1 << 16), 0);
    assert!(dev.codec_is_safe_for_test());
    assert_eq!(dev.rig.m.outbox.len(), outbox0, "no message is sent from a dying task");
    assert_eq!(dev.rig.m.amp_touches(), 0);
}

#[test]
fn a_task_that_died_mid_stream_leaves_hardware_a_fresh_safe_only_launch_makes_safe() {
    let Some(mut dev) = started(true) else { return };
    let mut c = Client::new();
    let id = open_stream(&mut dev, &mut c);
    call(&mut dev, &mut c, Opcode::PcmWrite, &pcm_payload(id, &ramp(4096, 1)));
    start_stream(&mut dev, &mut c, id);
    run_ms(&mut dev, 50);
    // the task is killed: nothing runs its clean-up. The DMA engine, the DSP core and the codec are exactly as they were.
    let mut sim = dev.into_machine();
    assert!(sim.hda.running(7), "the engine is still reading a buffer nobody owns");
    assert_ne!(sim.dsp.r32(0x04) & (1 << 16), 0);
    assert_eq!(sim.codec.regs[0x69] & 0x40, 0, "the codec is unmuted and driving");
    // the loader relaunches the device with the safe-only role over the same hardware
    sim.writes.clear();
    sim.i2c.clear();
    sim.pad_writes.clear();
    sim.grant_hda = None;
    sim.grant_dsp = None;
    sim.grant_cfg = None;
    let mut fresh = avs_device::Device::new(sim);
    let rep = fresh.safe_only();
    assert_eq!((rep.codec_safe, rep.dma_stopped, rep.dsp_off), (true, true, true));
    let m = &fresh.rig.m;
    assert!(!m.hda.running(7));
    assert_eq!(m.dsp.r32(0x04) & (1 << 16), 0);
    assert_eq!(m.codec.regs[0x6b] & 0xc8, 0x40);
    assert_eq!(m.codec.regs[0x69] & 0xc0, 0x40);
    assert_eq!(m.amp_touches(), 0);
    assert!(m.refused_writes.is_empty());
    assert_eq!(m.pad_writes.len(), 1, "the speakers' reset was held first");
    assert!(m.i2c.iter().all(|t| t.addr == 0x1a));
    assert!(m.outbox.iter().all(|_| false), "it serves nothing");
}
