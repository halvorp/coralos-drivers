// SPDX-License-Identifier: GPL-2.0-only
//! M1 and M2 against the simulated Atlas Wi-Fi: power-up, firmware upload, ALIVE, NVM, antennas, PHY
//! configuration, calibration, and every way that sequence fails. See tests/sim for what the card enforces;
//! every test asserts the card recorded NO violation (a register touched without the MAC awake, a chunk
//! aimed at the extended window without LMPM_CHICK, a doorbell with no firmware, ...).
//! Expected values are written out by hand from the Linux lines cited beside them.
mod sim;
use iwlwifi_device::driver::{Driver, DriverError};
use iwlwifi_device::fwfile::FwError;
use iwlwifi_device::sha256::sha256;
use iwlwifi_device::transport::{Fault, TransportError};
use iwlwifi_mvm_init_core::machine::Failure;
use sim::blob::{build, from_file, Blob, Opts};
use sim::fwmodel::{FwImageModel, FwState, SecModel};
use sim::Sim;

fn world(b: &Blob) -> Sim {
    let sim = Sim::new();
    {
        let mut w = sim.w();
        w.fw.init = Some(b.init.clone());
        w.fw.regular = Some(b.regular.clone());
    }
    sim
}

fn driver(sim: &Sim, b: &Blob) -> Driver<sim::SimHost> {
    Driver::new(sim.host(), b.bytes.clone(), &sha256(&b.bytes)).expect("the blob is a valid file")
}

fn up() -> (Sim, Driver<sim::SimHost>, iwlwifi_device::InitReport) {
    let b = build(&Opts::default());
    let sim = world(&b);
    let mut d = driver(&sim, &b);
    let rep = d.bring_up().expect("bring_up");
    (sim, d, rep)
}

fn no_violations(sim: &Sim) {
    assert_eq!(sim.w().violations, Vec::<String>::new());
}

#[test]
fn the_card_comes_up_through_init_complete_with_no_violation() {
    let (sim, _d, rep) = up();
    let w = sim.w();
    assert_eq!(w.violations, Vec::<String>::new());
    assert_eq!(w.fw.boots, [FwState::Init]);
    assert_eq!(w.forced_nmi, 0);
    assert_eq!(rep.hw_rev, 0x321);
    assert_eq!(rep.mac, [0x12, 0x22, 0x33, 0x44, 0x55, 0x66]);
    assert_eq!(rep.phy_db_entries, 4);
    assert_eq!(rep.nvm_bytes_read, 0x200 + 4096 + 0x120 + 0x40 + 0x60 + 0x20); // SW, REGULATORY, CALIB, PROD, HW, PHY_SKU
    assert!(!rep.rfkill);
    assert_eq!(rep.alive.status, 0xCAFE);
    assert_eq!(rep.alive.lmac.ucode_major, 46);
}

#[test]
fn the_upload_ladder_chunks_and_destinations_are_the_linux_ones() {
    let (sim, _d, rep) = up();
    let w = sim.w();
    // iwl_pcie_load_cpu_sections_8000 (trans.c:715-783): the INIT image has no separator, so ALL FIVE sections are
    // CPU1's; each ORs in the next section number (sec_num = sec_num << 1 | 1, :759), then 0xFFFF; CPU2 has none and
    // writes only 0xFFFFFFFF (:774-779).
    assert_eq!(w.load_status_history, [0x1, 0x3, 0x7, 0xf, 0x1f, 0xffff, 0xffff_ffff]);
    // chunk_sz = min(FH_MEM_TB_MAX_LENGTH 0x20000, section len), trans.c:663; dst = section->offset + offset, :685.
    let got: Vec<(u32, u32)> = w.srvc_chunks.iter().map(|c| (c.0, c.2)).collect();
    assert_eq!(
        got,
        [(0x40_2000, 712), (0x80_0000, 0x2_0000), (0x82_0000, 0x2_0000), (0x84_0000, 300_000 - 0x4_0000), (0, 5000), (0x4_8000, 70_000), (0x40_4000, 696)]
    );
    // ONE bounce buffer, reused (the DEVIATION named in nic-core load.rs).
    assert!(w.srvc_chunks.iter().all(|c| c.1 == w.srvc_chunks[0].1));
    assert!(w.srvc_chunks[0].1 >> 32 != 0, "the bounce buffer sits above 4 GiB: the TFDIB hi address bits carried it");
    assert_eq!((rep.fw_chunks, rep.fw_bytes), (7, 712 + 300_000 + 5000 + 70_000 + 696));
    // RELEASE_CPU_RESET (prph 0x300c) bit 24 was written BEFORE the first chunk (trans.c:997).
    let first_chunk_t = w.writes.iter().position(|x| x.1 == "w32" && x.2 == 0x1e20 && x.3 == 0x8010_0000).unwrap();
    let release_t = w.writes.iter().position(|x| x.1 == "prph" && x.2 == 0x300c && x.3 == 1 << 24).unwrap();
    assert!(release_t < first_chunk_t);
}

