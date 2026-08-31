// SPDX-License-Identifier: GPL-2.0-only
//! ERI — the Extended Register Interface, and the command word that drives it.
//!
//! Ported from Linux `drivers/net/ethernet/realtek/r8169_main.c`:
//!   * the `ERIAR_*` defines (:389-:403) and `ERIDR`/`ERIAR` (:387-:388);
//!   * `_rtl_eri_write` (:1047) and `_rtl_eri_read` (:1069), with `rtl_eri_write`/`rtl_eri_read`
//!     (:1062, :1079) as the EXGMAC-typed wrappers everything else calls;
//!   * `rtl_eriar_cond` (:1042) — the busy/ready poll;
//!   * `r8168fp_adjust_ocp_cmd` (:1035) — the one chip that needs the command word altered.
//!
//! Copyright (c) a lot of people. See the Linux source for the full authorship of r8169.
//!
//! WHY THIS MODULE EXISTS AHEAD OF ITS CALLERS: the MAC address on an 8168evl-and-later part is not
//! in a register, it is behind ERI at 0xe0/0xe4 (`rtl_read_mac_address`, :5343). `mac.rs` cannot be
//! honest about where an address comes from without this.

use crate::init::Bus;

/// `ERIDR = 0x70`, `ERIAR = 0x74` (r8169_main.c:387-:388). Data register and address/command
/// register: ERIDR is written BEFORE ERIAR on a write, and read AFTER ERIAR on a read.
pub const ERIDR: u32 = 0x70;
pub const ERIAR: u32 = 0x74;

/// `ERIAR_FLAG = 0x80000000` (:389) — the busy/ready bit read back from ERIAR.
pub const ERIAR_FLAG: u32 = 0x8000_0000;
/// `ERIAR_WRITE_CMD = 0x80000000` (:390) and `ERIAR_READ_CMD = 0x00000000` (:391).
///
/// THREE FACTS ABOUT THESE THAT A PORT LOSES BY TIDYING THEM.
///
/// One: `ERIAR_READ_CMD` is ZERO. It contributes nothing to the command word; what makes a
/// transaction a read is the ABSENCE of bit 31, not the presence of anything. Dropping the term
/// as dead code is harmless — writing `| 1` in its place is not.
///
/// Two: `ERIAR_WRITE_CMD` and `ERIAR_FLAG` are the SAME BIT. Bit 31 means "this is a write" on the
/// way in and "still busy" on the way back, so the command word cannot be read back to learn what
/// was asked for.
///
/// Three: because of one and two together, a read command word and a settled-idle status word are
/// bit-identical in their top halves. Only the direction of the poll distinguishes them, which is
/// why `read` and `write` below wait for OPPOSITE senses of the same bit.
pub const ERIAR_WRITE_CMD: u32 = 0x8000_0000;
pub const ERIAR_READ_CMD: u32 = 0x0000_0000;

/// `ERIAR_ADDR_BYTE_ALIGN = 4` (:392). The address must be a multiple of this; see [`EriError`].
pub const ERIAR_ADDR_BYTE_ALIGN: u32 = 4;

/// `ERIAR_TYPE_SHIFT = 16` (:393).
pub const ERIAR_TYPE_SHIFT: u32 = 16;
/// `ERIAR_EXGMAC = 0x00 << 16` (:394) — the type every ordinary caller uses.
pub const ERIAR_EXGMAC: u32 = 0x00 << ERIAR_TYPE_SHIFT;
/// `ERIAR_MSIX = 0x01 << 16` (:395).
pub const ERIAR_MSIX: u32 = 0x01 << ERIAR_TYPE_SHIFT;
/// `ERIAR_ASF = 0x02 << 16` (:396) and `ERIAR_OOB = 0x02 << 16` (:397).
///
/// TWO NAMES, ONE ENCODING. Linux defines both and they are numerically EQUAL. A port that makes
/// the type an enum with one variant per name invents a distinction the hardware does not have —
/// and then [`adjust_ocp_cmd`] below, whose condition tests the numeric type, appears to fire for
/// one name and not the other when in fact it fires for both.
pub const ERIAR_ASF: u32 = 0x02 << ERIAR_TYPE_SHIFT;
pub const ERIAR_OOB: u32 = 0x02 << ERIAR_TYPE_SHIFT;

/// `ERIAR_MASK_SHIFT = 12` (:398) and the five byte-enable masks (:399-:403).
pub const ERIAR_MASK_SHIFT: u32 = 12;
pub const ERIAR_MASK_0001: u32 = 0x1 << ERIAR_MASK_SHIFT;
pub const ERIAR_MASK_0011: u32 = 0x3 << ERIAR_MASK_SHIFT;
pub const ERIAR_MASK_0100: u32 = 0x4 << ERIAR_MASK_SHIFT;
pub const ERIAR_MASK_0101: u32 = 0x5 << ERIAR_MASK_SHIFT;
pub const ERIAR_MASK_1111: u32 = 0xf << ERIAR_MASK_SHIFT;

/// `rtl_loop_wait_{low,high}(tp, &rtl_eriar_cond, 100, 100)` (:1060, :1077) — 100 polls, 100 us
/// apart. Both directions use the same budget, unlike the two MDIO paths in `mdio.rs`.
pub const ERI_POLL_N: u32 = 100;
pub const ERI_POLL_US: u32 = 100;

/// `RTL_GIGA_MAC_VER_52` — the one part `r8168fp_adjust_ocp_cmd` (:1035) alters the command for.
pub const ADJUST_OCP_MAC_VER: u8 = 52;

