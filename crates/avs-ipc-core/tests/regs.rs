// SPDX-License-Identifier: GPL-2.0-only
//! regs.rs against the Linux literals (file:line in comments); expected values written out by hand.
use avs_ipc_core::regs::*;

#[test]
fn hipc_registers_of_the_skylake_row() {
    assert_eq!(AVS_ADSP_REG_ADSPIC, 0x08); // registers.h:26
    assert_eq!(AVS_ADSP_REG_ADSPIS, 0x0C); // registers.h:27
    assert_eq!(AVS_ADSP_ADSPIC_IPC, 0x1); // registers.h:29
    assert_eq!(AVS_ADSP_ADSPIC_CLDMA, 0x2); // registers.h:30
    assert_eq!(AVS_ADSP_ADSPIS_IPC, 0x1); // registers.h:31
    assert_eq!(AVS_ADSP_ADSPIS_CLDMA, 0x2); // registers.h:32
    assert_eq!(AVS_ADSP_HIPCCTL_BUSY, 0x1); // registers.h:42
    assert_eq!(AVS_ADSP_HIPCCTL_DONE, 0x2); // registers.h:43
    assert_eq!(SKL_ADSP_REG_HIPCT, 0x40); // registers.h:47
    assert_eq!(SKL_ADSP_REG_HIPCTE, 0x44); // registers.h:48
    assert_eq!(SKL_ADSP_REG_HIPCI, 0x48); // registers.h:49
    assert_eq!(SKL_ADSP_REG_HIPCIE, 0x4C); // registers.h:50
    assert_eq!(SKL_ADSP_REG_HIPCCTL, 0x50); // registers.h:51
    assert_eq!(SKL_ADSP_HIPCI_BUSY, 0x8000_0000); // registers.h:53
    assert_eq!(SKL_ADSP_HIPCIE_DONE, 0x4000_0000); // registers.h:54
    assert_eq!(SKL_ADSP_HIPCT_BUSY, 0x8000_0000); // registers.h:55
}

#[test]
fn the_skylake_hipc_spec_names_the_right_registers() {
    // core.c:767-776, each generic name against the register it is set to
    assert_eq!(SKL_REQ_OFFSET, 0x48); // .req_offset = SKL_ADSP_REG_HIPCI
    assert_eq!(SKL_REQ_EXT_OFFSET, 0x4C); // .req_ext_offset = SKL_ADSP_REG_HIPCIE
    assert_eq!(SKL_REQ_BUSY_MASK, 0x8000_0000); // .req_busy_mask = SKL_ADSP_HIPCI_BUSY
    assert_eq!(SKL_ACK_OFFSET, 0x4C); // .ack_offset = SKL_ADSP_REG_HIPCIE
    assert_eq!(SKL_ACK_DONE_MASK, 0x4000_0000); // .ack_done_mask = SKL_ADSP_HIPCIE_DONE
    assert_eq!(SKL_RSP_OFFSET, 0x40); // .rsp_offset = SKL_ADSP_REG_HIPCT
    assert_eq!(SKL_RSP_BUSY_MASK, 0x8000_0000); // .rsp_busy_mask = SKL_ADSP_HIPCT_BUSY
    assert_eq!(SKL_CTL_OFFSET, 0x50); // .ctl_offset = SKL_ADSP_REG_HIPCCTL
}

#[test]
fn sram_windows() {
    assert_eq!(SKL_ADSP_SRAM_BASE_OFFSET, 0x8000); // registers.h:107
    assert_eq!(SKL_ADSP_SRAM_WINDOW_SIZE, 0x2000); // registers.h:108
    assert_eq!(AVS_FW_REG_STATUS, 0x8000); // registers.h:116
    assert_eq!(AVS_FW_REG_ERROR, 0x8004); // registers.h:117
    assert_eq!(AVS_WINDOW_CHUNK_SIZE, 0x1000); // registers.h:119
    assert_eq!(AVS_FW_REGS_SIZE, 0x1000); // registers.h:120
    assert_eq!(AVS_UPLINK_WINDOW, 0); // registers.h:123
    assert_eq!(AVS_DOWNLINK_WINDOW, 1); // registers.h:125
    assert_eq!(sram_offset(0), 0x8000);
    assert_eq!(sram_offset(1), 0xA000);
    assert_eq!(sram_offset(2), 0xC000);
    // the DSP -> host payload sits after the firmware registers of window 0; the host -> DSP payload is window 1
    assert_eq!(UPLINK_OFFSET, 0x9000);
    assert_eq!(DOWNLINK_OFFSET, 0xA000);
}

#[test]
fn mailbox_and_timeouts() {
    assert_eq!(AVS_MAILBOX_SIZE, 4096); // messages.h:16
    assert_eq!(AVS_IPC_TIMEOUT_MS, 300); // ipc.c:18
    assert_eq!(AVS_CL_TIMEOUT_MS, 5000); // messages.c:13
    assert_eq!(AVS_IPC_REPEATS, 128); // ipc.c:320
    assert_eq!(AVS_IPC_NOT_SUPPORTED, 15); // messages.h:164
}