#[test]
fn the_rfh_receive_queue_is_programmed_as_rx_mq_hw_init_does() {
    let (sim, _d, _) = up();
    let w = sim.w();
    let prph = |a: u32| w.writes.iter().filter(|x| x.1 == "prph" && x.2 == a).map(|x| x.3).collect::<Vec<_>>();
    // RFH_RXF_DMA_CFG = EN(1<<31) | RB_SIZE_4K(4<<16) | MIN_RB_4_8(3<<24) | DROP_TOO_LARGE(0x04000000) | RBDCB_SIZE_512(9<<20), rx.c:960-964
    assert_eq!(prph(0x09820), [0, 0x8000_0000 | (4 << 16) | (3 << 24) | 0x0400_0000 | (9 << 20)]);
    // RFH_GEN_CFG = RFH_DMA_SNOOP | SERVICE_DMA_SNOOP | RB_CHUNK_SIZE_128 (BIT(4)), rx.c:971-978
    assert_eq!(prph(0x09800), [(1 << 1) | (1 << 0) | (1 << 4)]);
    // RXQ_ACTIVE: 0 first, then BIT(0) | BIT(16), rx.c:930, :950, :980
    assert_eq!(prph(0x0980c), [0, 0x0001_0001]);
    // The free-descriptor table, used table and status block addresses went in as 64-bit pairs.
    assert_eq!(prph(0x08000).len() + prph(0x08004).len(), 2);
    // The posted pool (128 buffers) was announced through the shadow write-index trigger, in multiples of 8 (rx.c:193).
    let trg: Vec<u32> = w.writes.iter().filter(|x| x.1 == "w32" && x.2 == 0x1c80).map(|x| x.3).collect();
    assert!(!trg.is_empty() && trg.iter().all(|v| v % 8 == 0), "{trg:?}");
    assert_eq!(*trg.first().unwrap(), 128);
    // CSR_INT_COALESCING byte write, rx.c:985
    assert!(w.writes.iter().any(|x| x.1 == "w8" && x.2 == 0x004 && x.3 == 0x40));
    // CSR_MAC_SHADOW_REG_CTRL <- 0x800FFFFF, trans.c:515
    assert!(w.writes.iter().any(|x| x.1 == "w32" && x.2 == 0xa8 && x.3 == 0x800f_ffff));
}

