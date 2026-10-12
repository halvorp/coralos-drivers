// SPDX-License-Identifier: GPL-2.0-only
//! What the finished boot hands on: the stop messages in Linux's order, with pipelines of several shapes.
mod common;
use avs_boot_core::plan::{PipelineInfo, Role};
use avs_boot_core::running::Running;
use avs_ipc_core::header::*;
use avs_ipc_core::payload::{FwCfg, HwCfg};
use hda_ext_core::stream::{Direction, HostStream};

fn running() -> Running {
    let p = |role, ppl| PipelineInfo { role, template_id: ppl as u32, ppl, auto: true };
    Running {
        host_stream: HostStream { index: 7, tag: 1, direction: Direction::Playback },
        // three front-end pipelines (5, 2, 9 - not in id order) and two back-end ones
        pipelines: vec![p(Role::FrontEnd, 5), p(Role::FrontEnd, 2), p(Role::FrontEnd, 9), p(Role::BackEnd, 0), p(Role::BackEnd, 1)],
        peakvols: vec![],
        fw_cfg: FwCfg::default(),
        hw_cfg: HwCfg::default(),
        dma_id: 0,
    }
}

fn decode(ms: &[avs_boot_core::msg::IpcMsg]) -> Vec<(u32, u32)> {
    ms.iter().map(|m| (SET_PPL_ID.get(m.header), SET_PPL_STATE.get(m.header))).collect()
}

#[test]
fn pause_runs_the_last_pipeline_first_and_reset_the_first_pipeline_first() {
    let r = running();
    // avs_path_pause iterates the pipeline list in reverse (path.c:1521); avs_path_reset in order (:1458)
    assert_eq!(decode(&r.pause_msgs(Role::FrontEnd).unwrap()), vec![(9, AVS_PPL_STATE_PAUSED), (2, AVS_PPL_STATE_PAUSED), (5, AVS_PPL_STATE_PAUSED)]);
    assert_eq!(decode(&r.pause_msgs(Role::BackEnd).unwrap()), vec![(1, AVS_PPL_STATE_PAUSED), (0, AVS_PPL_STATE_PAUSED)]);
    assert_eq!(decode(&r.reset_msgs(Role::FrontEnd).unwrap()), vec![(5, AVS_PPL_STATE_RESET), (2, AVS_PPL_STATE_RESET), (9, AVS_PPL_STATE_RESET)]);
    assert_eq!(decode(&r.reset_msgs(Role::BackEnd).unwrap()), vec![(0, AVS_PPL_STATE_RESET), (1, AVS_PPL_STATE_RESET)]);
    for m in r.pause_msgs(Role::FrontEnd).unwrap() {
        assert_eq!(m.name, "set pipeline state");
        assert!(m.payload.is_empty());
    }
}

#[test]
fn the_visit_order_filters_by_role_skips_non_auto_pipelines_on_run_and_reverses_on_pause() {
    use avs_boot_core::running::pipeline_order;
    let p = |role, ppl, auto| PipelineInfo { role, template_id: ppl as u32, ppl, auto };
    let all = [p(Role::FrontEnd, 3, true), p(Role::BackEnd, 0, true), p(Role::FrontEnd, 4, false), p(Role::FrontEnd, 8, true), p(Role::BackEnd, 1, false)];
    assert_eq!(pipeline_order(&all, Role::FrontEnd, false, false).unwrap(), vec![3, 4, 8]);
    assert_eq!(pipeline_order(&all, Role::FrontEnd, false, true).unwrap(), vec![8, 4, 3]);
    assert_eq!(pipeline_order(&all, Role::FrontEnd, true, false).unwrap(), vec![3, 8], "run: only AUTO pipelines (path.c:1588)");
    assert_eq!(pipeline_order(&all, Role::BackEnd, true, false).unwrap(), vec![0]);
    assert_eq!(pipeline_order(&all, Role::BackEnd, false, true).unwrap(), vec![1, 0]);
    assert_eq!(pipeline_order(&[], Role::FrontEnd, false, false).unwrap(), Vec::<u8>::new());
}
