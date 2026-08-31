// SPDX-License-Identifier: GPL-2.0-only
//! Where the MAC address comes from — the four-source cascade, and how it is written back.
//!
//! Ported from Linux `drivers/net/ethernet/realtek/r8169_main.c`:
//!   * `rtl_init_mac_address` (:5559-:5583) — the cascade itself;
//!   * `rtl_read_mac_address` (:5343-:5356) — the hardware sources;
//!   * `rtl_read_mac_from_reg` (:880-:886);
//!   * `rtl_rar_set` (:2559-:2574) and `rtl_rar_exgmac_set` (:2503-:2509);
//!   * `rtl_is_8168evl_up` (:866) and `rtl_is_8125` (:861);
//!   * `is_valid_ether_addr` and friends, `include/linux/etherdevice.h`.
//!
//! Copyright (c) a lot of people. See the Linux source for the full authorship of r8169.
//!
//! THERE IS NO SERIAL EEPROM ON THIS PATH, and looking for one is how a port goes wrong here.
//! Modern r8169 parts do not use the 93cx6 bit-banged EEPROM that older Realtek drivers read; the
//! address lives behind ERI at 0xe0/0xe4, in the backup register block at 0x19e0, or in MAC0
//! itself, and which of those applies is decided by the MAC version. `r8169_main.c` contains no
//! EEPROM accessor at all.

use crate::chip::MacVersion;
use crate::eri;
use crate::init::Bus;
use crate::regs::{CHIP_CMD, MAC0, MAC4};

/// `ETH_ALEN` (include/uapi/linux/if_ether.h:32).
pub const ETH_ALEN: usize = 6;

/// A hardware address. A plain array, because that is what the reference passes around and the
/// cascade below turns on the CONTENT of the buffer rather than on any wrapper's state.
pub type MacAddr = [u8; ETH_ALEN];

/// `MAC0_BKP = 0x19e0` (r8169_main.c:439) — the 8125's backup copy of the address.
pub const MAC0_BKP: u32 = 0x19e0;

/// The two ERI addresses `rtl_read_mac_address` reads (:5348, :5350).
pub const ERI_MAC_LOW: u32 = 0xe0;
pub const ERI_MAC_HIGH: u32 = 0xe4;

/// `is_multicast_ether_addr` — the low bit of the FIRST octet.
pub fn is_multicast(a: &MacAddr) -> bool {
    a[0] & 1 != 0
}

/// `is_zero_ether_addr`.
pub fn is_zero(a: &MacAddr) -> bool {
    a.iter().all(|b| *b == 0)
}

/// `is_valid_ether_addr` — not multicast AND not all zero.
///
/// Linux's own comment at the definition explains the omission a reader would otherwise file as a
/// bug: `ff:ff:ff:ff:ff:ff` needs no explicit case because broadcast IS multicast. That is also
/// what makes [`eri::LINUX_READ_TIMEOUT_SENTINEL`] survivable — see the test that pins it.
pub fn is_valid(a: &MacAddr) -> bool {
    !is_multicast(a) && !is_zero(a)
}

/// `rtl_is_8125` (:861-:864).
pub fn is_8125(v: MacVersion) -> bool {
    v.0 >= 61
}

/// `rtl_is_8168evl_up` (:866-:871) — THREE conditions, and the middle one is a HOLE, not a bound.
/// Version 39 sits inside the range and is excluded from it.
pub fn is_8168evl_up(v: MacVersion) -> bool {
    v.0 >= 34 && v.0 != 39 && v.0 <= 52
}

/// Which hardware location, if any, `rtl_read_mac_address` will consult for this part.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HwMacSource {
    /// ERI 0xe0/0xe4 (:5346-:5351).
    Eri,
    /// The `MAC0_BKP` register block (:5352-:5355).
    Mac0Bkp,
    /// NEITHER. `rtl_read_mac_address` is a `void` function with no `else`: for a part that is
    /// neither 8168evl-and-up nor 8125 it TOUCHES NOTHING and returns. The caller's buffer keeps
    /// whatever it already held — which is why `rtl_init_mac_address` zero-initialises it (:5561)
    /// and validates afterwards. A port that returns an address unconditionally, or that reports
    /// success here, collapses the cascade's second step into its third.
    None,
}