#[test]
fn the_host_commands_are_the_ones_iwl_run_init_mvm_ucode_sends_in_its_order() {
    let (sim, _d, _) = up();
    let w = sim.w();
    let kinds: Vec<(u8, u8)> = w.cmds.iter().map(|c| (c.group, c.cmd)).collect();
    // 13 NVM sections, REGULATORY read in three chunks (2048, 2048, then NOT_VALID_ADDRESS at 4096); 7 absent sections answer once.
    assert_eq!(kinds.iter().filter(|k| **k == (0, 0x88)).count(), 15);
    assert_eq!(&kinds[15..], [(0, 0x98), (0, 0x6a)]);
    // NVM reads: op_code 0 (read), target 0, type, offset, length 2048 (mvm/nvm.c:18, :75-80).
    assert_eq!(w.cmds[0].payload, [0, 0, 0, 0, 0, 0, 0x00, 0x08]);
    let reg: Vec<u16> = w.cmds.iter().filter(|c| c.payload[2] == 3).map(|c| u16::from_le_bytes([c.payload[4], c.payload[5]])).collect();
    assert_eq!(reg, [0, 2048, 4096]);
    // TX_ANT_CONFIGURATION_CMD: the firmware's 3 ANDed with the NVM's 3 (mvm/mvm.h:1757-1765).
    assert_eq!(w.cmds[15].payload, [3, 0, 0, 0]);
    // PHY_CONFIGURATION_CMD v1: phy_cfg (fw word with the valid chains), flow_trigger, event_trigger (mvm/fw.c:534-577).
    assert_eq!(w.fw.phy_cfg_seen, [[0x0033_0018, 0x1f0f_0001, 0x0000_0001]]);
    // Every command's sequence names its TFD (tx.c:1475-1477) -- the card checks it; and a 12-byte command is ONE TB.
    assert!(w.cmds.iter().take(15).all(|c| c.tbs.len() == 1 && c.tbs[0].1 == 12));
    assert_eq!(w.cmds.iter().map(|c| c.seq).collect::<Vec<_>>(), (0..17).collect::<Vec<u16>>());
    drop(w);
    no_violations(&sim);
}

#[test]
fn the_log_names_each_stage_in_order() {
    let (sim, _d, _) = up();
    let w = sim.w();
    let stages = ["[IWL-HW-READY", "[IWL-FW-LOADED init", "[IWL-ALIVE init", "[IWL-NVM", "[IWL-INIT-COMPLETE"];
    let mut at = 0;
    for s in stages {
        at += w.log[at..].iter().position(|l| l.starts_with(s)).unwrap_or_else(|| panic!("{s} missing in {:#?}", w.log));
    }
    assert!(w.log.iter().any(|l| l.contains("mac=12:22:33:44:55:66")));
}

#[test]
fn the_dma_budget_fits_with_room_for_the_firmware_read() {
    let (sim, _d, _) = up();
    let w = sim.w();
    // WLAN_DMA_PAGES is 1024 and the 1.49 MB firmware read (364 pages) is charged to the same budget (K5 of the readiness plan).
    let pages = w.dma_used / 4096;
    assert!(pages + 364 <= 1024, "{pages} pages held");
}

#[test]
fn a_firmware_that_never_says_alive_times_out_after_two_seconds_and_dumps_the_diagnostics() {
    let b = build(&Opts::default());
    let sim = world(&b);
    sim.w().fw.suppress_alive = true;
    let mut d = driver(&sim, &b);
    let t0 = sim.w().now_us;
    assert_eq!(d.bring_up().unwrap_err(), DriverError::Init(Failure::AliveTimeout));
    let w = sim.w();
    assert!(w.now_us - t0 >= 2_000_000, "waited {} us", w.now_us - t0);
    assert!(w.log.iter().any(|l| l.starts_with("[IWL-ALIVE FAIL timeout=2000ms csr_int=")));
    assert!(w.log.iter().any(|l| l.starts_with("[IWL-ALIVE-TIMEOUT img=INIT")), "{:#?}", w.log);
    assert_eq!(w.violations, Vec::<String>::new());
}

#[test]
fn an_alive_with_a_status_other_than_cafe_is_refused() {
    let b = build(&Opts::default());
    let sim = world(&b);
    sim.w().fw.alive_status = 0xDEAD;
    let mut d = driver(&sim, &b);
    assert_eq!(d.bring_up().unwrap_err(), DriverError::Init(Failure::AliveInvalid));
    assert!(sim.w().log.iter().any(|l| l.contains("[IWL-ALIVE FAIL status=0xdead]")));
    no_violations(&sim);
}

