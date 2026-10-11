// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for start_hw / stop_device and their pieces, including the INIT → stop → REGULAR cycle the
//! 9260 runs.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/trans.c` and
//! `pcie/gen1_2/internal.h`. Copyright (C) 2007-2015, 2018-2024 Intel Corporation, and the Linux
//! iwlwifi authors. Register numbers are LITERALS.

mod common;
use common::{Op, Sim};
use iwlwifi_pcie_trans_core::apm::ApmError;
use iwlwifi_pcie_trans_core::hw::*;

const UREG_CHICK_MSI: Op = Op::P(0xA05C00, 0x0100_0000); // iwl-prph.h:449-450

#[test]
fn start_hw_is_trans_c_1786_1832_in_order() {
    let mut s = Sim::new();
    let mut st = TransState::default();
    assert_eq!(start_hw(&mut s, &mut st), Ok(false));
    assert_eq!(
        s.writes(),
        vec![
            Op::W(0x000, 0x0040_0000), Op::W(0x088, 0x20), // prepare_card_hw, :1793
            // clear_persistence_bit: HPM_DEBUG reads 0 → no write, :1749-1750
            Op::W(0x020, 0x80),                            // sw_reset, :1803 → trans.c:86-87
            Op::W(0x000, 0x0040_0000), Op::W(0x088, 0x20), // … and the retake, trans.c:91-92
            Op::W(0x100, 0x0080_0000), Op::W(0x240, 0xFFFF_0000), Op::W(0x000, 0x0048_0000), Op::W(0x03C, 0x2),
            Op::W(0x024, 0x0800_0004),                     // apm_init + activate_nic, :1814
            UREG_CHICK_MSI,                                // conf_msix_hw MSI arm, :1818 → :1145-1146
            Op::W(0x00C, 0x80), Op::W(0x024, 0x0C00_0005), // enable_rfkill_int, :1821
        ]
    );
    assert_eq!(st, TransState { device_enabled: true, is_down: false, use_ict: false, inta_mask: 0x80 });
    // The switch in KILL (HW_RF_KILL_SW clear) is reported, not refused: INIT runs in rfkill.
    let mut killed = Sim::new();
    killed.csr.insert(0x024, 0);
    assert_eq!(start_hw(&mut killed, &mut TransState::default()), Ok(true));
}

#[test]
fn start_hw_refuses_by_name_and_leaves_the_device_disabled() {
    let mut owned = Sim::new();
    owned.own_after = u32::MAX;
    let mut st = TransState::default();
    assert_eq!(start_hw(&mut owned, &mut st), Err(HwError::Apm(ApmError::CardNotReady { attempts: 7501 })));
    let mut asleep = Sim::new();
    asleep.clock_follows_init_done = false;
    assert!(matches!(start_hw(&mut asleep, &mut st), Err(HwError::Apm(ApmError::MacClockNotReady { .. }))));
    assert!(!asleep.ops.contains(&UREG_CHICK_MSI), "no MSI select on a device that never woke");
    assert_eq!(st, TransState::default());
}

#[test]
fn the_persistence_bit_is_cleared_skipped_on_a_bus_error_and_refused_when_write_protected() {
    let mut s = Sim::new();
    s.prph.insert(0xA03440, 0x0000_1003); // HPM_DEBUG with PERSISTENCE_BIT (bit 12)
    assert_eq!(clear_persistence_bit(&mut s), Ok(true));
    assert_eq!(s.ops, vec![Op::P(0xA03440, 0x0000_0003)]); // trans.c:1758-1759
    let mut bus = Sim::new();
    bus.prph.insert(0xA03440, 0xa5a5_a5a2); // iwl_trans_is_hw_error_value, trans.c:1750
    assert_eq!(clear_persistence_bit(&mut bus), Ok(false));
    assert!(bus.ops.is_empty());
    let mut locked = Sim::new();
    locked.prph.insert(0xA03440, 0x1000);
    locked.prph.insert(0xA04CE0, 0x1000); // PREG_PRPH_WPROT_9000 & PREG_WFPM_ACCESS
    assert_eq!(clear_persistence_bit(&mut locked), Err(HwError::PersistenceLocked { hpm: 0x1000, wprot: 0x1000 }));
    assert!(locked.ops.is_empty());
    let mut st = TransState::default();
    assert!(matches!(start_hw(&mut locked, &mut st), Err(HwError::PersistenceLocked { .. })));
    assert!(!locked.ops.contains(&Op::W(0x020, 0x80)), "no SW reset after the refusal");
}

