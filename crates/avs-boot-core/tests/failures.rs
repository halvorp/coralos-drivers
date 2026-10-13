// SPDX-License-Identifier: GPL-2.0-only
//! A failure at each stage is NAMED and STOPS THE SEQUENCE: nothing past it is asked of the DSP, the gating is restored, and the caller is
//! told whether the DSP is dirty. The codec gate keeps the I2S clocks stopped.
mod common;
use avs_boot_core::boot::*;
use avs_boot_core::error::*;
use avs_fw_core::effect::{Cause, Input as FwInput};
use avs_ipc_core::error::IpcError;
use avs_ipc_core::header::*;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::Topology;
use common::*;

fn run(setup: impl FnOnce(&mut Sys)) -> Option<(Result<avs_boot_core::running::Running, BootFailure>, Sys)> {
    let tb = tplg_bytes()?;
    let nb = nhlt_bytes();
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let c = cfg(&topo, &nhlt);
    let mut sys = Sys::new(Fw::atlas(&topo));
    setup(&mut sys);
    let r = drive(&mut Boot::new(&c).unwrap(), &mut sys);
    Some((r, sys))
}

fn no_ipc_after(sys: &Sys, last: &str) {
    let n = names(sys);
    assert_eq!(*n.last().unwrap(), last, "{n:?}");
}

#[test]
fn a_refused_hardware_config_stops_everything_and_restores_the_gating() {
    let Some((r, sys)) = run(|s| s.ipc_status.push(("get hw config", 0, 5))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.step, f.name()), (Stage::ConfigQuery, "get hw config", "ipc-refused"));
    assert_eq!(f.reason, Reason::IpcRefused { status: 5 });
    assert!(f.dsp_dirty && f.gating_restored);
    assert_eq!(names(&sys), ["get hw config"]);
    assert!(fw_pos(&sys, "power-gating-on").unwrap() > pos(&sys, |t| matches!(t, T::Ipc { .. })).unwrap(), "the unwind ran after the failure");
}

#[test]
fn an_ipc_layer_failure_is_named_with_its_cause() {
    let Some((r, sys)) = run(|s| s.ipc_error.push(("get fw config", 0, IpcError::Timeout { exception: true }))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.step), (Stage::ConfigQuery, "get fw config"));
    assert_eq!(f.reason, Reason::Ipc(IpcError::Timeout { exception: true }));
    no_ipc_after(&sys, "get fw config");
}

#[test]
fn a_modules_table_that_is_refused_ends_the_boot_at_the_module_stage() {
    let Some((r, sys)) = run(|s| s.ipc_status.push(("get modules info", 0, 1))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.name()), (Stage::ModuleInfo, "ipc-refused"));
    assert!(f.gating_restored);
    no_ipc_after(&sys, "get modules info");
    assert!(fw_pos(&sys, "power-gating-on").is_some(), "gating was restored before the modules table was asked for");
}

#[test]
fn a_pipeline_the_firmware_refuses_stops_the_script_where_it_is() {
    // the second module's init (the front end's copier) is refused: nothing is bound, no BDL is written, no clock question is asked
    let Some((r, sys)) = run(|s| s.ipc_status.push(("init instance", 1, 3))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.step, f.reason.clone()), (Stage::PathCreate, "init instance", Reason::IpcRefused { status: 3 }));
    assert!(f.dsp_dirty, "two pipelines exist on the DSP: the caller must power it down");
    assert!(!names(&sys).contains(&"bind modules"));
    assert!(!sys.trace.iter().any(|t| matches!(t, T::Bdl(_) | T::Codec)));
    let Some((r, sys)) = run(|s| s.ipc_status.push(("create pipeline", 0, 9))) else { return };
    assert_eq!(r.unwrap_err().stage, Stage::PathCreate);
    assert_eq!(names(&sys).iter().filter(|n| **n == "init instance").count(), 0);
}

#[test]
fn a_bind_the_firmware_refuses_stops_before_the_host_stream() {
    let Some((r, sys)) = run(|s| s.ipc_status.push(("bind modules", 0, 4))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.step), (Stage::PathBind, "bind modules"));
    assert!(!sys.trace.iter().any(|t| matches!(t, T::Bdl(_) | T::Codec)));
    assert!(fw_pos(&sys, "ppctl-decouple").is_none());
}