#[test]
fn an_image_that_is_not_the_one_in_sram_never_boots() {
    let b = build(&Opts::default());
    let sim = world(&b);
    // The card was built to expect a different INIT image than the file carries.
    sim.w().fw.init = Some(FwImageModel { secs: vec![SecModel { offset: 0x0040_2000, data: vec![0xAA; 712] }] });
    sim.w().fw.regular = None;
    let mut d = driver(&sim, &b);
    assert_eq!(d.bring_up().unwrap_err(), DriverError::Init(Failure::AliveTimeout));
    assert_eq!(sim.w().fw.boots, [FwState::Dead]);
}

#[test]
fn silicon_step_a_is_refused_before_the_card_is_touched() {
    let b = build(&Opts::default());
    let sim = world(&b);
    sim.w().csr.insert(0x028, 0x0000_0320); // CSR_HW_REV: step 0 = A
    let mut d = driver(&sim, &b);
    assert_eq!(d.bring_up().unwrap_err(), DriverError::Transport(TransportError::UnsupportedStep { hw_rev: 0x320 }));
    assert!(sim.w().writes.is_empty(), "nothing was written");
}

#[test]
fn step_c_is_accepted_too() {
    let b = build(&Opts::default());
    let sim = world(&b);
    sim.w().csr.insert(0x028, 0x0000_0322);
    let mut d = driver(&sim, &b);
    d.bring_up().unwrap();
    assert!(sim.w().log[0].contains("step=C"));
}

#[test]
fn with_the_rf_kill_switch_in_kill_the_nvm_is_read_and_the_radio_is_not_configured() {
    let b = build(&Opts::default());
    let sim = world(&b);
    sim.w().hw_rfkill = true; // CSR_GP_CNTRL_REG_FLAG_HW_RF_KILL_SW reads clear = KILL (iwl-csr.h:297; internal.h:1048-1059)
    let mut d = driver(&sim, &b);
    let rep = d.bring_up().unwrap();
    assert!(rep.rfkill);
    let w = sim.w();
    assert!(w.fw.tx_ant_seen.is_empty() && w.fw.phy_cfg_seen.is_empty(), "mvm/fw.c:761-766: stop at the rfkill check");
    assert_eq!(w.cmds.iter().filter(|c| c.cmd == 0x88).count(), 15, "CMD_SEND_IN_RFKILL: the NVM reads went out");
    assert_eq!(w.violations, Vec::<String>::new());
}

#[test]
fn a_card_that_does_not_wake_forces_an_nmi_once_and_the_fault_is_remembered() {
    let (sim, mut d, _) = up();
    sim.w().wedge_mac_access = true;
    use iwlwifi_pcie_nic_core::bus::{NicFault, NicIo, READ_FAILED};
    use iwlwifi_pcie_trans_core::io::TransIo;
    let t0 = sim.w().now_us;
    assert_eq!(d.t.nic.prph_read(0x3000), READ_FAILED); // iwl-io.c:141-142
    assert!(matches!(d.t.nic.fault(), Some(NicFault::AccessTimeout { .. })));
    assert!(sim.w().now_us - t0 >= 15_000, "the grab polls for 15 ms (trans.c:2378)");
    assert_eq!(sim.w().forced_nmi, 1, "CSR_RESET <- FORCE_NMI (trans.c:2397-2398)");
    let t1 = sim.w().now_us;
    assert_eq!(d.t.nic.prph_read(0x3004), READ_FAILED);
    assert_eq!(sim.w().forced_nmi, 1, "a dead card is not NMI'd again");
    assert_eq!(sim.w().now_us, t1, "and not polled again");
}

#[test]
fn a_command_the_firmware_never_answers_times_out_after_two_seconds_with_an_nmi() {
    let b = build(&Opts::default());
    let sim = world(&b);
    sim.w().fw.swallow_cmd = Some(0x98);
    let mut d = driver(&sim, &b);
    let e = d.bring_up().unwrap_err();
    assert_eq!(e, DriverError::Init(Failure::TxAntSendFailed { errno: -110 }));
    let w = sim.w();
    assert_eq!(w.forced_nmi, 1, "iwl_trans_pcie_sync_nmi (tx.c:2597)");
    assert_eq!(w.violations, Vec::<String>::new());
}

