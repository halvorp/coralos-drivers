// SPDX-License-Identifier: GPL-2.0-only
//! M3 against the simulated card's STRICT RT firmware (tests/sim/rtfw.rs): restart on the REGULAR image, the paging
//! image, the post-ALIVE command set in `iwl_mvm_up`'s order, a passive SCAN_REQ_UMAC, and the RX_MPDU beacons and probe
//! responses it yields turned into scan results. Every test asserts the card recorded NO violation: a command the Linux
//! firmware would refuse is a violation here.
mod sim;
use iwlwifi_device::driver::{Driver, DriverError};
use iwlwifi_device::event::{Event, Security};
use iwlwifi_device::rt_up::RtWhy;
use iwlwifi_device::sha256::sha256;
use sim::blob::{build, from_file, Blob, Opts};
use sim::fwmodel::FwState;
use sim::rtfw::{ApSec, SimAp};
use sim::Sim;

fn world(b: &Blob) -> Sim {
    let sim = Sim::new();
    {
        let mut w = sim.w();
        w.fw.init = Some(b.init.clone());
        w.fw.regular = Some(b.regular.clone());
        w.fw.rt.n_scan = b.n_scan;
    }
    sim
}

fn rt_opts() -> Opts {
    Opts { rt: true, ..Opts::default() }
}

fn driver(sim: &Sim, b: &Blob) -> Driver<sim::SimHost> {
    Driver::new(sim.host(), b.bytes.clone(), &sha256(&b.bytes)).expect("the blob is a valid file")
}

fn aps() -> Vec<SimAp> {
    let mut hidden = SimAp::new("Secret", 0x07, 149, -71, ApSec::Wpa2Psk);
    hidden.hidden = true;
    vec![
        SimAp::new("CoralNet", 0x01, 6, -42, ApSec::Open),
        SimAp::new("Home", 0x02, 11, -61, ApSec::Wpa2Psk),
        SimAp::new("FiveG", 0x03, 36, -70, ApSec::Wpa2Psk),
        SimAp::new("OldWep", 0x04, 1, -80, ApSec::Wep),
        SimAp::new("Legacy", 0x05, 40, -75, ApSec::Wpa1),
        SimAp::new("Sae", 0x06, 44, -66, ApSec::Wpa2Sae),
        SimAp::new("Tkip", 0x08, 48, -77, ApSec::Wpa2Tkip),
        hidden,
        SimAp::new("Radar", 0x09, 100, -55, ApSec::Open),
        // on a channel the regulatory domain turns off (see the tests that set regdom_off): never heard
        SimAp::new("Forbidden", 0x0a, 13, -20, ApSec::Open),
    ]
}

fn up(regdom_off: &[u8]) -> (Sim, Driver<sim::SimHost>) {
    let b = build(&rt_opts());
    let sim = world(&b);
    {
        let mut w = sim.w();
        w.fw.rt.aps = aps();
        w.fw.rt.regdom_off = regdom_off.to_vec();
    }
    let mut d = driver(&sim, &b);
    d.bring_up().expect("INIT");
    (sim, d)
}

fn no_violations(sim: &Sim) {
    assert_eq!(sim.w().violations, Vec::<String>::new());
}

