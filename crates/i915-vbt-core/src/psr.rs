// SPDX-License-Identifier: GPL-2.0-only
//! Block 9 (BDB_PSR): the per-panel PSR (panel self refresh) table. `parse_psr()` — intel_bios.c:1550-1650;
//! `struct psr_table` / `struct bdb_psr` — intel_vbt_defs.h:711-731.

use crate::bdb::{BlockId, Vbt};
use crate::error::VbtError;
use crate::panel::{panel_bits, PANEL_COUNT};

/// sizeof(struct psr_table): 2 bytes of bits + u16 tp1 + u16 tp2_tp3 — intel_vbt_defs.h:711-724.
pub const PSR_TABLE_SIZE: usize = 6;
/// Offset of `psr2_tp2_tp3_wakeup_time` (BDB 226+) after `psr_table[16]` — intel_vbt_defs.h:727-731.
pub const PSR2_WAKEUP_OFFSET: usize = PSR_TABLE_SIZE * PANEL_COUNT;

/// Platform inputs `parse_psr()` branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PsrPlatform {
    pub display_ver: u8,
    pub broxton: bool,
}
impl PsrPlatform {
    /// Kaby Lake: display version 9, not Broxton.
    pub const KABYLAKE: PsrPlatform = PsrPlatform { display_ver: 9, broxton: false };
}

/// Decoded `panel->vbt.psr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PsrParams {
    pub full_link: bool,
    pub require_aux_wakeup: bool,
    pub idle_frames: u8,
    pub tp1_wakeup_time_us: u32,
    pub tp2_tp3_wakeup_time_us: u32,
    pub psr2_tp2_tp3_wakeup_time_us: u32,
}

/// The 2-bit wake-up encoding of BDB 205+ (0=500us, 1=100us, 2=2500us, 3=0us; out of range = 2500) —
/// intel_bios.c:1576-1594 (tp1) and :1592-1608 (tp2_tp3).
pub const fn wakeup_us_new(v: u16) -> u32 {
    match v {
        0 => 500,
        1 => 100,
        3 => 0,
        _ => 2500,
    }
}

/// `parse_psr()`; `Ok(None)` when the VBT has no PSR block ("No PSR BDB found").
pub fn parse_psr(vbt: &Vbt, panel_type: u8, plat: PsrPlatform) -> Result<Option<PsrParams>, VbtError> {
    if usize::from(panel_type) >= PANEL_COUNT {
        return Err(VbtError::PanelTypeOutOfRange { panel_type });
    }
    let Some(b) = vbt.find_block(BlockId::Psr as u8)? else { return Ok(None) };
    b.require(PSR2_WAKEUP_OFFSET)?;
    let o = usize::from(panel_type) * PSR_TABLE_SIZE;
    let bits0 = b.u8_at(o); // full_link:1, require_aux_to_wakeup:1
    let bits1 = b.u8_at(o + 1); // idle_frames:4, lines_to_wait:3
    let (tp1, tp23) = (b.u16_at(o + 2), b.u16_at(o + 4));
    let (tp1_us, tp23_us) = if vbt.version >= 205 && plat.display_ver >= 9 && !plat.broxton {
        (wakeup_us_new(tp1), wakeup_us_new(tp23))
    } else {
        // "Old decimal value is wake up time in multiples of 100 us" — intel_bios.c:1571 and :1615-1618
        (u32::from(tp1) * 100, u32::from(tp23) * 100)
    };
    let psr2 = if vbt.version >= 226 {
        match panel_bits(b.u32_at(PSR2_WAKEUP_OFFSET), panel_type, 2) {
            0 => 500,
            1 => 100,
            3 => 50,
            _ => 2500,
        }
    } else {
        tp23_us // "Reusing PSR1 wakeup time for PSR2 in older VBTs" — intel_bios.c:1641
    };
    Ok(Some(PsrParams {
        full_link: bits0 & 1 != 0,
        require_aux_wakeup: bits0 & 2 != 0,
        idle_frames: bits1 & 0xf,
        tp1_wakeup_time_us: tp1_us,
        tp2_tp3_wakeup_time_us: tp23_us,
        psr2_tp2_tp3_wakeup_time_us: psr2,
    }))
}