#[test]
fn a_firmware_assert_is_a_recorded_fault_and_the_error_table_is_logged() {
    let b = build(&Opts::default());
    let sim = world(&b);
    sim.w().fw.sw_error_on_cmd = Some(0x98);
    let mut d = driver(&sim, &b);
    let e = d.bring_up().unwrap_err();
    assert!(matches!(e, DriverError::Init(Failure::TxAntSendFailed { .. })), "{e:?}");
    assert!(matches!(d.fault(), Some(Fault::SwError { .. })));
    let w = sim.w();
    let line = w.log.iter().find(|l| l.starts_with("[IWL-FW-ERROR img=INIT")).unwrap_or_else(|| panic!("{:#?}", w.log));
    assert!(line.contains("id=0x00000035") && line.contains("desc=SYSASSERT") && line.contains("pc=0x0040beef") && line.contains("hcmd=0x00000098"), "{line}");
}

#[test]
fn many_commands_wrap_the_tfd_ring_and_the_32_slot_window() {
    let (sim, mut d, _) = up();
    use iwlwifi_pcie_ring_core::tx::Chunk;
    // 300 round trips: write_ptr wraps at 256 (iwl_txq_inc_wrap, max_tfd_queue_size), the command SLOT wraps at 32 (n_window).
    for i in 0..300u32 {
        let p = [0u8, 0, 12, 0, 0, 0, 0x00, 0x08]; // NVM read of section 12 (PHY_SKU), offset 0
        let r = d.t.send_cmd_in_rfkill(0, 0x88, 0, &[Chunk { data: &p, nocopy: false }]).unwrap_or_else(|e| panic!("command {i}: {e:?}"));
        assert_eq!(u16::from_le_bytes([r.payload[6], r.payload[7]]), 0, "status OK");
        assert_eq!(r.payload.len(), 8 + 0x20);
    }
    let w = sim.w();
    assert_eq!(w.violations, Vec::<String>::new());
    assert_eq!(w.cmds.len(), 17 + 300);
    let last = w.cmds.last().unwrap();
    assert_eq!(last.seq & 0xff, ((17 + 300 - 1) % 256) as u16);
}

#[test]
fn a_nocopy_chunk_rides_in_its_own_tb() {
    let (sim, mut d, _) = up();
    use iwlwifi_pcie_ring_core::tx::Chunk;
    // A wide (grouped) command: header 8 bytes, chunk0 copied (6 bytes), chunk1 NOCOPY (600 bytes).
    let c0 = [1u8, 2, 3, 4, 5, 6];
    let c1: Vec<u8> = (0..600u32).map(|i| i as u8).collect();
    sim.w().fw.extra = Some(Box::new(|_w, c| {
        assert_eq!((c.group, c.cmd), (5, 0x21));
        assert_eq!(c.payload.len(), 6 + 600);
        assert_eq!(&c.payload[..6], &[1, 2, 3, 4, 5, 6]);
        assert!(c.payload[6..].iter().enumerate().all(|(i, b)| *b == i as u8));
        // 8-byte wide header + 6 copied = 14 bytes in the slot (one TB, <= 20); then the NOCOPY chunk: its first
        // 6 bytes complete the 20-byte first TB? No: copy_size is 14 < 20, so the first 6 bytes of chunk1 are
        // copied (tx.c:1390-1397) into the slot and the TB0 is 20 bytes; the NOCOPY TB carries the other 594.
        assert_eq!(c.tbs.iter().map(|t| t.1).collect::<Vec<_>>(), [20, 594]);
        Some(vec![])
    }));
    sim.w().fw.swallow_cmd = None;
    let r = d.t.send_cmd(5, 0x21, 2, &[Chunk { data: &c0, nocopy: false }, Chunk { data: &c1, nocopy: true }]);
    // the extra handler answered with no reply packets at all: the wait times out, which is the proof the TFD was parsed.
    assert!(matches!(r, Err(TransportError::Timeout { .. })), "{r:?}");
    no_violations(&sim);
}

