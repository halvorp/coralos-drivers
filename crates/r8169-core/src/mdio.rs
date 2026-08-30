// SPDX-License-Identifier: GPL-2.0-only
//! PHY register access — the two MDIO paths, and the delays that are part of the contract.
//!
//! Ported from Linux `drivers/net/ethernet/realtek/r8169_main.c`:
//!   * `r8169_mdio_write` / `r8169_mdio_read` (:1291, :1303) — the legacy PHYAR path;
//!   * `r8168_phy_ocp_write` / `r8168_phy_ocp_read` (:1111, :1121) — the OCP GPHY path used by
//!     8168g and later, which is the part on the CoralOS reference board (VER_40);
//!   * `rtl_ocp_reg_failure` (:1101), `rtl_phyar_cond` (:1286), `rtl_ocp_gphy_cond` (:1106);
//!   * `rtl_loop_wait` (:893) — the poll shape both paths share.
//!
//! Copyright (c) a lot of people. See the Linux source for the full authorship of r8169.
//!
//! THE TWO PATHS ARE NOT INTERCHANGEABLE AND MUST NOT BE UNIFIED. They differ in the register they
//! drive, the shift applied to the register number, the poll interval, and — the one most easily
//! lost — whether a mandatory settling delay follows the transaction. The legacy path requires
//! 20 us after completion before the next command; the OCP path requires none. A port that tidies
//! these into one helper either inserts a delay the OCP path does not need or, far worse, drops the
//! one the legacy path does, which yields a driver that works until it is driven quickly.

use crate::init::Bus;

/// `PHYAR = 0x60` (r8169_main.c:296).
pub const PHYAR: u32 = 0x60;
/// `GPHY_OCP = 0xb8` (r8169_main.c:415).
pub const GPHY_OCP: u32 = 0xb8;
/// `OCPAR_FLAG = 0x80000000` (r8169_main.c:412). PHYAR uses the same bit 31 as its busy/ready flag
/// (`rtl_phyar_cond`, :1288).
pub const OCPAR_FLAG: u32 = 0x8000_0000;
/// `OCP_STD_PHY_BASE = 0xa400` (r8169_main.c:80).
pub const OCP_STD_PHY_BASE: u32 = 0xa400;

/// `rtl_loop_wait(..., 25, 20)` at :1295 and :1309 — 25 polls, 20 us apart.
pub const LEGACY_POLL_N: u32 = 25;
pub const LEGACY_POLL_US: u32 = 20;
/// The `udelay(20)` at :1297 and :1315. Linux quotes the hardware spec: "a 20us delay is required
/// after write complete indication, but before sending next command."
pub const LEGACY_POST_US: u32 = 20;
/// `rtl_loop_wait(..., 25, 10)` at :1118 and :1133 — 25 polls, 10 us apart. TEN, not twenty.
pub const OCP_POLL_N: u32 = 25;
pub const OCP_POLL_US: u32 = 10;

/// Why a PHY access gave up. A timeout is not a bad register, and reporting one as the other sends
/// the reader looking in the wrong place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MdioError {
    /// The busy flag never settled within the poll budget.
    Timeout,
    /// `rtl_ocp_reg_failure` (:1101): the OCP register is out of range or odd. Linux WARNs once and
    /// returns without touching the bus; refusing loudly is the same choice, made explicit.
    InvalidOcpReg(u32),
}

/// `rtl_ocp_reg_failure` (:1101) — `reg & 0xffff0001`. An OCP register must fit in 16 bits AND be
/// even; the low bit is not a register number, it is a mistake.
pub fn ocp_reg_is_invalid(reg: u32) -> bool {
    reg & 0xffff_0001 != 0
}

/// The word `r8169_mdio_write` puts in PHYAR (:1293).
pub fn phyar_write_word(reg: u32, value: u16) -> u32 {
    OCPAR_FLAG | (reg & 0x1f) << 16 | value as u32
}

/// The word `r8169_mdio_read` puts in PHYAR (:1305). Bit 31 CLEAR is what makes it a read.
pub fn phyar_read_word(reg: u32) -> u32 {
    (reg & 0x1f) << 16
}