/// `rtl_read_mac_address` (:5343-:5356), as a decision.
///
/// A FOURTH CONDITION LIVES HERE, NOT IN `rtl_is_8168evl_up`. The ERI branch reads
/// `rtl_is_8168evl_up(tp) && tp->mac_version != RTL_GIGA_MAC_VER_34` (:5346). Version 34 satisfies
/// the predicate — it is the predicate's own lower bound — and is then excluded by name. Losing
/// that one clause sends an 8168evl to ERI for an address it does not keep there.
///
/// Version 34 is special-cased in the OPPOSITE direction on the way out: it is the one part
/// [`rar_set`] gives the extra `rtl_rar_exgmac_set` writes to (:2569-:2570). It does not read its
/// address from ERI; it writes it there anyway.
pub fn hw_mac_source(v: MacVersion) -> HwMacSource {
    if is_8168evl_up(v) && v.0 != 34 {
        HwMacSource::Eri
    } else if is_8125(v) {
        HwMacSource::Mac0Bkp
    } else {
        HwMacSource::None
    }
}

/// Split a 48-bit address across the two ERI words the way `rtl_read_mac_address` reassembles it
/// (:5348-:5351).
///
/// THE HIGH WORD IS SIXTEEN BITS OF A THIRTY-TWO BIT READ. `put_unaligned_le16(value, mac_addr + 4)`
/// keeps the low half of the 0xe4 read and DISCARDS the upper half. Using `le32` there — the
/// symmetry a reader reaches for, having just written `le32` for 0xe0 — writes eight bytes into a
/// six-byte address.
pub fn mac_from_eri_words(low: u32, high: u32) -> MacAddr {
    let l = low.to_le_bytes();
    let h = (high as u16).to_le_bytes();
    [l[0], l[1], l[2], l[3], h[0], h[1]]
}

/// `rtl_read_mac_from_reg` (:880-:886) — six SEPARATE byte reads at consecutive offsets.
///
/// Byte reads, not one 32-bit plus one 16-bit read. `MAC0_BKP` at 0x19e0 is read by the same helper
/// as `MAC0` at 0, so whatever access width the block requires, it is the byte width.
pub fn read_mac_from_reg<B: Bus>(bus: &mut B, reg: u32) -> MacAddr {
    let mut mac = [0u8; ETH_ALEN];
    for (i, b) in mac.iter_mut().enumerate() {
        *b = bus.r8(reg + i as u32);
    }
    mac
}

/// `rtl_read_mac_address` (:5343). Returns `None` when the part has no hardware source, which is
/// the case the C signature cannot express.
pub fn read_mac_address<B: Bus>(bus: &mut B, v: MacVersion) -> Option<MacAddr> {
    match hw_mac_source(v) {
        HwMacSource::Eri => {
            let low = eri::read_exgmac(bus, ERI_MAC_LOW, v.0).ok()?;
            let high = eri::read_exgmac(bus, ERI_MAC_HIGH, v.0).ok()?;
            Some(mac_from_eri_words(low, high))
        }
        HwMacSource::Mac0Bkp => Some(read_mac_from_reg(bus, MAC0_BKP)),
        HwMacSource::None => None,
    }
}

/// Which of the four sources supplied the address, in the order `rtl_init_mac_address` tries them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MacSource {
    /// `eth_platform_get_mac_address` (:5565) — device tree, EFI, or a platform hook.
    Platform,
    /// `rtl_read_mac_address` (:5569) — ERI or MAC0_BKP, per [`hw_mac_source`].
    Hardware,
    /// `rtl_read_mac_from_reg(tp, mac_addr, MAC0)` (:5573).
    Mac0,
    /// `eth_random_addr` (:5577), with `NET_ADDR_RANDOM` and a warning.
    Random,
}

/// The cascade, as a value. A fixed-size array, so the count is a COMPILE-TIME invariant: adding or
/// dropping a source cannot pass the coverage test by changing a number in two places.
pub const MAC_SOURCE_ORDER: [MacSource; 4] = [
    MacSource::Platform,
    MacSource::Hardware,
    MacSource::Mac0,
    MacSource::Random,
];

