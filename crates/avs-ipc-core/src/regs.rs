// SPDX-License-Identifier: GPL-2.0-only
//! The Skylake-family HIPC registers, the mailbox windows and the timeouts, each with the Linux FILE and LINE it was read from.
//! Offsets are relative to BAR4 (`adev->dsp_ba`).

pub const AVS_ADSP_REG_ADSPIC: u32 = 0x08; // registers.h:26
pub const AVS_ADSP_REG_ADSPIS: u32 = 0x0C; // registers.h:27
pub const AVS_ADSP_ADSPIC_IPC: u32 = 0x1; // registers.h:29
pub const AVS_ADSP_ADSPIC_CLDMA: u32 = 1 << 1; // registers.h:30
pub const AVS_ADSP_ADSPIS_IPC: u32 = 0x1; // registers.h:31
pub const AVS_ADSP_ADSPIS_CLDMA: u32 = 1 << 1; // registers.h:32
pub const AVS_ADSP_HIPCCTL_BUSY: u32 = 0x1; // registers.h:42
pub const AVS_ADSP_HIPCCTL_DONE: u32 = 1 << 1; // registers.h:43

pub const SKL_ADSP_REG_HIPCT: u32 = 0x40; // registers.h:47
pub const SKL_ADSP_REG_HIPCTE: u32 = 0x44; // registers.h:48
pub const SKL_ADSP_REG_HIPCI: u32 = 0x48; // registers.h:49
pub const SKL_ADSP_REG_HIPCIE: u32 = 0x4C; // registers.h:50
pub const SKL_ADSP_REG_HIPCCTL: u32 = 0x50; // registers.h:51
pub const SKL_ADSP_HIPCI_BUSY: u32 = 1 << 31; // registers.h:53
pub const SKL_ADSP_HIPCIE_DONE: u32 = 1 << 30; // registers.h:54
pub const SKL_ADSP_HIPCT_BUSY: u32 = 1 << 31; // registers.h:55

/// `skl_hipc_spec` (core.c:767-776): the generic names of the Skylake row.
pub const SKL_REQ_OFFSET: u32 = SKL_ADSP_REG_HIPCI; // core.c:768
pub const SKL_REQ_EXT_OFFSET: u32 = SKL_ADSP_REG_HIPCIE; // core.c:769
pub const SKL_REQ_BUSY_MASK: u32 = SKL_ADSP_HIPCI_BUSY; // core.c:770
pub const SKL_ACK_OFFSET: u32 = SKL_ADSP_REG_HIPCIE; // core.c:771
pub const SKL_ACK_DONE_MASK: u32 = SKL_ADSP_HIPCIE_DONE; // core.c:772
pub const SKL_RSP_OFFSET: u32 = SKL_ADSP_REG_HIPCT; // core.c:773
pub const SKL_RSP_BUSY_MASK: u32 = SKL_ADSP_HIPCT_BUSY; // core.c:774
pub const SKL_CTL_OFFSET: u32 = SKL_ADSP_REG_HIPCCTL; // core.c:775

pub const SKL_ADSP_SRAM_BASE_OFFSET: u32 = 0x8000; // registers.h:107
pub const SKL_ADSP_SRAM_WINDOW_SIZE: u32 = 0x2000; // registers.h:108
/// `AVS_FW_REG_STATUS` / `AVS_FW_REG_ERROR` (registers.h:116-117) with `sts_offset` = the SRAM base (core.c:776).
pub const AVS_FW_REG_STATUS: u32 = SKL_ADSP_SRAM_BASE_OFFSET; // registers.h:116
pub const AVS_FW_REG_ERROR: u32 = SKL_ADSP_SRAM_BASE_OFFSET + 4; // registers.h:117
pub const AVS_WINDOW_CHUNK_SIZE: u32 = 4096; // registers.h:119
pub const AVS_FW_REGS_SIZE: u32 = AVS_WINDOW_CHUNK_SIZE; // registers.h:120
pub const AVS_UPLINK_WINDOW: u32 = 0; // registers.h:123
pub const AVS_DOWNLINK_WINDOW: u32 = 1; // registers.h:125

/// `avs_sram_offset(adev, window)` (registers.h:129-131) for the Skylake row.
pub const fn sram_offset(window: u32) -> u32 {
    SKL_ADSP_SRAM_BASE_OFFSET + SKL_ADSP_SRAM_WINDOW_SIZE * window
}
/// `avs_uplink_addr` (registers.h:136-137): the DSP -> host payload, after the 4 KiB of firmware registers.
pub const UPLINK_OFFSET: u32 = sram_offset(AVS_UPLINK_WINDOW) + AVS_FW_REGS_SIZE; // registers.h:136
/// `avs_downlink_addr` (registers.h:138-139): the host -> DSP payload.
pub const DOWNLINK_OFFSET: u32 = sram_offset(AVS_DOWNLINK_WINDOW); // registers.h:138

pub const AVS_MAILBOX_SIZE: usize = 4096; // messages.h:16
/// `AVS_IPC_TIMEOUT_MS` (ipc.c:18), installed as `default_timeout_ms` by `avs_ipc_init` (ipc.c:566).
pub const AVS_IPC_TIMEOUT_MS: u32 = 300; // ipc.c:18
/// `AVS_CL_TIMEOUT_MS` (messages.c:13): LOAD_LIBRARY and LOAD_MULTIPLE_MODULES.
pub const AVS_CL_TIMEOUT_MS: u32 = 5000; // messages.c:13
/// `u32 repeats_left = 128; /* to avoid infinite looping */` (ipc.c:320).
pub const AVS_IPC_REPEATS: u32 = 128; // ipc.c:320
/// The reply status that is "of no harm" (messages.h:164): the firmware does not support the request.
pub const AVS_IPC_NOT_SUPPORTED: u32 = 15; // messages.h:164
