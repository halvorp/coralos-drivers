// SPDX-License-Identifier: GPL-2.0-only
//! The whole headphone boot against a scripted system, in order: what was asked, in which order, with which values - and what was never asked.
mod common;
use avs_boot_core::boot::*;
use avs_boot_core::error::*;
use avs_boot_core::plan::Role;
use avs_ipc_core::header::*;
use avs_ipc_core::modcfg::*;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::Topology;
use common::*;

fn ready() -> Option<(Vec<u8>, Vec<u8>)> {
    Some((tplg_bytes()?, nhlt_bytes()))
}

#[test]
fn the_full_boot_runs_every_stage_in_linux_order_and_ends_running() {
    let Some((tb, nb)) = ready() else { return };
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let cfg = cfg(&topo, &nhlt);
    let mut sys = Sys::new(Fw::atlas(&topo));
    let mut boot = Boot::new(&cfg).unwrap();
    let run = drive(&mut boot, &mut sys).unwrap();

    // 1. gating off first (loader.c:625-627), 2. base firmware, 3. config queries, 5. gating on, modules table
    let at = |l: &str| fw_pos(&sys, l).unwrap_or_else(|| panic!("{l} missing"));
    let ipc = |n: &str, k: usize| {
        let mut seen = 0;
        for (i, t) in sys.trace.iter().enumerate() {
            if let T::Ipc { name, .. } = t {
                if *name == n {
                    if seen == k {
                        return i;
                    }
                    seen += 1;
                }
            }
        }
        panic!("{n} #{k} missing")
    };
    assert!(at("power-gating-off") < at("core-power-on-cpa"));
    assert!(at("l1sen-off") < at("core-power-on-cpa"));
    assert!(at("fw-ready") < ipc("get hw config", 0));
    assert!(ipc("get hw config", 0) < ipc("get fw config", 0));
    assert!(ipc("get fw config", 0) < at("power-gating-on"), "gating is restored after the libraries (none here), before the modules table");
    assert!(at("l1sen-on") < ipc("get modules info", 0));
    // 6. back end first (pipeline 0), then the front end (pipeline 1); each module is initialised in its own pipeline
    let c0 = ipc("create pipeline", 0);
    let c1 = ipc("create pipeline", 1);
    let i0 = ipc("init instance", 0);
    let i1 = ipc("init instance", 1);
    let b = ipc("bind modules", 0);
    assert!(ipc("get modules info", 0) < c0 && c0 < i0 && i0 < c1 && c1 < i1 && i1 < b);
    // 7. the host stream: the BDL, then decouple, reset, descriptor setup, SPIB
    let bdl = pos(&sys, |t| matches!(t, T::Bdl(_))).unwrap();
    assert!(b < bdl);
    assert!(bdl < at("ppctl-decouple") && at("ppctl-decouple") < at("sd-reset-enter") && at("sd-reset-exited") < at("sd-stream-tag"));
    assert!(at("sd-fifosize") < at("spbfcctl-enable") && at("spbfcctl-enable") < at("spib-write"));
    // 8. the codec question comes after the host stream is prepared and BEFORE any clock or pipeline state
    let codec = pos(&sys, |t| *t == T::Codec).unwrap();
    assert!(at("spib-write") < codec);
    assert!(codec < at("intctl-sie-on") && at("intctl-sie-on") < at("sd-dma-start"));
    assert!(codec < ipc("set pipeline state", 0));
    // 9. start: host DMA, then FE pause/run (pipeline 1), then BE pause/run (pipeline 0), each followed by a read-back
    assert!(at("sd-dma-start") < ipc("set pipeline state", 0));
    let sets = msgs(&sys, "set pipeline state");
    let decoded: Vec<(u32, u32)> = sets.iter().map(|(h, _)| (SET_PPL_ID.get(*h), SET_PPL_STATE.get(*h))).collect();
    assert_eq!(decoded, vec![(1, AVS_PPL_STATE_PAUSED), (1, AVS_PPL_STATE_RUNNING), (0, AVS_PPL_STATE_PAUSED), (0, AVS_PPL_STATE_RUNNING)]);
    assert_eq!(names(&sys).iter().filter(|n| **n == "get pipeline state").count(), 4);
    // the exact IPC sequence
    assert_eq!(
        names(&sys),
        vec![
            "get hw config",
            "get fw config",
            "get modules info",
            "create pipeline",
            "init instance",
            "create pipeline",
            "init instance",
            "bind modules",
            "set pipeline state",
            "get pipeline state",
            "set pipeline state",
            "get pipeline state",
            "set pipeline state",
            "get pipeline state",
            "set pipeline state",
            "get pipeline state",
        ]
    );
    // the result
    assert_eq!(run.dma_id, 0);
    assert_eq!(run.host_stream.index, 7);
    assert_eq!(run.pipelines.iter().map(|p| (p.role, p.ppl)).collect::<Vec<_>>(), vec![(Role::FrontEnd, 1), (Role::BackEnd, 0)]);
    assert!(run.peakvols.is_empty(), "the da7219 topology has no peakvol module: the codec's gain ceiling is the limit");
    assert_eq!(sys.ppl_state.iter().filter(|p| p.1 == AVS_PPL_STATE_RUNNING).count(), 2);
}