/// The word `r8168_phy_ocp_write` puts in GPHY_OCP (:1116).
///
/// The shift is FIFTEEN, not sixteen. `reg` is a byte address and always even, so `reg << 15` is
/// `(reg / 2) << 16` — the register index, not the byte offset. Writing `reg << 16` compiles,
/// addresses the wrong PHY register, and is silent.
pub fn ocp_write_word(reg: u32, data: u16) -> u32 {
    OCPAR_FLAG | (reg << 15) | data as u32
}

/// The word `r8168_phy_ocp_read` puts in GPHY_OCP (:1131).
pub fn ocp_read_word(reg: u32) -> u32 {
    reg << 15
}

/// `r8168g_mdio_write`/`read` (:1250-:1268): an MII register number becomes an OCP byte address.
/// The `reg -= 0x10` applies only when the page base is not the standard one.
pub fn ocp_addr_for_mii(ocp_base: u32, reg: u32) -> u32 {
    let reg = if ocp_base != OCP_STD_PHY_BASE { reg - 0x10 } else { reg };
    ocp_base + reg * 2
}

/// `rtl_loop_wait` (:893): check FIRST, then sleep — so a device that is already ready costs no
/// delay at all. Returns whether the flag reached `high` within `n` polls.
fn loop_wait<B: Bus>(bus: &mut B, reg: u32, flag: u32, high: bool, n: u32, us: u32) -> bool {
    for _ in 0..n {
        if (bus.r32(reg) & flag != 0) == high {
            return true;
        }
        bus.delay_us(us);
    }
    false
}

/// `r8169_mdio_write` (:1291) — legacy PHYAR path.
pub fn legacy_write<B: Bus>(bus: &mut B, reg: u32, value: u16) -> Result<(), MdioError> {
    bus.w32(PHYAR, phyar_write_word(reg, value));
    let settled = loop_wait(bus, PHYAR, OCPAR_FLAG, false, LEGACY_POLL_N, LEGACY_POLL_US);
    // The post-delay is issued whether or not the flag settled, exactly as Linux does: the C code
    // calls udelay(20) unconditionally after rtl_loop_wait_low, because the delay protects the NEXT
    // command from this one, and a command that timed out is the case most likely to be retried.
    bus.delay_us(LEGACY_POST_US);
    if settled { Ok(()) } else { Err(MdioError::Timeout) }
}

/// `r8169_mdio_read` (:1303) — legacy PHYAR path.
pub fn legacy_read<B: Bus>(bus: &mut B, reg: u32) -> Result<u16, MdioError> {
    bus.w32(PHYAR, phyar_read_word(reg));
    let ready = loop_wait(bus, PHYAR, OCPAR_FLAG, true, LEGACY_POLL_N, LEGACY_POLL_US);
    let value = if ready { Some((bus.r32(PHYAR) & 0xffff) as u16) } else { None };
    bus.delay_us(LEGACY_POST_US);
    value.ok_or(MdioError::Timeout)
}

/// `r8168_phy_ocp_write` (:1111) — the 8168g+ path. No post-delay: see the module note.
pub fn ocp_write<B: Bus>(bus: &mut B, reg: u32, data: u16) -> Result<(), MdioError> {
    if ocp_reg_is_invalid(reg) {
        return Err(MdioError::InvalidOcpReg(reg));
    }
    bus.w32(GPHY_OCP, ocp_write_word(reg, data));
    if loop_wait(bus, GPHY_OCP, OCPAR_FLAG, false, OCP_POLL_N, OCP_POLL_US) {
        Ok(())
    } else {
        Err(MdioError::Timeout)
    }
}

/// `r8168_phy_ocp_read` (:1121) — the 8168g+ path.
pub fn ocp_read<B: Bus>(bus: &mut B, reg: u32) -> Result<u16, MdioError> {
    if ocp_reg_is_invalid(reg) {
        return Err(MdioError::InvalidOcpReg(reg));
    }
    bus.w32(GPHY_OCP, ocp_read_word(reg));
    if loop_wait(bus, GPHY_OCP, OCPAR_FLAG, true, OCP_POLL_N, OCP_POLL_US) {
        Ok((bus.r32(GPHY_OCP) & 0xffff) as u16)
    } else {
        Err(MdioError::Timeout)
    }
}