/// `rtl_init_mac_address` (:5559-:5583), as a pure decision over the candidates.
///
/// THE PLATFORM ADDRESS IS NOT VALIDATED. Linux writes
/// `rc = eth_platform_get_mac_address(...); if (!rc) goto done;` — it jumps straight past the
/// checks. Both HARDWARE sources are then put through `is_valid_ether_addr`, and so is MAC0. A port
/// that validates all four uniformly, which is the tidier code and the obvious reading, REJECTS a
/// platform-supplied address that the reference accepts, and falls through to a register or to a
/// random address on a board whose whole point was to override them. Preserved deliberately: the
/// platform is a trusted source, the silicon is not.
///
/// `Random` carries no address because one cannot be produced from the inputs; the caller draws it.
pub fn select_source(
    platform: Option<MacAddr>,
    hardware: Option<MacAddr>,
    mac0: MacAddr,
) -> (MacSource, Option<MacAddr>) {
    if let Some(a) = platform {
        return (MacSource::Platform, Some(a));
    }
    if let Some(a) = hardware {
        if is_valid(&a) {
            return (MacSource::Hardware, Some(a));
        }
    }
    if is_valid(&mac0) {
        return (MacSource::Mac0, Some(mac0));
    }
    (MacSource::Random, None)
}

/// The four `rtl_eri_write` calls of `rtl_rar_exgmac_set` (:2503-:2509), as `(addr, value)`.
///
/// THE SECOND PAIR IS NOT A COPY OF THE FIRST. 0xe0/0xe4 hold the address aligned at byte 0;
/// 0xf0/0xf4 hold the SAME address shifted by two bytes — `le16(addr[0..2]) << 16` into the upper
/// half of 0xf0, and `le32(addr[2..6])` into 0xf4. The low half of 0xf0 is written as zero. A
/// reader who sees 0xe0/0xe4 and assumes 0xf0/0xf4 repeat them writes a garbled second copy, and
/// every one of the four writes still succeeds.
pub fn rar_exgmac_words(a: &MacAddr) -> [(u32, u32); 4] {
    let le32_0 = u32::from_le_bytes([a[0], a[1], a[2], a[3]]);
    let le16_4 = u16::from_le_bytes([a[4], a[5]]) as u32;
    let le16_0 = u16::from_le_bytes([a[0], a[1]]) as u32;
    let le32_2 = u32::from_le_bytes([a[2], a[3], a[4], a[5]]);
    [
        (0xe0, le32_0),
        (0xe4, le16_4),
        (0xf0, le16_0 << 16),
        (0xf4, le32_2),
    ]
}

/// `rtl_rar_set` (:2559-:2574).
///
/// MAC4 IS WRITTEN FIRST. The order is not incidental: the address is only coherent once both
/// halves have landed, and each is followed by `rtl_pci_commit` (:2564, :2567) — a dummy read of
/// `ChipCmd` whose only job is to flush the posted write before the next one is issued. Dropping
/// either commit leaves two writes in flight over a bus that may reorder them; dropping the order
/// leaves the part briefly holding a new low half against an old high half.
///
/// MAC4 also takes a 32-BIT WRITE OF A 16-BIT VALUE — `RTL_W32(tp, MAC4, get_unaligned_le16(...))`
/// — so the upper two bytes of that register are deliberately zeroed on every set.
///
/// NOT MODELLED HERE: Linux brackets the whole sequence in `rtl_unlock_config_regs` /
/// `rtl_lock_config_regs` (:2561, :2573). That pair is `init.rs`'s, and this function does not
/// issue it — a caller driving real silicon must, and the omission is recorded rather than hidden
/// because a set that runs against locked config registers fails silently.
pub fn rar_set<B: Bus>(bus: &mut B, a: &MacAddr, v: MacVersion) {
    bus.w32(MAC4, u16::from_le_bytes([a[4], a[5]]) as u32);
    let _ = bus.r8(CHIP_CMD);
    bus.w32(MAC0, u32::from_le_bytes([a[0], a[1], a[2], a[3]]));
    let _ = bus.r8(CHIP_CMD);
    if v.0 == 34 {
        for (addr, val) in rar_exgmac_words(a) {
            let _ = eri::write_exgmac(bus, addr, eri::ERIAR_MASK_1111, val, v.0);
        }
    }
}