#[test]
fn two_notifications_in_one_receive_buffer_are_both_delivered_in_order() {
    let (sim, mut d, _) = up();
    sim.w().schedule(
        100,
        sim::Ev::Custom(Box::new(|w| {
            sim::fwmodel::post_many(w, &[(0, 0x77, 0x8001, vec![1, 2, 3]), (3, 0x08, 0x8002, vec![9; 100]), (0, 0x78, 0x8003, vec![])]);
        })),
    );
    d.t.pump(10).unwrap();
    let got: Vec<(u8, u8, usize)> = d.t.inbox.iter().map(|r| (r.group, r.cmd, r.payload.len())).collect();
    assert_eq!(got, [(0, 0x77, 3), (3, 0x08, 100), (0, 0x78, 0)]);
    no_violations(&sim);
}

#[test]
fn a_long_burst_of_notifications_never_runs_the_pool_dry() {
    let (sim, mut d, _) = up();
    for i in 0..600u32 {
        sim.w().schedule(50 + 10 * i as u64, sim::Ev::Post { group: 0, cmd: 0x42, seq: 0x8000 | (i as u16 & 0x7fff), payload: vec![(i & 0xff) as u8; 40] });
    }
    let mut seen = 0;
    while seen < 600 {
        d.t.pump(50).unwrap();
        seen += d.t.inbox.drain(..).count();
    }
    let w = sim.w();
    assert_eq!(w.rx_overruns, 0);
    assert_eq!(w.violations, Vec::<String>::new());
    let trg: Vec<u32> = w.writes.iter().filter(|x| x.1 == "w32" && x.2 == 0x1c80).map(|x| x.3).collect();
    assert!(trg.iter().all(|v| v % 8 == 0), "the device write index moves in eights: {trg:?}");
}

#[test]
fn a_file_the_build_did_not_bake_is_refused_by_name_before_the_card_is_touched() {
    let b = build(&Opts::default());
    let sim = world(&b);
    let mut wrong = sha256(&b.bytes);
    wrong[0] ^= 1;
    let e = Driver::new(sim.host(), b.bytes.clone(), &wrong).err().unwrap();
    assert!(matches!(e, DriverError::Fw(FwError::DigestMismatch { .. })));
    assert_eq!(e.name(), "IWL-FW-DIGEST");
    assert!(sim.w().writes.is_empty());
}

#[test]
fn files_this_driver_cannot_use_are_refused_by_name() {
    let refuse = |o: Opts| -> FwError {
        let b = build(&o);
        match Driver::new(Sim::new().host(), b.bytes.clone(), &sha256(&b.bytes)).err().unwrap() {
            DriverError::Fw(e) => e,
            e => panic!("{e:?}"),
        }
    };
    assert_eq!(refuse(Opts { phy_sku: None, ..Opts::default() }), FwError::NoPhySku);
    assert_eq!(refuse(Opts { mq_rx: false, ..Opts::default() }), FwError::NoMultiQueueRx);
    assert_eq!(refuse(Opts { init_sections: false, ..Opts::default() }), FwError::NoSections { image: "INIT" });
    assert_eq!(refuse(Opts { regular_sections: false, ..Opts::default() }), FwError::NoSections { image: "REGULAR" });
    // not a TLV file at all
    let junk = vec![7u8; 4096];
    let e = Driver::new(Sim::new().host(), junk.clone(), &sha256(&junk)).err().unwrap();
    assert!(matches!(e, DriverError::Fw(FwError::Params(_))));
}

#[test]
fn a_phy_configuration_command_of_version_three_is_refused_not_sent_short() {
    let b = build(&Opts { phy_cfg_cmd_ver: 3, ..Opts::default() });
    let sim = world(&b);
    let mut d = driver(&sim, &b);
    assert_eq!(d.bring_up().unwrap_err(), DriverError::Init(Failure::PhyCfgV3Unsupported));
    assert!(sim.w().fw.phy_cfg_seen.is_empty());
}

