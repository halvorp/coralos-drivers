// SPDX-License-Identifier: GPL-2.0-only
//! The RTL8169 MMIO register map.
//!
//! Every offset below is transcribed from the `enum rtl_registers` in
//! `drivers/net/ethernet/realtek/r8169_main.c`, with the source line recorded beside it. The line
//! numbers are against the tree vendored at `references/linux-ref` (r8169_main.c, 5828 lines).
//!
//! Offsets are what a port must get right and what it is easiest to get quietly wrong: an off-by-one
//! here does not panic, it reads a neighbouring register. `IntrMask` at 0x3c and `IntrStatus` at
//! 0x3e are two bytes apart and both 16-bit — transposing them yields a driver that arms nothing and
//! acknowledges nothing, which presents as a NIC that never interrupts.

/// Ethernet hardware address, 6 bytes at MAC0/MAC4. (r8169_main.c:253-254)
pub const MAC0: u32 = 0;
pub const MAC4: u32 = 4;

/// Multicast filter, 8 bytes. (r8169_main.c:255)
pub const MAR0: u32 = 8;

/// Hardware statistics counter block address. (r8169_main.c:256)
pub const COUNTER_ADDR_LOW: u32 = 0x10;

/// TX descriptor ring base, split low/high. The ring is DMA memory the NIC walks itself, so these
/// take a physical address. (r8169_main.c:258-259)
pub const TX_DESC_START_ADDR_LOW: u32 = 0x20;
pub const TX_DESC_START_ADDR_HIGH: u32 = 0x24;

/// Command register: reset, and RX/TX enable. (r8169_main.c:264)
pub const CHIP_CMD: u32 = 0x37;

/// Poke the NIC to look at the TX ring again. (r8169_main.c:265)
pub const TX_POLL: u32 = 0x38;

/// Interrupt mask and status. BOTH 16-bit and only two bytes apart — see the module note.
/// (r8169_main.c:266-267)
pub const INTR_MASK: u32 = 0x3c;
pub const INTR_STATUS: u32 = 0x3e;

/// TX and RX configuration. (r8169_main.c:269, :275)
pub const TX_CONFIG: u32 = 0x40;
pub const RX_CONFIG: u32 = 0x44;

/// The register-write lock. RealTek gates writes to Config0..5 behind an unlock value written here;
/// forgetting it makes those writes silently do nothing. (r8169_main.c:287)
pub const CFG_9346: u32 = 0x50;

/// Config0..Config5. (r8169_main.c:288-295)
pub const CONFIG0: u32 = 0x51;
pub const CONFIG1: u32 = 0x52;
pub const CONFIG2: u32 = 0x53;
pub const CONFIG3: u32 = 0x54;
pub const CONFIG4: u32 = 0x55;
pub const CONFIG5: u32 = 0x56;

/// MII/PHY access register. (r8169_main.c:296)
pub const PHYAR: u32 = 0x60;

/// PHY link status. (r8169_main.c:297)
pub const PHY_STATUS: u32 = 0x6c;

// ── Register VALUES, as distinct from offsets ───────────────────────────────────────────────────

/// `ChipCmd` bits. (r8169_main.c:474-477)
pub mod chip_cmd {
    /// Stop request. (r8169_main.c:474)
    pub const STOP_REQ: u8 = 0x80;
    /// Software reset. Self-clearing: the chip drops it when the reset completes, which is why the
    /// driver POLLS it low rather than sleeping a fixed time. (r8169_main.c:475)
    pub const RESET: u8 = 0x10;
    /// Receiver enable. (r8169_main.c:476)
    pub const RX_ENB: u8 = 0x08;
    /// Transmitter enable. (r8169_main.c:477)
    pub const TX_ENB: u8 = 0x04;
}

/// `Cfg9346` values. RealTek gates writes to Config0..5 behind this register: without the unlock
/// those writes are silently DISCARDED, which is the classic RealTek bring-up bug — the config
/// appears to be written, reads back wrong, and nothing reports an error. (r8169_main.c:486-487)
pub mod cfg9346 {
    pub const LOCK: u8 = 0x00;
    pub const UNLOCK: u8 = 0xc0;
}

/// Reset poll budget, from `rtl_loop_wait_low(tp, &rtl_chipcmd_cond, 100, 100)` — 100 iterations,
/// 100 microseconds apart. (r8169_main.c:2675)
pub const RESET_POLL_INTERVAL_US: u32 = 100;
pub const RESET_POLL_MAX: u32 = 100;
