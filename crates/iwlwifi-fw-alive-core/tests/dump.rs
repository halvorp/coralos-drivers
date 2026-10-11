// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the one-line markers.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/fw/dump.c`, `mvm/fw.c` and `iwl-prph.h`.
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation, and the Linux iwlwifi authors.

use iwlwifi_fw_alive_core::dump::*;
use iwlwifi_fw_alive_core::error_table::*;

#[test]
fn alive_timeout_registers_are_iwl_prph_h_in_linux_print_order() {
    assert_eq!(MVM_UCODE_ALIVE_TIMEOUT_MS, 2000); // mvm/fw.c:28
    assert_eq!(
        ALIVE_TIMEOUT_PRPH_REGS,
        [
            ("sb_cpu1", 0xA01E30),       // iwl-prph.h:400
            ("sb_cpu2", 0xA01E34),       // iwl-prph.h:401
            ("pd_notif", 0xa03044),      // iwl-prph.h:410
            ("hpm_sec_state", 0xa03404), // iwl-prph.h:411
            ("otp_cfg7_addr", 0xa03338), // iwl-prph.h:412
            ("otp_cfg7_data", 0xa0333c), // iwl-prph.h:413
            ("umac_pc", 0xa05c18),       // iwl-prph.h:405
            ("lmac_pc", 0xa05c1c),       // iwl-prph.h:406
        ]
    );
    let m = AliveTimeoutMarker { image: Image::Init, values: [1, 2, 3, 4, 5, 6, 0xC0DE, 0xBEEF] };
    assert_eq!(
        m.to_string(),
        "[IWL-ALIVE-TIMEOUT img=INIT sb_cpu1=0x00000001 sb_cpu2=0x00000002 pd_notif=0x00000003 \
         hpm_sec_state=0x00000004 otp_cfg7_addr=0x00000005 otp_cfg7_data=0x00000006 umac_pc=0x0000c0de \
         lmac_pc=0x0000beef]"
    );
}

fn lmac() -> LmacErrorTable {
    // Every field = 0x100 + its dump.c position, then the decisive ones set by hand (literal indices).
    let mut d = [0u32; 38];
    for (i, w) in d.iter_mut().enumerate() {
        *w = 0x100 + i as u32;
    }
    d[0] = 1; // valid
    d[1] = 0x0000_0071; // error_id: NMI_INTERRUPT_UMAC_FATAL
    d[16] = 46; // major
    d[17] = 0x7e3e_4b69; // minor
    LmacErrorTable::from_dwords(&d).unwrap()
}

fn umac() -> UmacErrorTable {
    let mut d = [0u32; 15];
    for (i, w) in d.iter_mut().enumerate() {
        *w = 0x200 + i as u32;
    }
    d[0] = 1;
    d[1] = 0x2000_0035; // SYSASSERT on CPU 2
    UmacErrorTable::from_dwords(&d).unwrap()
}

#[test]
fn the_fw_error_marker_reads_each_field_from_its_dump_c_position() {
    let (l, u) = (lmac(), umac());
    let m = FwErrorMarker { image: Image::Regular, lmac: &l, umac: Some(&u) };
    assert_eq!(
        m.to_string(),
        "[IWL-FW-ERROR img=RT valid=1 id=0x00000071 desc=NMI_INTERRUPT_UMAC_FATAL pc=0x00000114 \
         blink1=0x00000202 blink2=0x00000104 ilink1=0x00000105 ilink2=0x00000106 data1=0x00000107 \
         data2=0x00000108 data3=0x00000109 hcmd=0x00000117 last_cmd=0x0000011d fw=46.7e3e4b69 \
         umac_id=0x20000035 umac_desc=SYSASSERT umac_data1=0x00000206 umac_cmd=0x0000020d]"
    );
    let n = FwErrorMarker { image: Image::Init, lmac: &l, umac: None };
    let s = n.to_string();
    assert!(s.starts_with("[IWL-FW-ERROR img=INIT valid=1 id=0x00000071"));
    assert!(s.contains(" blink1=none ") && s.ends_with(" umac=none]"));
}

#[test]
fn the_hw_error_marker_names_value_and_base() {
    let m = HwErrorValueMarker { image: Image::Regular, base: 0x0081_3AF4, value: 0xa5a5_a5a2 };
    assert_eq!(m.to_string(), "[IWL-FW-ERROR img=RT hw_error_value=0xa5a5a5a2 base=0x00813af4]");
}