/// The REAL file, when pointed at. The simulated card's image models come from a TLV walk written in the
/// test (not the driver's), so the driver's section walk, the upload and the card's check are independent.
#[test]
fn the_real_9260_46_file_comes_up_on_the_simulated_card() {
    let Ok(path) = std::env::var("CORAL_IWL_UCODE") else {
        eprintln!("the_real_9260_46_file_comes_up_on_the_simulated_card: SKIPPED (set CORAL_IWL_UCODE=<decompressed -46 blob>)");
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes.len(), 1_490_508);
    let digest = sha256(&bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, "05f4c12514fddfd56fea660b3d2eb6b6e6f4a87a324d5d40464050312ddca911", "the digest firmware/intel/WHENCE bakes");
    let b = from_file(bytes);
    let sim = world(&b);
    let mut d = driver(&sim, &b);
    let rep = d.bring_up().expect("bring_up on the real file");
    let w = sim.w();
    assert_eq!(w.violations, Vec::<String>::new());
    // The real INIT image: 9 sections, no separator (measured with iwlwifi-fw-image-core's walk).
    assert_eq!(b.init.secs.len(), 9);
    assert_eq!(rep.fw_bytes, 712 + 96704 + 154804 + 113984 + 28928 + 696 + 65536 + 32768 + 56140);
    assert_eq!(w.load_status_history.last(), Some(&0xffff_ffff));
    assert_eq!(w.fw.boots, [FwState::Init]);
    assert_eq!(rep.alive.lmac.ucode_major, 46);
}

/// The 2026-10-16 Atlas silicon failure (teal-maple-glade-ring): `[IWL-FW-LOADED FAIL IWL-NIC-ACCESS
/// AccessTimeout { gp_cntrl: 134479880 }]` = 0x08040008 = HW_RF_KILL_SW | bit 18 | MAC_ACCESS_REQ, with INIT_DONE and
/// MAC_CLOCK_READY CLEAR. The card gives no clock to an access request made before INIT_DONE (iwl-csr.h:277-278), so a
/// `grab` before `apm_init` fails and latches STATUS_TRANS_DEAD. Linux reads HPM_DEBUG in `clear_persistence_bit` with
/// `iwl_read_umac_prph_no_grab` (trans.c:1749) precisely because the MAC is not up yet; this driver grabbed.
#[test]
fn no_nic_access_is_requested_before_init_done_and_the_silicon_gp_cntrl_never_appears() {
    use iwlwifi_pcie_nic_core::bus::{NicFault, NicIo};
    let b = build(&Opts::default());
    let sim = world(&b);
    let mut d = driver(&sim, &b);
    let r = d.bring_up();
    let fault = d.t.nic.fault();
    assert_ne!(fault, Some(NicFault::AccessTimeout { gp_cntrl: 0x0804_0008 }), "the silicon failure: a grab before INIT_DONE");
    assert!(r.is_ok(), "bring_up: {r:?}");
    assert_eq!(fault, None);
    no_violations(&sim);
}

/// The wrapper narrows BAR0 to `BAR0_WRITE_RANGES`; a refused write would silently leave the card in a half-programmed state.
/// Every BAR0 write the whole bring-up makes must fall inside the allow-list (`devices/iwlwifi/src/rt.rs`, which the
/// driver notes called unverified: this is the verification, against the sim's record of every write).
#[test]
fn every_bar0_write_of_a_full_bring_up_is_inside_the_write_allow_list() {
    use iwlwifi_device::rt::BAR0_WRITE_RANGES;
    let (sim, _d, _) = up();
    let outside: Vec<_> = sim
        .w()
        .writes
        .iter()
        .filter(|(_, kind, off, _)| *kind != "prph" && !BAR0_WRITE_RANGES.iter().any(|(o, l)| *off >= *o && *off < *o + *l))
        .cloned()
        .collect();
    assert_eq!(outside, vec![], "BAR0 writes the allow-list would refuse");
}