fn results(ev: &[Event]) -> Vec<(String, [u8; 6], i16, u8, Security)> {
    ev.iter()
        .filter_map(|e| match e {
            Event::ScanResult { bss, .. } => Some((String::from_utf8_lossy(bss.ssid.as_bytes()).into_owned(), bss.bssid, bss.rssi, bss.channel, bss.security)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_rt_image_comes_up_through_paging_and_the_post_alive_commands() {
    let (sim, mut d) = up(&[13]);
    let rep = d.start_rt().expect("start_rt");
    no_violations(&sim);
    assert_eq!(sim.w().fw.boots, [FwState::Init, FwState::Regular]);
    assert_eq!((rep.paging_blocks, rep.mcc), (1, 0x5553));
    // the 51 NVM channels: 1-11 valid (12-14 are not), 36-48 and 149-165 active, 52-64 and 100-144 radar, rest not valid; ch 13 disabled by regdom
    let want: Vec<u8> = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].into_iter().chain([36, 40, 44, 48, 52, 56, 60, 64, 100, 104, 108, 112, 116, 120, 124, 128, 132, 136, 140, 144, 149, 153, 157, 161, 165]).collect();
    assert_eq!(d.rt.scan_channels, want);
    assert_eq!(rep.scan_channels, want.len());
    // Linux's order of commands after ALIVE (mvm/fw.c:1581-1714), by group.cmd
    let cmds = sim.w().fw.rt.cmd_log.clone();
    assert_eq!(
        cmds,
        ["1.0x4f", "0.0x98", "0.0x6c", "0.0x6c", "0.0x6c", "0.0x6c", "0.0x6a", "0.0x9b", "2.0x1", "5.0x0", "0.0x1d", "0.0x18", "4.0x4", "0.0x77", "0.0xc8", "1.0xc"]
    );
    let log = sim.w().log.clone();
    let at = |needle: &str| log.iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("no marker {needle}: {log:#?}"));
    let order = ["[IWL-INIT-COMPLETE", "[IWL-RT-STOP", "[IWL-RT-HW-READY", "[IWL-FW-LOADED rt", "[IWL-ALIVE rt", "[IWL-PAGING blocks=1 last_pages=2", "[IWL-RT-CMD paging ok", "[IWL-RT-CMD tx_ant ok", "[IWL-RT-CMD phy_db ok sections=4", "[IWL-RT-CMD phy_cfg ok", "[IWL-RT-CMD bt ok", "[IWL-RT-CMD soc ok", "[IWL-RT-CMD dqa ok", "[IWL-RT-CMD aux_queue ok", "[IWL-RT-CMD aux_sta ok sta=1 queue=1", "[IWL-RT-CMD temp_ths ok", "[IWL-RT-CMD power ok", "[IWL-RT-CMD mcc ok", "[IWL-RT-CMD scan_cfg ok", "[IWL-RT-UP"];
    let pos: Vec<usize> = order.iter().map(|n| at(n)).collect();
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "markers out of order: {pos:?}");
    assert!(log[at("[IWL-RT-CMD mcc ok")].contains("mcc=US n_channels=51 valid=36"));
    assert!(!log.iter().any(|l| l.contains("[IWL-NIC-FAULT") || l.contains("FAIL")), "{log:#?}");
}

#[test]
fn a_passive_scan_reports_every_network_once_with_its_security_strongest_first() {
    let (sim, mut d) = up(&[13]);
    d.start_rt().unwrap();
    let out = d.scan(7).expect("scan");
    no_violations(&sim);
    assert!(out.complete);
    assert_eq!(out.events.last(), Some(&Event::ScanDone { scan_id: 7, complete: true }));
    let b = |last: u8| [0x02, 0xaa, 0xbb, 0xcc, 0xdd, last];
    assert_eq!(
        results(&out.events),
        vec![
            ("CoralNet".to_string(), b(1), -42, 6, Security::Open),
            ("Radar".to_string(), b(9), -55, 100, Security::Open),
            ("Home".to_string(), b(2), -61, 11, Security::Wpa2Psk),
            ("Sae".to_string(), b(6), -66, 44, Security::Unsupported),
            ("FiveG".to_string(), b(3), -70, 36, Security::Wpa2Psk),
            ("Secret".to_string(), b(7), -71, 149, Security::Wpa2Psk),
            ("Legacy".to_string(), b(5), -75, 40, Security::Unsupported),
            ("Tkip".to_string(), b(8), -77, 48, Security::Unsupported),
            ("OldWep".to_string(), b(4), -80, 1, Security::Unsupported),
        ]
    );
    let s = &out.stats;
    // 9 networks + the hidden one's probe response + 3 junk frames (bad CRC, data, truncated)
    assert_eq!((s.rx_packets, s.heard, s.bad_crc, s.not_beacon, s.malformed), (13, 10, 1, 1, 1));
    assert_eq!(s.status, 1);
    // the request the card saw: the allowed channels in NVM order, uid 0
    let scans = sim.w().fw.rt.scans.clone();
    assert_eq!(scans.len(), 1);
    assert_eq!(&scans[0][4..8], &[0, 0, 0, 0]);
    assert_eq!(scans[0][49] as usize, d.rt.scan_channels.len());
    let log = sim.w().log.clone();
    assert!(log.iter().any(|l| l.starts_with("[IWL-SCAN-REQ id=7 uid=0 passive channels=36 len=1688 ebs=1]")), "{log:#?}");
    assert!(log.iter().any(|l| l.starts_with("[IWL-SCAN-DONE id=7 complete=1 status=1 ebs=0 bss=9 frames=13 heard=10 bad_crc=1 not_beacon=1 malformed=1")), "{log:#?}");
}

#[test]
fn a_network_on_a_channel_the_regulatory_domain_disables_is_never_asked_for_or_heard() {
    let (sim, mut d) = up(&[13, 11, 100, 104]);
    d.start_rt().unwrap();
    assert!(!d.rt.scan_channels.contains(&11) && !d.rt.scan_channels.contains(&100) && !d.rt.scan_channels.contains(&104));
    let out = d.scan(1).unwrap();
    no_violations(&sim);
    let names: Vec<String> = results(&out.events).into_iter().map(|r| r.0).collect();
    assert!(!names.contains(&"Home".to_string()) && !names.contains(&"Radar".to_string()) && !names.contains(&"Forbidden".to_string()), "{names:?}");
    assert_eq!(names.len(), 7);
}

#[test]
fn a_second_scan_reuses_the_card_and_finds_the_same_networks() {
    let (sim, mut d) = up(&[13]);
    d.start_rt().unwrap();
    let a = d.scan(1).unwrap();
    let b = d.scan(2).unwrap();
    no_violations(&sim);
    assert_eq!(results(&a.events), results(&b.events));
    assert_eq!(b.events.last(), Some(&Event::ScanDone { scan_id: 2, complete: true }));
    assert_eq!(sim.w().fw.rt.scans.len(), 2);
}

#[test]
fn a_firmware_that_never_completes_is_aborted_after_the_scan_timeout_and_reports_incomplete() {
    let (sim, mut d) = up(&[13]);
    sim.w().fw.rt.never_complete = true;
    d.start_rt().unwrap();
    let out = d.scan(3).unwrap();
    assert!(!out.complete);
    assert_eq!(out.events.last(), Some(&Event::ScanDone { scan_id: 3, complete: false }));
    assert_eq!(results(&out.events).len(), 9, "the networks heard before the timeout are still reported");
    assert!((20_000..20_100).contains(&out.stats.elapsed_ms), "waited the 20 s SCAN_TIMEOUT: {} ms", out.stats.elapsed_ms);
    assert_eq!(sim.w().fw.rt.aborts, vec![0], "SCAN_ABORT_UMAC for uid 0");
    assert!(sim.w().log.iter().any(|l| l.starts_with("[IWL-SCAN FAIL id=3 no SCAN_COMPLETE_UMAC within 20000ms; aborted, 9 BSS so far]")));
    no_violations(&sim);
}

#[test]
fn a_scan_the_firmware_aborts_is_reported_incomplete_with_the_status() {
    let (sim, mut d) = up(&[13]);
    sim.w().fw.rt.complete_status = 2;
    d.start_rt().unwrap();
    let out = d.scan(4).unwrap();
    assert!(!out.complete);
    assert_eq!(out.stats.status, 2);
    assert!(sim.w().log.iter().any(|l| l.starts_with("[IWL-SCAN-DONE id=4 complete=0 status=2")));
    no_violations(&sim);
}

#[test]
fn a_scan_before_the_rt_image_is_up_is_refused_by_name() {
    let (sim, mut d) = up(&[13]);
    let r = d.scan(1);
    assert!(matches!(r, Err(DriverError::Rt { stage: "scan", why: RtWhy::NotUp })), "{r:?}");
    assert!(sim.w().log.iter().any(|l| l.starts_with("[IWL-SCAN FAIL id=1 the RT image is not up]")));
}

#[test]
fn an_aux_station_the_firmware_refuses_fails_the_stage_by_name() {
    let (sim, mut d) = up(&[13]);
    sim.w().fw.rt.add_sta_status = 2; // ADD_STA_STATIONS_OVERLOAD
    let r = d.start_rt();
    assert!(matches!(r, Err(DriverError::Rt { stage: "aux_sta", why: RtWhy::Status(2) })), "{r:?}");
    assert!(sim.w().log.iter().any(|l| l.starts_with("[IWL-RT FAIL stage=aux_sta Status(2)]")));
    assert!(!d.rt.up);
    no_violations(&sim);
}

#[test]
fn an_rt_image_that_never_says_alive_times_out_by_name() {
    let (sim, mut d) = up(&[13]);
    // the INIT image booted and said ALIVE; make the NEXT boot silent
    sim.w().schedule(1, sim::Ev::Custom(Box::new(|_w| {})));
    sim.w().fw.suppress_alive = true;
    let r = d.start_rt();
    assert!(matches!(r, Err(DriverError::Rt { stage: "alive", why: RtWhy::Alive("timeout") })), "{r:?}");
    assert!(sim.w().log.iter().any(|l| l.starts_with("[IWL-ALIVE FAIL rt timeout=2000ms")), "{:#?}", sim.w().log);
}

#[test]
fn the_dma_budget_still_fits_with_the_paging_blocks_and_the_scan_buffer() {
    let (sim, mut d) = up(&[13]);
    d.start_rt().unwrap();
    d.scan(1).unwrap();
    let pages = sim.w().dma_used / 4096;
    eprintln!("DMA-PAGES after RT + scan: {pages}");
    // WLAN_DMA_PAGES is 1024 and the 1.49 MB firmware read (364 pages) is charged to the same budget.
    assert!(pages + 364 <= 1024, "{pages} pages held");
}

/// The REAL file, when pointed at: its REGULAR image loads, the card boots it, and it is paged.
#[test]
fn the_real_9260_46_file_restarts_on_the_regular_image_and_scans() {
    let Ok(path) = std::env::var("CORAL_IWL_UCODE") else {
        eprintln!("the_real_9260_46_file_restarts_on_the_regular_image_and_scans: SKIPPED (set CORAL_IWL_UCODE=<decompressed -46 blob>)");
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let b = from_file(bytes);
    assert_eq!((b.n_scan, b.paging_size), (52, 0x3b000));
    let sim = world(&b);
    {
        let mut w = sim.w();
        w.fw.rt.aps = aps();
        w.fw.rt.regdom_off = vec![13];
        w.dma_quota_bytes = 1024 * 4096;
    }
    let mut d = driver(&sim, &b);
    d.bring_up().expect("INIT");
    let rep = d.start_rt().expect("start_rt on the real file");
    assert_eq!(rep.paging_blocks, 8);
    let out = d.scan(1).unwrap();
    assert!(out.complete);
    assert_eq!(results(&out.events).len(), 9);
    no_violations(&sim);
    assert_eq!(sim.w().fw.boots, [FwState::Init, FwState::Regular]);
    eprintln!("DMA-PAGES real file: {}", sim.w().dma_used / 4096);
}