#[test]
fn the_pipelines_and_modules_carry_the_topologys_values() {
    let Some((tb, nb)) = ready() else { return };
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let cfg = cfg(&topo, &nhlt);
    let mut sys = Sys::new(Fw::atlas(&topo));
    drive(&mut Boot::new(&cfg).unwrap(), &mut sys).unwrap();
    // pplcfg: req_size 2, priority 0, lp false, attributes 0 (da7219-tplg.bin dump in avs-topology-core/tests/real_blob.rs)
    let creates = msgs(&sys, "create pipeline");
    for (i, (h, _)) in creates.iter().enumerate() {
        assert_eq!(CREATE_PPL_MEM_SIZE.get(*h), 2);
        assert_eq!(CREATE_PPL_PRIORITY.get(*h), 0);
        assert_eq!(CREATE_PPL_INSTANCE_ID.get(*h), i as u32, "back end is pipeline 0, front end pipeline 1");
        assert_eq!(CREATE_PPL_LP.get(*h), 0);
        assert_eq!(CREATE_PPL_ATTRIBUTES.get(*h), 0);
    }
    let inits = msgs(&sys, "init instance");
    assert_eq!(inits.len(), 2);
    for (k, (h, param)) in inits.iter().enumerate() {
        // the firmware's copier is module id 1; each is instance 0 and 1 of it; core 0, domain 0 from the topology
        assert_eq!(MOD_MODULE_ID.get(*h), 1);
        assert_eq!(MOD_INSTANCE_ID.get(*h), k as u32, "second copier is instance 1");
        assert_eq!(INIT_PPL_INSTANCE_ID.get(*h), k as u32);
        assert_eq!((INIT_CORE_ID.get(*h), INIT_PROC_DOMAIN.get(*h)), (0, 0));
        // the parameter block: the copier base (cpc, ibs, obs, pages + input format), the output format, the feature mask, the node id,
        // the DMA buffer size, the blob length in dwords, then the blob (COPIER_CFG_FIXED_SIZE = 68, messages.h:836)
        assert_eq!(INIT_PARAM_BLOCK_SIZE.get(*h) as usize, param.len().div_ceil(4));
        let rd = |o: usize| u32::from_le_bytes([param[o], param[o + 1], param[o + 2], param[o + 3]]);
        let (node, dmabuf, blob_dwords) = (rd(68), rd(72), rd(76) as usize);
        assert_eq!(dmabuf, 768, "copier.dma_buffer_size of da7219-tplg.bin");
        assert_eq!(param.len(), 80 + blob_dwords * 4);
        if k == 0 {
            // the back end: I2S link output (12), virtual index 0x10 = SSP1 time slot 0 (messages.h:755-777); the NHLT blob of SSP1 render
            assert_eq!(node, 0x10 | (12 << 8));
            assert_eq!(&param[80..], blob(88, DA7219_RENDER_SEED).as_slice());
        } else {
            // the front end: HDA host output (0), virtual index = the DMA id (stream tag 1 - 1 = 0); the default 4-byte blob
            assert_eq!(node, 0);
            assert_eq!(blob_dwords, 1);
        }
    }
    // base values: cpc 12000, pages 1 (da7219-tplg.bin modcfg bases)
    let be = inits[0].1;
    assert_eq!(u32::from_le_bytes(be[0..4].try_into().unwrap()), 12000);
}