#[test]
fn a_host_stream_that_will_not_reset_stops_before_the_codec_is_asked_about() {
    let Some((r, sys)) = run(|s| s.fw_override.push(("sd-reset-entered", FwInput::PollTimedOut(0)))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.step), (Stage::HostStream, "sd-reset-entered"));
    assert!(matches!(f.reason, Reason::Fw(ff) if ff.cause == Cause::PollTimeout { last: 0 }));
    assert!(!sys.trace.iter().any(|t| *t == T::Codec));
    assert!(fw_pos(&sys, "sd-stream-tag").is_none(), "the descriptor was never programmed");
    assert!(fw_pos(&sys, "sd-dma-start").is_none());
}

#[test]
fn if_the_codec_is_not_proven_safe_no_clock_starts_and_no_pipeline_runs() {
    let Some((r, sys)) = run(|s| s.codec_safe = false) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.step, f.name()), (Stage::CodecGate, "codec-safe", "codec-not-safe"));
    assert_eq!(f.reason, Reason::CodecNotSafe);
    // the host stream is prepared and the pipelines exist, but the DMA never started and no pipeline left RESET
    assert!(fw_pos(&sys, "spib-write").is_some());
    assert!(fw_pos(&sys, "intctl-sie-on").is_none() && fw_pos(&sys, "sd-dma-start").is_none());
    assert!(!names(&sys).contains(&"set pipeline state"));
    assert!(sys.ppl_state.is_empty());
    assert!(f.dsp_dirty);
}

#[test]
fn a_pipeline_that_does_not_reach_the_requested_state_is_caught_by_the_read_back() {
    // front-end pipeline 1 reports RESET after being set PAUSED: the run is never issued
    let Some((r, sys)) = run(|s| s.state_lie.push((1, AVS_PPL_STATE_PAUSED, AVS_PPL_STATE_RESET))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.step), (Stage::PathPause, "get pipeline state"));
    assert_eq!(f.reason, Reason::PipelineStateMismatch { ppl: 1, want: AVS_PPL_STATE_PAUSED, got: AVS_PPL_STATE_RESET });
    assert_eq!(msgs(&sys, "set pipeline state").len(), 1, "no RUNNING after a failed PAUSED");
    // RUNNING read back as PAUSED: the back end is never touched
    let Some((r, sys)) = run(|s| s.state_lie.push((1, AVS_PPL_STATE_RUNNING, AVS_PPL_STATE_PAUSED))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.reason.clone()), (Stage::PathRun, Reason::PipelineStateMismatch { ppl: 1, want: AVS_PPL_STATE_RUNNING, got: AVS_PPL_STATE_PAUSED }));
    assert!(msgs(&sys, "set pipeline state").iter().all(|(h, _)| SET_PPL_ID.get(*h) == 1), "pipeline 0 (the SSP1 gateway) never left RESET");
    // the back end's own read-back
    let Some((r, _)) = run(|s| s.state_lie.push((0, AVS_PPL_STATE_RUNNING, AVS_PPL_STATE_PAUSED))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.reason), (Stage::PathRun, Reason::PipelineStateMismatch { ppl: 0, want: AVS_PPL_STATE_RUNNING, got: AVS_PPL_STATE_PAUSED }));
}

#[test]
fn a_caller_that_answers_the_wrong_thing_gets_a_protocol_failure_not_a_guess() {
    let Some(tb) = tplg_bytes() else { return };
    let nb = nhlt_bytes();
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let c = cfg(&topo, &nhlt);
    // the first event must be Begin
    let f = match Boot::new(&c).unwrap().next(Event::CodecSafe(true)) {
        Action::Failed(f) => f,
        other => panic!("{other:?}"),
    };
    assert_eq!(f.reason, Reason::Protocol);
    // an IPC reply to a hardware step
    let mut b = Boot::new(&c).unwrap();
    assert!(matches!(b.next(Event::Begin), Action::Fw(_)));
    let f = match b.next(Event::IpcReply { header: 0, payload: &[] }) {
        Action::Failed(f) => f,
        other => panic!("{other:?}"),
    };
    assert_eq!((f.reason.clone(), f.name()), (Reason::Protocol, "protocol"));
    // the failure is final: asking again fails again, it does not panic or resume
    assert!(matches!(b.next(Event::Begin), Action::Failed(_)));
    // a codec answer to a hardware step is also a protocol failure
    let mut b = Boot::new(&c).unwrap();
    let _ = b.next(Event::Begin);
    assert!(matches!(b.next(Event::CodecSafe(true)), Action::Failed(f) if f.reason == Reason::Protocol));
}

#[test]
fn a_success_leaves_no_failure_to_report() {
    let Some((r, sys)) = run(|_| {}) else { return };
    assert!(r.is_ok());
    assert!(sys.trace.iter().filter(|t| matches!(t, T::Codec)).count() == 1, "the codec is asked once");
}
