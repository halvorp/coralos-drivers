// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the LMAC / UMAC error tables, the assert-name lookup and the table-base choice.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/fw/dump.c`, `fw/img.c`, `fw/img.h` and
//! `iwl-trans.h`. Copyright (C) 2012-2014, 2018-2025 Intel Corporation, and the Linux iwlwifi authors.

use iwlwifi_fw_alive_core::error_table::*;

#[test]
fn every_lmac_field_index_is_its_position_in_dump_c_23_71() {
    let got = [
        LMAC_VALID, LMAC_ERROR_ID, LMAC_TRM_HW_STATUS0, LMAC_TRM_HW_STATUS1, LMAC_BLINK2, LMAC_ILINK1, LMAC_ILINK2,
        LMAC_DATA1, LMAC_DATA2, LMAC_DATA3, LMAC_BCON_TIME, LMAC_TSF_LOW, LMAC_TSF_HI, LMAC_GP1, LMAC_GP2,
        LMAC_FW_REV_TYPE, LMAC_MAJOR, LMAC_MINOR, LMAC_HW_VER, LMAC_BRD_VER, LMAC_LOG_PC, LMAC_FRAME_PTR,
        LMAC_STACK_PTR, LMAC_HCMD, LMAC_ISR0, LMAC_ISR1, LMAC_ISR2, LMAC_ISR3, LMAC_ISR4, LMAC_LAST_CMD_ID,
        LMAC_WAIT_EVENT, LMAC_L2P_CONTROL, LMAC_L2P_DURATION, LMAC_L2P_MHVALID, LMAC_L2P_ADDR_MATCH,
        LMAC_LMPM_PMG_SEL, LMAC_U_TIMESTAMP, LMAC_FLOW_HANDLER,
    ];
    // dump.c declares them in this order, one u32 each: index i is field i.
    let want: Vec<usize> = (0..38).collect();
    assert_eq!(got.to_vec(), want);
    assert_eq!(LMAC_ERROR_TABLE_DWORDS, 38);
    assert_eq!(LMAC_VALID, 0); // zero-valued: also pinned by is_valid below
}

#[test]
fn every_umac_field_index_is_its_position_in_dump_c_78_94() {
    let got = [
        UMAC_VALID, UMAC_ERROR_ID, UMAC_BLINK1, UMAC_BLINK2, UMAC_ILINK1, UMAC_ILINK2, UMAC_DATA1, UMAC_DATA2,
        UMAC_DATA3, UMAC_MAJOR, UMAC_MINOR, UMAC_FRAME_POINTER, UMAC_STACK_POINTER, UMAC_CMD_HEADER, UMAC_NIC_ISR_PREF,
    ];
    assert_eq!(got.to_vec(), (0..15).collect::<Vec<usize>>());
    assert_eq!(UMAC_ERROR_TABLE_DWORDS, 15);
}

#[test]
fn the_assert_table_is_img_c_55_81() {
    assert_eq!(FW_SYSASSERT_CPU_MASK, 0xf0000000); // img.h:282
    assert_eq!(FW_SYSASSERT_PNVM_MISSING, 0x0010070d); // img.h:283
    let want: [(&str, u32); 21] = [
        ("NMI_INTERRUPT_WDG", 0x34),
        ("SYSASSERT", 0x35),
        ("UCODE_VERSION_MISMATCH", 0x37),
        ("BAD_COMMAND", 0x38),
        ("BAD_COMMAND", 0x39),
        ("NMI_INTERRUPT_DATA_ACTION_PT", 0x3C),
        ("FATAL_ERROR", 0x3D),
        ("NMI_TRM_HW_ERR", 0x46),
        ("NMI_INTERRUPT_TRM", 0x4C),
        ("NMI_INTERRUPT_BREAK_POINT", 0x54),
        ("NMI_INTERRUPT_WDG_RXF_FULL", 0x5C),
        ("NMI_INTERRUPT_WDG_NO_RBD_RXF_FULL", 0x64),
        ("NMI_INTERRUPT_HOST", 0x66),
        ("NMI_INTERRUPT_LMAC_FATAL", 0x70),
        ("NMI_INTERRUPT_UMAC_FATAL", 0x71),
        ("NMI_INTERRUPT_OTHER_LMAC_FATAL", 0x73),
        ("NMI_INTERRUPT_ACTION_PT", 0x7C),
        ("NMI_INTERRUPT_UNKNOWN", 0x84),
        ("NMI_INTERRUPT_INST_ACTION_PT", 0x86),
        ("NMI_INTERRUPT_PREG", 0x88),
        ("PNVM_MISSING", 0x0010070d),
    ];
    assert_eq!(ADVANCED_LOOKUP, want);
    // Every entry looks up to its own name, with any CPU nibble. // img.c:88-90
    for (name, num) in want {
        assert_eq!(lookup_assert_desc(num), name);
        assert_eq!(lookup_assert_desc(num | 0x2000_0000), name);
    }
    assert_eq!(ADVANCED_SYSASSERT, "ADVANCED_SYSASSERT");
    assert_eq!(lookup_assert_desc(0x36), "ADVANCED_SYSASSERT"); // img.c:92-93
    assert_eq!(lookup_assert_desc(0), "ADVANCED_SYSASSERT"); // the sentinel's 0 is never matched
}

#[test]
fn hw_error_values_and_the_valid_rule() {
    assert!(is_hw_error_value(0xa5a5a5a0) && is_hw_error_value(0x5a5a5a5f)); // iwl-trans.h:1171
    assert!(!is_hw_error_value(0xa5a5a5b0) && !is_hw_error_value(0));
    assert_eq!((ERROR_START_OFFSET, ERROR_ELEM_SIZE), (4, 28)); // dump.c:96-97
    let mut d = [0u32; 38];
    assert!(!LmacErrorTable::from_dwords(&d).unwrap().is_valid());
    d[0] = 1;
    assert!(LmacErrorTable::from_dwords(&d).unwrap().is_valid());
    d[0] = 0x8000_0000; // size_t arithmetic in Linux: never wraps to "empty"
    assert!(LmacErrorTable::from_dwords(&d).unwrap().is_valid());
    let mut u = [0u32; 15];
    assert!(!UmacErrorTable::from_dwords(&u).unwrap().is_valid());
    u[0] = 2;
    assert!(UmacErrorTable::from_dwords(&u).unwrap().is_valid());
    assert_eq!(LmacErrorTable::from_dwords(&d[..37]), Err(TableError::Short { got: 37, need: 38 }));
    assert_eq!(UmacErrorTable::from_dwords(&u[..14]), Err(TableError::Short { got: 14, need: 15 }));
}

#[test]
fn the_lmac_base_is_the_alive_pointer_else_the_images_tlv_pointer_else_refused() {
    // dump.c:150-166
    assert_eq!(lmac_table_base(0x0081_3AF4, true, 0x111, 0x222), Ok(0x0081_3AF4));
    assert_eq!(lmac_table_base(0, true, 0x111, 0x222), Ok(0x111));
    assert_eq!(lmac_table_base(0, false, 0x111, 0x222), Ok(0x222));
    assert_eq!(lmac_table_base(0, true, 0, 0x222), Err(TableError::NoLmacPointer { init_image: true }));
    assert_eq!(lmac_table_base(0, false, 0x111, 0), Err(TableError::NoLmacPointer { init_image: false }));
}