#[test]
fn the_front_end_is_bound_to_the_back_end_at_pin_zero() {
    let Some((tb, nb)) = ready() else { return };
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let cfg = cfg(&topo, &nhlt);
    let mut sys = Sys::new(Fw::atlas(&topo));
    drive(&mut Boot::new(&cfg).unwrap(), &mut sys).unwrap();
    let binds = msgs(&sys, "bind modules");
    assert_eq!(binds.len(), 1, "one binding; the single-module pipelines bind nothing internally");
    let h = binds[0].0;
    // source = the front end's copier (instance 1), sink = the back end's (instance 0); both module id 1
    assert_eq!((MOD_MODULE_ID.get(h), MOD_INSTANCE_ID.get(h)), (1, 1));
    assert_eq!((BIND_DST_MODULE_ID.get(h), BIND_DST_INSTANCE_ID.get(h)), (1, 0));
    assert_eq!((BIND_DST_QUEUE.get(h), BIND_SRC_QUEUE.get(h)), (0, 0));
}

#[test]
fn speakers_are_never_touched_by_a_successful_boot() {
    let Some((tb, nb)) = ready() else { return };
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let cfg = cfg(&topo, &nhlt);
    let mut sys = Sys::new(Fw::atlas(&topo));
    drive(&mut Boot::new(&cfg).unwrap(), &mut sys).unwrap();
    let max_blob = blob(88, MAX98373_SEED);
    for (h, param) in msgs(&sys, "init instance") {
        // no gateway on SSP0: the node id's virtual index never has instance 0 for an I2S link
        let node = u32::from_le_bytes(param[68..72].try_into().unwrap());
        if (node >> 8) & 0x1f == AVS_DMA_I2S_LINK_OUTPUT {
            assert_eq!((node & 0xff) >> 4, 1, "SSP1 only (header {h:#x})");
        }
        assert!(!param.windows(max_blob.len()).any(|w| w == max_blob.as_slice()), "the MAX98373 render blob must never be sent");
    }
    // and no message addressed a module other than the copier (no amplifier/DSM/Waves modules, no libraries loaded)
    assert!(!sys.trace.iter().any(|t| matches!(t, T::Fw(l) if l.starts_with("code-ipc"))));
}

#[test]
fn the_unwind_after_a_failure_restores_the_gating_it_disabled_and_nothing_else() {
    let Some((tb, nb)) = ready() else { return };
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let cfg = cfg(&topo, &nhlt);
    let mut sys = Sys::new(Fw::atlas(&topo));
    sys.fw_override.push(("rom-init", avs_fw_core::effect::Input::PollTimedOut(0)));
    let f = drive(&mut Boot::new(&cfg).unwrap(), &mut sys).unwrap_err();
    assert_eq!((f.stage, f.step), (Stage::BaseFirmware, "rom-init"));
    assert!(matches!(f.reason, Reason::Fw(_)));
    assert!(f.dsp_dirty, "the core was powered: the caller must power it down");
    assert!(f.gating_restored);
    assert!(names(&sys).is_empty(), "no IPC was attempted on a DSP that never reached the ROM");
    // the gating restore ran AFTER the failure
    let l = fw_pos(&sys, "power-gating-on").expect("gating restored");
    assert!(l > fw_pos(&sys, "rom-init").unwrap());
}

#[test]
fn shutdown_program_is_the_main_core_disable() {
    let p = Boot::shutdown_program();
    let labels: Vec<_> = p.as_slice().iter().map(|s| s.label).collect();
    assert_eq!(labels, vec!["core-stall", "core-stall-ack", "core-stall-settle", "core-reset-enter", "core-reset-enter-ack", "core-power-off", "core-power-off-settle", "core-power-off-cpa"]);
}