#[test]
fn stop_device_is_trans_c_1184_1267_and_does_not_reselect_msi() {
    let mut s = Sim::new();
    let mut st = TransState::default();
    start_hw(&mut s, &mut st).unwrap();
    st.use_ict = true;
    s.ops.clear();
    let r = stop_device(&mut s, &mut st);
    assert_eq!(r, StopReport { already_down: false, tx_stop: Some(Ok(())), rx_stop: Some(Ok(())), master_stopped: true, retake: Ok(()) });
    let gp = 0x0C00_0005; // radio, L1A wake, INIT_DONE, clock
    assert_eq!(
        s.writes(),
        vec![
            Op::W(0x00C, 0), Op::W(0x008, 0xFFFF_FFFF), Op::W(0x010, 0xFFFF_FFFF), // :1196
            Op::P(0xa02c10, 0),                                                    // tx_stop, :1214
            Op::W(0x1D00, 0), Op::W(0x1D20, 0), Op::W(0x1D40, 0), Op::W(0x1D60, 0),
            Op::W(0x1D80, 0), Op::W(0x1DA0, 0), Op::W(0x1DC0, 0), Op::W(0x1DE0, 0),
            Op::P(0xA09820, 0),                                                    // rx_stop, :1215
            Op::W(0x024, gp & !0x8),                                               // MAC_ACCESS_REQ off, :1230-1231
            Op::W(0x020, 0x200), Op::W(0x020, 0x380), Op::W(0x024, 0x0C00_0001),   // apm_stop, :1234
            Op::W(0x020, 0x380), Op::W(0x000, 0x0048_0000), Op::W(0x088, 0x20),    // sw_reset + retake, :1237
            // :1246 conf_msix_hw: device disabled → NO UREG_CHICK write
            Op::W(0x00C, 0), Op::W(0x008, 0xFFFF_FFFF), Op::W(0x010, 0xFFFF_FFFF), // :1255
            Op::W(0x00C, 0x80), Op::W(0x024, 0x0C00_0000),                         // :1266
        ]
    );
    assert_eq!(st, TransState { device_enabled: false, is_down: true, use_ict: false, inta_mask: 0x80 });
    // A second stop touches nothing. // :1190-1191
    s.ops.clear();
    assert!(stop_device(&mut s, &mut st).already_down);
    assert!(s.ops.is_empty());
}

#[test]
fn the_9260_cycle_init_stop_regular_selects_msi_exactly_twice() {
    // mvm/fw.c:1509-1540 iwl_mvm_load_rt_fw: start_hw (INIT) … stop_device … start_hw (REGULAR).
    let mut s = Sim::new();
    let mut st = TransState::default();
    start_hw(&mut s, &mut st).unwrap();
    stop_device(&mut s, &mut st);
    start_hw(&mut s, &mut st).unwrap();
    assert_eq!(s.ops.iter().filter(|o| **o == UREG_CHICK_MSI).count(), 2);
    assert!(st.device_enabled && !st.is_down);
}

#[test]
fn a_stop_on_a_disabled_device_skips_the_dma_stops_but_still_resets() {
    let mut s = Sim::new();
    let mut st = TransState { device_enabled: false, is_down: false, use_ict: true, inta_mask: 0 };
    let r = stop_device(&mut s, &mut st);
    assert_eq!((r.tx_stop, r.rx_stop), (None, None));
    assert!(!s.ops.contains(&Op::P(0xA09820, 0)));
    assert!(s.ops.contains(&Op::W(0x020, 0x200)));
    let mut stuck = Sim::new();
    stuck.master_follows_stop = false;
    stuck.tx_status = 0;
    stuck.rfh_status = 0;
    let mut st = TransState { device_enabled: true, is_down: false, use_ict: false, inta_mask: 0 };
    let r = stop_device(&mut stuck, &mut st);
    assert_eq!((r.tx_stop, r.rx_stop, r.master_stopped), (Some(Err(0)), Some(Err(0)), false));
}

#[test]
fn interrupt_helpers_are_the_msi_arms() {
    let mut s = Sim::new();
    let mut st = TransState { inta_mask: 0xFFFF, ..TransState::default() };
    disable_interrupts(&mut s, &mut st);
    enable_rfkill_int(&mut s, &mut st);
    assert_eq!(
        s.ops,
        vec![
            Op::W(0x00C, 0), Op::W(0x008, 0xFFFF_FFFF), Op::W(0x010, 0xFFFF_FFFF), // internal.h:834-839
            Op::W(0x00C, 0x80), Op::W(0x024, 0x0C00_0000),                         // internal.h:1025-1042
        ]
    );
    assert_eq!(st.inta_mask, 0x80);
    assert!(!is_rfkill_set(&mut s));
    s.csr.insert(0x024, 0x0400_0000);
    assert!(is_rfkill_set(&mut s)); // internal.h:1057-1058
    assert!(!conf_msi_hw(&mut s, &TransState::default()));
    assert!(conf_msi_hw(&mut s, &TransState { device_enabled: true, ..TransState::default() }));
}
