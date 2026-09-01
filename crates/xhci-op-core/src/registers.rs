// SPDX-License-Identifier: GPL-2.0-only
//! Operational register layout and pure word codecs, ported from Linux
//! `drivers/usb/host/xhci.c:488-:512, :545-:573`,
//! `drivers/usb/host/xhci.h:92-:205`, `drivers/usb/host/xhci-caps.h:17-:20`, and
//! `drivers/usb/host/xhci-ext-caps.h:72-:85`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

/// Operational-register offsets relative to the operational-register base.
pub mod offset {
    pub const USBCMD: u32 = 0x00; // xhci.h:105
    pub const USBSTS: u32 = 0x04; // xhci.h:106
    pub const PAGESIZE: u32 = 0x08; // xhci.h:107
    pub const DNCTRL: u32 = 0x14; // xhci.h:110
    pub const CRCR: u32 = 0x18; // xhci.h:111
    pub const DCBAAP: u32 = 0x30; // xhci.h:114
    pub const CONFIG: u32 = 0x38; // xhci.h:115
    pub const PORT_REGS: u32 = 0x400; // xhci.h:116-118
}

/// USBCMD fields.
pub mod command {
    pub const RUN: u32 = 1 << 0; // xhci-ext-caps.h:74; xhci.h:123
    pub const RESET: u32 = 1 << 1; // xhci.h:128
    pub const EVENT_INTERRUPT_ENABLE: u32 = 1 << 2; // xhci-ext-caps.h:76; xhci.h:130
    pub const HOST_SYSTEM_ERROR_INTERRUPT_ENABLE: u32 = 1 << 3; // xhci-ext-caps.h:78; xhci.h:132
    pub const LIGHT_RESET: u32 = 1 << 7; // xhci.h:135
    pub const SAVE_STATE: u32 = 1 << 8; // xhci.h:137
    pub const RESTORE_STATE: u32 = 1 << 9; // xhci.h:138
    pub const ENABLE_WRAP_EVENT: u32 = 1 << 10; // xhci-ext-caps.h:80; xhci.h:140
    pub const MFINDEX_POWER_MANAGEMENT: u32 = 1 << 11; // xhci.h:146
    pub const EXTENDED_TBC_ENABLE: u32 = 1 << 14; // xhci.h:148
    pub const INTERRUPTS: u32 = 0x40c; // xhci-ext-caps.h:82
}

/// USBSTS fields.
pub mod status {
    pub const HALTED: u32 = 1 << 0; // xhci-ext-caps.h:14; xhci.h:156
    pub const HOST_SYSTEM_ERROR: u32 = 1 << 2; // xhci.h:158
    pub const EVENT_INTERRUPT: u32 = 1 << 3; // xhci.h:160
    pub const PORT_CHANGE: u32 = 1 << 4; // xhci.h:162
    pub const SAVING_STATE: u32 = 1 << 8; // xhci.h:165
    pub const RESTORING_STATE: u32 = 1 << 9; // xhci.h:167
    pub const SAVE_RESTORE_ERROR: u32 = 1 << 10; // xhci.h:169
    pub const CONTROLLER_NOT_READY: u32 = 1 << 11; // xhci-ext-caps.h:85; xhci.h:171
    pub const HOST_CONTROLLER_ERROR: u32 = 1 << 12; // xhci.h:173
}

/// CONFIG NumSlotsEn field mask.
pub const CONFIG_MAX_SLOTS_MASK: u32 = 0xff; // xhci-caps.h:19; xhci.h:201
/// CONFIG U3 Entry Enable.
pub const CONFIG_U3_ENTRY_ENABLE: u32 = 1 << 8; // xhci.h:203
/// CONFIG Configuration Information Enable.
pub const CONFIG_INFORMATION_ENABLE: u32 = 1 << 9; // xhci.h:205
/// Linux's fixed software array size for HC device slots.
pub const MAX_HC_SLOTS: usize = 256; // xhci.h:36

/// Names of every operational register represented by [`offset`].
pub const OP_REGISTER_NAMES: [&str; 8] = [
    "USBCMD",
    "USBSTS",
    "PAGESIZE",
    "DNCTRL",
    "CRCR",
    "DCBAAP",
    "CONFIG",
    "PORT_REGS",
]; // xhci.h:92-118
/// Names of every USBCMD field Linux defines.
pub const USBCMD_FIELD_NAMES: [&str; 10] = [
    "RUN",
    "RESET",
    "EVENT_INTERRUPT_ENABLE",
    "HOST_SYSTEM_ERROR_INTERRUPT_ENABLE",
    "LIGHT_RESET",
    "SAVE_STATE",
    "RESTORE_STATE",
    "ENABLE_WRAP_EVENT",
    "MFINDEX_POWER_MANAGEMENT",
    "EXTENDED_TBC_ENABLE",
]; // xhci.h:121-148
/// Names of every USBSTS field Linux defines.
pub const USBSTS_FIELD_NAMES: [&str; 9] = [
    "HALTED",
    "HOST_SYSTEM_ERROR",
    "EVENT_INTERRUPT",
    "PORT_CHANGE",
    "SAVING_STATE",
    "RESTORING_STATE",
    "SAVE_RESTORE_ERROR",
    "CONTROLLER_NOT_READY",
    "HOST_CONTROLLER_ERROR",
]; // xhci.h:154-173

/// Disable xHCI interrupt sources and, unless USBSTS already says halted, clear Run/Stop.
/// This is the register-word form of `xhci_quiesce` (xhci.c:103-:117).
pub const fn quiesce_command(command_word: u32, status_word: u32) -> u32 {
    let mut mask = !command::INTERRUPTS;
    if status_word & status::HALTED == 0 {
        mask &= !command::RUN;
    }
    command_word & mask
}

/// Set Run/Stop while preserving every other USBCMD bit (`xhci_start`, xhci.c:155-:159).
pub const fn start_command(command_word: u32) -> u32 {
    command_word | command::RUN
}

/// Set Host Controller Reset while preserving every other USBCMD bit (`xhci_reset`,
/// xhci.c:207-:210).
pub const fn reset_command(command_word: u32) -> u32 {
    command_word | command::RESET
}

/// Set Event Interrupt Enable while preserving USBCMD (`xhci_run_finished`, xhci.c:607-:610).
pub const fn enable_event_interrupt(command_word: u32) -> u32 {
    command_word | command::EVENT_INTERRUPT_ENABLE
}

/// Program CONFIG.NumSlotsEn while preserving all other CONFIG bits (`xhci_enable_max_dev_slots`,
/// xhci.c:488-:500).
pub const fn program_config_slots(config_word: u32, max_slots: u8) -> u32 {
    (config_word & !CONFIG_MAX_SLOTS_MASK) | max_slots as u32
}

/// DCBAAP programming is the DCBAA DMA address itself (`xhci_init`, xhci.c:570-:571).
pub const fn program_dcbaap(dcbaa_dma: u64) -> u64 {
    dcbaa_dma
}

/// Decode whether hardware reports the host halted (`xhci_halt`, xhci.c:133-:136).
pub const fn is_halted(status_word: u32) -> bool {
    status_word & status::HALTED != 0
}

/// Decode whether reset is still asserted (`xhci_reset`, xhci.c:222).
pub const fn reset_in_progress(command_word: u32) -> bool {
    command_word & command::RESET != 0
}

/// Decode whether operational-register and doorbell access is still barred after reset
/// (`xhci_reset`, xhci.c:228-:235).
pub const fn controller_not_ready(status_word: u32) -> bool {
    status_word & status::CONTROLLER_NOT_READY != 0
}