/// The value `r8168fp_adjust_ocp_cmd` ORs in: `0xf70 << 18` (:1039). Linux's own comment says this
/// is "based on RTL8168FP_OOBMAC_BASE in vendor driver" — it is a magic base address from a vendor
/// blob, not a derivable constant, which is exactly why it is quoted rather than computed.
pub const ADJUST_OCP_OR: u32 = 0xf70 << 18;

/// Why an ERI access did not happen, or did not complete.
///
/// Linux expresses the first two as `WARN(...)` plus a bare `return` from a `void` function, so a
/// refused write is INDISTINGUISHABLE FROM A COMPLETED ONE at the call site. Naming them is the
/// same choice made explicit: a caller that ignores the result at least had the option not to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EriError {
    /// `addr & 3` (:1053). The address is not [`ERIAR_ADDR_BYTE_ALIGN`]-aligned.
    UnalignedAddr(u32),
    /// `!mask` (:1053). A write with no byte enables set would write nothing; Linux treats asking
    /// for it as a bug rather than as a no-op.
    EmptyMask,
    /// The busy flag never settled within [`ERI_POLL_N`] polls.
    Timeout,
}

/// What `_rtl_eri_read` returns when the poll times out (:1076-:1077): `~0`.
///
/// A SENTINEL THAT IS ALSO A LEGAL READING. `0xffffffff` is what an absent or powered-down device
/// reads back on many buses, and it is also a value the interface could legitimately hold. Linux
/// gets away with it here only because its single most important caller — the MAC address read at
/// :5347 — feeds the result through `is_valid_ether_addr`, and `ff:ff:ff:ff:ff:ff` has the
/// multicast bit set and is therefore rejected. `mac.rs` asserts that coincidence rather than
/// relying on it silently. [`read`] returns a `Result` so a new caller need not depend on it.
pub const LINUX_READ_TIMEOUT_SENTINEL: u32 = !0;

/// `r8168fp_adjust_ocp_cmd` (:1035-:1040).
///
/// The condition is BOTH `type == ERIAR_OOB` AND `mac_version == 52`. Applied to reads and writes
/// alike, and applied AFTER the command word is otherwise complete.
pub fn adjust_ocp_cmd(cmd: u32, typ: u32, mac_version: u8) -> u32 {
    if typ == ERIAR_OOB && mac_version == ADJUST_OCP_MAC_VER {
        cmd | ADJUST_OCP_OR
    } else {
        cmd
    }
}

/// The word `_rtl_eri_write` puts in ERIAR (:1050), before [`adjust_ocp_cmd`].
pub fn write_cmd_word(addr: u32, mask: u32, typ: u32) -> u32 {
    ERIAR_WRITE_CMD | typ | mask | addr
}

/// The word `_rtl_eri_read` puts in ERIAR (:1070), before [`adjust_ocp_cmd`].
///
/// THE MASK IS NOT A PARAMETER. A read always asserts all four byte enables; only a write takes a
/// caller-supplied mask. A port that gives `read` a mask argument for symmetry has invented an
/// interface the hardware does not offer.
pub fn read_cmd_word(addr: u32, typ: u32) -> u32 {
    ERIAR_READ_CMD | typ | ERIAR_MASK_1111 | addr
}

/// `_rtl_eri_write` (:1047-:1061). ERIDR first, then ERIAR, then wait for bit 31 to CLEAR.
pub fn write<B: Bus>(
    bus: &mut B,
    addr: u32,
    mask: u32,
    val: u32,
    typ: u32,
    mac_version: u8,
) -> Result<(), EriError> {
    if addr & (ERIAR_ADDR_BYTE_ALIGN - 1) != 0 {
        return Err(EriError::UnalignedAddr(addr));
    }
    if mask == 0 {
        return Err(EriError::EmptyMask);
    }
    let cmd = adjust_ocp_cmd(write_cmd_word(addr, mask, typ), typ, mac_version);
    bus.w32(ERIDR, val);
    bus.w32(ERIAR, cmd);
    for _ in 0..ERI_POLL_N {
        if bus.r32(ERIAR) & ERIAR_FLAG == 0 {
            return Ok(());
        }
        bus.delay_us(ERI_POLL_US);
    }
    Err(EriError::Timeout)
}

/// `_rtl_eri_read` (:1069-:1078). ERIAR first, then wait for bit 31 to SET, then read ERIDR.
///
/// The alignment and mask refusals of [`write`] are deliberately absent: Linux checks them only on
/// the write path, and adding them here would refuse accesses the reference performs.
pub fn read<B: Bus>(bus: &mut B, addr: u32, typ: u32, mac_version: u8) -> Result<u32, EriError> {
    let cmd = adjust_ocp_cmd(read_cmd_word(addr, typ), typ, mac_version);
    bus.w32(ERIAR, cmd);
    for _ in 0..ERI_POLL_N {
        if bus.r32(ERIAR) & ERIAR_FLAG != 0 {
            return Ok(bus.r32(ERIDR));
        }
        bus.delay_us(ERI_POLL_US);
    }
    Err(EriError::Timeout)
}

/// `rtl_eri_read` (:1079) — the EXGMAC-typed wrapper.
pub fn read_exgmac<B: Bus>(bus: &mut B, addr: u32, mac_version: u8) -> Result<u32, EriError> {
    read(bus, addr, ERIAR_EXGMAC, mac_version)
}

/// `rtl_eri_write` (:1062) — the EXGMAC-typed wrapper.
pub fn write_exgmac<B: Bus>(
    bus: &mut B,
    addr: u32,
    mask: u32,
    val: u32,
    mac_version: u8,
) -> Result<(), EriError> {
    write(bus, addr, mask, val, ERIAR_EXGMAC, mac_version)
}
